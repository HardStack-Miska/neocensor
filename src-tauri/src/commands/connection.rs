use tauri::{Emitter, State};

use crate::app_state::ManagedState;
use crate::core::config_gen::ConfigGenerator;
use crate::models::ConnectionStatus;

/// Health-check: poll sing-box mixed inbound port until it responds.
async fn wait_for_singbox(port: u16) -> Result<(), String> {
    let addr = format!("127.0.0.1:{port}");
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(&addr).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!("sing-box did not bind to port {port} within 5s"))
}

/// Tracks which steps have been completed so we can roll them back on failure.
#[derive(Default)]
struct ConnectRollback {
    singbox_started: bool,
}

impl ConnectRollback {
    async fn rollback(self, state: &ManagedState) {
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

    // Check binary exists
    if !state.singbox.binary_path().exists() {
        let msg = format!(
            "sing-box not found at {}. Download it from Settings > Components.",
            state.singbox.binary_path().display()
        );
        tracing::error!("{msg}");
        return Err(msg);
    }

    // If already connected to a different server, stop sing-box first
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
            tracing::debug!("stopping sing-box");
            state.singbox.stop().await.ok();

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

    // Get routing info
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

    // Generate sing-box config
    tracing::debug!("generating sing-box config");
    let singbox_config = ConfigGenerator::generate_singbox_config(
        &server, &settings, &routes, default_mode,
    )
    .map_err(|e| {
        tracing::error!("sing-box config generation failed: {e}");
        e.to_string()
    })?;

    // Start sing-box
    tracing::info!("starting sing-box (mixed port: {})", settings.mixed_port);
    if let Err(e) = state.singbox.start(&singbox_config).await {
        tracing::error!("sing-box start failed: {e}");
        rb.rollback(state).await;
        return Err(format!("failed to start sing-box: {e}"));
    }
    rb.singbox_started = true;

    // Give sing-box a moment to start, then check if it crashed immediately
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    if !state.singbox.is_alive().await {
        rb.rollback(state).await;
        return Err(
            "sing-box exited immediately. This usually means:\n\
             - App not running as Administrator (TUN requires admin rights)\n\
             - Another VPN/proxy is using the TUN adapter\n\
             Check Logs tab for details."
                .to_string(),
        );
    }

    // Wait for sing-box to bind ports
    tracing::info!("waiting for sing-box to bind ports");
    if let Err(e) = wait_for_singbox(settings.mixed_port).await {
        tracing::error!("sing-box health check failed: {e}");
        rb.rollback(state).await;
        return Err(e);
    }

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

    // Clear connection log
    tracing::debug!("disconnect: clearing connections");
    state.clear_connections().await;
    let _ = app_handle.emit("connection-status", "disconnecting");

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
