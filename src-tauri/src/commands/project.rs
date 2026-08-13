use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::State;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;

use crate::{
    application::{
        jobs::ExportRuntime,
        session::{ProjectSession, SessionProjection},
    },
    domain::{interval::IntervalError, project::ProjectV1, time::Micros},
    infrastructure::{
        audio_waveform::{AudioWaveform, extract_audio_waveform},
        diagnostics::{DiagnosticLevel, DiagnosticLog},
        fingerprint::fingerprint_source,
        preview_server::PreviewServer,
        probe::probe_media,
        project_store::{ProjectStore, ProjectStoreError},
        tool_locator::{MediaTool, locate_media_tool, media_command},
    },
};

#[derive(Clone)]
pub struct ManagedState {
    pub session: Arc<RwLock<Option<ProjectSession>>>,
    pub workflow_gate: Arc<Mutex<()>>,
    pub store: ProjectStore,
}

impl ManagedState {
    pub fn new(store: ProjectStore) -> Self {
        Self {
            session: Arc::new(RwLock::new(None)),
            workflow_gate: Arc::new(Mutex::new(())),
            store,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSourceResult {
    pub session: SessionProjection,
    pub resumed: bool,
    pub preview_url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackDiagnostic {
    pub ffmpeg_can_decode: bool,
    pub details: String,
}

#[tauri::command]
pub async fn get_audio_waveform(
    project_id: String,
    state: State<'_, ManagedState>,
    diagnostics: State<'_, DiagnosticLog>,
) -> Result<AudioWaveform, CommandError> {
    let (source, duration_us, has_audio) = {
        let guard = state.session.read().await;
        let session = guard
            .as_ref()
            .ok_or_else(|| CommandError::new("no_project", "当前没有打开的项目"))?;
        if session.project().project_id != project_id {
            return Err(CommandError::new(
                "stale_project",
                "已忽略上一个项目迟到的音频波形请求",
            ));
        }
        (
            PathBuf::from(&session.project().source.canonical_path),
            session.project().media.duration_us,
            session.project().media.has_audio,
        )
    };

    if !has_audio {
        diagnostics.record(DiagnosticLevel::Info, "waveform_skipped", "reason=no_audio");
        return Ok(AudioWaveform::empty());
    }
    let started = Instant::now();
    diagnostics.record(
        DiagnosticLevel::Info,
        "waveform_started",
        &format!("duration_us={duration_us}"),
    );
    let ffmpeg = locate_media_tool(MediaTool::Ffmpeg).map_err(CommandError::internal)?;
    let waveform = match extract_audio_waveform(&ffmpeg, &source, duration_us).await {
        Ok(waveform) => waveform,
        Err(error) => {
            diagnostics.record(
                DiagnosticLevel::Error,
                "waveform_failed",
                &format!("elapsed_ms={} error={error}", started.elapsed().as_millis()),
            );
            return Err(CommandError::new(
                "waveform_failed",
                format!("无法生成音频波形：{error}"),
            ));
        }
    };

    let guard = state.session.read().await;
    if guard
        .as_ref()
        .is_none_or(|session| session.project().project_id != project_id)
    {
        return Err(CommandError::new(
            "stale_project",
            "已忽略上一个项目迟到的音频波形结果",
        ));
    }
    diagnostics.record(
        DiagnosticLevel::Info,
        "waveform_completed",
        &format!(
            "elapsed_ms={} peaks={}",
            started.elapsed().as_millis(),
            waveform.peaks.len()
        ),
    );
    Ok(waveform)
}

impl CommandError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        Self::new("internal", error.to_string())
    }
}

#[tauri::command]
pub async fn open_source(
    path: String,
    state: State<'_, ManagedState>,
    export_runtime: State<'_, ExportRuntime>,
    preview_server: State<'_, PreviewServer>,
    diagnostics: State<'_, DiagnosticLog>,
) -> Result<OpenSourceResult, CommandError> {
    let started = Instant::now();
    diagnostics.record(
        DiagnosticLevel::Info,
        "source_open_started",
        "stage=workflow_gate",
    );
    let _workflow_guard = state.workflow_gate.lock().await;
    ensure_editing_available(&export_runtime)
        .await
        .inspect_err(|error| {
            record_source_open_failure(&diagnostics, started, "workflow_gate", &error.message);
        })?;
    let source_path = PathBuf::from(path);
    let ffprobe = locate_media_tool(MediaTool::Ffprobe).map_err(|error| {
        record_source_open_failure(&diagnostics, started, "ffprobe_lookup", &error.to_string());
        CommandError::internal(error)
    })?;
    let media = probe_media(&ffprobe, &source_path).await.map_err(|error| {
        record_source_open_failure(&diagnostics, started, "media_probe", &error.to_string());
        CommandError::new("probe_failed", error.to_string())
    })?;
    diagnostics.record(
        DiagnosticLevel::Info,
        "source_probe_completed",
        &format!(
            "duration_us={} codec={:?} has_audio={}",
            media.duration_us, media.video_codec, media.has_audio
        ),
    );
    let fingerprint_path = source_path.clone();
    let source = tokio::task::spawn_blocking(move || fingerprint_source(&fingerprint_path))
        .await
        .map_err(|error| {
            record_source_open_failure(
                &diagnostics,
                started,
                "fingerprint_task",
                &error.to_string(),
            );
            CommandError::internal(error)
        })?
        .map_err(|error| {
            record_source_open_failure(
                &diagnostics,
                started,
                "source_fingerprint",
                &error.to_string(),
            );
            CommandError::new("fingerprint_failed", error.to_string())
        })?;
    diagnostics.record(
        DiagnosticLevel::Info,
        "source_fingerprint_completed",
        "stage=edge_hash_ready",
    );

    let store = state.store.clone();
    let source_for_lookup = source.clone();
    let existing =
        tokio::task::spawn_blocking(move || store.find_matching_source(&source_for_lookup))
            .await
            .map_err(|error| {
                record_source_open_failure(
                    &diagnostics,
                    started,
                    "project_lookup_task",
                    &error.to_string(),
                );
                CommandError::internal(error)
            })?
            .map_err(|error| {
                let command_error = project_load_error(error);
                record_source_open_failure(
                    &diagnostics,
                    started,
                    "project_lookup",
                    &command_error.message,
                );
                command_error
            })?;

    let resumed = existing.is_some();
    diagnostics.record(
        DiagnosticLevel::Info,
        "source_project_lookup_completed",
        &format!("resumed={resumed}"),
    );
    let mut project = existing.unwrap_or_else(|| ProjectV1::new(source.clone(), media.clone()));
    project.source = source;
    project.media = media;
    project.touch();

    persist_project(state.store.clone(), project.clone())
        .await
        .inspect_err(|error| {
            record_source_open_failure(&diagnostics, started, "project_persist", &error.message);
        })?;

    let canonical_source = PathBuf::from(&project.source.canonical_path);
    let preview_url = preview_server
        .publish_source(&canonical_source)
        .map_err(|error| {
            record_source_open_failure(
                &diagnostics,
                started,
                "preview_publish",
                &error.to_string(),
            );
            CommandError::internal(error)
        })?;

    let session = ProjectSession::new(project);
    let projection = session.projection().map_err(|error| {
        record_source_open_failure(
            &diagnostics,
            started,
            "session_projection",
            &error.to_string(),
        );
        CommandError::internal(error)
    })?;
    *state.session.write().await = Some(session);

    diagnostics.record(
        DiagnosticLevel::Info,
        "source_open_completed",
        &format!("elapsed_ms={}", started.elapsed().as_millis()),
    );

    Ok(OpenSourceResult {
        session: projection,
        resumed,
        preview_url,
    })
}

fn record_source_open_failure(
    diagnostics: &DiagnosticLog,
    started: Instant,
    stage: &str,
    error: &str,
) {
    diagnostics.record(
        DiagnosticLevel::Error,
        "source_open_failed",
        &format!(
            "stage={stage} elapsed_ms={} error={error}",
            started.elapsed().as_millis()
        ),
    );
}

#[tauri::command]
pub async fn get_session(
    state: State<'_, ManagedState>,
    preview_server: State<'_, PreviewServer>,
    diagnostics: State<'_, DiagnosticLog>,
) -> Result<Option<OpenSourceResult>, CommandError> {
    if let Some((projection, source_path)) = state
        .session
        .read()
        .await
        .as_ref()
        .map(|session| {
            Ok::<_, IntervalError>((
                session.projection()?,
                PathBuf::from(&session.project().source.canonical_path),
            ))
        })
        .transpose()
        .map_err(CommandError::internal)?
    {
        let preview_url = preview_server
            .publish_source(&source_path)
            .map_err(|error| {
                diagnostics.record(
                    DiagnosticLevel::Error,
                    "session_query_failed",
                    &format!("stage=preview_publish error={error}"),
                );
                CommandError::internal(error)
            })?;
        diagnostics.record(
            DiagnosticLevel::Info,
            "session_query_completed",
            "source=memory",
        );
        return Ok(Some(OpenSourceResult {
            session: projection,
            resumed: true,
            preview_url,
        }));
    }
    diagnostics.record(
        DiagnosticLevel::Info,
        "session_query_completed",
        "source=empty",
    );
    Ok(None)
}

#[tauri::command]
pub fn get_launch_source() -> Option<String> {
    std::env::var("SPYCUT_E2E_SOURCE")
        .ok()
        .filter(|path| PathBuf::from(path).is_file())
        .or_else(|| {
            std::env::args().skip(1).find(|argument| {
                PathBuf::from(argument).is_file()
                    && PathBuf::from(argument)
                        .extension()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| value.eq_ignore_ascii_case("mp4"))
            })
        })
}

#[tauri::command]
pub async fn diagnose_playback(
    state: State<'_, ManagedState>,
) -> Result<PlaybackDiagnostic, CommandError> {
    let source = state
        .session
        .read()
        .await
        .as_ref()
        .map(|session| session.project().source.canonical_path.clone())
        .ok_or_else(|| CommandError::new("no_project", "当前没有打开的项目"))?;
    let ffmpeg = locate_media_tool(MediaTool::Ffmpeg).map_err(CommandError::internal)?;
    let output = timeout(
        Duration::from_secs(30),
        media_command(ffmpeg)
            .args([
                "-hide_banner",
                "-nostdin",
                "-loglevel",
                "error",
                "-ss",
                "0",
                "-i",
            ])
            .arg(source)
            .args(["-t", "1", "-map", "0:v:0", "-an", "-f", "null", "-"])
            .output(),
    )
    .await
    .map_err(|_| CommandError::new("decode_timeout", "FFmpeg 解码诊断超时"))?
    .map_err(CommandError::internal)?;
    let details = String::from_utf8_lossy(&output.stderr)
        .chars()
        .take(2_000)
        .collect();
    Ok(PlaybackDiagnostic {
        ffmpeg_can_decode: output.status.success(),
        details,
    })
}

#[tauri::command]
pub async fn add_delete_interval(
    start_us: Micros,
    end_us: Micros,
    project_id: String,
    state: State<'_, ManagedState>,
    export_runtime: State<'_, ExportRuntime>,
) -> Result<SessionProjection, CommandError> {
    mutate_and_save(
        &state,
        Some(&project_id),
        Some(&export_runtime),
        |session| session.add_delete_interval(start_us, end_us),
    )
    .await
}

#[tauri::command]
pub async fn resize_delete_interval(
    id: u64,
    start_us: Micros,
    end_us: Micros,
    project_id: String,
    state: State<'_, ManagedState>,
    export_runtime: State<'_, ExportRuntime>,
) -> Result<SessionProjection, CommandError> {
    mutate_and_save(
        &state,
        Some(&project_id),
        Some(&export_runtime),
        |session| session.resize_delete_interval(id, start_us, end_us),
    )
    .await
}

#[tauri::command]
pub async fn remove_delete_interval(
    id: u64,
    project_id: String,
    state: State<'_, ManagedState>,
    export_runtime: State<'_, ExportRuntime>,
) -> Result<SessionProjection, CommandError> {
    mutate_and_save(
        &state,
        Some(&project_id),
        Some(&export_runtime),
        |session| session.remove_delete_interval(id),
    )
    .await
}

#[tauri::command]
pub async fn set_playhead(
    playhead_us: Micros,
    project_id: String,
    state: State<'_, ManagedState>,
) -> Result<SessionProjection, CommandError> {
    mutate_and_save(&state, Some(&project_id), None, |session| {
        session.set_playhead(playhead_us);
        Ok::<(), IntervalError>(())
    })
    .await
}

#[tauri::command]
pub async fn set_join_reviewed(
    id: u64,
    reviewed: bool,
    project_id: String,
    state: State<'_, ManagedState>,
) -> Result<SessionProjection, CommandError> {
    mutate_and_save(&state, Some(&project_id), None, |session| {
        session.set_reviewed(id, reviewed)
    })
    .await
}

#[tauri::command]
pub async fn undo(
    project_id: String,
    state: State<'_, ManagedState>,
    export_runtime: State<'_, ExportRuntime>,
) -> Result<SessionProjection, CommandError> {
    mutate_and_save(
        &state,
        Some(&project_id),
        Some(&export_runtime),
        |session| {
            session.undo();
            Ok::<(), IntervalError>(())
        },
    )
    .await
}

#[tauri::command]
pub async fn redo(
    project_id: String,
    state: State<'_, ManagedState>,
    export_runtime: State<'_, ExportRuntime>,
) -> Result<SessionProjection, CommandError> {
    mutate_and_save(
        &state,
        Some(&project_id),
        Some(&export_runtime),
        |session| {
            session.redo();
            Ok::<(), IntervalError>(())
        },
    )
    .await
}

async fn ensure_editing_available(runtime: &ExportRuntime) -> Result<(), CommandError> {
    if runtime.is_active().await {
        return Err(CommandError::new(
            "export_in_progress",
            "导出期间不能更换视频或修改删除区间，请先等待完成或取消导出",
        ));
    }
    Ok(())
}

async fn mutate_and_save<F>(
    state: &ManagedState,
    expected_project_id: Option<&str>,
    export_runtime: Option<&ExportRuntime>,
    mutation: F,
) -> Result<SessionProjection, CommandError>
where
    F: FnOnce(&mut ProjectSession) -> Result<(), IntervalError>,
{
    let _workflow_guard = state.workflow_gate.lock().await;
    if let Some(export_runtime) = export_runtime {
        ensure_editing_available(export_runtime).await?;
    }
    // Keep the write lock until the atomic project save finishes. Otherwise two
    // commands can persist out of order and an older snapshot can overwrite the
    // user's newest edit. Restore the complete session (including undo history)
    // when saving fails so memory never claims an unsaved edit succeeded.
    let mut guard = state.session.write().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| CommandError::new("no_project", "当前没有打开的项目"))?;
    if expected_project_id.is_some_and(|expected| expected != session.project().project_id) {
        return Err(CommandError::new(
            "stale_project",
            "已忽略上一个项目迟到的保存请求",
        ));
    }
    let previous = session.clone();
    mutation(session).map_err(|error| CommandError::new("invalid_edit", error.to_string()))?;
    let projection = session.projection().map_err(CommandError::internal)?;
    let project = session.project().clone();
    if let Err(error) = persist_project(state.store.clone(), project).await {
        *session = previous;
        return Err(error);
    }
    Ok(projection)
}

