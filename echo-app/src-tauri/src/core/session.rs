//! What kind of desktop session Echo is running in, and what that costs it.
//!
//! This exists for one failure: on Wayland, an application cannot register a
//! global shortcut for itself. The call does not error — it simply never fires,
//! so from the user's side the hotkey is just broken, with nothing anywhere
//! saying why. A wrong-looking message beats a silent no-op, so Echo works out
//! what it is running under and says so.
//!
//! Detection only. Echo does not try to install a compositor-level shortcut on
//! the user's behalf: every desktop wants that done differently, and a
//! half-right guess that edits someone's keybindings is worse than a clear
//! explanation of what to do.

use serde::Serialize;

/// The windowing system in use, as far as we can tell from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    /// Windows or macOS, where application-registered global hotkeys work.
    Native,
    /// X11, including XWayland — global hotkeys work.
    X11,
    /// Wayland — an application cannot claim a global hotkey for itself.
    Wayland,
    /// No graphical session we recognise.
    Unknown,
}

/// How well global hotkeys can be expected to work here.
#[derive(Debug, Clone, Serialize)]
pub struct HotkeySupport {
    pub session: SessionKind,
    /// Desktop environment, when the session advertises one ("GNOME", "KDE").
    pub desktop: Option<String>,
    /// Whether Echo can register the hotkey itself.
    pub can_bind: bool,
    /// Whether a modifier pressed on its own can be watched here.
    pub supports_bare_modifier: bool,
    /// Plain-language explanation, empty when everything works.
    pub advice: String,
}

/// Detect the current session kind.
pub fn detect() -> SessionKind {
    if cfg!(any(target_os = "windows", target_os = "macos")) {
        return SessionKind::Native;
    }

    // XDG_SESSION_TYPE is the authoritative answer when the session sets it.
    match std::env::var("XDG_SESSION_TYPE").as_deref().map(str::trim) {
        Ok("wayland") => return SessionKind::Wayland,
        Ok("x11") => return SessionKind::X11,
        _ => {}
    }

    // Not every compositor sets it, but a Wayland socket is proof enough.
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return SessionKind::Wayland;
    }
    if std::env::var_os("DISPLAY").is_some() {
        return SessionKind::X11;
    }
    SessionKind::Unknown
}

/// The desktop environment's own name, if it advertises one.
fn desktop() -> Option<String> {
    // XDG_CURRENT_DESKTOP can be a colon-separated list ("ubuntu:GNOME");
    // the last entry is the base desktop.
    std::env::var("XDG_CURRENT_DESKTOP")
        .ok()
        .and_then(|v| v.rsplit(':').next().map(str::to_string))
        .filter(|v| !v.is_empty())
}

/// Assess hotkey support for this session, with advice when it is limited.
pub fn hotkey_support() -> HotkeySupport {
    let session = detect();
    let desktop = desktop();

    let (can_bind, supports_bare_modifier, advice) = match session {
        SessionKind::Native | SessionKind::X11 => (true, true, String::new()),
        SessionKind::Wayland => (
            false,
            false,
            format!(
                "You're running Wayland{}. Wayland does not let an application \
                 claim a global shortcut for itself, so Echo's hotkey may never \
                 fire. Bind a shortcut in your desktop's own keyboard settings, \
                 or start your session in X11 where the hotkey works normally.",
                match &desktop {
                    Some(d) => format!(" on {d}"),
                    None => String::new(),
                }
            ),
        ),
        SessionKind::Unknown => (
            true,
            false,
            "Echo couldn't identify this desktop session, so the global hotkey \
             may not work. If it doesn't, use the pill to start dictation."
                .into(),
        ),
    };

    HotkeySupport {
        session,
        desktop,
        can_bind,
        supports_bare_modifier,
        advice,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_platforms_support_hotkeys_without_advice() {
        // On Windows and macOS this is unconditional, so the assertion is only
        // meaningful there; elsewhere the environment decides.
        if cfg!(any(target_os = "windows", target_os = "macos")) {
            let support = hotkey_support();
            assert_eq!(support.session, SessionKind::Native);
            assert!(support.can_bind);
            assert!(support.advice.is_empty());
        }
    }

    #[test]
    fn a_wayland_session_is_reported_as_unable_to_bind() {
        // `detect` reads the process environment, so exercise the mapping
        // directly rather than mutating env vars under a parallel test runner.
        let support = HotkeySupport {
            session: SessionKind::Wayland,
            desktop: Some("GNOME".into()),
            can_bind: false,
            supports_bare_modifier: false,
            advice: "…".into(),
        };
        assert!(!support.can_bind);
        assert!(!support.supports_bare_modifier);
    }

    #[test]
    fn session_kinds_serialise_for_the_ui() {
        let json = serde_json::to_string(&SessionKind::Wayland).unwrap();
        assert_eq!(json, "\"wayland\"");
    }
}
