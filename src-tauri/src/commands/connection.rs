use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Emitter, State};

use crate::app_state::ManagedState;
use crate::core::config_gen::ConfigGenerator;
use crate::models::ConnectionStatus;

/// Strip control characters and clamp length so user-supplied server names cannot
/// inject ANSI escapes / terminal commands into log files.
fn sanitize_for_log(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect()
}

/// Emit the proxy-only fallback warning at most once per session.
fn maybe_emit_proxy_only_warning(app_handle: &tauri::AppHandle) {
    static EMITTED: AtomicBool = AtomicBool::new(false);
    if EMITTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = app_handle.emit(
        "vpn-mode-warning",
        serde_json::json!({
            "mode": "proxy_only",
            "message": "Running in proxy-only mode (no admin rights). Per-app routing is disabled — all configured browsers share one server."
        }),
    );
}

/// Quick TCP probe to the geosite host. Used to decide whether Auto mode can
/// safely include `rule_set: type=remote` entries (sing-box otherwise hangs
/// waiting for them at startup if the host is blocked / no internet).
async fn probe_geosite_reachability() -> bool {
    let target = "raw.githubusercontent.com:443";
    let res = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        tokio::net::TcpStream::connect(target),
    )
    .await;
    matches!(res, Ok(Ok(_)))
}

/// Health-check: confirm sing-box is actually serving the mixed (HTTP+SOCKS5) inbound.
/// A bare TCP connect succeeds for ANY listener on this port — including a stale
/// process or unrelated app. Send a minimal HTTP/1.1 request and verify we get a
/// proxy-style response.
async fn wait_for_singbox(port: u16) -> Result<(), String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let addr = format!("127.0.0.1:{port}");
    for _ in 0..100 {
        let probe = async {
            let mut sock = tokio::net::TcpStream::connect(&addr).await.ok()?;
            // Mixed inbound speaks HTTP — a malformed/foreign request gets a 4xx
            // from sing-box, but a non-HTTP listener will close or return garbage.
            sock.write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n").await.ok()?;
            let mut buf = [0u8; 16];
            let n = tokio::time::timeout(
                std::time::Duration::from_millis(200),
                sock.read(&mut buf),
            )
            .await
            .ok()?
            .ok()?;
            if n >= 5 && buf.starts_with(b"HTTP/") {
                Some(())
            } else {
                None
            }
        };
        if probe.await.is_some() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!(
        "sing-box did not respond as HTTP proxy on port {port} within 5s (port conflict?)"
    ))
}

/// Tracks which steps have been completed so we can roll them back on failure.
#[derive(Default)]
struct ConnectRollback {
    singbox_started: bool,
    system_proxy_set: bool,
}

impl ConnectRollback {
    async fn rollback(self, state: &ManagedState) {
        if self.system_proxy_set {
            tracing::debug!("rollback: unsetting system proxy");
            let _ = crate::core::system_proxy::unset_system_proxy();
        }
        if self.singbox_started {
            tracing::debug!("rollback: stopping sing-box");
            state.singbox.stop().await.ok();
        }
    }
}

#[tauri::command]
pub async fn connect(
    state: State<'_, ManagedState>,
    app_handle: tauri::AppHandle,
    server_id: String,
) -> Result<(), String> {
    let id: uuid::Uuid = server_id.parse().map_err(|e: uuid::Error| e.to_string())?;
    tracing::info!("connect requested for server {id}");

    // Top-level serialize: only one connect/disconnect operation at a time.
    let _connect_guard = state.connect_lock.lock().await;

    // Check binary exists
    if !state.singbox.binary_path().exists() {
        let msg = format!(
            "sing-box not found at {}. Download it from Settings > Components.",
            state.singbox.binary_path().display()
        );
        tracing::error!("{msg}");
        return Err(msg);
    }

    // If already connected, stop the previous connection cleanly first.
    let prev_active_id = {
        let s = state.app_state.lock().await;
        if matches!(
            s.status,
            ConnectionStatus::Connected | ConnectionStatus::Connecting
        ) {
            s.active_server_id
        } else {
            None
        }
    };
    if let Some(prev_id) = prev_active_id {
        if prev_id != id {
            tracing::info!("stopping previous connection (server={prev_id}) before connecting to {id}");
        }
        state.singbox.stop().await.ok();
        state.clear_connections().await;
        if state
            .system_proxy_set
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            let _ = crate::core::system_proxy::unset_system_proxy();
        }
        let mut store = state.server_store.lock().await;
        if let Some(entry) = store.iter_mut().find(|s| s.config.id == prev_id) {
            entry.online = false;
        }
    }

    // Update status atomically
    {
        let mut s = state.app_state.lock().await;
        s.status = ConnectionStatus::Connecting;
        s.active_server_id = Some(id);
    }
    let _ = app_handle.emit("connection-status", "connecting");

    // Run the connection sequence with rollback on failure
    match connect_inner(&state, &app_handle, id).await {
        Ok(()) => {
            let tun = state.singbox.is_tun_active();
            let mode = if tun { "tun" } else { "proxy_only" };
            let _ = app_handle.emit("connection-status", "connected");
            let _ = app_handle.emit("vpn-mode", mode);
            let mode_label = if tun { "TUN" } else { "Proxy-only" };
            crate::core::tray::update_tray_tooltip(
                &app_handle,
                &format!("NeoCensor — Connected ({mode_label})"),
            );
            Ok(())
        }
        Err(e) => {
            let mut s = state.app_state.lock().await;
            s.status = ConnectionStatus::Error;
            s.active_server_id = None;
            drop(s);
            let _ = app_handle.emit("connection-status", "error");
            let _ = app_handle.emit("vpn-mode", "off");
            crate::core::tray::update_tray_tooltip(&app_handle, "NeoCensor — Error");
            Err(e)
        }
    }
}

