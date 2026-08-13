use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{
    application::jobs::{ActiveExportProjection, ExportRuntime},
    domain::export_plan::ExportPlan,
    infrastructure::{
        encoder::{EncoderSelection, select_encoder},
        ffmpeg_job::{ExportJobConfig, run_export_job},
        filter_script::build_filter_script,
        fingerprint::fingerprint_source,
        recovery::{RecoverableExport, RecoveryStore},
        tool_locator::{MediaTool, locate_media_tool},
    },
};

use super::project::ManagedState;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportStarted {
    pub job_id: String,
    pub encoder: EncoderSelection,
    pub expected_output_us: i64,
    pub destination: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCommandError {
    pub code: String,
    pub message: String,
}

impl ExportCommandError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        Self::new("export_failed", error.to_string())
    }
}

#[tauri::command]
pub async fn start_export(
    destination: String,
    allow_unreviewed: bool,
    allow_bit_depth_fallback: bool,
    app: AppHandle,
    project_state: State<'_, ManagedState>,
    export_runtime: State<'_, ExportRuntime>,
    recovery_store: State<'_, RecoveryStore>,
) -> Result<ExportStarted, ExportCommandError> {
    let workflow_guard = project_state.workflow_gate.lock().await;
    let reservation = export_runtime
        .reserve()
        .await
        .map_err(|error| ExportCommandError::new("export_busy", error.to_string()))?;
    let job_id = reservation.id.clone();

    let prepared = prepare_export(
        &job_id,
        destination,
        allow_unreviewed,
        allow_bit_depth_fallback,
        &project_state,
        &recovery_store,
    )
    .await;
    let (config, started) = match prepared {
        Ok(value) => value,
        Err(error) => {
            export_runtime.release(&job_id).await;
            return Err(error);
        }
    };
    drop(workflow_guard);

    let runtime = export_runtime.inner().clone();
    tauri::async_runtime::spawn(run_export_job(app, runtime, reservation.cancel_rx, config));
    Ok(started)
}

#[tauri::command]
pub async fn cancel_export(
    job_id: String,
    export_runtime: State<'_, ExportRuntime>,
) -> Result<(), ExportCommandError> {
    export_runtime
        .cancel(&job_id)
        .await
        .map_err(|error| ExportCommandError::new("cancel_failed", error.to_string()))
}

#[tauri::command]
pub async fn get_active_export(
    export_runtime: State<'_, ExportRuntime>,
) -> Result<Option<ActiveExportProjection>, ExportCommandError> {
    Ok(export_runtime.current().await)
}

#[tauri::command]
pub async fn list_recoverable_exports(
    recovery_store: State<'_, RecoveryStore>,
    export_runtime: State<'_, ExportRuntime>,
) -> Result<Vec<RecoverableExport>, ExportCommandError> {
    let active_job_id = export_runtime.current().await.map(|active| active.job_id);
    let store = recovery_store.inner().clone();
    let mut records = tokio::task::spawn_blocking(move || store.list())
        .await
        .map_err(ExportCommandError::internal)?
        .map_err(ExportCommandError::internal)?;
    records.retain(|record| Some(&record.job_id) != active_job_id.as_ref());
    Ok(records)
}

#[tauri::command]
pub async fn cleanup_recoverable_export(
    job_id: String,
    recovery_store: State<'_, RecoveryStore>,
    export_runtime: State<'_, ExportRuntime>,
) -> Result<(), ExportCommandError> {
    if export_runtime
        .current()
        .await
        .is_some_and(|active| active.job_id == job_id)
    {
        return Err(ExportCommandError::new(
            "export_in_progress",
            "当前导出仍在运行，不能清理它的临时文件",
        ));
    }
    let store = recovery_store.inner().clone();
    tokio::task::spawn_blocking(move || store.cleanup(&job_id))
        .await
        .map_err(ExportCommandError::internal)?
        .map_err(ExportCommandError::internal)
}

