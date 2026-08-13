use std::{fs, path::Path, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{task::JoinSet, time::timeout};

use crate::domain::{export_plan::ExportPlan, media::MediaInfo, time::Micros};

use super::probe::{ProbeError, probe_media};
use super::tool_locator::media_command;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationSummary {
    pub actual_duration_us: Micros,
    pub expected_duration_us: Micros,
    pub duration_delta_us: Micros,
    pub av_duration_delta_us: Option<Micros>,
    pub start_time_us: Micros,
    pub output_size_bytes: u64,
    pub decoded_checkpoints: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("cannot inspect the exported file: {0}")]
    Probe(#[from] ProbeError),
    #[error("the output codec does not match the source codec")]
    CodecMismatch,
    #[error("the output duration differs from the edit plan by {delta_us} microseconds")]
    DurationMismatch { delta_us: Micros },
    #[error("cannot inspect exported stream structure: {0}")]
    StructureProbe(String),
    #[error("the exported stream layout is invalid: {0}")]
    InvalidStreamLayout(String),
    #[error("the source has audio but the exported file has no measurable audio stream")]
    MissingAudio,
    #[error("the exported file starts at {start_time_us} microseconds instead of near zero")]
    InvalidStartTime { start_time_us: Micros },
    #[error("the exported file is unexpectedly small ({size_bytes} bytes)")]
    OutputTooSmall { size_bytes: u64 },
    #[error("output audio/video duration differs by {delta_us} microseconds")]
    AvDrift { delta_us: Micros },
    #[error("the exported file failed to decode near {0:.3} seconds")]
    DecodeFailed(f64),
}

pub async fn validate_export(
    ffmpeg: &Path,
    ffprobe: &Path,
    output: &Path,
    source_media: &MediaInfo,
    plan: &ExportPlan,
) -> Result<ValidationSummary, ValidationError> {
    let output_media = probe_media(ffprobe, output).await?;
    if output_media.video_codec != source_media.video_codec {
        return Err(ValidationError::CodecMismatch);
    }
    let delta = (output_media.duration_us - plan.kept_duration_us).abs();
    let tolerance = (plan.output_frame_rate.frame_duration_us() * 2).max(75_000);
    if delta > tolerance {
        return Err(ValidationError::DurationMismatch { delta_us: delta });
    }

    let structure = probe_output_structure(
        ffprobe,
        output,
        source_media.has_audio,
        plan.output_frame_rate.frame_duration_us(),
    )
    .await?;
    let av_duration_delta_us = if let Some(drift) = structure.av_duration_delta_us {
        let frame_tolerance = plan.output_frame_rate.frame_duration_us();
        if drift > frame_tolerance {
            return Err(ValidationError::AvDrift { delta_us: drift });
        }
        Some(drift)
    } else {
        None
    };

    let checkpoints = validation_checkpoints(plan);
    decode_checkpoints(ffmpeg, output, &checkpoints).await?;
    Ok(ValidationSummary {
        actual_duration_us: output_media.duration_us,
        expected_duration_us: plan.kept_duration_us,
        duration_delta_us: delta,
        av_duration_delta_us,
        start_time_us: structure.start_time_us,
        output_size_bytes: structure.output_size_bytes,
        decoded_checkpoints: checkpoints.len(),
    })
}

#[derive(Debug, Deserialize)]
struct OutputStructureDocument {
    #[serde(default)]
    streams: Vec<OutputStream>,
    format: Option<OutputFormat>,
}

#[derive(Debug, Deserialize)]
struct OutputStream {
    codec_type: Option<String>,
    duration: Option<String>,
    start_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OutputFormat {
    start_time: Option<String>,
    size: Option<String>,
}

#[derive(Debug)]
struct OutputStructure {
    av_duration_delta_us: Option<Micros>,
    start_time_us: Micros,
    output_size_bytes: u64,
}

async fn probe_output_structure(
    ffprobe: &Path,
    output: &Path,
    expects_audio: bool,
    frame_tolerance_us: Micros,
) -> Result<OutputStructure, ValidationError> {
    let result = timeout(
        Duration::from_secs(30),
        media_command(ffprobe)
            .args([
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_entries",
                "format=start_time,size:stream=codec_type,start_time,duration",
            ])
            .arg(output)
            .output(),
    )
    .await
    .map_err(|_| ValidationError::StructureProbe("ffprobe timed out".into()))?
    .map_err(|error| ValidationError::StructureProbe(error.to_string()))?;
    if !result.status.success() {
        return Err(ValidationError::StructureProbe(
            String::from_utf8_lossy(&result.stderr).trim().to_string(),
        ));
    }
    let document: OutputStructureDocument = serde_json::from_slice(&result.stdout)
        .map_err(|error| ValidationError::StructureProbe(error.to_string()))?;
    let actual_size = fs::metadata(output)
        .map_err(|error| ValidationError::StructureProbe(error.to_string()))?
        .len();
    evaluate_output_structure(document, expects_audio, frame_tolerance_us, actual_size)
}

fn evaluate_output_structure(
    document: OutputStructureDocument,
    expects_audio: bool,
    frame_tolerance_us: Micros,
    actual_size: u64,
) -> Result<OutputStructure, ValidationError> {
    let videos: Vec<_> = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("video"))
        .collect();
    let audios: Vec<_> = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .collect();
    if videos.len() != 1 {
        return Err(ValidationError::InvalidStreamLayout(format!(
            "expected exactly one video stream, found {}",
            videos.len()
        )));
    }
    let expected_audio_count = usize::from(expects_audio);
    if audios.len() != expected_audio_count {
        return Err(ValidationError::InvalidStreamLayout(format!(
            "expected {expected_audio_count} audio stream(s), found {}",
            audios.len()
        )));
    }

    let format = document
        .format
        .ok_or_else(|| ValidationError::StructureProbe("format section is missing".into()))?;
    let reported_size = format
        .size
        .as_deref()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(actual_size);
    let output_size_bytes = actual_size.min(reported_size);
    if output_size_bytes < 1_024 {
        return Err(ValidationError::OutputTooSmall {
            size_bytes: output_size_bytes,
        });
    }

    let start_time_us = parse_seconds_to_micros(format.start_time.as_deref())
        .or_else(|| parse_seconds_to_micros(videos[0].start_time.as_deref()))
        .ok_or_else(|| ValidationError::StructureProbe("output start time is missing".into()))?;
    if start_time_us.abs() > frame_tolerance_us {
        return Err(ValidationError::InvalidStartTime { start_time_us });
    }

    let av_duration_delta_us = if expects_audio {
        let video_duration = parse_positive_seconds_to_micros(videos[0].duration.as_deref())
            .ok_or_else(|| ValidationError::StructureProbe("video duration is missing".into()))?;
        let audio_duration = parse_positive_seconds_to_micros(audios[0].duration.as_deref())
            .ok_or(ValidationError::MissingAudio)?;
        Some((video_duration - audio_duration).abs())
    } else {
        None
    };

    Ok(OutputStructure {
        av_duration_delta_us,
        start_time_us,
        output_size_bytes,
    })
}

fn parse_seconds_to_micros(value: Option<&str>) -> Option<Micros> {
    let seconds = value?.parse::<f64>().ok()?;
    seconds
        .is_finite()
        .then_some((seconds * 1_000_000.0).round() as Micros)
}

fn parse_positive_seconds_to_micros(value: Option<&str>) -> Option<Micros> {
    parse_seconds_to_micros(value).filter(|value| *value > 0)
}

fn validation_checkpoints(plan: &ExportPlan) -> Vec<f64> {
    let mut points = vec![0.0];
    let mut output_cursor = 0_i64;
    for (index, range) in plan.keep_intervals.iter().enumerate() {
        output_cursor += range.duration_us();
        if index + 1 < plan.keep_intervals.len() {
            points.push((output_cursor.saturating_sub(50_000)) as f64 / 1_000_000.0);
            points.push((output_cursor + 50_000) as f64 / 1_000_000.0);
        }
    }
    points.push((plan.kept_duration_us.saturating_sub(100_000)) as f64 / 1_000_000.0);
    points.sort_by(|a, b| a.total_cmp(b));
    points.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    if points.len() <= 66 {
        return points;
    }
    let mut sampled = Vec::with_capacity(66);
    sampled.push(points[0]);
    for index in 1..65 {
        sampled.push(points[index * (points.len() - 1) / 65]);
    }
    sampled.push(*points.last().unwrap());
    sampled
}

async fn decode_checkpoints(
    ffmpeg: &Path,
    output: &Path,
    checkpoints: &[f64],
) -> Result<(), ValidationError> {
    let mut pending = checkpoints.iter().copied();
    let mut running = JoinSet::new();
    loop {
        while running.len() < 4 {
            let Some(seconds) = pending.next() else { break };
            let ffmpeg = ffmpeg.to_path_buf();
            let output = output.to_path_buf();
            running.spawn(async move {
                let result = timeout(
                    Duration::from_secs(30),
                    media_command(ffmpeg)
                        .args(["-hide_banner", "-loglevel", "error", "-ss"])
                        .arg(format!("{seconds:.6}"))
                        .arg("-i")
                        .arg(output)
                        .args(["-frames:v", "2", "-an", "-f", "null", "-"])
                        .output(),
                )
                .await;
                let success = matches!(result, Ok(Ok(output)) if output.status.success());
                (seconds, success)
            });
        }
        let Some(result) = running.join_next().await else {
            break;
        };
        let (seconds, success) = result.unwrap_or((-1.0, false));
        if !success {
            return Err(ValidationError::DecodeFailed(seconds));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        interval::DeleteInterval,
        media::{FrameRate, VideoCodec},
    };

    use super::*;

    #[test]
    fn includes_both_sides_of_each_join() {
        let media = MediaInfo {
            duration_us: 10_000_000,
            container: "mp4".into(),
            video_codec: VideoCodec::H264,
            width: 1280,
            height: 720,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            variable_frame_rate: false,
            video_stream_count: 1,
            audio_stream_count: 0,
            pixel_format: None,
            bit_depth: None,
            video_bit_rate: None,
            has_audio: false,
            audio_codec: None,
            audio_sample_rate: None,
            audio_channels: None,
            audio_bit_rate: None,
        };
        let plan = ExportPlan::build(
            &media,
            &[DeleteInterval::new(1, 2_000_000, 4_000_000).unwrap()],
        )
        .unwrap();
        assert_eq!(validation_checkpoints(&plan), vec![0.0, 1.95, 2.05, 7.9]);
    }

    fn structure_document(json: &str) -> OutputStructureDocument {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn accepts_one_video_one_audio_and_near_zero_start() {
        let output = evaluate_output_structure(
            structure_document(
                r#"{
                  "streams": [
                    {"codec_type":"video","duration":"5.000","start_time":"0.000"},
                    {"codec_type":"audio","duration":"4.990","start_time":"0.000"}
                  ],
                  "format":{"start_time":"0.000","size":"4096"}
                }"#,
            ),
            true,
            33_334,
            4_096,
        )
        .unwrap();
        assert_eq!(output.av_duration_delta_us, Some(10_000));
        assert_eq!(output.start_time_us, 0);
        assert_eq!(output.output_size_bytes, 4_096);
    }

    #[test]
    fn rejects_extra_streams_and_unexpected_audio() {
        let extra_video = evaluate_output_structure(
            structure_document(
                r#"{"streams":[{"codec_type":"video"},{"codec_type":"video"}],"format":{"start_time":"0","size":"4096"}}"#,
            ),
            false,
            33_334,
            4_096,
        )
        .unwrap_err();
        assert!(matches!(
            extra_video,
            ValidationError::InvalidStreamLayout(_)
        ));

        let unexpected_audio = evaluate_output_structure(
            structure_document(
                r#"{"streams":[{"codec_type":"video","start_time":"0"},{"codec_type":"audio"}],"format":{"start_time":"0","size":"4096"}}"#,
            ),
            false,
            33_334,
            4_096,
        )
        .unwrap_err();
        assert!(matches!(
            unexpected_audio,
            ValidationError::InvalidStreamLayout(_)
        ));
    }

    #[test]
    fn rejects_nonzero_start_and_tiny_output() {
        let late = evaluate_output_structure(
            structure_document(
                r#"{"streams":[{"codec_type":"video","start_time":"0.100"}],"format":{"start_time":"0.100","size":"4096"}}"#,
            ),
            false,
            33_334,
            4_096,
        )
        .unwrap_err();
        assert!(matches!(late, ValidationError::InvalidStartTime { .. }));

        let tiny = evaluate_output_structure(
            structure_document(
                r#"{"streams":[{"codec_type":"video","start_time":"0"}],"format":{"start_time":"0","size":"128"}}"#,
            ),
            false,
            33_334,
            128,
        )
        .unwrap_err();
        assert!(matches!(tiny, ValidationError::OutputTooSmall { .. }));
    }
}
