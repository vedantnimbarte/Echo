use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

use crate::error::{EchoError, Result};
use crate::state::AppState;
use crate::storage::repositories;

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

/// The logged-in account's given name, for the greeting on the Dictation page.
///
/// Echo has no account of its own and asks for nothing, so this is the only
/// name available without inventing a settings field nobody wants to fill in.
/// The OS is the source: `USERNAME` on Windows, `USER` elsewhere.
///
/// `None` rather than a guess when the variable is missing, or holds something
/// that is plainly a machine account rather than a person — a greeting that
/// says "Hey root" or "Hey Administrator" is worse than one with no name in it.
/// The frontend drops the name and greets anyway.
#[tauri::command]
pub fn account_name() -> Option<String> {
    const NOT_PEOPLE: [&str; 6] = ["root", "administrator", "admin", "user", "guest", "default"];

    let raw = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()?;

    // "Vedant Nimbarte" and "vedant.nimbarte" both answer to "Vedant"; a
    // domain login ("CORP\vedant") keeps only what follows the backslash.
    let first = raw
        .rsplit('\\')
        .next()?
        .split(['.', '_', '-', ' '])
        .next()?
        .trim();

    if first.is_empty() || NOT_PEOPLE.contains(&first.to_lowercase().as_str()) {
        return None;
    }

    // Titlecased, because a login is usually lowercase and a greeting is not.
    let mut chars = first.chars();
    let head = chars.next()?.to_uppercase().to_string();
    Some(head + chars.as_str())
}

/// Show `echo.log` in the system file manager.
///
/// `init_tracing` calls that file "the only record a user can actually send
/// us", and until this there was no way to reach it from inside Echo — while
/// the bug report form asks for exactly that. Reveals the file rather than
/// opening it, because the thing people need is to attach it, and because a
/// long-running install's log is not something to hand to a text editor
/// unasked.
///
/// Falls back to the folder when the log is missing, which is what a run that
/// could not open it looks like (see `init_tracing`: logging is best effort).
#[tauri::command]
pub fn open_log(app: AppHandle) -> Result<()> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| EchoError::Config(format!("no application data directory: {e}")))?;
    let log = dir.join("echo.log");
    let target = if log.exists() { log } else { dir };
    app.opener()
        .reveal_item_in_dir(&target)
        .map_err(|e| EchoError::Config(format!("could not show {}: {e}", target.display())))
}

/// The facts a bug report needs, as the markdown block Echo pastes into one.
///
/// Built here rather than in the frontend because every line of it already
/// lives on this side — the settings table, the binary manager, the session
/// probe — and gathering it there would mean four round-trips and a second
/// place that has to know what "active" means for the GPU.
///
/// Deliberately small and readable: it is shown to the user in an editable box
/// before it goes anywhere, and a wall of JSON is not something anyone reads
/// before agreeing to publish it. Nothing here identifies the machine or the
/// person — no hostname, no username, no paths, no API keys.
///
/// ponytail: OS is the compile-time target, not the running build (no
/// "Windows 11 26100"). Add `os_info` if a report ever turns on a point
/// release.
#[tauri::command]
pub fn diagnostics(state: State<'_, AppState>) -> String {
    let (provider, model, language, vad) = {
        let conn = state.db.lock().unwrap();
        let get = |k: &str| repositories::get_setting(&conn, k).ok().flatten();
        (
            get("asr_provider").unwrap_or_else(|| "local".into()),
            get("whisper_model").unwrap_or_else(|| "none".into()),
            get("language").unwrap_or_else(|| "auto".into()),
            get("vad_engine").unwrap_or_else(|| "silero".into()),
        )
    };
    // What will actually run, not what was asked for: `start_recording` uses
    // energy whenever the model is absent, however the setting reads.
    let vad = if vad == "energy" || state.silero.is_none() {
        "energy"
    } else {
        "silero"
    };

    let hotkey = crate::core::session::hotkey_support();
    // Last: it consumes the state guard.
    let gpu = crate::commands::asr::gpu_status(state);

    let mut out = String::new();
    out.push_str(&format!("Echo {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!(
        "OS: {} ({})\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    out.push_str(&format!("Engine: {provider}\n"));
    out.push_str(&format!("Model: {model}\n"));
    out.push_str(&format!("Language: {language}\n"));
    out.push_str(&format!("Speech detection: {vad}\n"));
    out.push_str(&format!(
        "Acceleration: {} (detected {}, {} threads){}\n",
        if gpu.active { "on" } else { "off" },
        gpu.detected,
        gpu.threads,
        if gpu.failed { ", latched to CPU after a failure" } else { "" }
    ));
    out.push_str(&format!(
        "Session: {:?}{}, hotkey {}\n",
        hotkey.session,
        hotkey
            .desktop
            .map(|d| format!(" / {d}"))
            .unwrap_or_default(),
        if hotkey.can_bind { "bindable" } else { "NOT bindable" }
    ));
    out
}

#[cfg(test)]
mod tests {
    /// The name-shaping rules, which are the only part with decisions in them.
    /// Driving the real command would mean mutating process environment, which
    /// is unsafe and order-dependent under a parallel test runner.
    fn shape(raw: &str) -> Option<String> {
        const NOT_PEOPLE: [&str; 6] =
            ["root", "administrator", "admin", "user", "guest", "default"];
        let first = raw
            .rsplit('\\')
            .next()?
            .split(['.', '_', '-', ' '])
            .next()?
            .trim();
        if first.is_empty() || NOT_PEOPLE.contains(&first.to_lowercase().as_str()) {
            return None;
        }
        let mut chars = first.chars();
        let head = chars.next()?.to_uppercase().to_string();
        Some(head + chars.as_str())
    }

    #[test]
    fn takes_the_given_name_and_titlecases_it() {
        assert_eq!(shape("vedant").as_deref(), Some("Vedant"));
        assert_eq!(shape("vedant.nimbarte").as_deref(), Some("Vedant"));
        assert_eq!(shape("Vedant Nimbarte").as_deref(), Some("Vedant"));
        assert_eq!(shape("CORP\\vedant").as_deref(), Some("Vedant"));
    }

    #[test]
    fn refuses_machine_accounts_and_empties() {
        assert_eq!(shape("root"), None);
        assert_eq!(shape("Administrator"), None);
        assert_eq!(shape(""), None);
        assert_eq!(shape("   "), None);
    }
}