async fn prepare_export(
    job_id: &str,
    destination: String,
    allow_unreviewed: bool,
    allow_bit_depth_fallback: bool,
    state: &State<'_, ManagedState>,
    recovery_store: &RecoveryStore,
) -> Result<(ExportJobConfig, ExportStarted), ExportCommandError> {
    let project = state
        .session
        .read()
        .await
        .as_ref()
        .map(|session| session.project().clone())
        .ok_or_else(|| ExportCommandError::new("no_project", "当前没有打开的项目"))?;
    if project.delete_intervals.is_empty() {
        return Err(ExportCommandError::new(
            "no_cuts",
            "没有删除区间；无需重新编码，请直接使用原视频",
        ));
    }
    ensure_reviewed_or_confirmed(&project, allow_unreviewed)?;
    ensure_bit_depth_fallback_confirmed(&project, allow_bit_depth_fallback)?;

    let destination = validate_destination(&destination, &project.source.canonical_path)?;
    let source = PathBuf::from(&project.source.canonical_path);
    let source_for_check = source.clone();
    let current_identity =
        tokio::task::spawn_blocking(move || fingerprint_source(&source_for_check))
            .await
            .map_err(ExportCommandError::internal)?
            .map_err(|error| ExportCommandError::new("source_unavailable", error.to_string()))?;
    if current_identity != project.source {
        return Err(ExportCommandError::new(
            "source_changed",
            "源视频自打开后发生了变化。请重新打开文件并检查删除区间。",
        ));
    }

    let plan = ExportPlan::build(&project.media, &project.delete_intervals)
        .map_err(|error| ExportCommandError::new("invalid_plan", error.to_string()))?;
    ensure_disk_space(&destination, project.source.size_bytes, &plan)?;

    let ffmpeg = locate_media_tool(MediaTool::Ffmpeg)
        .map_err(|error| ExportCommandError::new("ffmpeg_missing", error.to_string()))?;
    let ffprobe = locate_media_tool(MediaTool::Ffprobe)
        .map_err(|error| ExportCommandError::new("ffprobe_missing", error.to_string()))?;
    let encoder = select_encoder(&ffmpeg, plan.output_codec)
        .await
        .map_err(|error| ExportCommandError::new("encoder_unavailable", error.to_string()))?;

    let (partial, filter_script) = working_paths(&destination, job_id)?;
    let filter = build_filter_script(&plan).map_err(ExportCommandError::internal)?;
    tokio::fs::write(&filter_script, filter)
        .await
        .map_err(|error| ExportCommandError::new("temporary_file_failed", error.to_string()))?;

    let recovery = recovery_store.clone();
    let recovery_job_id = job_id.to_string();
    let recovery_partial = partial.clone();
    let recovery_filter = filter_script.clone();
    let recovery_destination = destination.clone();
    let recovery_result = match tokio::task::spawn_blocking(move || {
        recovery.record(
            &recovery_job_id,
            recovery_partial,
            recovery_filter,
            recovery_destination,
        )
    })
    .await
    {
        Ok(result) => result
            .map_err(|error| ExportCommandError::new("recovery_record_failed", error.to_string())),
        Err(error) => Err(ExportCommandError::internal(error)),
    };
    if let Err(error) = recovery_result {
        let _ = tokio::fs::remove_file(&filter_script).await;
        return Err(error);
    }

    let config = ExportJobConfig {
        job_id: job_id.into(),
        ffmpeg,
        ffprobe,
        source,
        destination: destination.clone(),
        partial,
        filter_script,
        media: project.media,
        plan: plan.clone(),
        encoder: encoder.clone(),
        recovery_store: Some(recovery_store.clone()),
    };
    let started = ExportStarted {
        job_id: job_id.into(),
        encoder,
        expected_output_us: plan.kept_duration_us,
        destination: destination.to_string_lossy().into_owned(),
    };
    Ok((config, started))
}

