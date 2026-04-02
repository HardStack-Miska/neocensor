use tauri::State;

use crate::app_state::ManagedState;
use crate::core::traffic::TrafficSnapshot;

#[tauri::command]
pub async fn get_traffic_stats(
    state: State<'_, ManagedState>,
) -> Result<TrafficSnapshot, String> {
    // Lock order: app_state (3) before connections (10)
    let active = {
        let app = state.app_state.lock().await;
        app.status == crate::models::ConnectionStatus::Connected
    };
    let conns = state.connections.lock().await;
    Ok(TrafficSnapshot {
        connections: conns.clone(),
        total_connections: conns.len() as u64,
        active,
    })
}
