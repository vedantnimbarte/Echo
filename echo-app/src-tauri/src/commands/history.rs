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