async fn persist_project(store: ProjectStore, project: ProjectV1) -> Result<(), CommandError> {
    tokio::task::spawn_blocking(move || store.save(&project))
        .await
        .map_err(CommandError::internal)?
        .map_err(|error| {
            CommandError::new(
                "save_failed",
                format!("无法把项目设置保存到视频同目录的 JSON 文件：{error}"),
            )
        })
}

fn project_load_error(error: ProjectStoreError) -> CommandError {
    match error {
        ProjectStoreError::SidecarSourceMismatch => CommandError::new(
            "project_source_mismatch",
            "视频旁的 SpyCut 项目文件与当前视频不匹配，已停止读取且不会覆盖该 JSON 文件",
        ),
        ProjectStoreError::UnsupportedSchema(version) => CommandError::new(
            "project_schema_unsupported",
            format!("SpyCut 项目文件版本 {version} 过新，当前版本无法安全读取"),
        ),
        error => CommandError::new(
            "project_load_failed",
            format!("无法读取 SpyCut 项目文件：{error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::tempdir;

    use crate::domain::{
        media::{FrameRate, MediaInfo, VideoCodec},
        project::SourceIdentity,
    };

    use super::*;

    fn sample_project(source_path: &Path) -> ProjectV1 {
        ProjectV1::new(
            SourceIdentity {
                canonical_path: source_path.to_string_lossy().into_owned(),
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
                audio_stream_count: 1,
                pixel_format: Some("yuv420p".into()),
                bit_depth: Some(8),
                video_bit_rate: Some(8_000_000),
                has_audio: true,
                audio_codec: Some("aac".into()),
                audio_sample_rate: Some(48_000),
                audio_channels: Some(2),
                audio_bit_rate: Some(160_000),
            },
        )
    }

    #[tokio::test]
    async fn failed_save_rolls_back_project_and_history() {
        let directory = tempdir().unwrap();
        let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
        let source_path = directory.path().join("source.mp4");
        let project = sample_project(&source_path);

        // A directory at the target JSON path makes the atomic rename fail on
        // every supported desktop platform without relying on Unix permissions.
        std::fs::create_dir(ProjectStore::sidecar_path(&source_path).unwrap()).unwrap();
        let state = ManagedState::new(store);
        *state.session.write().await = Some(ProjectSession::new(project));

        let error = mutate_and_save(&state, None, None, |session| {
            session.add_delete_interval(1_000_000, 2_000_000)
        })
        .await
        .unwrap_err();
        assert_eq!(error.code, "save_failed");

        let projection = state
            .session
            .read()
            .await
            .as_ref()
            .unwrap()
            .projection()
            .unwrap();
        assert!(projection.project.delete_intervals.is_empty());
        assert!(!projection.can_undo);
    }

    #[tokio::test]
    async fn stale_project_request_cannot_modify_the_active_project() {
        let directory = tempdir().unwrap();
        let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
        let project = sample_project(&directory.path().join("source.mp4"));
        let current_id = project.project_id.clone();
        let state = ManagedState::new(store);
        *state.session.write().await = Some(ProjectSession::new(project));

        let error = mutate_and_save(&state, Some("previous-project"), None, |session| {
            session.set_playhead(5_000_000);
            Ok(())
        })
        .await
        .unwrap_err();
        assert_eq!(error.code, "stale_project");

        let guard = state.session.read().await;
        let project = guard.as_ref().unwrap().project();
        assert_eq!(project.project_id, current_id);
        assert_eq!(project.last_playhead_us, 0);
    }

    #[tokio::test]
    async fn edit_waiting_on_workflow_gate_cannot_slip_into_export_snapshot() {
        let directory = tempdir().unwrap();
        let store = ProjectStore::new(directory.path().join("app-data")).unwrap();
        let state = ManagedState::new(store);
        *state.session.write().await = Some(ProjectSession::new(sample_project(
            &directory.path().join("source.mp4"),
        )));
        let runtime = ExportRuntime::default();

        let gate = state.workflow_gate.lock().await;
        let state_for_edit = state.clone();
        let runtime_for_edit = runtime.clone();
        let edit = tokio::spawn(async move {
            mutate_and_save(&state_for_edit, None, Some(&runtime_for_edit), |session| {
                session.add_delete_interval(1_000_000, 2_000_000)
            })
            .await
        });
        tokio::task::yield_now().await;
        let reservation = runtime.reserve().await.unwrap();
        drop(gate);

        let error = edit.await.unwrap().unwrap_err();
        assert_eq!(error.code, "export_in_progress");
        assert!(
            state
                .session
                .read()
                .await
                .as_ref()
                .unwrap()
                .project()
                .delete_intervals
                .is_empty()
        );
        runtime.release(&reservation.id).await;
    }
}
