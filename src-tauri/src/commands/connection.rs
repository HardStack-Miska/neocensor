use tauri::{Emitter, State};

use crate::app_state::ManagedState;
use crate::core::config_gen::ConfigGenerator;
use crate::models::ConnectionStatus;

/// Health-check: poll xray-core SOCKS5 port until it responds.
async fn wait_for_xray(port: u16) -> Result<(), String> {
    let addr = format!("127.0.0.1:{port}");
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(&addr).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!("xray-core did not bind to port {port} within 5s"))
}

/// Tracks which steps have been completed so we can roll them back on failure.
#[derive(Default)]
struct ConnectRollback {
    xray_started: bool,
    pac_started: bool,
    system_proxy_set: bool,
    wfp_started: bool,
}

impl ConnectRollback {
    async fn rollback(self, state: &ManagedState) {
        if self.wfp_started {
            tracing::debug!("rollback: stopping WFP");
            let mut wfp = state.wfp.lock().await;
            wfp.stop();
        }
        if self.pac_started {
            tracing::debug!("rollback: stopping PAC server");
            let mut pac = state.pac_server.lock().await;
            pac.stop().await;
        }
        if self.system_proxy_set {
            tracing::debug!("rollback: unsetting system proxy");
            let _ = crate::core::system_proxy::unset_system_proxy();
        }
        if self.xray_started {
            tracing::debug!("rollback: stopping xray-core");
            state.xray.stop().await.ok();
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

    // Check binary exists
    if !state.xray.binary_path().exists() {
        let msg = format!(
            "xray-core not found at {}. Download it from Settings > Components.",
            state.xray.binary_path().display()
        );
        tracing::error!("{msg}");
        return Err(msg);
    }

    // If already connected to a different server, stop xray first
    {
        let app_state = state.app_state.lock().await;
        if app_state.status == ConnectionStatus::Connected {
            let prev_id = app_state.active_server_id;
            drop(app_state);

            tracing::info!(
                "stopping previous connection (server={:?}) before connecting to {id}",
                prev_id
            );

            // Stop current connection
            tracing::debug!("step 1/4: stopping xray-core");
            state.xray.stop().await.ok();
            tracing::debug!("step 2/4: unsetting system proxy");
            let _ = crate::core::system_proxy::unset_system_proxy();
            tracing::debug!("step 3/4: stopping WFP");
            {
                let mut wfp = state.wfp.lock().await;
                wfp.stop();
            }
            tracing::debug!("step 4/4: stopping PAC server");
            {
                let mut pac = state.pac_server.lock().await;
                pac.stop().await;
            }

            // Mark previous server as offline
            if let Some(prev) = prev_id {
                let mut store = state.server_store.lock().await;
                if let Some(entry) = store.iter_mut().find(|s| s.config.id == prev) {
                    entry.online = false;
                }
            }
        }
    }

    // Update status
    {
        let mut app_state = state.app_state.lock().await;
        app_state.status = ConnectionStatus::Connecting;
        app_state.active_server_id = Some(id);
    }
    let _ = app_handle.emit("connection-status", "connecting");

    // Run the connection sequence with rollback on failure
    match connect_inner(&state, &app_handle, id).await {
        Ok(()) => {
            let _ = app_handle.emit("connection-status", "connected");
            Ok(())
        }
        Err(e) => {
            let mut app_state = state.app_state.lock().await;
            app_state.status = ConnectionStatus::Error;
            app_state.active_server_id = None;
            drop(app_state);
            let _ = app_handle.emit("connection-status", "error");
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

    // Find server config
    let server = {
        let store = state.server_store.lock().await;
        store
            .iter()
            .find(|s| s.config.id == id)
            .map(|s| s.config.clone())
            .ok_or_else(|| {
                tracing::error!("server {id} not found in store");
                "server not found".to_string()
            })?
    };

    // Validate server config
    server.validate()?;

    tracing::info!(
        "connecting to {} ({}:{})",
        server.name,
        server.address,
        server.port
    );

    let settings = state.settings.lock().await.clone();

    // Generate xray config
    tracing::debug!("generating xray config");
    let xray_config = ConfigGenerator::generate_xray_config(&server, &settings)
        .map_err(|e| {
            tracing::error!("xray config generation failed: {e}");
            e.to_string()
        })?;

    // Start xray-core
    tracing::info!("starting xray-core (SOCKS5:{} HTTP:{})", settings.xray_socks_port, settings.xray_http_port);
    if let Err(e) = state.xray.start(&xray_config).await {
        tracing::error!("xray-core start failed: {e}");
        rb.rollback(state).await;
        return Err(format!("failed to start xray-core: {e}"));
    }
    rb.xray_started = true;

    // Wait for xray-core to bind ports
    tracing::info!("waiting for xray-core to bind ports");
    if let Err(e) = wait_for_xray(settings.xray_socks_port).await {
        tracing::error!("xray-core health check failed: {e}");
        rb.rollback(state).await;
        return Err(e);
    }

    // Set system proxy + PAC for DIRECT fallback
    tracing::info!("connect sequence: xray started, setting up proxy");
    if settings.system_proxy {
        let mut pac = state.pac_server.lock().await;
        match pac.start("127.0.0.1", settings.xray_http_port).await {
            Ok(pac_port) => {
                rb.pac_started = true;
                if let Err(e) = crate::core::system_proxy::set_system_proxy_with_pac(
                    "127.0.0.1",
                    settings.xray_http_port,
                    pac_port,
                ) {
                    tracing::error!("failed to set system proxy with PAC: {e}");
                    let _ = crate::core::system_proxy::set_system_proxy("127.0.0.1", settings.xray_http_port);
                }
                rb.system_proxy_set = true;
            }
            Err(e) => {
                tracing::warn!("PAC server start failed: {e}, using ProxyServer only");
                let _ = crate::core::system_proxy::set_system_proxy("127.0.0.1", settings.xray_http_port);
                rb.system_proxy_set = true;
            }
        }
    }

    // Start WFP per-process routing
    // Lock order: profiles (2) -> app_state (3) -> app_routes (5) -> wfp (7) -> process_monitor (8)
    tracing::info!("connect sequence: starting WFP per-process routing");
    let default_mode = {
        let profiles = state.profiles.lock().await;
        let app_state = state.app_state.lock().await;
        app_state
            .active_profile_id
            .as_ref()
            .and_then(|pid| profiles.iter().find(|p| &p.id == pid))
            .map(|p| p.default_mode)
            .unwrap_or(crate::models::RouteMode::Direct)
    };
    let routes = state.app_routes.lock().await.clone();
    // Build exe map once (single process scan)
    let exe_map = {
        let mut monitor = state.process_monitor.lock().await;
        monitor.build_exe_map()
    };
    {
        let mut wfp = state.wfp.lock().await;
        if let Err(e) = wfp.start() {
            tracing::warn!("WFP start failed (per-process routing disabled): {e}");
        }
        if wfp.is_active() {
            rb.wfp_started = true;
            if let Err(e) = wfp.apply_rules(&routes, default_mode, settings.xray_http_port, &exe_map) {
                tracing::error!("WFP apply_rules failed: {e}");
            }
        }
    }

    // Persist any newly-discovered exe_paths back to routes
    persist_discovered_paths(state, &routes, &exe_map).await;

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
    tracing::info!("connected to {} successfully", server.name);
    let _ = app_handle.emit("connection-status", "connected");

    Ok(())
}

/// Save any newly-discovered exe_paths from the process scan back to routes,
/// so future WFP applications work even when apps aren't running.
async fn persist_discovered_paths(
    state: &ManagedState,
    routes: &[crate::models::AppRoute],
    exe_map: &std::collections::HashMap<String, String>,
) {
    let mut updated = false;
    let mut stored_routes = state.app_routes.lock().await;
    for route in stored_routes.iter_mut() {
        if route.exe_path.is_some() {
            continue;
        }
        if let Some(path) = exe_map.get(&route.process_name.to_lowercase()) {
            tracing::debug!("persisting discovered exe_path for {}: {}", route.process_name, path);
            route.exe_path = Some(path.clone());
            updated = true;
        }
    }
    drop(stored_routes);
    if updated {
        state.persist_routes().await;
    }
    let _ = routes; // used for type inference only
}

#[tauri::command]
pub async fn disconnect(
    state: State<'_, ManagedState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    tracing::info!("disconnect requested");

    {
        let mut app_state = state.app_state.lock().await;
        app_state.status = ConnectionStatus::Disconnecting;
    }

    // Stop WFP per-process routing
    tracing::debug!("disconnect: stopping WFP");
    {
        let mut wfp = state.wfp.lock().await;
        wfp.stop();
    }

    // Stop PAC server
    tracing::debug!("disconnect: stopping PAC server");
    {
        let mut pac = state.pac_server.lock().await;
        pac.stop().await;
    }

    // Clear connection log
    tracing::debug!("disconnect: clearing connections and unsetting proxy");
    state.clear_connections().await;
    let _ = app_handle.emit("connection-status", "disconnecting");

    // Remove system proxy first
    if let Err(e) = crate::core::system_proxy::unset_system_proxy() {
        tracing::error!("failed to unset system proxy: {e}");
    }

    // Stop xray-core
    tracing::debug!("stopping xray-core");
    if let Err(e) = state.xray.stop().await {
        tracing::error!("xray-core stop failed: {e}");
    }

    {
        let mut app_state = state.app_state.lock().await;
        app_state.status = ConnectionStatus::Disconnected;
        app_state.active_server_id = None;
    }
    let _ = app_handle.emit("connection-status", "disconnected");
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