fn ensure_reviewed_or_confirmed(
    project: &crate::domain::project::ProjectV1,
    allow_unreviewed: bool,
) -> Result<(), ExportCommandError> {
    let unreviewed = project
        .delete_intervals
        .iter()
        .filter(|interval| !project.reviewed_interval_ids.contains(&interval.id))
        .count();
    if unreviewed > 0 && !allow_unreviewed {
        return Err(ExportCommandError::new(
            "unreviewed_joins",
            format!("还有 {unreviewed} 个连接点未复核；请逐处确认，或在复核页再次明确选择仍然导出"),
        ));
    }
    Ok(())
}

fn ensure_bit_depth_fallback_confirmed(
    project: &crate::domain::project::ProjectV1,
    allow_bit_depth_fallback: bool,
) -> Result<(), ExportCommandError> {
    if project.media.bit_depth.is_some_and(|depth| depth > 8) && !allow_bit_depth_fallback {
        return Err(ExportCommandError::new(
            "bit_depth_fallback_unconfirmed",
            "源视频是 10-bit；SpyCut V1 将精确重建为兼容性更广的 8-bit。请在复核页再次明确确认",
        ));
    }
    Ok(())
}

fn validate_destination(value: &str, source: &str) -> Result<PathBuf, ExportCommandError> {
    let requested = PathBuf::from(value);
    if requested
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        != Some("mp4".into())
    {
        return Err(ExportCommandError::new(
            "invalid_destination",
            "导出文件必须使用 .mp4 扩展名",
        ));
    }
    let parent = requested
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            ExportCommandError::new("invalid_destination", "请选择一个有效的导出文件夹")
        })?;
    if !parent.is_dir() {
        return Err(ExportCommandError::new(
            "invalid_destination",
            "导出文件夹不存在",
        ));
    }
    let file_name = requested
        .file_name()
        .ok_or_else(|| ExportCommandError::new("invalid_destination", "导出路径缺少文件名"))?;
    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|error| ExportCommandError::new("invalid_destination", error.to_string()))?;
    let path = canonical_parent.join(file_name);
    if path.is_dir() {
        return Err(ExportCommandError::new(
            "invalid_destination",
            "导出目标不能是文件夹",
        ));
    }
    if Path::new(source) == path {
        return Err(ExportCommandError::new(
            "source_overwrite",
            "不能覆盖源视频，请选择另一个文件名",
        ));
    }
    if path.exists() && std::fs::canonicalize(&path).ok() == std::fs::canonicalize(source).ok() {
        return Err(ExportCommandError::new(
            "source_overwrite",
            "不能覆盖源视频，请选择另一个文件名",
        ));
    }
    Ok(path)
}

fn ensure_disk_space(
    destination: &Path,
    source_size: u64,
    plan: &ExportPlan,
) -> Result<(), ExportCommandError> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let available = fs2::available_space(parent).map_err(ExportCommandError::internal)?;
    ensure_available_space(available, source_size, plan)
}

fn ensure_available_space(
    available: u64,
    source_size: u64,
    plan: &ExportPlan,
) -> Result<(), ExportCommandError> {
    let ratio = plan.kept_duration_us as f64 / plan.source_duration_us as f64;
    let estimate = (source_size as f64 * ratio * 1.30) as u64 + 512 * 1024 * 1024;
    if available < estimate {
        return Err(ExportCommandError::new(
            "insufficient_space",
            format!(
                "磁盘空间不足：预计需要约 {:.1} GB，当前可用 {:.1} GB",
                estimate as f64 / 1_073_741_824.0,
                available as f64 / 1_073_741_824.0
            ),
        ));
    }
    Ok(())
}