/// Inner connection sequence. Returns Ok on success, Err with automatic rollback.
async fn connect_inner(
    state: &ManagedState,
    app_handle: &tauri::AppHandle,
    id: uuid::Uuid,
) -> Result<(), String> {
    let mut rb = ConnectRollback::default();

    // Snapshot all data we need, acquiring locks in documented order:
    //   1 settings → 2 profiles → 3 app_state → 4 server_store → 5 app_routes
    let settings = state.settings.lock().await.clone();
    let active_profile_id = {
        // 2 + 3 (cloned, then dropped before later locks)
        let profiles = state.profiles.lock().await;
        let app_state = state.app_state.lock().await;
        app_state.active_profile_id.clone().and_then(|pid| {
            profiles.iter().find(|p| p.id == pid).cloned()
        })
    };
    let default_mode = active_profile_id
        .as_ref()
        .map(|p| p.default_mode)
        .unwrap_or(crate::models::RouteMode::Direct);
    let server = {
        let store = state.server_store.lock().await; // 4
        store
            .iter()
            .find(|s| s.config.id == id)
            .map(|s| s.config.clone())
            .ok_or_else(|| {
                tracing::error!("server {id} not found in store");
                "server not found".to_string()
            })?
    };
    let routes = state.app_routes.lock().await.clone(); // 5

    // Validate server config
    server.validate()?;

    let safe_name = sanitize_for_log(&server.name);
    tracing::info!(
        "connecting to {} ({}:{})",
        safe_name,
        server.address,
        server.port
    );

    // Try TUN mode first (needs admin), fallback to proxy-only
    let mut tun_mode = true;

    // Auto mode requires fetching geosite rule_sets from GitHub. If GitHub is
    // unreachable on cold start, sing-box will hang waiting for the download —
    // degrade to a config without geosite rules so the user gets at least a
    // working VPN. Quick reachability probe with a short timeout.
    let geosite_ok = probe_geosite_reachability().await;
    if !geosite_ok {
        tracing::warn!(
            "geosite host (raw.githubusercontent.com) is unreachable; degrading Auto mode to plain proxy for this connection"
        );
    }

    let gen_config = |tun: bool| -> Result<serde_json::Value, String> {
        let result = if geosite_ok {
            ConfigGenerator::generate_singbox_config(&server, &settings, &routes, default_mode, tun)
        } else {
            ConfigGenerator::generate_singbox_config_no_geosite(
                &server, &settings, &routes, default_mode, tun,
            )
        };
        result.map_err(|e| {
            tracing::error!("sing-box config generation failed: {e}");
            e.to_string()
        })
    };

    // Generate TUN config
    tracing::debug!("generating sing-box config (TUN mode)");
    let singbox_config = gen_config(tun_mode)?;

    // Start sing-box with TUN
    tracing::info!("starting sing-box (TUN + mixed port: {})", settings.mixed_port);
    if let Err(e) = state.singbox.start(&singbox_config, true).await {
        tracing::error!("sing-box start failed: {e}");
        rb.rollback(state).await;
        return Err(format!("failed to start sing-box: {e}"));
    }
    rb.singbox_started = true;

    // Check if sing-box crashed (TUN requires admin — may get Access Denied)
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    if !state.singbox.is_alive().await {
        tracing::warn!("sing-box TUN mode failed, falling back to proxy-only mode");
        tun_mode = false;

        // Ensure dead process is fully cleaned up and port released
        state.singbox.stop().await.ok();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Regenerate config without TUN (still respects geosite_ok)
        let proxy_config = gen_config(false)?;

        if let Err(e) = state.singbox.start(&proxy_config, false).await {
            rb.rollback(state).await;
            return Err(format!("failed to start sing-box: {e}"));
        }

        // Check again
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        if !state.singbox.is_alive().await {
            rb.rollback(state).await;
            return Err("sing-box failed to start. Check Logs for details.".to_string());
        }

        // Set system proxy to route browser traffic through sing-box
        if let Err(e) = crate::core::system_proxy::set_system_proxy("127.0.0.1", settings.mixed_port) {
            tracing::error!("failed to set system proxy: {e}");
        } else {
            rb.system_proxy_set = true;
            // Track on shared state so disconnect/exit knows it must unset
            state.system_proxy_set.store(true, std::sync::atomic::Ordering::Release);
        }

        // Notify UI once per session that we're in proxy-only mode
        maybe_emit_proxy_only_warning(app_handle);
    }

    // Wait for sing-box to bind mixed port
    tracing::info!("waiting for sing-box to bind ports");
    if let Err(e) = wait_for_singbox(settings.mixed_port).await {
        tracing::error!("sing-box health check failed: {e}");
        rb.rollback(state).await;
        return Err(e);
    }

    let mode_str = if tun_mode { "TUN" } else { "proxy-only (no admin)" };
    tracing::info!("sing-box running in {mode_str} mode");

    // Mark server as online
    {
        let mut store = state.server_store.lock().await;
        if let Some(entry) = store.iter_mut().find(|s| s.config.id == id) {
            entry.online = true;
        }
    }

    // Success
    {
        let mut app_state = state.app_state.lock().await;
        app_state.status = ConnectionStatus::Connected;
    }
    tracing::info!("connected to {} successfully", safe_name);
    let _ = app_handle.emit("connection-status", "connected");

    Ok(())
}

