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

/// Whether this platform can tell a password field from an ordinary one.
///
/// Surfaced next to the guard's toggle: on Linux the answer is no, and a user
/// who believes otherwise is worse off than one who knows.
#[tauri::command]
pub fn secure_field_detection() -> bool {
    crate::core::field::detection_available()
}

/// Type `text` into the focused application. Used by the History panel to
/// re-insert a past transcript and by onboarding to test text output.
#[tauri::command]
pub async fn inject_text(state: State<'_, AppState>, text: String) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let (method, settle_ms) = {
        let conn = state.db.lock().unwrap();
        let get = |k: &str| crate::storage::repositories::get_setting(&conn, k).unwrap_or(None);
        (
            get("injection_method"),
            get("clipboard_settle_ms")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(crate::core::injection::DEFAULT_SETTLE_MS),
        )
    };
    let injector = state.injector.clone();
    tokio::task::spawn_blocking(move || {
        let use_paste = crate::core::injection::use_paste_for(method.as_deref(), &text);
        crate::core::injection::deliver(injector.as_ref(), &text, use_paste, settle_ms)
    })
    .await
    .map_err(|e| crate::error::EchoError::Plugin(e.to_string()))?
}
