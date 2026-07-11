use crate::error::{EchoError, Result};

/// Platform-agnostic text injection trait.
pub trait TextInjector: Send + Sync {
    /// Type `text` into the focused app by synthesizing per-character keystrokes.
    fn inject_text(&self, text: &str) -> Result<()>;

    /// Send the OS "paste" shortcut (Cmd+V on macOS, Ctrl+V elsewhere) to the
    /// focused app. Used by [`deliver`] for clipboard-paste injection.
    fn send_paste(&self) -> Result<()>;
}

/// Returns the correct injector for the current platform.
pub fn platform_injector() -> Box<dyn TextInjector> {
    #[cfg(target_os = "windows")]
    return Box::new(crate::platform::windows::WindowsInjector::new());

    #[cfg(target_os = "macos")]
    return Box::new(crate::platform::macos::MacosInjector::new());

    #[cfg(target_os = "linux")]
    return Box::new(crate::platform::linux::LinuxInjector::new());
}

/// Deliver `text` to the focused app, choosing the mechanism:
/// - `use_paste = false` → synthesize keystrokes (universal, but slow/racy for
///   long text and blocked on some Wayland compositors).
/// - `use_paste = true` → put `text` on the clipboard, send the paste shortcut,
///   then restore the prior clipboard. Reliable for long transcripts.
pub fn deliver(inj: &dyn TextInjector, text: &str, use_paste: bool) -> Result<()> {
    if use_paste {
        paste_text(inj, text)
    } else {
        inj.inject_text(text)
    }
}

/// Clipboard-paste injection: save the current clipboard, set it to `text`,
/// send the paste shortcut, then restore the original clipboard.
fn paste_text(inj: &dyn TextInjector, text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }

    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| EchoError::Injection(format!("clipboard unavailable: {e}")))?;

    // Best-effort save of the prior clipboard so we can put it back afterwards.
    let prior = clipboard.get_text().ok();

    clipboard
        .set_text(text.to_owned())
        .map_err(|e| EchoError::Injection(format!("failed to set clipboard: {e}")))?;

    inj.send_paste()?;

    // ponytail: fixed 120ms lets the target read the clipboard before we restore
    // it. Make it a setting if some apps paste slower.
    std::thread::sleep(std::time::Duration::from_millis(120));

    if let Some(prev) = prior {
        let _ = clipboard.set_text(prev);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Default)]
    struct SpyInjector {
        typed: AtomicBool,
        pasted: AtomicBool,
    }
    impl TextInjector for SpyInjector {
        fn inject_text(&self, _: &str) -> Result<()> {
            self.typed.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn send_paste(&self) -> Result<()> {
            self.pasted.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    // `deliver(_, _, false)` types keystrokes and never sends a paste shortcut.
    // (The paste branch touches the real clipboard, so it needs device testing.)
    #[test]
    fn deliver_type_routes_to_keystrokes() {
        let spy = SpyInjector::default();
        deliver(&spy, "hello", false).unwrap();
        assert!(spy.typed.load(Ordering::SeqCst));
        assert!(!spy.pasted.load(Ordering::SeqCst));
    }
}
