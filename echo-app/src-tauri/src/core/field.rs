//! What the focused *field* is, not just which app owns it.
//!
//! Until now Echo knew the application and nothing inside it, which means it
//! would happily type a transcript into a password box — and, with live text
//! on, type the partials there too, one revision at a time. Nothing in the app
//! stopped it, and the transcript was written to History on the way past.
//!
//! The accessibility APIs can answer one question well: **is the focused
//! control a secure text field?** Windows UI Automation exposes `IsPassword`,
//! macOS exposes the `AXSecureTextField` subrole, and both cover browsers and
//! Electron apps, which is where password fields actually live.
//!
//! **Linux answers through AT-SPI, when it can.** AT-SPI is the accessibility
//! protocol screen readers use, carried on its own D-Bus bus. It has no "what
//! has focus?" call, so a thread started with the app ([`start`]) listens for
//! `object:state-changed:focused` and remembers the last control that gained
//! focus; [`focused_field`] then asks that control for its role. GTK 3
//! (`GtkEntry` with visibility off), GTK 4 (input purpose password or PIN), Qt
//! (`EditableText` with `passwordEdit`) and Chromium/Electron (a text field
//! with the protected state) all publish the same thing: `ROLE_PASSWORD_TEXT`.
//! AT-SPI has no "protected" *state* — that exists inside Chromium and is
//! folded into the role before it reaches the bus — so the role is the only
//! thing worth reading.
//!
//! That is weaker than Windows and macOS in three ways, and [`detection`]
//! reports which of them apply on this machine rather than claiming parity:
//!
//! - **No accessibility bus, no answer.** Minimal window managers and some
//!   Wayland sessions never start `at-spi-bus-launcher`. Echo logs one warning
//!   and stays `Unknown` for the rest of the run.
//! - **Most toolkits publish nothing unless asked.** Chromium and Electron
//!   read `org.a11y.Status.IsEnabled` once at launch; Qt watches it (or
//!   `ScreenReaderEnabled`). With it off, only GTK apps answer. Echo *reads*
//!   the flag and does not set it: it is a session-wide switch that makes every
//!   running Qt app, and every browser started afterwards, build and maintain
//!   an accessibility tree it otherwise would not — a real cost in memory and
//!   CPU on every keystroke and DOM change, paid by apps that have nothing to
//!   do with dictation, and on by default because the guard is. Nor does it
//!   end with Echo: `at-spi-bus-launcher` writes the property straight back to
//!   GNOME's `toolkit-accessibility` setting, so it persists across logins.
//!   And it would not reach a browser already open. So Settings says "GTK apps
//!   only" and names the switch, and the user decides.
//! - **Focus is only as fresh as the last event.** An app that publishes no
//!   tree (a terminal, a game, an X11 client with no bridge) sends no focus
//!   event, so the remembered control is still the previous one. It is asked
//!   whether it still holds focus, and when it does not the answer is
//!   `Unknown`, never a stale `Secure` or `Plain`.
//!
//! ponytail: `Unknown` is treated as "not secure" on purpose. Refusing to type
//! whenever the OS declines to answer would break dictation in every app on
//! any platform that publishes no accessibility tree — which is a much larger
//! blast radius than the case being guarded.

use serde::Serialize;

/// What the OS says about the control holding keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// A password or other masked entry. Never type into one.
    Secure,
    /// An ordinary text field.
    Plain,
    /// The OS did not say — no accessibility tree, no permission, or a
    /// platform with no way to ask.
    Unknown,
}

impl FieldKind {
    /// Whether typing into this field would put text somewhere it must not go.
    pub fn is_secure(&self) -> bool {
        matches!(self, FieldKind::Secure)
    }
}

/// How much of the desktop the guard can see, on this machine, right now.
///
/// Surfaced in Settings beside the toggle, because the failure it describes is
/// silent: the guard is on, and where detection is missing it never fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Detection {
    /// Every app that publishes an accessibility tree can be asked.
    Available,
    /// Linux with accessibility switched off for the session: GTK apps answer,
    /// Chromium, Electron and Qt apps publish nothing.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Partial,
    /// Nothing can be asked — no accessibility bus, or no API on this platform.
    Unavailable,
}

/// Whether, and how widely, this machine can identify the focused field.
///
/// A compile-time answer on Windows and macOS. On Linux it depends on whether
/// the accessibility bus was reachable at startup and whether the session has
/// accessibility switched on, so it is asked each time — blocking, briefly,
/// over D-Bus. Keep it off the async runtime.
pub fn detection() -> Detection {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    return Detection::Available;

    #[cfg(target_os = "linux")]
    return linux_impl::detection();

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    Detection::Unavailable
}

