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

    // Safety: unset system proxy on panic to avoid leaving proxy settings broken
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = core::system_proxy::unset_system_proxy();
        default_hook(info);
    }));

    tracing::info!("=== NeoCensor v0.1.0 starting ===");
    tracing::info!("log directory: {}", log_dir.display());

    let config_dir = utils::config_dir().expect("failed to determine config directory");
    let data_dir = utils::data_dir().expect("failed to determine data directory");
    let xray_path = utils::xray_binary_path().expect("failed to determine xray binary path");

    tracing::info!("data directory: {}", data_dir.display());
    tracing::info!("xray binary: {} (exists={})", xray_path.display(), xray_path.exists());

    let xray = core::XrayManager::new(xray_path, config_dir);
    let store = core::Store::new(data_dir);

    // Clean up stale proxy settings from a previous crashed session
    // Read port from saved settings (not hardcoded) so it works even if user changed it
    let saved_settings = store.load_settings();
    core::system_proxy::cleanup_stale_proxy(saved_settings.xray_http_port);

    let state = ManagedState::new(xray, store, log_sender);

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
            // WFP
            check_admin,
            is_wfp_active,
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
            if let Ok(p) = utils::xray_binary_path() {
                if let Some(bin_dir) = p.parent() {
                    std::fs::create_dir_all(bin_dir).ok();
                }
            }

            // Set up system tray
            if let Err(e) = core::tray::setup_tray(app.handle()) {
                tracing::error!("failed to set up system tray: {e}");
            }

            // Auto-download xray-core if missing
            let dl_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<'_, ManagedState> = dl_handle.state();
                let xray_exists = state.xray.binary_path().exists();

                if xray_exists {
                    tracing::info!("xray-core binary found");
                    return;
                }

                tracing::info!("xray-core missing, starting auto-download");

                let bin_dir = match state.xray.binary_path().parent() {
                    Some(dir) => dir.to_path_buf(),
                    None => {
                        tracing::error!("xray binary path has no parent directory");
                        return;
                    }
                };
                std::fs::create_dir_all(&bin_dir).ok();

                let _ = dl_handle.emit("download-progress", serde_json::json!({
                    "component": "xray-core", "status": "downloading"
                }));

                let version = core::downloader::check_latest_version("XTLS/Xray-core")
                    .await
                    .unwrap_or_else(|_| "25.3.6".into());

                match tokio::time::timeout(
                    std::time::Duration::from_secs(300),
                    core::downloader::download_xray(&version, &bin_dir),
                ).await {
                    Ok(Ok(_)) => {
                        tracing::info!("xray-core v{version} downloaded successfully");
                        let _ = dl_handle.emit("download-progress", serde_json::json!({
                            "component": "xray-core", "status": "installed"
                        }));
                    }
                    Ok(Err(e)) => {
                        tracing::error!("xray-core download failed: {e}");
                        let _ = dl_handle.emit("download-progress", serde_json::json!({
                            "component": "xray-core", "status": "failed", "error": e.to_string()
                        }));
                    }
                    Err(_) => {
                        tracing::error!("xray-core download timed out after 5 minutes");
                        let _ = dl_handle.emit("download-progress", serde_json::json!({
                            "component": "xray-core", "status": "timeout"
                        }));
                    }
                }
            });

            // Parse xray log lines into connection events and emit to frontend
            let traffic_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<'_, ManagedState> = traffic_handle.state();
                let mut rx = state.log_sender.subscribe();

                loop {
                    match rx.recv().await {
                        Ok(line) => {
                            // Only parse [xray] lines containing "accepted"
                            // Note: log_sender receives tracing-formatted lines with timestamp prefix
                            if line.contains("[xray]") && line.contains(" accepted ") {
                                let id = state.next_conn_id();
                                if let Some(event) = core::traffic::parse_xray_connection(&line, id) {
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
                tracing::info!("app exiting, cleaning up system proxy");
                let _ = core::system_proxy::unset_system_proxy();
            }
        });
}
