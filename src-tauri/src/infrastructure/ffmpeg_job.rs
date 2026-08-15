use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    process::Stdio,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::watch,
};

use crate::{
    application::jobs::ExportRuntime,
    domain::{export_plan::ExportPlan, media::MediaInfo},
};

use super::{
    encoder::{EncoderSelection, video_arguments},
    progress::{ExportPhase, ExportProgress, ProgressParser},
    recovery::RecoveryStore,
    tool_locator::media_command,
    validation::{ValidationSummary, validate_export},
};

pub const EXPORT_PROGRESS_EVENT: &str = "spycut://export-progress";
pub const EXPORT_RESULT_EVENT: &str = "spycut://export-result";

#[derive(Clone, Debug)]
pub struct ExportJobConfig {
    pub job_id: String,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub partial: PathBuf,
    pub filter_script: PathBuf,
    pub media: MediaInfo,
    pub plan: ExportPlan,
    pub encoder: EncoderSelection,
    pub recovery_store: Option<RecoveryStore>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportOutcomeStatus {
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub job_id: String,
    pub status: ExportOutcomeStatus,
    pub output_path: Option<String>,
    pub message: String,
    pub validation: Option<ValidationSummary>,
}

pub async fn run_export_job<R: Runtime>(
    app: AppHandle<R>,
    runtime: ExportRuntime,
    mut cancel_rx: watch::Receiver<bool>,
    config: ExportJobConfig,
) {
    emit_progress(
        &app,
        &config.job_id,
        ExportPhase::Preparing,
        0.0,
        &config.plan,
        None,
        "正在创建安全的临时输出文件",
    );

    let result = run_process(&app, &mut cancel_rx, &config).await;
    cleanup_file(&config.filter_script);

    let export_result = match result {
        ProcessOutcome::ExitedSuccessfully => {
            emit_progress(
                &app,
                &config.job_id,
                ExportPhase::Validating,
                96.0,
                &config.plan,
                None,
                "正在核对时长、编码格式和每个连接点",
            );
            match validate_export(
                &config.ffmpeg,
                &config.ffprobe,
                &config.partial,
                &config.media,
                &config.plan,
            )
            .await
            {
                Ok(validation) => {
                    emit_progress(
                        &app,
                        &config.job_id,
                        ExportPhase::Finalizing,
                        99.0,
                        &config.plan,
                        None,
                        "验收通过，正在提交最终 MP4",
                    );
                    match promote_partial(&config.partial, &config.destination, &config.job_id) {
                        Ok(()) => ExportResult {
                            job_id: config.job_id.clone(),
                            status: ExportOutcomeStatus::Completed,
                            output_path: Some(config.destination.to_string_lossy().into_owned()),
                            message: "精确导出和自动验收均已完成".into(),
                            validation: Some(validation),
                        },
                        Err(error) => {
                            cleanup_file(&config.partial);
                            failed_result(&config.job_id, format!("无法提交最终文件：{error}"))
                        }
                    }
                }
                Err(error) => {
                    cleanup_file(&config.partial);
                    failed_result(&config.job_id, format!("自动验收未通过：{error}"))
                }
            }
        }
        ProcessOutcome::Cancelled => {
            cleanup_file(&config.partial);
            ExportResult {
                job_id: config.job_id.clone(),
                status: ExportOutcomeStatus::Cancelled,
                output_path: None,
                message: "导出已取消，临时文件已清理".into(),
                validation: None,
            }
        }
        ProcessOutcome::Failed(message) => {
            cleanup_file(&config.partial);
            failed_result(&config.job_id, message)
        }
    };

    if let Some(recovery_store) = &config.recovery_store {
        let _ = recovery_store.clear(&config.job_id);
    }
    let _ = app.emit(EXPORT_RESULT_EVENT, &export_result);
    runtime.release(&config.job_id).await;
}

enum ProcessOutcome {
    ExitedSuccessfully,
    Cancelled,
    Failed(String),
}

async fn run_process<R: Runtime>(
    app: &AppHandle<R>,
    cancel_rx: &mut watch::Receiver<bool>,
    config: &ExportJobConfig,
) -> ProcessOutcome {
    if *cancel_rx.borrow() {
        return ProcessOutcome::Cancelled;
    }

    let mut command = build_command(config);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return ProcessOutcome::Failed(format!("无法启动 FFmpeg：{error}")),
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => return ProcessOutcome::Failed("无法读取 FFmpeg 进度".into()),
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => return ProcessOutcome::Failed("无法读取 FFmpeg 诊断信息".into()),
    };

