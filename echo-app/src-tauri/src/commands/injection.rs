/// Whether the OS grants this app permission to synthesize keystrokes.
///
/// On macOS this reflects the Accessibility permission; other platforms don't
/// gate keyboard injection, so they always return `true`.
#[tauri::command]
pub fn check_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::platform::macos::is_accessibility_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}
