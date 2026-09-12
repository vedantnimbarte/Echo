use tauri::State;

use crate::{
    error::Result,
    state::AppState,
    storage::{models::TranscriptionRecord, repositories},
};

#[tauri::command]
pub fn get_history(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<TranscriptionRecord>> {
    let conn = state.db.lock().unwrap();
    repositories::list_history(&conn, limit.unwrap_or(100))
}

/// What dictation has added up to, for the stats card.
///
/// Derived from History, so it is empty when History is off — which is the
/// honest outcome: there is nothing to count, rather than a number invented
/// from somewhere else.
#[tauri::command]
pub fn get_dictation_stats(
    state: State<'_, AppState>,
) -> Result<crate::storage::repositories::DictationStats> {
    let conn = state.db.lock().unwrap();
    crate::storage::repositories::dictation_stats(&conn)
}

/// The Insights page, in one call.
///
/// Same source and the same caveat as [`get_dictation_stats`]: it is History,
/// so it is empty when History is off.
#[tauri::command]
pub fn get_insights(state: State<'_, AppState>) -> Result<crate::storage::repositories::Insights> {
    let conn = state.db.lock().unwrap();
    crate::storage::repositories::insights(&conn)
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
