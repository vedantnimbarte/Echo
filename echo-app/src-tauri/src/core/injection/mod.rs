use crate::error::{EchoError, Result};

/// Platform-agnostic text injection trait.
pub trait TextInjector: Send + Sync {
    /// Type `text` into the focused app by synthesizing per-character keystrokes.
    fn inject_text(&self, text: &str) -> Result<()>;

    /// Send the OS "paste" shortcut (Cmd+V on macOS, Ctrl+V elsewhere) to the
    /// focused app. Used by [`deliver`] for clipboard-paste injection.
    fn send_paste(&self) -> Result<()>;

    /// Send the OS "copy" shortcut. Used by [`copy_selection`] to read whatever
    /// the focused app currently has selected.
    fn send_copy(&self) -> Result<()>;
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

/// How long to wait for the focused app to read the clipboard before restoring
/// it. 120ms was fine for native text fields and too short for Electron apps,
/// terminals and anything over remote desktop — those dropped the paste or got
/// the old clipboard back mid-read.
///
/// ponytail: still a timer, because a process cannot observe another app
/// reading the clipboard. Exposed as `clipboard_settle_ms` so a user on a slow
/// target can raise it rather than filing a bug we can't reproduce.
pub const DEFAULT_SETTLE_MS: u64 = 180;

/// Upper bound on waiting for the focused app to *produce* a selection after a
/// copy shortcut. Unlike paste, this one is observable, so we poll and return
/// as soon as the clipboard changes.
const COPY_POLL_TIMEOUT_MS: u64 = 800;
const COPY_POLL_STEP_MS: u64 = 20;

/// Deliver `text` to the focused app, choosing the mechanism:
/// - `use_paste = false` → synthesize keystrokes (universal, but slow/racy for
///   long text and blocked on some Wayland compositors).
/// - `use_paste = true` → put `text` on the clipboard, send the paste shortcut,
///   then restore the prior clipboard. Reliable for long transcripts.
pub fn deliver(inj: &dyn TextInjector, text: &str, use_paste: bool, settle_ms: u64) -> Result<()> {
    if use_paste {
        paste_text(inj, text, settle_ms)
    } else {
        inj.inject_text(text)
    }
}

/// Clipboard-paste injection: save the current clipboard, set it to `text`,
/// send the paste shortcut, then restore the original clipboard.
fn paste_text(inj: &dyn TextInjector, text: &str, settle_ms: u64) -> Result<()> {
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

    std::thread::sleep(std::time::Duration::from_millis(settle_ms.max(1)));

    if let Some(prev) = prior {
        let _ = clipboard.set_text(prev);
    }
    Ok(())
}

/// Read the focused app's current selection via the clipboard, restoring the
/// user's clipboard contents afterwards.
///
/// Returns `None` when nothing is selected. A sentinel is written first so an
/// app that ignores the copy shortcut is distinguishable from one that copied
/// text identical to what was already on the clipboard.
pub fn copy_selection(inj: &dyn TextInjector) -> Result<Option<String>> {
    const SENTINEL: &str = "\u{0}echo-no-selection\u{0}";

    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| EchoError::Injection(format!("clipboard unavailable: {e}")))?;

    let prior = clipboard.get_text().ok();
    let _ = clipboard.set_text(SENTINEL);

    inj.send_copy()?;

    // Poll rather than sleep: a fast app answers in ~20ms and a slow one gets
    // the full budget, instead of everyone paying the same fixed wait and slow
    // apps still losing the race.
    let copied = poll_clipboard_change(&mut clipboard, SENTINEL);

    if let Some(prev) = prior {
        let _ = clipboard.set_text(prev);
    }

    Ok(match copied {
        Some(text) if text != SENTINEL && !text.is_empty() => Some(text),
        _ => None,
    })
}

/// Read the clipboard until it stops being `sentinel`, or the budget runs out.
/// `None` means nothing was copied — either no selection, or the app ignored
/// the shortcut.
fn poll_clipboard_change(
    clipboard: &mut arboard::Clipboard,
    sentinel: &str,
) -> Option<String> {
    let step = std::time::Duration::from_millis(COPY_POLL_STEP_MS);
    let attempts = COPY_POLL_TIMEOUT_MS / COPY_POLL_STEP_MS;

    for _ in 0..attempts {
        std::thread::sleep(step);
        match clipboard.get_text() {
            Ok(text) if text != sentinel && !text.is_empty() => return Some(text),
            _ => continue,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Default)]
    struct SpyInjector {
        typed: AtomicBool,
        pasted: AtomicBool,
        copied: AtomicBool,
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
        fn send_copy(&self) -> Result<()> {
            self.copied.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    // `deliver(_, _, false)` types keystrokes and never sends a paste shortcut.
    // (The paste branch touches the real clipboard, so it needs device testing.)
    #[test]
    fn deliver_type_routes_to_keystrokes() {
        let spy = SpyInjector::default();
        deliver(&spy, "hello", false, 1).unwrap();
        assert!(spy.typed.load(Ordering::SeqCst));
        assert!(!spy.pasted.load(Ordering::SeqCst));
    }
}
