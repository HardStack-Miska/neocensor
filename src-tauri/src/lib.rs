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
            get_vpn_mode,
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

            // Self-heal orphaned system proxy from a previous crashed run
            #[cfg(windows)]
            {
                let port = {
                    let state: tauri::State<'_, ManagedState> = app.state();
                    let s = tauri::async_runtime::block_on(state.settings.lock());
                    s.mixed_port
                };
                match core::system_proxy::restore_if_orphaned(port) {
                    Ok(true) => tracing::warn!("restored orphaned system proxy from previous run"),
                    Ok(false) => {}
                    Err(e) => tracing::warn!("orphaned-proxy self-heal failed: {e}"),
                }
            }

            // Set up system tray
            if let Err(e) = core::tray::setup_tray(app.handle()) {
                tracing::error!("failed to set up system tray: {e}");
            }

            // Auto-download sing-box if missing
            let dl_handle = app.handle().clone();
            let dl_task = tauri::async_runtime::spawn(async move {
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

            // Parse sing-box log lines into connection events.
            // sing-box stderr is fed into tracing, which routes through BroadcastWriter
            // into `state.log_sender` (the unified channel). We subscribe to it here.
            let traffic_handle = app.handle().clone();
            let traffic_task = tauri::async_runtime::spawn(async move {
                let state: tauri::State<'_, ManagedState> = traffic_handle.state();
                let mut rx = state.log_sender.subscribe();

                loop {
                    match rx.recv().await {
                        Ok(line) => {
                            if line.contains("inbound") && line.contains("outbound") {
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

            // Watchdog: detect sing-box crashes and surface to UI + cleanup.
            // Requires TWO consecutive failures before reacting — avoids false positives
            // during brief restart windows from route changes.
            let watchdog_handle = app.handle().clone();
            let watchdog_task = tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
                let mut consecutive_dead = 0u8;
                loop {
                    interval.tick().await;
                    let state: tauri::State<'_, ManagedState> = watchdog_handle.state();

                    // Skip if a known transient restart is in progress
                    if state
                        .transition_in_progress
                        .load(std::sync::atomic::Ordering::Acquire)
                    {
                        consecutive_dead = 0;
                        continue;
                    }

                    let status = {
                        let s = state.app_state.lock().await;
                        s.status
                    };

                    use crate::models::ConnectionStatus;
                    if status != ConnectionStatus::Connected {
                        consecutive_dead = 0;
                        continue;
                    }

                    if state.singbox.is_alive().await {
                        consecutive_dead = 0;
                        continue;
                    }

                    consecutive_dead += 1;
                    if consecutive_dead < 2 {
                        // One miss can be a transient restart we don't track explicitly
                        continue;
                    }

                    tracing::error!("watchdog: sing-box died unexpectedly");
                    consecutive_dead = 0;

                    if state
                        .system_proxy_set
                        .swap(false, std::sync::atomic::Ordering::AcqRel)
                    {
                        let _ = crate::core::system_proxy::unset_system_proxy();
                    }
                    {
                        let mut s = state.app_state.lock().await;
                        s.status = ConnectionStatus::Error;
                        s.active_server_id = None;
                    }
                    let _ = watchdog_handle.emit("connection-status", "error");
                    let _ = watchdog_handle.emit("vpn-mode", "off");
                    let _ = watchdog_handle.emit(
                        "vpn-error",
                        serde_json::json!({
                            "code": "singbox_died",
                            "message": "sing-box stopped unexpectedly. Check logs for details."
                        }),
                    );
                    if let Some(tray) = watchdog_handle.tray_by_id("main") {
                        let _ = tray.set_tooltip(Some("NeoCensor — Error"));
                    }
                }
            });

            // Subscription auto-refresh scheduler
            let sub_handle = app.handle().clone();
            let sub_task = tauri::async_runtime::spawn(async move {
                // Wait a bit before first run to let the app settle
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    let state: tauri::State<'_, ManagedState> = sub_handle.state();
                    let due_ids: Vec<uuid::Uuid> = {
                        let subs = state.subscriptions.lock().await;
                        let now = chrono::Utc::now();
                        subs.iter()
                            .filter(|s| s.enabled)
                            .filter(|s| match s.last_updated {
                                Some(last) => {
                                    let elapsed = now.signed_duration_since(last).num_seconds();
                                    elapsed.max(0) as u64 >= s.update_interval_secs
                                }
                                None => true,
                            })
                            .map(|s| s.id)
                            .collect()
                    };
                    for id in due_ids {
                        if let Err(e) = crate::commands::subscription::refresh_subscription_internal(&state, id).await {
                            tracing::warn!("auto-refresh failed for subscription {id}: {e}");
                        } else {
                            tracing::info!("auto-refreshed subscription {id}");
                        }
                    }
                }
            });

            // Track all background tasks so we can abort them cleanly on exit
            {
                let state: tauri::State<'_, ManagedState> = app.state();
                let st = state.inner();
                st.register_task(dl_task);
                st.register_task(traffic_task);
                st.register_task(watchdog_task);
                st.register_task(sub_task);
            }

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
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                tracing::info!("app exit cleanup running");
                let state: tauri::State<'_, ManagedState> = app_handle.state();

                // Abort tasks synchronously (sync mutex), then async cleanup.
                state.abort_background_tasks();
                tauri::async_runtime::block_on(async {
                    if state
                        .system_proxy_set
                        .swap(false, std::sync::atomic::Ordering::AcqRel)
                    {
                        let _ = crate::core::system_proxy::unset_system_proxy();
                    }
                    let _ = state.singbox.stop().await;
                });

                tracing::info!("app exit cleanup complete");
            }
        });
}
