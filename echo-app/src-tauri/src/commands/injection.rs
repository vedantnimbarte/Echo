use tauri::State;

use crate::{error::Result, state::AppState};

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

/// Type `text` into the focused application. Used by the History panel to
/// re-insert a past transcript and by onboarding to test text output.
#[tauri::command]
pub async fn inject_text(state: State<'_, AppState>, text: String) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let injector = state.injector.clone();
    tokio::task::spawn_blocking(move || injector.inject_text(&text))
        .await
        .map_err(|e| crate::error::EchoError::Plugin(e.to_string()))?
}
