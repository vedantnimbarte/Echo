use tauri::State;

use crate::{
    error::Result,
    state::AppState,
    storage::{models::TranscriptionRecord, repositories},
};

#[tauri::command]
pub fn get_history(state: State<'_, AppState>, limit: Option<i64>) -> Result<Vec<TranscriptionRecord>> {
    let conn = state.db.lock().unwrap();
    repositories::list_history(&conn, limit.unwrap_or(100))
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> Result<()> {
    let conn = state.db.lock().unwrap();
    repositories::clear_history(&conn)
}

/// Serialize history to a JSON file at the user-chosen path.
///
/// Exports everything, not the 100-row window the UI shows — an export the user
/// has to paginate is not an export.
#[tauri::command]
pub async fn export_history(state: State<'_, AppState>, path: String) -> Result<()> {
    let records = {
        let conn = state.db.lock().unwrap();
        repositories::list_history(&conn, i64::MAX)?
    };
    let json = serde_json::to_string_pretty(&records)?;
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| crate::error::EchoError::Config(e.to_string()))?;
    Ok(())
}
