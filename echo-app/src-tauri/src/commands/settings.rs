use tauri::State;

use crate::{
    error::Result,
    state::AppState,
    storage::repositories,
};

#[tauri::command]
pub fn get_setting(state: State<'_, AppState>, key: String) -> Result<Option<String>> {
    let conn = state.db.lock().unwrap();
    repositories::get_setting(&conn, &key)
}

#[tauri::command]
pub fn set_setting(state: State<'_, AppState>, key: String, value: String) -> Result<()> {
    let conn = state.db.lock().unwrap();
    repositories::set_setting(&conn, &key, &value)
}
