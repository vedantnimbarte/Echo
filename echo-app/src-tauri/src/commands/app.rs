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
