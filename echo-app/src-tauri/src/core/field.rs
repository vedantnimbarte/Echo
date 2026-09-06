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
//! **Linux cannot answer, and says so.** AT-SPI would need a D-Bus dependency
//! and a toolkit that chose to publish the tree, and under Wayland often
//! neither holds. Rather than a guard that silently protects nobody,
//! [`detection_available`] reports the truth per platform and the UI states it
//! plainly. A protection you wrongly believe in is worse than none.
//!
//! ponytail: `Unknown` is treated as "not secure" on purpose. Refusing to type
//! whenever the OS declines to answer would break dictation everywhere on
//! Linux, and in every app on Windows and macOS that publishes no
//! accessibility tree — which is a much larger blast radius than the case being
//! guarded.

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

/// Whether this build can determine the focused field at all.
///
/// Surfaced in Settings beside the toggle, because the failure it describes is
/// silent: the guard is on, and on Linux it never fires.
pub const fn detection_available() -> bool {
    cfg!(any(target_os = "windows", target_os = "macos"))
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

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
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

    /// Pins the platform matrix so nobody has to read three cfg blocks to find
    /// out where the protection actually applies.
    #[test]
    fn detection_is_claimed_only_where_it_exists() {
        assert_eq!(
            detection_available(),
            cfg!(any(target_os = "windows", target_os = "macos"))
        );
    }

    /// On a platform that cannot ask, the answer must be Unknown rather than a
    /// cheerful Plain — the difference is what the UI tells the user.
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    #[test]
    fn a_platform_that_cannot_ask_says_so() {
        assert_eq!(focused_field(), FieldKind::Unknown);
    }
}
