use serde::Serialize;
use tauri::State;

use crate::infrastructure::diagnostics::{DiagnosticLevel, DiagnosticLog};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticStatus {
    pub available: bool,
    pub log_path: String,
    pub previous_session_unclean: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticCommandError {
    pub code: String,
    pub message: String,
}

#[tauri::command]
pub fn get_diagnostic_status(log: State<'_, DiagnosticLog>) -> DiagnosticStatus {
    DiagnosticStatus {
        available: log.is_available(),
        log_path: log.log_path().to_string_lossy().into_owned(),
        previous_session_unclean: log.previous_session_unclean(),
    }
}

#[tauri::command]
pub fn record_frontend_diagnostic(
    kind: String,
    message: String,
    log: State<'_, DiagnosticLog>,
) -> Result<(), DiagnosticCommandError> {
    if !matches!(
        kind.as_str(),
        "frontend_ready" | "frontend_error" | "unhandled_rejection" | "player_error"
    ) {
        return Err(DiagnosticCommandError {
            code: "invalid_diagnostic_kind".into(),
            message: "不支持的诊断事件类型".into(),
        });
    }
    let level = if kind == "frontend_ready" {
        DiagnosticLevel::Info
    } else {
        DiagnosticLevel::Error
    };
    log.record(level, &kind, &message);
    Ok(())
}