/// Begin whatever background work detection needs. Called once at startup;
/// returns immediately.
///
/// Only Linux has any: AT-SPI reports focus as events, so a listener has to be
/// running *before* the user clicks into a password box, not started by the
/// first dictation that needs the answer.
pub fn start() {
    #[cfg(target_os = "linux")]
    linux_impl::start();
}

/// The focused control's kind.
///
/// Blocking, and on Windows it talks to COM — never call it from the async
/// runtime.
pub fn focused_field() -> FieldKind {
    #[cfg(target_os = "windows")]
    return windows_impl::focused_field();

    #[cfg(target_os = "macos")]
    return macos_impl::focused_field();

    #[cfg(target_os = "linux")]
    return linux_impl::focused_field();

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    FieldKind::Unknown
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::FieldKind;

    use windows::core::Interface;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};

    /// Ask UI Automation whether the focused element is a password field.
    ///
    /// UIA rather than the Win32 window style: `ES_PASSWORD` only describes
    /// native edit controls, and the password boxes people actually use are in
    /// browsers and Electron apps, which draw their own. UIA sees both.
    pub fn focused_field() -> FieldKind {
        unsafe {
            // Each call initializes the thread it runs on. `spawn_blocking`
            // hands out threads from a pool, so the same thread may already be
            // initialized — that returns an error which is not a failure, and
            // the result is deliberately ignored rather than checked.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let Ok(automation) =
                CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            else {
                return FieldKind::Unknown;
            };
            let Ok(element) = automation.GetFocusedElement() else {
                return FieldKind::Unknown;
            };
            // Elements that answer at all answer this; the ones that do not
            // have no accessibility tree to consult.
            match element.CurrentIsPassword() {
                Ok(is_password) if is_password.as_bool() => FieldKind::Secure,
                Ok(_) => FieldKind::Plain,
                Err(_) => FieldKind::Unknown,
            }
        }
    }

    // `Interface` is imported for the `CoCreateInstance` type parameter's
    // bound; naming it here keeps the import from reading as unused.
    const _: fn() = || {
        fn assert_interface<T: Interface>() {}
        assert_interface::<IUIAutomation>();
    };
}

#[cfg(target_os = "macos")]
mod macos_impl {
    use super::FieldKind;

