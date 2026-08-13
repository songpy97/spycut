#![recursion_limit = "256"]

pub mod application;
pub mod commands;
pub mod domain;
pub mod infrastructure;

pub fn run() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use application::jobs::ExportRuntime;
    use commands::diagnostics::{get_diagnostic_status, record_frontend_diagnostic};
    use commands::export::{
        cancel_export, cleanup_recoverable_export, get_active_export, list_recoverable_exports,
        start_export,
    };
    use commands::project::{
        ManagedState, add_delete_interval, diagnose_playback, get_audio_waveform,
        get_launch_source, get_session, open_source, redo, remove_delete_interval,
        resize_delete_interval, set_join_reviewed, set_playhead, undo,
    };
    use tauri::Manager;

    let _ = tracing_subscriber::fmt()
        .with_env_filter("spycut=info,warn")
        .with_target(false)
        .try_init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let diagnostics = infrastructure::diagnostics::DiagnosticLog::open(&app_data_dir)
                .unwrap_or_else(|_| {
                    infrastructure::diagnostics::DiagnosticLog::disabled(&app_data_dir)
                });
            diagnostics.install_panic_hook();
            diagnostics.record(
                infrastructure::diagnostics::DiagnosticLevel::Info,
                "app_started",
                &format!(
                    "version={} os={} arch={} previous_session_unclean={}",
                    app.package_info().version,
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                    diagnostics.previous_session_unclean()
                ),
            );
            if diagnostics.previous_session_unclean() {
                diagnostics.record(
                    infrastructure::diagnostics::DiagnosticLevel::Warn,
                    "previous_session_unclean",
                    "previous process did not complete the normal exit sequence",
                );
            }
            let store = infrastructure::project_store::ProjectStore::new(&app_data_dir)?;
            let recovery_store = infrastructure::recovery::RecoveryStore::new(app_data_dir)?;
            let preview_server = infrastructure::preview_server::PreviewServer::start()?;
            app.manage(diagnostics);
            app.manage(ManagedState::new(store));
            app.manage(ExportRuntime::default());
            app.manage(recovery_store);
            app.manage(preview_server);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_source,
            get_session,
            get_launch_source,
            diagnose_playback,
            get_audio_waveform,
            add_delete_interval,
            resize_delete_interval,
            remove_delete_interval,
            set_playhead,
            set_join_reviewed,
            undo,
            redo,
            start_export,
            cancel_export,
            get_active_export,
            list_recoverable_exports,
            cleanup_recoverable_export,
            get_diagnostic_status,
            record_frontend_diagnostic
        ])
        .build(tauri::generate_context!())
        .expect("failed to build SpyCut");

    let exit_in_progress = Arc::new(AtomicBool::new(false));
    app.run(move |app_handle, event| {
        let tauri::RunEvent::ExitRequested { code, api, .. } = event else {
            return;
        };
        // A programmatic exit after cleanup is the second request and must be
        // allowed through. User-initiated exits are briefly held so the
        // supervised FFmpeg child can be cancelled and reaped first.
        if code.is_some() && exit_in_progress.load(Ordering::Acquire) {
            return;
        }
        api.prevent_exit();
        if exit_in_progress.swap(true, Ordering::AcqRel) {
            return;
        }

        app_handle
            .state::<infrastructure::diagnostics::DiagnosticLog>()
            .record(
                infrastructure::diagnostics::DiagnosticLevel::Info,
                "app_exit_requested",
                "waiting for supervised media jobs to stop",
            );

        let handle = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            let runtime = handle.state::<ExportRuntime>().inner().clone();
            if let Some(active) = runtime.current().await {
                let _ = runtime.cancel(&active.job_id).await;
                let _ = tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    while runtime.is_active().await {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                })
                .await;
            }
            handle
                .state::<infrastructure::diagnostics::DiagnosticLog>()
                .mark_clean_exit();
            handle.exit(0);
        });
    });
}
