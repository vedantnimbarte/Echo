use tauri::AppHandle;

/// Quit the entire application. The floating pill has no native window chrome,
/// so the frontend needs an explicit way to exit (exposed from Settings).
#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}
