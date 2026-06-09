use core_graphics::event::{CGEvent, CGEventTapLocation};
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
        for ch in text.chars() {
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
}
