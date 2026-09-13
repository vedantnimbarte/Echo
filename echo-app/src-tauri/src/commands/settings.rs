use tauri::State;

use crate::{error::Result, state::AppState, storage::repositories};

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

/// Language codes that number conversion has a parser for. Same reason as
/// [`spoken_punctuation_languages`]: which languages a stage covers is a fact to
/// read, not one to discover by being ignored.
#[tauri::command]
pub fn number_languages() -> Vec<&'static str> {
    crate::core::format::numbers::supported_languages()
}

/// Language codes that filler and stutter cleanup has rules for. Same reason
/// as [`spoken_punctuation_languages`].
#[tauri::command]
pub fn cleanup_languages() -> Vec<&'static str> {
    crate::core::format::cleanup::supported_languages()
}

/// The dictation languages the settings `<select>` and the tray submenu both
/// render — one list, so the two cannot drift.
#[tauri::command]
pub fn dictation_languages() -> &'static [crate::core::asr::languages::Language] {
    crate::core::asr::languages::LANGUAGES
}

#[tauri::command]
pub fn set_setting(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<()> {
    // Scoped: `tray::refresh` reads the same settings back, and the lock is
    // not reentrant.
    {
        let conn = state.db.lock().unwrap();
        repositories::set_setting(&conn, &key, &value)?;
    }
    // The tray shows a tick beside the current language and microphone, so a
    // change made in this window has to reach it.
    if key == "language" || key == "audio_device" {
        crate::tray::refresh(&app);
    }
    Ok(())
}
