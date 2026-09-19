use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

use crate::core::injection::TextInjector;
use crate::error::{EchoError, Result};

// AXIsProcessTrusted lives in the ApplicationServices framework and reports
// whether this process currently has Accessibility permission (required to post
// synthetic keyboard events). Returns a C `Boolean` (u8).
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

/// True if the app currently holds macOS Accessibility permission.
pub fn is_accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

pub struct MacosInjector;

impl MacosInjector {
    pub fn new() -> Self {
        Self
    }
}

impl TextInjector for MacosInjector {
    fn inject_text(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        if !is_accessibility_trusted() {
            return Err(EchoError::PermissionDenied(
                "Accessibility permission required. Grant Echo access in System Settings → \
                 Privacy & Security → Accessibility."
                    .into(),
            ));
        }

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| EchoError::Injection("Failed to create CGEventSource".into()))?;

        // Emit one keyboard event per character, attaching the character as a
        // Unicode string (virtual keycode 0). This handles arbitrary text
        // including characters with no dedicated key.
        //
        // A newline is the exception: it is a key, not a character. Posted as
        // a string it is accepted by some text views and dropped by others, so
        // a multi-line snippet lands inconsistently. Return (keycode 36) is
        // posted as a real key press instead.
        for ch in text.chars() {
            if ch == '\n' {
                for down in [true, false] {
                    let ev =
                        CGEvent::new_keyboard_event(source.clone(), 36, down).map_err(|_| {
                            EchoError::Injection("Failed to create return event".into())
                        })?;
                    ev.post(CGEventTapLocation::HID);
                }
                continue;
            }
            if ch == '\r' {
                continue;
            }
            let buf = ch.to_string();

            let down = CGEvent::new_keyboard_event(source.clone(), 0, true)
                .map_err(|_| EchoError::Injection("Failed to create key-down event".into()))?;
            down.set_string(&buf);
            down.post(CGEventTapLocation::HID);

            let up = CGEvent::new_keyboard_event(source.clone(), 0, false)
                .map_err(|_| EchoError::Injection("Failed to create key-up event".into()))?;
            up.set_string(&buf);
            up.post(CGEventTapLocation::HID);
        }

        Ok(())
    }

    fn send_paste(&self) -> Result<()> {
        // Virtual keycode 9 is ANSI 'V'.
        send_command_chord(9, "paste")
    }

    fn send_copy(&self) -> Result<()> {
        // Virtual keycode 8 is ANSI 'C'.
        send_command_chord(8, "copy")
    }

    fn send_undo(&self) -> Result<()> {
        // Virtual keycode 6 is ANSI 'Z'.
        send_command_chord(6, "undo")
    }

    fn send_backspace(&self, n: usize) -> Result<()> {
        // Virtual keycode 51 is Delete (backspace on a Mac keyboard).
        send_plain_key(51, n, "backspace")
    }
}

/// Post `keycode` down/up `n` times with no modifier flags set.
fn send_plain_key(keycode: u16, n: usize, label: &str) -> Result<()> {
    if n == 0 {
        return Ok(());
    }
    if !is_accessibility_trusted() {
        return Err(EchoError::PermissionDenied(
            "Accessibility permission required. Grant Echo access in System Settings → \
             Privacy & Security → Accessibility."
                .into(),
        ));
    }

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| EchoError::Injection("Failed to create CGEventSource".into()))?;

    for _ in 0..n {
        for down in [true, false] {
            let ev = CGEvent::new_keyboard_event(source.clone(), keycode, down)
                .map_err(|_| EchoError::Injection(format!("Failed to create {label} event")))?;
            ev.post(CGEventTapLocation::HID);
        }
    }
    Ok(())
}

/// Post Cmd+`keycode` as a down/up pair. `label` only names the shortcut in the
/// error message.
fn send_command_chord(keycode: u16, label: &str) -> Result<()> {
    if !is_accessibility_trusted() {
        return Err(EchoError::PermissionDenied(
            "Accessibility permission required. Grant Echo access in System Settings → \
             Privacy & Security → Accessibility."
                .into(),
        ));
    }

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| EchoError::Injection("Failed to create CGEventSource".into()))?;

    for down in [true, false] {
        let ev = CGEvent::new_keyboard_event(source.clone(), keycode, down)
            .map_err(|_| EchoError::Injection(format!("Failed to create {label} event")))?;
        ev.set_flags(CGEventFlags::CGEventFlagCommand);
        ev.post(CGEventTapLocation::HID);
    }
    Ok(())
}

/**
 * SOURCE OF TRUTH KEYWORDS: play_system_sound, NSSound
 * WHAT:  Plays one of macOS's named system sounds.
 * WHY:   `NSSound(named:)` resolves against /System/Library/Sounds, so these
 *        are the sounds the machine already makes and they follow the system
 *        alert volume. Nothing is bundled and nothing can be missing.
 * WHERE: core/cues.rs on macOS.
 */
#[cfg(target_os = "macos")]
pub fn play_system_sound(name: &str) -> std::io::Result<()> {
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSString;

    // Safe bindings in objc2 0.3 — no unsafe block needed, and `play()` reports
    // whether playback started rather than returning a Result. Playback is
    // asynchronous inside AppKit, so this returns immediately.
    match NSSound::soundNamed(&NSString::from_str(name)) {
        Some(sound) => {
            sound.play();
            Ok(())
        }
        None => Err(std::io::Error::other(format!(
            "no system sound named {name}"
        ))),
    }
}
