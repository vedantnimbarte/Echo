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

/// Language codes that spoken punctuation actually has rules for.
///
/// Surfaced in Settings so the list is a fact the user can read, rather than
/// something they discover by dictating "coma" and being ignored.
#[tauri::command]
pub fn spoken_punctuation_languages() -> Vec<&'static str> {
    crate::core::format::punctuation::supported_languages()
}

#[tauri::command]
pub fn set_setting(state: State<'_, AppState>, key: String, value: String) -> Result<()> {
    let conn = state.db.lock().unwrap();
    repositories::set_setting(&conn, &key, &value)
}