fn working_paths(
    destination: &Path,
    job_id: &str,
) -> Result<(PathBuf, PathBuf), ExportCommandError> {
    let parent = destination
        .parent()
        .ok_or_else(|| ExportCommandError::new("invalid_destination", "导出路径缺少父文件夹"))?;
    let stem = destination
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("spycut-export");
    Ok((
        parent.join(format!(".{stem}.spycut-{job_id}.partial.mp4")),
        parent.join(format!(".{stem}.spycut-{job_id}.filter.txt")),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        interval::DeleteInterval,
        media::{FrameRate, MediaInfo, VideoCodec},
        project::{ProjectV1, SourceIdentity},
    };

    fn project_with_unreviewed_join() -> ProjectV1 {
        let mut project = ProjectV1::new(
            SourceIdentity {
                canonical_path: "/tmp/source.mp4".into(),
                size_bytes: 100,
                modified_unix_ms: 1,
                edge_hash_blake3: "edge".into(),
            },
            MediaInfo {
                duration_us: 10_000_000,
                container: "mp4".into(),
                video_codec: VideoCodec::H264,
                width: 1920,
                height: 1080,
                frame_rate: FrameRate::new(30, 1).unwrap(),
                variable_frame_rate: false,
                video_stream_count: 1,
                audio_stream_count: 0,
                pixel_format: Some("yuv420p".into()),
                bit_depth: Some(8),
                video_bit_rate: Some(8_000_000),
                has_audio: false,
                audio_codec: None,
                audio_sample_rate: None,
                audio_channels: None,
                audio_bit_rate: None,
            },
        );
        project.delete_intervals = vec![DeleteInterval::new(1, 1_000_000, 2_000_000).unwrap()];
        project
    }

    #[test]
    fn creates_hidden_same_volume_working_paths() {
        let destination = Path::new("/tmp/lesson.mp4");
        let (partial, filter) = working_paths(destination, "abc").unwrap();
        assert_eq!(partial, Path::new("/tmp/.lesson.spycut-abc.partial.mp4"));
        assert_eq!(filter, Path::new("/tmp/.lesson.spycut-abc.filter.txt"));
    }

    #[test]
    fn rejects_non_mp4_destinations() {
        let error = validate_destination("/tmp/lesson.mov", "/tmp/source.mp4").unwrap_err();
        assert_eq!(error.code, "invalid_destination");
    }

    #[test]
    fn canonicalizes_unicode_and_quoted_destination_paths() {
        let directory = tempfile::tempdir().unwrap();
        let requested = directory.path().join("课程 '公开版'.mp4");
        let validated = validate_destination(
            requested.to_str().unwrap(),
            "/definitely/not/the/source.mp4",
        )
        .unwrap();
        assert!(validated.is_absolute());
        assert_eq!(validated.file_name().unwrap(), "课程 '公开版'.mp4");
        let (partial, filter) = working_paths(&validated, "abc").unwrap();
        assert_eq!(
            partial.file_name().unwrap(),
            ".课程 '公开版'.spycut-abc.partial.mp4"
        );
        assert_eq!(
            filter.file_name().unwrap(),
            ".课程 '公开版'.spycut-abc.filter.txt"
        );
    }

    #[test]
    fn disk_preflight_rejects_below_required_headroom() {
        let project = project_with_unreviewed_join();
        let plan = ExportPlan::build(&project.media, &project.delete_intervals).unwrap();
        let error = ensure_available_space(1, 10 * 1024 * 1024 * 1024, &plan).unwrap_err();
        assert_eq!(error.code, "insufficient_space");
    }

    #[test]
    fn unreviewed_join_requires_explicit_override() {
        let project = project_with_unreviewed_join();
        let error = ensure_reviewed_or_confirmed(&project, false).unwrap_err();
        assert_eq!(error.code, "unreviewed_joins");
        assert!(ensure_reviewed_or_confirmed(&project, true).is_ok());
    }

    #[test]
    fn fully_reviewed_project_needs_no_override() {
        let mut project = project_with_unreviewed_join();
        project.reviewed_interval_ids = vec![1];
        assert!(ensure_reviewed_or_confirmed(&project, false).is_ok());
    }

    #[test]
    fn ten_bit_source_requires_explicit_fallback_confirmation() {
        let mut project = project_with_unreviewed_join();
        project.media.bit_depth = Some(10);
        let error = ensure_bit_depth_fallback_confirmed(&project, false).unwrap_err();
        assert_eq!(error.code, "bit_depth_fallback_unconfirmed");
        assert!(ensure_bit_depth_fallback_confirmed(&project, true).is_ok());
    }
}