    let progress_app = app.clone();
    let progress_job = config.job_id.clone();
    let source_duration_us = config.plan.source_duration_us;
    let progress_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        let mut parser = ProgressParser::default();
        loop {
            match tokio::time::timeout(std::time::Duration::from_secs(60), lines.next_line()).await
            {
                Ok(Ok(Some(line))) => {
                    if parser.push_line(&line) {
                        let _ = progress_app.emit(
                            EXPORT_PROGRESS_EVENT,
                            parser.report(&progress_job, source_duration_us),
                        );
                    }
                }
                Ok(Ok(None)) | Ok(Err(_)) => break,
                Err(_) => {
                    let _ = progress_app.emit(
                        EXPORT_PROGRESS_EVENT,
                        parser.stalled_report(&progress_job, source_duration_us),
                    );
                }
            }
        }
    });
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut tail = VecDeque::with_capacity(24);
        while let Ok(Some(line)) = lines.next_line().await {
            if tail.len() == 24 {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        tail.into_iter().collect::<Vec<_>>().join("\n")
    });

    enum WaitResult {
        Exited(std::io::Result<std::process::ExitStatus>),
        Cancelled,
    }
    let wait_result = tokio::select! {
        status = child.wait() => WaitResult::Exited(status),
        changed = cancel_rx.changed() => {
            let _ = changed;
            WaitResult::Cancelled
        }
    };
    let outcome = match wait_result {
        WaitResult::Cancelled => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            ProcessOutcome::Cancelled
        }
        WaitResult::Exited(Ok(status)) if status.success() => ProcessOutcome::ExitedSuccessfully,
        WaitResult::Exited(Ok(status)) => {
            ProcessOutcome::Failed(format!("FFmpeg 异常退出（状态 {status}）"))
        }
        WaitResult::Exited(Err(error)) => {
            ProcessOutcome::Failed(format!("等待 FFmpeg 时发生错误：{error}"))
        }
    };
    let _ = progress_task.await;
    let diagnostics = stderr_task.await.unwrap_or_default();

    match outcome {
        ProcessOutcome::Failed(message) if !diagnostics.trim().is_empty() => {
            ProcessOutcome::Failed(format!("{message}\n{diagnostics}"))
        }
        other => other,
    }
}

