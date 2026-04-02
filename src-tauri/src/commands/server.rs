use std::time::Duration;

use tauri::State;

use crate::app_state::ManagedState;
use crate::models::{ServerConfig, ServerEntry, ServerSource};
use crate::parsers::vless::{parse_vless_uri, to_vless_uri};

#[tauri::command]
pub async fn get_servers(state: State<'_, ManagedState>) -> Result<Vec<ServerEntry>, String> {
    let store = state.server_store.lock().await;
    Ok(store.clone())
}

#[tauri::command]
pub async fn add_server(
    state: State<'_, ManagedState>,
    uri: String,
) -> Result<ServerEntry, String> {
    let config = parse_vless_uri(&uri).map_err(|e| e.to_string())?;

    // Check for duplicates (same address + port + uuid)
    {
        let store = state.server_store.lock().await;
        let exists = store.iter().any(|s| {
            s.config.address == config.address
                && s.config.port == config.port
                && s.config.uuid == config.uuid
        });
        if exists {
            return Err("Server already exists".to_string());
        }
    }

    let entry = ServerEntry::from_config(config, ServerSource::Manual);

    {
        let mut store = state.server_store.lock().await;
        store.push(entry.clone());
    }
    state.persist_servers().await;

    Ok(entry)
}

#[tauri::command]
pub async fn remove_server(
    state: State<'_, ManagedState>,
    server_id: String,
) -> Result<(), String> {
    let id: uuid::Uuid = server_id.parse().map_err(|e: uuid::Error| e.to_string())?;
    {
        let mut store = state.server_store.lock().await;
        store.retain(|s| s.config.id != id);
    }
    state.persist_servers().await;
    Ok(())
}

#[tauri::command]
pub async fn parse_vless(uri: String) -> Result<ServerConfig, String> {
    parse_vless_uri(&uri).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_vless_uri(
    state: State<'_, ManagedState>,
    server_id: String,
) -> Result<String, String> {
    let id: uuid::Uuid = server_id.parse().map_err(|e: uuid::Error| e.to_string())?;
    let store = state.server_store.lock().await;
    let entry = store
        .iter()
        .find(|s| s.config.id == id)
        .ok_or("server not found")?;
    Ok(to_vless_uri(&entry.config))
}

#[tauri::command]
pub async fn ping_server(
    state: State<'_, ManagedState>,
    server_id: String,
) -> Result<u32, String> {
    let id: uuid::Uuid = server_id.parse().map_err(|e: uuid::Error| e.to_string())?;
    let (host, port) = {
        let store = state.server_store.lock().await;
        let entry = store
            .iter()
            .find(|s| s.config.id == id)
            .ok_or("server not found".to_string())?;
        (entry.config.address.clone(), entry.config.port)
    };

    let ms = crate::core::ping::tcp_ping(&host, port, Duration::from_secs(5))
        .await
        .map_err(|e| e.to_string())?;

    {
        let mut store = state.server_store.lock().await;
        if let Some(entry) = store.iter_mut().find(|s| s.config.id == id) {
            entry.ping_ms = Some(ms);
            entry.online = true;
        }
    }
    state.persist_servers().await;
    Ok(ms)
}

#[tauri::command]
pub async fn ping_all_servers(
    state: State<'_, ManagedState>,
) -> Result<Vec<(String, Option<u32>)>, String> {
    let targets: Vec<_> = {
        let store = state.server_store.lock().await;
        store
            .iter()
            .map(|s| (s.config.id, s.config.address.clone(), s.config.port))
            .collect()
    };

    let server_addrs: Vec<_> = targets.iter().map(|(_, h, p)| (h.clone(), *p)).collect();
    let results = crate::core::ping::ping_all(&server_addrs, Duration::from_secs(5)).await;

    let mut output = Vec::new();
    {
        let mut store = state.server_store.lock().await;
        for (i, result) in results {
            let id = targets[i].0;
            let ms = result.ok();
            if let Some(entry) = store.iter_mut().find(|s| s.config.id == id) {
                entry.ping_ms = ms;
                entry.online = ms.is_some();
            }
            output.push((id.to_string(), ms));
        }
    }
    state.persist_servers().await;
    Ok(output)
}

#[tauri::command]
pub async fn toggle_favorite(
    state: State<'_, ManagedState>,
    server_id: String,
) -> Result<bool, String> {
    let id: uuid::Uuid = server_id.parse().map_err(|e: uuid::Error| e.to_string())?;
    let fav = {
        let mut store = state.server_store.lock().await;
        let entry = store
            .iter_mut()
            .find(|s| s.config.id == id)
            .ok_or("server not found")?;
        entry.favorite = !entry.favorite;
        entry.favorite
    };
    state.persist_servers().await;
    Ok(fav)
}
