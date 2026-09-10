use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::error::{EchoError, Result};

/// Quit the entire application. The floating pill has no native window chrome,
/// so the frontend needs an explicit way to exit (exposed from Settings, and
/// from the tray menu — see [`crate::tray`]).
#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

/// Whether Echo is registered to start when the user logs in.
///
/// Read from the OS every time rather than cached in `echo.db`. The
/// registration lives outside Echo — a registry Run key, a LaunchAgent plist,
/// an XDG autostart entry — and a user who removes it with the OS's own tools
/// would otherwise see a settings toggle that lies about the state of their
/// machine.
///
/// A platform that cannot answer reports `false` rather than failing, so
/// Settings renders an honest "off" instead of an error where the feature
/// simply does not apply.
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Register or unregister Echo as a login item.
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<()> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    // Surfaced rather than swallowed: this writes outside Echo's own data
    // directory, so it is the one setting that can fail for reasons the user
    // can act on — a locked-down registry, a managed macOS profile.
    result.map_err(|e| EchoError::Config(format!("could not change launch at login: {e}")))
}