fn build_command(config: &ExportJobConfig) -> Command {
    let mut command = media_command(&config.ffmpeg);
    command.kill_on_drop(true);
    command
        .args([
            "-hide_banner",
            "-nostdin",
            "-y",
            "-loglevel",
            "warning",
            "-stats_period",
            "0.25",
            "-progress",
            "pipe:1",
            "-i",
        ])
        .arg(&config.source)
        // The deprecated `-filter_complex_script` alias is absent from newer
        // FFmpeg builds; this is the supported file-backed option syntax.
        .arg("-/filter_complex")
        .arg(&config.filter_script)
        .args(["-map", "[vout]"]);
    if config.plan.has_audio {
        command.args(["-map", "[aout]"]);
    }
    for argument in video_arguments(&config.encoder, &config.media) {
        command.arg(argument);
    }
    command.args([
        "-r",
        &format!(
            "{}/{}",
            config.plan.output_frame_rate.num, config.plan.output_frame_rate.den
        ),
        "-fps_mode",
        "cfr",
    ]);
    if config.plan.has_audio {
        command.args(["-c:a", "aac", "-b:a"]);
        command.arg(
            config
                .media
                .audio_bit_rate
                .unwrap_or(160_000)
                .clamp(96_000, 320_000)
                .to_string(),
        );
        if let Some(sample_rate) = config.media.audio_sample_rate {
            command.args(["-ar", &sample_rate.to_string()]);
        }
        if let Some(channels) = config.media.audio_channels {
            command.args(["-ac", &channels.to_string()]);
        }
        command.arg("-shortest");
    }
    command
        .args([
            "-sn",
            "-dn",
            "-map_metadata",
            "-1",
            "-map_chapters",
            "-1",
            "-metadata",
            "encoder=SpyCut",
            "-movflags",
            "+faststart",
        ])
        .arg(&config.partial)
        .args([
            "-map",
            "[vprogress]",
            "-an",
            "-c:v",
            "wrapped_avframe",
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn promote_partial(partial: &Path, destination: &Path, job_id: &str) -> std::io::Result<()> {
    if !destination.exists() {
        return std::fs::rename(partial, destination);
    }
    let backup = destination.with_extension(format!("spycut-{job_id}.backup"));
    std::fs::rename(destination, &backup)?;
    match std::fs::rename(partial, destination) {
        Ok(()) => {
            cleanup_file(&backup);
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::rename(&backup, destination);
            Err(error)
        }
    }
}

fn cleanup_file(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

fn failed_result(job_id: &str, message: String) -> ExportResult {
    ExportResult {
        job_id: job_id.into(),
        status: ExportOutcomeStatus::Failed,
        output_path: None,
        message,
        validation: None,
    }
}

fn emit_progress<R: Runtime>(
    app: &AppHandle<R>,
    job_id: &str,
    phase: ExportPhase,
    percent: f64,
    plan: &ExportPlan,
    speed: Option<String>,
    message: &str,
) {
    let _ = app.emit(
        EXPORT_PROGRESS_EVENT,
        ExportProgress {
            job_id: job_id.into(),
            phase,
            percent,
            processed_source_us: 0,
            source_duration_us: plan.source_duration_us,
            speed,
            message: message.into(),
        },
    );
}

#[cfg(test)]
mod ffmpeg_integration_tests {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[cfg(unix)]
    use tauri::Manager;
    use tempfile::tempdir;
    use tokio::process::Command;

    use crate::{
        domain::{interval::DeleteInterval, media::VideoCodec},
        infrastructure::{
            encoder::select_encoder,
            filter_script::build_filter_script,
            fingerprint::fingerprint_source,
            probe::probe_media,
            tool_locator::{MediaTool, locate_media_tool},
            validation::validate_export,
        },
    };

    use super::*;

    #[test]
    fn export_command_loads_the_filter_graph_from_a_file_with_the_supported_option() {
        for codec in [VideoCodec::H264, VideoCodec::Hevc] {
            let media = MediaInfo {
                duration_us: 10_000_000,
                container: "mp4".into(),
                video_codec: codec,
                width: 1920,
                height: 1080,
                frame_rate: crate::domain::media::FrameRate::new(30, 1).unwrap(),
                variable_frame_rate: false,
                video_stream_count: 1,
                audio_stream_count: 1,
                pixel_format: Some("yuv420p".into()),
                bit_depth: Some(8),
                video_bit_rate: Some(8_000_000),
                has_audio: true,
                audio_codec: Some("aac".into()),
                audio_sample_rate: Some(48_000),
                audio_channels: Some(2),
                audio_bit_rate: Some(160_000),
            };
            let plan = ExportPlan::build(
                &media,
                &[DeleteInterval::new(1, 2_000_000, 3_000_000).unwrap()],
            )
            .unwrap();
            let config = ExportJobConfig {
                job_id: format!("filter-option-{}", codec.ffmpeg_name()),
                ffmpeg: PathBuf::from("ffmpeg"),
                ffprobe: PathBuf::from("ffprobe"),
                source: PathBuf::from("source.mp4"),
                destination: PathBuf::from("output.mp4"),
                partial: PathBuf::from("output.partial.mp4"),
                filter_script: PathBuf::from("filter graph.txt"),
                media,
                plan,
                encoder: EncoderSelection {
                    name: "test-encoder".into(),
                    hardware_accelerated: false,
                    display_name: "test encoder".into(),
                },
                recovery_store: None,
            };

            let command = build_command(&config);
            let arguments = command
                .as_std()
                .get_args()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            let option_index = arguments
                .iter()
                .position(|argument| argument == "-/filter_complex")
                .expect("the export command should use FFmpeg's file-backed option syntax");

            assert_eq!(
                arguments.get(option_index + 1),
                Some(&config.filter_script.to_string_lossy().into_owned())
            );
            assert!(
                !arguments
                    .iter()
                    .any(|argument| argument == "-filter_complex_script")
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires the local FFmpeg toolchain"]
    async fn exact_export_pipeline_accepts_h264_and_h265_mp4() {
        let ffmpeg = locate_media_tool(MediaTool::Ffmpeg).expect("FFmpeg is required");
        let ffprobe = locate_media_tool(MediaTool::Ffprobe).expect("ffprobe is required");
        let directory = tempdir().unwrap();

        for codec in [VideoCodec::H264, VideoCodec::Hevc] {
            let source_encoder = match codec {
                #[cfg(target_os = "macos")]
                VideoCodec::H264 => "h264_videotoolbox",
                #[cfg(target_os = "macos")]
                VideoCodec::Hevc => "hevc_videotoolbox",
                #[cfg(target_os = "windows")]
                VideoCodec::H264 => "h264_mf",
                #[cfg(target_os = "windows")]
                VideoCodec::Hevc => "hevc_mf",
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                VideoCodec::H264 => "libx264",
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                VideoCodec::Hevc => "libx265",
            };
            let suffix = codec.ffmpeg_name();
            let source = directory.path().join(format!("source-{suffix}.mp4"));
            let output = directory.path().join(format!("output-{suffix}.mp4"));
            let filter = directory.path().join(format!("filter-{suffix}.txt"));
            let mut fixture_command = Command::new(&ffmpeg);
            fixture_command.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=30:duration=6",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=880:sample_rate=48000:duration=6",
                "-c:v",
                source_encoder,
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ]);
            if source_encoder.starts_with("libx") {
                fixture_command.args(["-preset", "ultrafast"]);
            }
            let generated = fixture_command.arg(&source).output().await.unwrap();
            assert!(
                generated.status.success(),
                "fixture generation failed: {}",
                String::from_utf8_lossy(&generated.stderr)
            );

            let media = probe_media(&ffprobe, &source).await.unwrap();
            let source_identity_before = fingerprint_source(&source).unwrap();
            assert_eq!(media.video_codec, codec);
            let plan = ExportPlan::build(
                &media,
                &[
                    DeleteInterval::new(1, 1_000_000, 2_500_000).unwrap(),
                    DeleteInterval::new(2, 4_000_000, 4_750_000).unwrap(),
                ],
            )
            .unwrap();
            std::fs::write(&filter, build_filter_script(&plan).unwrap()).unwrap();
            let encoder = select_encoder(&ffmpeg, codec).await.unwrap();
            let config = ExportJobConfig {
                job_id: format!("test-{suffix}"),
                ffmpeg: ffmpeg.clone(),
                ffprobe: ffprobe.clone(),
                source: source.clone(),
                destination: output.clone(),
                partial: output.clone(),
                filter_script: filter,
                media: media.clone(),
                plan: plan.clone(),
                encoder,
                recovery_store: None,
            };
            let exported = build_command(&config).output().await.unwrap();
            assert!(
                exported.status.success(),
                "export failed: {}",
                String::from_utf8_lossy(&exported.stderr)
            );
            let summary = validate_export(&ffmpeg, &ffprobe, &output, &media, &plan)
                .await
                .unwrap();
            assert!(summary.duration_delta_us <= 75_000);
            assert!(
                summary.av_duration_delta_us.unwrap() <= plan.output_frame_rate.frame_duration_us()
            );
            assert!(summary.decoded_checkpoints >= 6);
            assert_eq!(source_identity_before, fingerprint_source(&source).unwrap());
            if let Some(acceptance_directory) = std::env::var_os("SPYCUT_ACCEPTANCE_OUTPUT_DIR") {
                let acceptance_directory = std::path::PathBuf::from(acceptance_directory);
                std::fs::create_dir_all(&acceptance_directory).unwrap();
                std::fs::copy(
                    &output,
                    acceptance_directory.join(format!("spycut-validated-{suffix}.mp4")),
                )
                .unwrap();
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires the local FFmpeg toolchain"]
    async fn exact_export_preserves_vfr_wall_clock_duration() {
        let ffmpeg = locate_media_tool(MediaTool::Ffmpeg).expect("FFmpeg is required");
        let ffprobe = locate_media_tool(MediaTool::Ffprobe).expect("ffprobe is required");
        let directory = tempdir().unwrap();
        let source = directory.path().join("vfr-wall-clock-source.mp4");
        let output = directory.path().join("vfr-wall-clock-output.mp4");
        let filter = directory.path().join("vfr-wall-clock.filter.txt");
        #[cfg(target_os = "macos")]
        let source_encoder = "h264_videotoolbox";
        #[cfg(target_os = "windows")]
        let source_encoder = "h264_mf";
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let source_encoder = "libx264";

        let mut fixture = Command::new(&ffmpeg);
        fixture.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=30:duration=10",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=10",
            "-filter_complex",
            "[0:v:0]select='lt(t,5)+gte(t,5)*not(mod(n,3))'[vfr]",
            "-map",
            "[vfr]",
            "-map",
            "1:a:0",
            "-c:v",
            source_encoder,
            "-pix_fmt",
            "yuv420p",
            "-fps_mode",
            "vfr",
            "-c:a",
            "aac",
        ]);
        if source_encoder == "libx264" {
            fixture.args(["-preset", "ultrafast"]);
        }
        let generated = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            fixture.arg(&source).output(),
        )
        .await
        .expect("VFR fixture generation timed out")
        .unwrap();
        assert!(
            generated.status.success(),
            "VFR fixture generation failed: {}",
            String::from_utf8_lossy(&generated.stderr)
        );

        let media = probe_media(&ffprobe, &source).await.unwrap();
        assert!(media.variable_frame_rate);
        let plan =
            ExportPlan::build(&media, &[DeleteInterval::new(1, 0, 5_000_000).unwrap()]).unwrap();
        std::fs::write(&filter, build_filter_script(&plan).unwrap()).unwrap();
        let config = ExportJobConfig {
            job_id: "vfr-wall-clock".into(),
            ffmpeg: ffmpeg.clone(),
            ffprobe: ffprobe.clone(),
            source,
            destination: output.clone(),
            partial: output.clone(),
            filter_script: filter,
            media: media.clone(),
            plan: plan.clone(),
            encoder: select_encoder(&ffmpeg, VideoCodec::H264).await.unwrap(),
            recovery_store: None,
        };
        let exported = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            build_command(&config).output(),
        )
        .await
        .expect("VFR export timed out")
        .unwrap();
        assert!(
            exported.status.success(),
            "VFR export failed: {}",
            String::from_utf8_lossy(&exported.stderr)
        );

        let summary = validate_export(&ffmpeg, &ffprobe, &output, &media, &plan)
            .await
            .unwrap();
        assert!(summary.duration_delta_us <= 100_000);
        assert!(
            summary.av_duration_delta_us.unwrap() <= plan.output_frame_rate.frame_duration_us()
        );
    }

    #[tokio::test]
    #[ignore = "requires the local FFmpeg toolchain"]
    async fn exact_export_keeps_the_expected_source_frames_in_order() {
        let ffmpeg = locate_media_tool(MediaTool::Ffmpeg).expect("FFmpeg is required");
        let ffprobe = locate_media_tool(MediaTool::Ffprobe).expect("ffprobe is required");
        let directory = tempdir().unwrap();
        let source = directory.path().join("逐帧编号 source.mp4");
        let output = directory.path().join("逐帧编号 output.mp4");
        let filter = directory.path().join("frame-map.filter.txt");
        #[cfg(target_os = "macos")]
        let source_encoder = "h264_videotoolbox";
        #[cfg(target_os = "windows")]
        let source_encoder = "h264_mf";
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let source_encoder = "libx264";

        let mut fixture = Command::new(&ffmpeg);
        fixture.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=64x64:r=30:d=5,geq=lum='N+30':cb=128:cr=128",
            "-c:v",
            source_encoder,
            "-pix_fmt",
            "yuv420p",
            "-an",
        ]);
        if source_encoder == "libx264" {
            fixture.args(["-preset", "ultrafast"]);
        }
        let generated = fixture.arg(&source).output().await.unwrap();
        assert!(
            generated.status.success(),
            "fixture generation failed: {}",
            String::from_utf8_lossy(&generated.stderr)
        );

        let media = probe_media(&ffprobe, &source).await.unwrap();
        let plan = ExportPlan::build(
            &media,
            &[
                // Delete source frames 31..74 and 105..119. All boundaries
                // are deliberately away from typical GOP keyframes.
                DeleteInterval::new(1, 1_033_333, 2_500_000).unwrap(),
                DeleteInterval::new(2, 3_500_000, 4_000_000).unwrap(),
            ],
        )
        .unwrap();
        std::fs::write(&filter, build_filter_script(&plan).unwrap()).unwrap();
        let config = ExportJobConfig {
            job_id: "frame-map".into(),
            ffmpeg: ffmpeg.clone(),
            ffprobe,
            source: source.clone(),
            destination: output.clone(),
            partial: output.clone(),
            filter_script: filter,
            media,
            plan,
            encoder: select_encoder(&ffmpeg, VideoCodec::H264).await.unwrap(),
            recovery_store: None,
        };
        let exported = build_command(&config).output().await.unwrap();
        assert!(
            exported.status.success(),
            "export failed: {}",
            String::from_utf8_lossy(&exported.stderr)
        );

        let source_luma = decoded_luma_sequence(&ffmpeg, &source).await;
        let output_luma = decoded_luma_sequence(&ffmpeg, &output).await;
        let expected_indices = (0..31).chain(75..105).chain(120..150).collect::<Vec<_>>();
        assert_eq!(output_luma.len(), expected_indices.len());
        for (output_index, source_index) in expected_indices.into_iter().enumerate() {
            let difference =
                i16::from(output_luma[output_index]).abs_diff(i16::from(source_luma[source_index]));
            assert!(
                difference <= 3,
                "output frame {output_index} does not match source frame {source_index}: luma delta {difference}"
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires the local FFmpeg toolchain"]
    async fn export_matrix_accepts_fractional_fps_sixty_fps_and_silent_media() {
        let ffmpeg = locate_media_tool(MediaTool::Ffmpeg).expect("FFmpeg is required");
        let ffprobe = locate_media_tool(MediaTool::Ffprobe).expect("ffprobe is required");
        let directory = tempdir().unwrap();
        let cases = [
            (
                "h264-2997-silent",
                VideoCodec::H264,
                "30000/1001",
                false,
                "yuv420p",
                8,
            ),
            ("hevc-60-audio", VideoCodec::Hevc, "60", true, "yuv420p", 8),
            (
                "hevc-main10-audio",
                VideoCodec::Hevc,
                "30",
                true,
                "p010le",
                10,
            ),
        ];

        for (label, codec, frame_rate, has_audio, pixel_format, expected_bit_depth) in cases {
            let source_encoder = match codec {
                #[cfg(target_os = "macos")]
                VideoCodec::H264 => "h264_videotoolbox",
                #[cfg(target_os = "macos")]
                VideoCodec::Hevc => "hevc_videotoolbox",
                #[cfg(target_os = "windows")]
                VideoCodec::H264 => "h264_mf",
                #[cfg(target_os = "windows")]
                VideoCodec::Hevc => "hevc_mf",
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                VideoCodec::H264 => "libx264",
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                VideoCodec::Hevc => "libx265",
            };
            let source = directory.path().join(format!("{label} 源 'quoted'.mp4"));
            let output = directory.path().join(format!("{label} 输出.mp4"));
            let filter = directory.path().join(format!("{label}.filter.txt"));
            let video_filter = format!("testsrc2=size=320x180:rate={frame_rate}:duration=4");
            let mut fixture = Command::new(&ffmpeg);
            fixture.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                &video_filter,
            ]);
            if has_audio {
                fixture.args([
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=523:sample_rate=48000:duration=4",
                ]);
            }
            fixture.args(["-c:v", source_encoder, "-pix_fmt", pixel_format]);
            if source_encoder.starts_with("libx") {
                fixture.args(["-preset", "ultrafast"]);
            }
            if has_audio {
                fixture.args(["-c:a", "aac", "-shortest"]);
            } else {
                fixture.arg("-an");
            }
            let generated = fixture.arg(&source).output().await.unwrap();
            assert!(
                generated.status.success(),
                "{label} generation failed: {}",
                String::from_utf8_lossy(&generated.stderr)
            );

            let media = probe_media(&ffprobe, &source).await.unwrap();
            assert_eq!(media.video_codec, codec);
            assert_eq!(media.has_audio, has_audio);
            assert_eq!(media.bit_depth, Some(expected_bit_depth));
            let plan = ExportPlan::build(
                &media,
                &[
                    DeleteInterval::new(1, 733_333, 1_466_667).unwrap(),
                    DeleteInterval::new(2, 2_100_000, 2_500_000).unwrap(),
                ],
            )
            .unwrap();
            std::fs::write(&filter, build_filter_script(&plan).unwrap()).unwrap();
            let config = ExportJobConfig {
                job_id: label.into(),
                ffmpeg: ffmpeg.clone(),
                ffprobe: ffprobe.clone(),
                source,
                destination: output.clone(),
                partial: output.clone(),
                filter_script: filter,
                media: media.clone(),
                plan: plan.clone(),
                encoder: select_encoder(&ffmpeg, codec).await.unwrap(),
                recovery_store: None,
            };
            let exported = build_command(&config).output().await.unwrap();
            assert!(
                exported.status.success(),
                "{label} export failed: {}",
                String::from_utf8_lossy(&exported.stderr)
            );
            let summary = validate_export(&ffmpeg, &ffprobe, &output, &media, &plan)
                .await
                .unwrap();
            if expected_bit_depth > 8 {
                assert_eq!(
                    probe_media(&ffprobe, &output).await.unwrap().bit_depth,
                    Some(8)
                );
            }
            if has_audio {
                assert!(
                    summary.av_duration_delta_us.unwrap()
                        <= plan.output_frame_rate.frame_duration_us(),
                    "{label} exceeded one-frame A/V drift"
                );
            } else {
                assert_eq!(summary.av_duration_delta_us, None);
            }
        }
    }

    async fn decoded_luma_sequence(ffmpeg: &std::path::Path, path: &std::path::Path) -> Vec<u8> {
        let decoded = Command::new(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-i"])
            .arg(path)
            .args([
                "-map",
                "0:v:0",
                "-vf",
                "scale=1:1:flags=area,format=gray",
                "-fps_mode",
                "passthrough",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .await
            .unwrap();
        assert!(
            decoded.status.success(),
            "luma decode failed: {}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        decoded.stdout
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancellation_cleans_work_files_and_preserves_existing_destination() {
        let directory = tempdir().unwrap();
        let fake_ffmpeg = directory.path().join("fake ffmpeg 'loop'.sh");
        std::fs::write(&fake_ffmpeg, "#!/bin/sh\nwhile :; do :; done\n").unwrap();
        let mut permissions = std::fs::metadata(&fake_ffmpeg).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&fake_ffmpeg, permissions).unwrap();

        let source = directory.path().join("含中文 source 'quoted'.mp4");
        let destination = directory.path().join("existing output.mp4");
        std::fs::write(&source, b"source").unwrap();
        std::fs::write(&destination, b"existing-target").unwrap();

        let media = MediaInfo {
            duration_us: 10_000_000,
            container: "mp4".into(),
            video_codec: VideoCodec::H264,
            width: 320,
            height: 180,
            frame_rate: crate::domain::media::FrameRate::new(30, 1).unwrap(),
            variable_frame_rate: false,
            video_stream_count: 1,
            audio_stream_count: 0,
            pixel_format: Some("yuv420p".into()),
            bit_depth: Some(8),
            video_bit_rate: Some(2_000_000),
            has_audio: false,
            audio_codec: None,
            audio_sample_rate: None,
            audio_channels: None,
            audio_bit_rate: None,
        };
        let plan = ExportPlan::build(
            &media,
            &[DeleteInterval::new(1, 2_000_000, 3_000_000).unwrap()],
        )
        .unwrap();
        let runtime = ExportRuntime::default();
        let reservation = runtime.reserve().await.unwrap();
        let partial = directory.path().join(format!(
            ".existing output.spycut-{}.partial.mp4",
            reservation.id
        ));
        let filter = directory.path().join(format!(
            ".existing output.spycut-{}.filter.txt",
            reservation.id
        ));
        std::fs::write(&partial, b"partial").unwrap();
        std::fs::write(&filter, b"filter").unwrap();
        let recovery_store = RecoveryStore::new(directory.path().join("app-data")).unwrap();
        recovery_store
            .record(
                &reservation.id,
                partial.clone(),
                filter.clone(),
                destination.clone(),
            )
            .unwrap();
        let config = ExportJobConfig {
            job_id: reservation.id.clone(),
            ffmpeg: fake_ffmpeg,
            ffprobe: directory.path().join("unused-ffprobe"),
            source,
            destination: destination.clone(),
            partial: partial.clone(),
            filter_script: filter.clone(),
            media,
            plan,
            encoder: EncoderSelection {
                name: "unused".into(),
                hardware_accelerated: false,
                display_name: "test".into(),
            },
            recovery_store: Some(recovery_store.clone()),
        };
        let app = tauri::test::mock_app();
        let handle = app.app_handle().clone();
        let runtime_for_job = runtime.clone();
        let job = tokio::spawn(run_export_job(
            handle,
            runtime_for_job,
            reservation.cancel_rx,
            config,
        ));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        runtime.cancel(&reservation.id).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), job)
            .await
            .expect("cancelled FFmpeg job did not stop")
            .unwrap();

        assert_eq!(std::fs::read(&destination).unwrap(), b"existing-target");
        assert!(!partial.exists());
        assert!(!filter.exists());
        assert!(recovery_store.list().unwrap().is_empty());
        assert!(runtime.current().await.is_none());
    }
}
