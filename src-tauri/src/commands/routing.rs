use tauri::State;

use crate::app_state::ManagedState;
use crate::core::config_gen::ConfigGenerator;
use crate::models::{AppRoute, ConnectionStatus, Profile, RouteMode};

#[tauri::command]
pub async fn get_app_routes(state: State<'_, ManagedState>) -> Result<Vec<AppRoute>, String> {
    let routes = state.app_routes.lock().await;
    Ok(routes.clone())
}

#[tauri::command]
pub async fn set_app_route(
    state: State<'_, ManagedState>,
    process_name: String,
    display_name: String,
    mode: RouteMode,
) -> Result<(), String> {
    {
        let mut routes = state.app_routes.lock().await;
        if let Some(route) = routes.iter_mut().find(|r| r.process_name == process_name) {
            route.mode = mode;
        } else {
            let category = {
                let mut monitor = state.process_monitor.lock().await;
                monitor.category(&process_name)
            };
            let route = AppRoute::new(&process_name, mode)
                .with_display_name(&display_name)
                .with_category(category);
            routes.push(route);
        }
    }
    state.persist_routes().await;
    tracing::debug!(
        "set_app_route: syncing routes to profile after setting {} to {:?}",
        process_name,
        mode
    );
    sync_routes_to_profile(&state).await;

    // Restart sing-box with updated routes if connected
    restart_singbox_with_routes(&state).await;

    Ok(())
}

#[tauri::command]
pub async fn remove_app_route(
    state: State<'_, ManagedState>,
    process_name: String,
) -> Result<(), String> {
    {
        let mut routes = state.app_routes.lock().await;
        routes.retain(|r| r.process_name != process_name);
    }
    state.persist_routes().await;
    tracing::debug!(
        "remove_app_route: syncing routes to profile after removing {}",
        process_name
    );
    sync_routes_to_profile(&state).await;

    // Restart sing-box with updated routes if connected
    restart_singbox_with_routes(&state).await;

    Ok(())
}

#[tauri::command]
pub async fn get_profiles(state: State<'_, ManagedState>) -> Result<Vec<Profile>, String> {
    let profiles = state.profiles.lock().await;
    Ok(profiles.clone())
}

#[tauri::command]
pub async fn set_active_profile(
    state: State<'_, ManagedState>,
    profile_id: String,
) -> Result<(), String> {
    // Save current routes into the currently active profile before switching
    {
        let app_state = state.app_state.lock().await;
        let current_profile_id = app_state.active_profile_id.clone();
        drop(app_state);

        if let Some(current_id) = current_profile_id {
            let current_routes = state.app_routes.lock().await.clone();
            let mut profiles = state.profiles.lock().await;
            if let Some(profile) = profiles.iter_mut().find(|p| p.id == current_id) {
                profile.routes = current_routes;
            }
            drop(profiles);
            // Persist updated profiles
            let profiles = state.profiles.lock().await;
            if let Err(e) = state.store.save_profiles(&profiles) {
                tracing::error!("failed to save profiles: {e}");
            }
        }
    }

    // Load the new profile's routes
    tracing::info!("switching to profile: {}", profile_id);
    let profile = {
        let profiles = state.profiles.lock().await;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .ok_or("profile not found")?
            .clone()
    };

    tracing::info!(
        "loading profile '{}' with {} routes, default_mode={:?}",
        profile.name,
        profile.routes.len(),
        profile.default_mode
    );

    {
        let mut routes = state.app_routes.lock().await;
        *routes = profile.routes.clone();
    }
    {
        let mut app_state = state.app_state.lock().await;
        app_state.active_profile_id = Some(profile_id.clone());
    }
    {
        let mut settings = state.settings.lock().await;
        settings.active_profile_id = Some(profile_id);
    }

    state.persist_routes().await;
    state.persist_settings().await;

    // Restart sing-box with new profile routes if connected
    restart_singbox_with_routes(&state).await;

    Ok(())
}

#[tauri::command]
pub async fn get_settings(
    state: State<'_, ManagedState>,
) -> Result<crate::models::Settings, String> {
    let settings = state.settings.lock().await;
    Ok(settings.clone())
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, ManagedState>,
    new_settings: crate::models::Settings,
) -> Result<(), String> {
    new_settings.validate()?;
    {
        let mut settings = state.settings.lock().await;
        *settings = new_settings;
    }
    state.persist_settings().await;
    Ok(())
}

/// Restart sing-box with current routes if VPN is connected.
async fn restart_singbox_with_routes(state: &ManagedState) {
    let is_connected = {
        let app_state = state.app_state.lock().await;
        app_state.status == ConnectionStatus::Connected
    };

    if !is_connected {
        return;
    }

    let settings = state.settings.lock().await.clone();

    let default_mode = {
        let profiles = state.profiles.lock().await;
        let app_state = state.app_state.lock().await;
        app_state
            .active_profile_id
            .as_ref()
            .and_then(|pid| profiles.iter().find(|p| &p.id == pid))
            .map(|p| p.default_mode)
            .unwrap_or(RouteMode::Direct)
    };

    let routes = state.app_routes.lock().await.clone();

    // Find active server config
    let server = {
        let app_state = state.app_state.lock().await;
        let server_id = match app_state.active_server_id {
            Some(id) => id,
            None => return,
        };
        drop(app_state);

        let store = state.server_store.lock().await;
        match store.iter().find(|s| s.config.id == server_id) {
            Some(entry) => entry.config.clone(),
            None => return,
        }
    };

    // Generate new config and restart
    // Check if sing-box is running with TUN (process alive = TUN likely worked)
    let tun_mode = state.singbox.is_alive().await;
    match ConfigGenerator::generate_singbox_config(&server, &settings, &routes, default_mode, tun_mode) {
        Ok(config) => {
            if let Err(e) = state.singbox.restart(&config).await {
                tracing::error!("sing-box restart failed: {e}");
            } else {
                tracing::info!("sing-box restarted with updated routes");
            }
        }
        Err(e) => {
            tracing::error!("sing-box config generation failed on restart: {e}");
        }
    }
}

/// Sync current routes back to the active profile so changes are remembered.
async fn sync_routes_to_profile(state: &ManagedState) {
    let profile_id = {
        let app_state = state.app_state.lock().await;
        app_state.active_profile_id.clone()
    };

    if let Some(pid) = profile_id {
        let current_routes = state.app_routes.lock().await.clone();
        tracing::debug!(
            "sync_routes_to_profile: saving {} routes to profile {}",
            current_routes.len(),
            pid
        );
        let mut profiles = state.profiles.lock().await;
        if let Some(profile) = profiles.iter_mut().find(|p| p.id == pid) {
            profile.routes = current_routes;
        }
        drop(profiles);

        let profiles = state.profiles.lock().await;
        if let Err(e) = state.store.save_profiles(&profiles) {
            tracing::error!("failed to save profiles: {e}");
        }
    } else {
        tracing::debug!("sync_routes_to_profile: no active profile, skipping");
    }
}
