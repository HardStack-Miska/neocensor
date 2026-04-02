use tauri::State;

use crate::app_state::ManagedState;
use crate::models::{AppRoute, Profile, RouteMode};

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
    // Lock order: app_routes (5) then process_monitor (8) — but not simultaneously.
    // Check if route exists first, resolve process info separately if needed.
    let needs_new_route = {
        let mut routes = state.app_routes.lock().await;
        if let Some(route) = routes.iter_mut().find(|r| r.process_name == process_name) {
            route.mode = mode;
            false
        } else {
            true
        }
    };
    if needs_new_route {
        let (category, exe_path) = {
            let mut monitor = state.process_monitor.lock().await;
            let category = monitor.category(&process_name);
            let exe_path = crate::core::wfp::manager::resolve_exe_for_route(
                &process_name,
                &mut monitor,
            );
            (category, exe_path)
        };
        let mut routes = state.app_routes.lock().await;
        // Double-check it wasn't added by a concurrent call
        if !routes.iter().any(|r| r.process_name == process_name) {
            let mut route = AppRoute::new(&process_name, mode)
                .with_display_name(&display_name)
                .with_category(category);
            if let Some(path) = exe_path {
                route = route.with_exe_path(path);
            }
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

    // Re-apply WFP rules if connected, then force browsers to re-read proxy
    reapply_wfp_rules(&state).await;
    refresh_proxy_settings(&state).await;

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

    // Re-apply WFP rules if connected, then force browsers to re-read proxy
    reapply_wfp_rules(&state).await;
    refresh_proxy_settings(&state).await;

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

    // Re-apply WFP rules if connected
    reapply_wfp_rules(&state).await;

    // Force browsers to re-read proxy settings after profile switch
    // This fixes the issue where Brave caches "proxy unreachable" from Direct mode
    refresh_proxy_settings(&state).await;

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

#[tauri::command]
pub async fn check_admin(
) -> Result<bool, String> {
    Ok(crate::core::wfp::is_admin())
}

#[tauri::command]
pub async fn is_wfp_active(
    state: State<'_, ManagedState>,
) -> Result<bool, String> {
    let wfp = state.wfp.lock().await;
    Ok(wfp.is_active())
}

/// Re-apply WFP rules if VPN is connected.
/// Uses TCP reset technique: temporarily blocks all traffic for affected apps,
/// waits for connections to die, then applies the real rules.
/// This forces Chromium browsers to re-establish connections with new proxy settings.
async fn reapply_wfp_rules(state: &ManagedState) {
    // Lock order: settings (1) → profiles (2) → app_state (3) → app_routes (5) → wfp (7) → process_monitor (8)
    // Clone and drop early locks before acquiring later ones.
    let settings = state.settings.lock().await.clone();

    let is_connected = {
        let app_state = state.app_state.lock().await;
        app_state.status == crate::models::ConnectionStatus::Connected
    };

    if !is_connected {
        return;
    }

    let default_mode = {
        let profiles = state.profiles.lock().await;
        let app_state = state.app_state.lock().await;
        let mode = app_state
            .active_profile_id
            .as_ref()
            .and_then(|pid| profiles.iter().find(|p| &p.id == pid))
            .map(|p| p.default_mode)
            .unwrap_or(RouteMode::Direct);
        mode
    };

    let routes = state.app_routes.lock().await.clone();
    let proxy_port = settings.xray_http_port;

    // Build exe map once (single process scan) for all phases
    let exe_map = {
        let mut monitor = state.process_monitor.lock().await;
        monitor.build_exe_map()
    };

    // Phase 1: Remove old filters and add temporary BLOCK ALL to kill TCP connections
    let resets = {
        let mut wfp = state.wfp.lock().await;
        if !wfp.is_active() {
            return;
        }
        match wfp.apply_temporary_blocks_only(&routes, &exe_map) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("temp blocks failed: {e}, applying rules directly");
                let _ = wfp.apply_rules(&routes, default_mode, proxy_port, &exe_map);
                return;
            }
        }
    };

    if !resets.is_empty() {
        // Phase 2: Wait for Chromium to notice connections are dead
        tracing::info!(
            "TCP reset: blocking {} apps for 700ms to kill keepalive connections",
            resets.len()
        );
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    }

    // Phase 3: Remove temp blocks and apply real rules
    {
        let mut wfp = state.wfp.lock().await;
        wfp.remove_temp_blocks(&resets);
        if let Err(e) = wfp.apply_rules(&routes, default_mode, proxy_port, &exe_map) {
            tracing::error!("WFP apply_rules failed after reset: {e}");
        }
        tracing::info!("WFP rules applied after TCP reset");
    }

    // Persist discovered exe_paths back to routes
    {
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

/// Force browsers to re-read proxy settings by toggling proxy off then on.
///
/// Chromium caches "proxy unreachable" aggressively. Simply re-setting the same
/// proxy is ignored. The toggle (unset → 200ms → set) forces Chromium to see a
/// real configuration change and re-evaluate proxy availability.
async fn refresh_proxy_settings(state: &ManagedState) {
    let is_connected = {
        let app_state = state.app_state.lock().await;
        app_state.status == crate::models::ConnectionStatus::Connected
    };

    if !is_connected {
        return;
    }

    let settings = state.settings.lock().await;
    if !settings.system_proxy {
        return;
    }
    let http_port = settings.xray_http_port;
    drop(settings);

    let pac_port = {
        let pac = state.pac_server.lock().await;
        pac.port()
    };

    tracing::info!("toggling proxy settings to force Chromium to re-read");

    // Phase 1: Briefly unset proxy so Chromium sees the change
    let _ = crate::core::system_proxy::unset_system_proxy();

    // Phase 2: Wait for Chromium to notice proxy was removed
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Phase 3: Re-enable proxy — Chromium treats this as a new proxy and retries
    if pac_port > 0 {
        let _ = crate::core::system_proxy::set_system_proxy_with_pac("127.0.0.1", http_port, pac_port);
    } else {
        let _ = crate::core::system_proxy::set_system_proxy("127.0.0.1", http_port);
    }

    tracing::info!("proxy toggle complete");
}
