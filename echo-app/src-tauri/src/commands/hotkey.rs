use tauri::{AppHandle, State};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::{
    error::{EchoError, Result},
    state::AppState,
    storage::repositories,
};

/// Default global hotkey used when none is configured.
pub const DEFAULT_HOTKEY: &str = "CommandOrControl+Shift+Space";

/// The currently configured global hotkey (or the default).
#[tauri::command]
pub fn get_hotkey(state: State<'_, AppState>) -> Result<String> {
    let conn = state.db.lock().unwrap();
    Ok(repositories::get_setting(&conn, "hotkey")?.unwrap_or_else(|| DEFAULT_HOTKEY.to_string()))
}

/// Replace the registered global hotkey and persist it.
#[tauri::command]
pub fn register_hotkey(
    app: AppHandle,
    state: State<'_, AppState>,
    shortcut: String,
) -> Result<()> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.register(shortcut.as_str())
        .map_err(|e| EchoError::Config(format!("Invalid shortcut '{shortcut}': {e}")))?;

    let conn = state.db.lock().unwrap();
    repositories::set_setting(&conn, "hotkey", &shortcut)?;
    Ok(())
}
