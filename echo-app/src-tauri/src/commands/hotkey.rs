use tauri::{AppHandle, Emitter, State};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::{
    core::modtap::{Activation, ModTapWatcher, ModifierKey},
    error::{EchoError, Result},
    state::AppState,
    storage::repositories,
};

/// Default global hotkey used when none is configured.
pub const DEFAULT_HOTKEY: &str = "CommandOrControl+Shift+Space";

/// Default recording mode. Historically `"manual"`, which meant this.
pub const DEFAULT_MODE: &str = "toggle";

/// Tap the hotkey: start if idle, stop if recording.
const TOGGLE: &str = "echo://hotkey-toggle";
/// Hold the hotkey: these bracket a single utterance.
const PRESS: &str = "echo://hotkey-press";
const RELEASE: &str = "echo://hotkey-release";

/// Hold-to-talk is the only mode that needs the key's release; the others act
/// on the press alone.
fn activation_of(mode: &str) -> Activation {
    if mode == "hold" {
        Activation::Hold
    } else {
        Activation::Tap
    }
}

fn setting(state: &AppState, key: &str, fallback: &str) -> String {
    let conn = state.db.lock().unwrap();
    repositories::get_setting(&conn, key)
        .unwrap_or(None)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

/// The currently configured global hotkey (or the default).
#[tauri::command]
pub fn get_hotkey(state: State<'_, AppState>) -> Result<String> {
    Ok(setting(state.inner(), "hotkey", DEFAULT_HOTKEY))
}

/// Bind `accelerator`, using whichever mechanism can express it.
///
/// A modifier on its own cannot be registered as a system shortcut — see
/// [`crate::core::modtap`] — so those are watched directly and everything else
/// goes to the global-shortcut plugin. Only one of the two is ever live.
pub fn bind(app: &AppHandle, state: &AppState, accelerator: &str, mode: &str) -> Result<()> {
    let activation = activation_of(mode);

    let _ = app.global_shortcut().unregister_all();
    *state.modtap.lock().unwrap() = None;

    let Some(key) = ModifierKey::parse(accelerator) else {
        return app
            .global_shortcut()
            .register(accelerator)
            .map_err(|e| EchoError::Config(format!("Can't use {accelerator} as a shortcut: {e}")));
    };

    let start_app = app.clone();
    let stop_app = app.clone();
    let start_event = if activation == Activation::Hold {
        PRESS
    } else {
        TOGGLE
    };

    let watcher = ModTapWatcher::start(
        key,
        activation,
        move || {
            let _ = start_app.emit(start_event, ());
        },
        move || {
            let _ = stop_app.emit(RELEASE, ());
        },
    )
    .ok_or_else(|| {
        EchoError::Config(
            "This desktop won't report a modifier key on its own. Wayland blocks it; \
             use a key combination instead."
                .into(),
        )
    })?;

    *state.modtap.lock().unwrap() = Some(watcher);
    Ok(())
}

/// Rebind from what is stored. Used at startup and after a mode change.
pub fn apply(app: &AppHandle, state: &AppState) -> Result<()> {
    let accelerator = setting(state, "hotkey", DEFAULT_HOTKEY);
    let mode = setting(state, "recording_mode", DEFAULT_MODE);
    bind(app, state, &accelerator, &mode)
}

/// Replace the registered global hotkey and persist it.
///
/// Bound before it is saved, so a shortcut the system refuses leaves the
/// working one in place instead of persisting something that does nothing.
/// Whatever is already stored is honoured as-is — the picker is where new
/// bindings are vetted, and rejecting an old one here would only strand the
/// user with a hotkey they cannot change.
#[tauri::command]
pub fn register_hotkey(
    app: AppHandle,
    state: State<'_, AppState>,
    shortcut: String,
) -> Result<()> {
    let mode = setting(state.inner(), "recording_mode", DEFAULT_MODE);
    bind(&app, state.inner(), &shortcut, &mode)?;

    let conn = state.db.lock().unwrap();
    repositories::set_setting(&conn, "hotkey", &shortcut)?;
    Ok(())
}

/// Persist the recording mode and rebind the hotkey to match.
///
/// Hold-to-talk needs the key's release as well as its press, and for a bare
/// modifier it also needs a different rule for telling a hold from a chord — so
/// the binding is not independent of the mode.
#[tauri::command]
pub fn set_recording_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: String,
) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        repositories::set_setting(&conn, "recording_mode", &mode)?;
    }
    apply(&app, state.inner())
}
