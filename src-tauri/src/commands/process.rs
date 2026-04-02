use tauri::State;

use crate::app_state::ManagedState;
use crate::core::process_monitor::RunningProcess;

#[tauri::command]
pub async fn get_processes(
    state: State<'_, ManagedState>,
) -> Result<Vec<RunningProcess>, String> {
    let mut monitor = state.process_monitor.lock().await;
    Ok(monitor.list_processes())
}
