use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Emitter, State};
use tokio::sync::broadcast;

use crate::app_state::ManagedState;

/// Global flag to prevent spawning multiple log stream tasks.
static LOG_STREAM_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Start streaming logs to the frontend via Tauri events.
/// Only one stream can be active at a time — subsequent calls are no-ops.
#[tauri::command]
pub async fn start_log_stream(
    state: State<'_, ManagedState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    if LOG_STREAM_ACTIVE.swap(true, Ordering::SeqCst) {
        tracing::debug!("log stream already active, skipping");
        return Ok(());
    }

    let mut rx = state.log_sender.subscribe();

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(line) => {
                    let _ = app_handle.emit("log-entry", &line);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("log stream lagged, skipped {n} entries");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
        LOG_STREAM_ACTIVE.store(false, Ordering::SeqCst);
    });

    Ok(())
}

/// Get the log file directory path.
#[tauri::command]
pub async fn get_log_path() -> Result<String, String> {
    crate::utils::logs_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}
