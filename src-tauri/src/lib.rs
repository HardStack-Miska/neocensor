mod app_state;
mod commands;
mod core;
mod models;
mod parsers;
mod utils;

use tauri::{Emitter, Manager};

use app_state::ManagedState;
use commands::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize structured logging (stdout + file + broadcast)
    let log_dir = utils::logs_dir().expect("failed to determine log directory");
    let log_sender = core::logger::init_logging(&log_dir);

    tracing::info!("=== NeoCensor v0.1.0 starting ===");
    tracing::info!("log directory: {}", log_dir.display());

    let config_dir = utils::config_dir().expect("failed to determine config directory");
    let data_dir = utils::data_dir().expect("failed to determine data directory");
    let singbox_path = utils::singbox_binary_path().expect("failed to determine sing-box binary path");

    tracing::info!("data directory: {}", data_dir.display());
    tracing::info!("sing-box binary: {} (exists={})", singbox_path.display(), singbox_path.exists());

    let singbox = core::SingboxManager::new(singbox_path, config_dir);
    let store = core::Store::new(data_dir);

    let state = ManagedState::new(singbox, store, log_sender);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            // Server
            get_servers,
            add_server,
            remove_server,
            parse_vless,
            export_vless_uri,
            ping_server,
            ping_all_servers,
            toggle_favorite,
            // Connection
            connect,
            disconnect,
            get_connection_status,
            // Routing
            get_app_routes,
            set_app_route,
            remove_app_route,
            get_profiles,
            set_active_profile,
            get_settings,
            update_settings,
            // Subscriptions
            get_subscriptions,
            add_subscription,
            refresh_subscription,
            remove_subscription,
            refresh_all_subscriptions,
            // Process
            get_processes,
            // Traffic
            get_traffic_stats,
            // Download
            check_binaries,
            download_components,
            check_latest_versions,
            // Logs
            start_log_stream,
            get_log_path,
        ])
        .setup(|app| {
            tracing::info!("running setup");

            // Ensure directories exist
            for dir_fn in [
                utils::data_dir,
                utils::config_dir,
                utils::logs_dir,
                utils::geo_dir,
                utils::icons_dir,
            ] {
                if let Ok(dir) = dir_fn() {
                    std::fs::create_dir_all(&dir).ok();
                }
            }
            if let Ok(p) = utils::singbox_binary_path() {
                if let Some(bin_dir) = p.parent() {
                    std::fs::create_dir_all(bin_dir).ok();
                }
            }

            // Set up system tray
            if let Err(e) = core::tray::setup_tray(app.handle()) {
                tracing::error!("failed to set up system tray: {e}");
            }

            // Auto-download sing-box if missing
            let dl_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<'_, ManagedState> = dl_handle.state();
                let singbox_exists = state.singbox.binary_path().exists();

                if singbox_exists {
                    tracing::info!("sing-box binary found");
                    return;
                }

                tracing::info!("sing-box missing, starting auto-download");

                let bin_dir = match state.singbox.binary_path().parent() {
                    Some(dir) => dir.to_path_buf(),
                    None => {
                        tracing::error!("sing-box binary path has no parent directory");
                        return;
                    }
                };
                std::fs::create_dir_all(&bin_dir).ok();

                let _ = dl_handle.emit("download-progress", serde_json::json!({
                    "component": "sing-box", "status": "downloading"
                }));

                let version = core::downloader::check_latest_version("SagerNet/sing-box")
                    .await
                    .unwrap_or_else(|_| "1.11.0".into());

                match tokio::time::timeout(
                    std::time::Duration::from_secs(300),
                    core::downloader::download_singbox(&version, &bin_dir),
                ).await {
                    Ok(Ok(_)) => {
                        tracing::info!("sing-box v{version} downloaded successfully");
                        let _ = dl_handle.emit("download-progress", serde_json::json!({
                            "component": "sing-box", "status": "installed"
                        }));
                    }
                    Ok(Err(e)) => {
                        tracing::error!("sing-box download failed: {e}");
                        let _ = dl_handle.emit("download-progress", serde_json::json!({
                            "component": "sing-box", "status": "failed", "error": e.to_string()
                        }));
                    }
                    Err(_) => {
                        tracing::error!("sing-box download timed out after 5 minutes");
                        let _ = dl_handle.emit("download-progress", serde_json::json!({
                            "component": "sing-box", "status": "timeout"
                        }));
                    }
                }
            });

            // Parse sing-box log lines into connection events and emit to frontend
            let traffic_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<'_, ManagedState> = traffic_handle.state();
                let mut rx = state.log_sender.subscribe();

                loop {
                    match rx.recv().await {
                        Ok(line) => {
                            // Parse sing-box router log lines
                            if line.contains("[sing-box]") && line.contains("inbound") && line.contains("outbound") {
                                let id = state.next_conn_id();
                                if let Some(event) = core::traffic::parse_singbox_connection(&line, id) {
                                    let _ = traffic_handle.emit("connection-event", &event);
                                    state.push_connection(event).await;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("traffic parser lagged, skipped {n} log lines");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            tracing::info!("=== NeoCensor ready ===");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                tracing::debug!("window close requested, hiding to tray");
                window.hide().ok();
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building NeoCensor")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                tracing::info!("app exiting");
            }
        });
}