#[tauri::command]
pub async fn disconnect(
    state: State<'_, ManagedState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    tracing::info!("disconnect requested");

    // Serialize against connect()
    let _connect_guard = state.connect_lock.lock().await;

    {
        let mut app_state = state.app_state.lock().await;
        app_state.status = ConnectionStatus::Disconnecting;
    }

    // Clear connection log
    tracing::debug!("disconnect: clearing connections");
    state.clear_connections().await;
    let _ = app_handle.emit("connection-status", "disconnecting");

    // Unset system proxy only if we set it (avoid clobbering corporate proxy in TUN mode)
    if state
        .system_proxy_set
        .swap(false, std::sync::atomic::Ordering::AcqRel)
    {
        let _ = crate::core::system_proxy::unset_system_proxy();
    }

    // Stop sing-box
    tracing::debug!("stopping sing-box");
    if let Err(e) = state.singbox.stop().await {
        tracing::error!("sing-box stop failed: {e}");
    }

    {
        let mut app_state = state.app_state.lock().await;
        app_state.status = ConnectionStatus::Disconnected;
        app_state.active_server_id = None;
    }
    let _ = app_handle.emit("connection-status", "disconnected");
    let _ = app_handle.emit("vpn-mode", "off");
    crate::core::tray::update_tray_tooltip(&app_handle, "NeoCensor — Disconnected");
    tracing::info!("disconnected");

    Ok(())
}

#[tauri::command]
pub async fn get_connection_status(
    state: State<'_, ManagedState>,
) -> Result<crate::models::AppState, String> {
    let app_state = state.app_state.lock().await;
    Ok(app_state.clone())
}

/// Returns the active VPN mode: "tun" | "proxy_only" | "off".
/// UI uses this to show whether per-app routing is functional.
#[tauri::command]
pub async fn get_vpn_mode(state: State<'_, ManagedState>) -> Result<String, String> {
    let app_state = state.app_state.lock().await;
    if app_state.status != ConnectionStatus::Connected {
        return Ok("off".to_string());
    }
    drop(app_state);
    Ok(if state.singbox.is_tun_active() {
        "tun".to_string()
    } else {
        "proxy_only".to_string()
    })
}
