use tauri::{AppHandle, Emitter, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

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

/// Take back the last insert. Global, because by the time you notice the
/// mistake the focus is in the app that received the text.
///
/// Alt rather than Shift: `Ctrl+Shift+Z` is *redo* in most editors, and a
/// global binding would take it away from every app on the machine.
pub const DEFAULT_UNDO_HOTKEY: &str = "CommandOrControl+Alt+Z";

/// Re-decode the last utterance on a stronger model.
pub const DEFAULT_RETRY_HOTKEY: &str = "CommandOrControl+Alt+R";

/// Stored in place of an accelerator to leave a fix-up unbound. A global
/// shortcut is taken from every other app on the machine, so being able to give
/// one back matters more here than for the dictation hotkey.
pub const UNBOUND: &str = "off";

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

/// Whether global hotkeys can work in this desktop session, and what to do if
/// they cannot.
///
/// Surfaced in Settings beside the hotkey picker, because the failure this
/// describes is silent: on Wayland the shortcut registers without complaint and
/// then never fires.
#[tauri::command]
pub fn hotkey_support() -> crate::core::session::HotkeySupport {
    crate::core::session::hotkey_support()
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

    // Rebound alongside the dictation hotkey because `unregister_all` above
    // clears them too. They are failures worth surviving: a fix-up shortcut the
    // system refuses must not stop dictation itself from binding.
    bind_fixups(app, state);

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

/// Bind the undo and retry shortcuts.
///
/// Each gets its own handler rather than routing through the shared one in
/// `lib.rs`: that handler exists to translate a press into a recording
/// transition, and these two do their work in Rust without a frontend window
/// needing to be open at all.
///
/// A shortcut the system refuses is logged and skipped. Losing undo is an
/// annoyance; refusing to start because of it would be worse.
fn bind_fixups(app: &AppHandle, state: &AppState) {
    let undo = setting(state, "undo_hotkey", DEFAULT_UNDO_HOTKEY);
    let retry = setting(state, "retry_hotkey", DEFAULT_RETRY_HOTKEY);

    for (accelerator, action) in [(undo, Fixup::Undo), (retry, Fixup::Retry)] {
        if accelerator == UNBOUND {
            continue;
        }
        let handle = app.clone();
        let result = app.global_shortcut().on_shortcut(
            accelerator.as_str(),
            move |_app, _shortcut, event| {
                // Act on the press only. Reacting to the release as well would
                // run every fix-up twice.
                if event.state != ShortcutState::Pressed {
                    return;
                }
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    match action {
                        Fixup::Undo => match crate::commands::fixup::undo_delivery(&handle).await {
                            Ok(true) => tracing::info!("Undo hotkey: last insert taken back"),
                            Ok(false) => tracing::info!("Undo hotkey: nothing to undo"),
                            Err(e) => tracing::error!("Undo hotkey failed: {e}"),
                        },
                        Fixup::Retry => {
                            if let Err(e) = crate::commands::fixup::retry_last(handle.clone()).await
                            {
                                tracing::error!("Retry hotkey failed: {e}");
                                let _ = handle.emit(
                                    "echo://error",
                                    serde_json::json!({ "message": e.to_string() }),
                                );
                            }
                        }
                    }
                });
            },
        );
        if let Err(e) = result {
            tracing::warn!("Couldn't bind {accelerator} for {action:?}: {e}");
        }
    }
}

/// Which after-the-fact correction a shortcut triggers.
#[derive(Debug, Clone, Copy)]
enum Fixup {
    Undo,
    Retry,
}

/// Rebind from what is stored. Used at startup and after a mode change.
pub fn apply(app: &AppHandle, state: &AppState) -> Result<()> {
    let accelerator = setting(state, "hotkey", DEFAULT_HOTKEY);
    let mode = setting(state, "recording_mode", DEFAULT_MODE);
    bind(app, state, &accelerator, &mode)
}

/// Change one of the fix-up shortcuts (undo or retry) and persist it.
///
/// Unlike the dictation hotkey this saves first and then rebinds everything:
/// there is no "replace one shortcut" call in the plugin, so unbinding only
/// happens by way of the `unregister_all` inside [`bind`]. A shortcut the
/// system refuses is reported by [`bind_fixups`] and leaves the rest working.
/// Pass [`UNBOUND`] to turn one off.
#[tauri::command]
pub fn set_fixup_hotkey(
    app: AppHandle,
    state: State<'_, AppState>,
    which: String,
    shortcut: String,
) -> Result<()> {
    let key = match which.as_str() {
        "undo" => "undo_hotkey",
        "retry" => "retry_hotkey",
        other => return Err(EchoError::Config(format!("Unknown fix-up '{other}'"))),
    };
    {
        let conn = state.db.lock().unwrap();
        repositories::set_setting(&conn, key, &shortcut)?;
    }
    // Rebinding everything is how a shortcut gets *un*bound: the plugin has no
    // "replace this one" call, and `unregister_all` inside `apply` is what
    // clears the old accelerator.
    apply(&app, state.inner())
}

/// The fix-up shortcuts as currently configured.
#[tauri::command]
pub fn get_fixup_hotkeys(state: State<'_, AppState>) -> (String, String) {
    (
        setting(state.inner(), "undo_hotkey", DEFAULT_UNDO_HOTKEY),
        setting(state.inner(), "retry_hotkey", DEFAULT_RETRY_HOTKEY),
    )
}

/// Replace the registered global hotkey and persist it.
///
/// Bound before it is saved, so a shortcut the system refuses leaves the
/// working one in place instead of persisting something that does nothing.
/// Whatever is already stored is honoured as-is — the picker is where new
/// bindings are vetted, and rejecting an old one here would only strand the
/// user with a hotkey they cannot change.
#[tauri::command]
pub fn register_hotkey(app: AppHandle, state: State<'_, AppState>, shortcut: String) -> Result<()> {
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
pub fn set_recording_mode(app: AppHandle, state: State<'_, AppState>, mode: String) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        repositories::set_setting(&conn, "recording_mode", &mode)?;
    }
    apply(&app, state.inner())
}