    use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};

    // The Accessibility API. `AXUIElementRef` is an opaque CFType, so it is
    // handled as `CFTypeRef` rather than given a newtype that would only exist
    // to be cast away again.
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> CFTypeRef;
        fn AXUIElementCopyAttributeValue(
            element: CFTypeRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
    }

    /// `kAXErrorSuccess`.
    const AX_SUCCESS: i32 = 0;

    /// The subrole macOS gives a masked text field.
    const SECURE_SUBROLE: &str = "AXSecureTextField";

    pub fn focused_field() -> FieldKind {
        // Without Accessibility permission every query fails, and Echo cannot
        // type either — so the answer is the honest one rather than a guess.
        if !super::super::super::platform::macos::is_accessibility_trusted() {
            return FieldKind::Unknown;
        }

        unsafe {
            let system = AXUIElementCreateSystemWide();
            if system.is_null() {
                return FieldKind::Unknown;
            }

            let focused = copy_attribute(system, "AXFocusedUIElement");
            CFRelease(system);

            let Some(focused) = focused else {
                return FieldKind::Unknown;
            };

            let subrole = copy_attribute(focused, "AXSubrole");
            CFRelease(focused);

            match subrole {
                Some(value) => {
                    let string = CFString::wrap_under_create_rule(value as CFStringRef);
                    if string.to_string() == SECURE_SUBROLE {
                        FieldKind::Secure
                    } else {
                        FieldKind::Plain
                    }
                }
                // A field with no subrole is an ordinary one; a control that
                // answered nothing at all is indistinguishable from it here,
                // which is why the caller treats Plain as "not proven secure"
                // rather than "proven safe".
                None => FieldKind::Plain,
            }
        }
    }

    /// Read one accessibility attribute, returning an owned value the caller
    /// must release.
    unsafe fn copy_attribute(element: CFTypeRef, name: &str) -> Option<CFTypeRef> {
        let attribute = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        let status =
            AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value);
        (status == AX_SUCCESS && !value.is_null()).then_some(value)
    }
}

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::{Detection, FieldKind};

    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use zbus::blocking::{connection, Connection, MessageIterator};
    use zbus::zvariant::{OwnedObjectPath, OwnedValue};

    /// `ATSPI_ROLE_PASSWORD_TEXT`. The numbering is the wire protocol, fixed
    /// since AT-SPI 2.0.
    const ROLE_PASSWORD_TEXT: u32 = 40;
    /// `ATSPI_STATE_FOCUSED`: a bit index into the first word of `GetState`.
    const STATE_FOCUSED: u32 = 12;

    /// An app that has hung, or is stopped in a debugger, must not hold up
    /// dictation. Every answer here is a local round trip that takes a
    /// millisecond or two; half a second is already a wrong one.
    const CALL_TIMEOUT: Duration = Duration::from_millis(500);

    /// The session bus (for the enablement flag) and the accessibility bus
    /// (for everything else). Set once the listener is up; absent forever when
    /// it could not start, which is how "no bus" reaches [`detection`].
    static BUSES: OnceLock<(Connection, Connection)> = OnceLock::new();

    /// The last control that announced it gained focus: its owner's unique bus
    /// name and object path.
    static FOCUS: Mutex<Option<(String, OwnedObjectPath)>> = Mutex::new(None);

    /// The mapping from what AT-SPI says to what the guard acts on, kept free
    /// of D-Bus so it can be tested without a desktop.
    ///
    /// A control that no longer holds focus is `Unknown`, whatever its role:
    /// focus has moved somewhere that sent no event, so this control is not
    /// where the text would go.
    pub(super) fn classify(role: u32, states: &[u32]) -> FieldKind {
        let focused = states
            .first()
            .is_some_and(|word| word & (1 << STATE_FOCUSED) != 0);
        match (focused, role) {
            (false, _) => FieldKind::Unknown,
            (true, ROLE_PASSWORD_TEXT) => FieldKind::Secure,
            (true, _) => FieldKind::Plain,
        }
    }

    pub fn start() {
        // A thread of its own rather than a task: the loop below blocks on the
        // bus for the life of the app, and zbus's blocking API brings its own
        // executor, so this needs nothing from Tauri's runtime and cannot
        // stall it.
        let spawned = std::thread::Builder::new()
            .name("echo-atspi-focus".into())
            .spawn(|| {
                if let Err(e) = listen() {
                    // The only log line this module writes. Without a bus the
                    // guard is blind for the whole run, which Settings shows;
                    // repeating it per dictation would say nothing new.
                    tracing::warn!(
                        "Accessibility bus unavailable, password fields cannot be detected: {e}"
                    );
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("Could not start the accessibility focus listener: {e}");
        }
    }

    fn listen() -> zbus::Result<()> {
        let session = connection::Builder::session()?
            .method_timeout(CALL_TIMEOUT)
            .build()?;
        // The accessibility bus is a separate bus whose address the session
        // bus hands out. Asking may start `at-spi-bus-launcher` through D-Bus
        // activation — exactly what the first GTK app of the session does
        // anyway, and it changes no setting.
        let address: String = session
            .call_method(
                Some("org.a11y.Bus"),
                "/org/a11y/bus",
                Some("org.a11y.Bus"),
                "GetAddress",
                &(),
            )?
            .body()
            .deserialize()?;
        let a11y = connection::Builder::address(address.as_str())?
            .method_timeout(CALL_TIMEOUT)
            .build()?;

        let rule = "type='signal',interface='org.a11y.atspi.Event.Object',\
                    member='StateChanged',arg0='focused'";
        let events = MessageIterator::for_match_rule(rule, &a11y, None)?;
        // A match rule alone is not enough: toolkits consult the registry's
        // listener list and skip emitting events nobody registered for. The
        // registration is dropped by the registry when this connection closes.
        a11y.call_method(
            Some("org.a11y.atspi.Registry"),
            "/org/a11y/atspi/registry",
            Some("org.a11y.atspi.Registry"),
            "RegisterEvent",
            &("Object:StateChanged:focused",),
        )?;

        let _ = BUSES.set((session, a11y));

        for message in events {
            let Ok(message) = message else { continue };
            let header = message.header();
            let (Some(sender), Some(path)) = (header.sender(), header.path()) else {
                continue;
            };
            // `(kind, detail1, detail2, any_data, properties)`. Only gaining
            // focus is recorded; losing it is caught when the control is asked.
            let Ok((_, gained, _, _, _)) =
                message
                    .body()
                    .deserialize::<(String, i32, i32, OwnedValue, HashMap<String, OwnedValue>)>()
            else {
                continue;
            };
            if gained == 1 {
                *FOCUS.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((sender.to_string(), path.clone().into()));
            }
        }
        Err(zbus::Error::Failure(
            "the accessibility bus closed the connection".into(),
        ))
    }

    pub fn focused_field() -> FieldKind {
        let Some((_, a11y)) = BUSES.get() else {
            return FieldKind::Unknown;
        };
        let Some((owner, path)) = FOCUS.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
            return FieldKind::Unknown;
        };
        let ask = |method: &str| {
            a11y.call_method(
                Some(owner.as_str()),
                &path,
                Some("org.a11y.atspi.Accessible"),
                method,
                &(),
            )
        };
        // Any failure — the app quit, hung past the timeout, or the object is
        // gone — is an app that did not answer, not a field that is safe.
        let role = ask("GetRole").and_then(|m| m.body().deserialize::<u32>());
        let states = ask("GetState").and_then(|m| m.body().deserialize::<Vec<u32>>());
        match (role, states) {
            (Ok(role), Ok(states)) => classify(role, &states),
            _ => FieldKind::Unknown,
        }
    }

    pub fn detection() -> Detection {
        let Some((session, _)) = BUSES.get() else {
            return Detection::Unavailable;
        };
        // Only `IsEnabled`. Qt also accepts `ScreenReaderEnabled`, but
        // Chromium does not, and a guard that misses the browser is the
        // partial one whatever Qt does. Read every time rather than cached:
        // the user may flip it after reading the hint.
        let enabled = session
            .call_method(
                Some("org.a11y.Bus"),
                "/org/a11y/bus",
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.a11y.Status", "IsEnabled"),
            )
            .and_then(|m| m.body().deserialize::<OwnedValue>())
            .ok()
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false);
        if enabled {
            Detection::Available
        } else {
            Detection::Partial
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard's contract: only a field the OS positively identified as
    /// secure blocks typing. Unknown must not, or dictation stops working
    /// everywhere the accessibility tree is absent.
    #[test]
    fn only_a_confirmed_secure_field_blocks_typing() {
        assert!(FieldKind::Secure.is_secure());
        assert!(!FieldKind::Plain.is_secure());
        assert!(!FieldKind::Unknown.is_secure());
    }

    /// Pins the platform matrix so nobody has to read four cfg blocks to find
    /// out where the protection actually applies. On Linux the listener is
    /// never started under test, so the answer is the no-bus one.
    #[test]
    fn detection_is_claimed_only_where_it_exists() {
        let expected = if cfg!(any(target_os = "windows", target_os = "macos")) {
            Detection::Available
        } else {
            Detection::Unavailable
        };
        assert_eq!(detection(), expected);
    }

    /// On a platform that cannot ask — or a Linux whose listener never came
    /// up — the answer must be Unknown rather than a cheerful Plain; the
    /// difference is what the UI tells the user.
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    #[test]
    fn a_platform_that_cannot_ask_says_so() {
        assert_eq!(focused_field(), FieldKind::Unknown);
    }

    /// The Settings hint switches on these exact strings.
    #[test]
    fn detection_serializes_as_the_frontend_expects() {
        let json = serde_json::to_string(&[
            Detection::Available,
            Detection::Partial,
            Detection::Unavailable,
        ])
        .unwrap();
        assert_eq!(json, r#"["available","partial","unavailable"]"#);
    }

    #[cfg(target_os = "linux")]
    mod atspi {
        use super::super::linux_impl::classify;
        use super::super::FieldKind;

        /// `ATSPI_STATE_FOCUSED` set, plus the enabled/sensitive noise a real
        /// entry carries alongside it.
        const FOCUSED: [u32; 2] = [(1 << 12) | (1 << 8) | (1 << 24), 0];

        #[test]
        fn a_focused_password_role_is_secure() {
            assert_eq!(classify(40, &FOCUSED), FieldKind::Secure);
        }

        /// `ROLE_TEXT` (61) and `ROLE_ENTRY` (79): what GTK and Chromium give
        /// an ordinary field.
        #[test]
        fn a_focused_ordinary_field_is_plain() {
            assert_eq!(classify(61, &FOCUSED), FieldKind::Plain);
            assert_eq!(classify(79, &FOCUSED), FieldKind::Plain);
        }

        /// Focus moved to something that sent no event. The remembered control
        /// — password box or not — is not where the text would land.
        #[test]
        fn a_control_that_lost_focus_is_unknown() {
            let unfocused = [FOCUSED[0] & !(1 << 12), 0];
            assert_eq!(classify(40, &unfocused), FieldKind::Unknown);
            assert_eq!(classify(61, &unfocused), FieldKind::Unknown);
            assert_eq!(classify(40, &[]), FieldKind::Unknown);
        }

        /// Against a real desktop: start the listener, then focus a field
        /// within 30 seconds. `ECHO_LIVE_EXPECT=plain` for an ordinary one.
        ///
        /// ```sh
        /// cargo test --lib field::tests::atspi::live -- --ignored --nocapture &
        /// sleep 3; zenity --password
        /// ```
        #[test]
        #[ignore = "needs a desktop session with an accessibility bus"]
        fn live() {
            let expected = match std::env::var("ECHO_LIVE_EXPECT").as_deref() {
                Ok("plain") => FieldKind::Plain,
                _ => FieldKind::Secure,
            };
            super::super::start();
            let mut seen = FieldKind::Unknown;
            for _ in 0..300 {
                seen = super::super::focused_field();
                if seen == expected {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            println!("detection: {:?}", super::super::detection());
            assert_eq!(seen, expected);
        }
    }
}
