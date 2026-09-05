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
    let text = &smart_spacing(text);
    if use_paste {
        paste_text(inj, text, settle_ms)
    } else {
        inj.inject_text(text)
    }
}

/// Append a trailing space so the next dictation does not run into this one
/// ("hello worldgoodbye"), except where a space would be wrong.
///
/// Han, kana and CJK punctuation are not word-separated: a trailing ASCII space
/// after "你好" or "です。" is a typography error, and because it is added on
/// every dictation the gaps accumulate down the line. Hangul is deliberately
/// *not* in the exclusion list — Korean does separate words with spaces.
///
/// Applied here, in the one function every delivery path routes through, rather
/// than at the call sites: re-injecting from History and command-mode replies
/// want the same treatment, and this way they cannot drift apart.
fn smart_spacing(text: &str) -> String {
    match text.chars().last() {
        // Nothing to space, or the caller already ended with whitespace.
        None => String::new(),
        Some(last) if last.is_whitespace() => text.to_string(),
        Some(last) if is_unspaced_script(last) => text.to_string(),
        Some(_) => format!("{text} "),
    }
}

/// Which tool types text on a Linux desktop, and with which arguments.
///
/// Lives here rather than in `platform/linux.rs` so it can be tested from any
/// host. That module only compiles on Linux, which is precisely why this — the
/// part with a real failure mode — had no tests at all.
///
/// The `--` matters more than it looks: without it a transcript that happens to
/// begin with a dash ("-- as I was saying") is parsed by xdotool as options
/// rather than text, and either errors or types nothing.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn linux_type_command(wayland: bool, text: &str) -> (&'static str, Vec<String>) {
    // ydotool talks to the kernel uinput device and so works under Wayland,
    // but needs the ydotoold daemon; xdotool drives X11.
    if wayland {
        ("ydotool", vec!["type".into(), "--".into(), text.to_owned()])
    } else {
        (
            "xdotool",
            vec![
                "type".into(),
                "--clearmodifiers".into(),
                "--".into(),
                text.to_owned(),
            ],
        )
    }
}

/// A Ctrl chord. ydotool addresses keys by Linux input-event code
/// (29 = LEFTCTRL); xdotool uses the key name.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn linux_chord_command(
    wayland: bool,
    keycode: u8,
    xdotool_key: &str,
) -> (&'static str, Vec<String>) {
    if wayland {
        (
            "ydotool",
            vec![
                "key".into(),
                "29:1".into(),
                format!("{keycode}:1"),
                format!("{keycode}:0"),
                "29:0".into(),
            ],
        )
    } else {
        (
            "xdotool",
            vec![
                "key".into(),
                "--clearmodifiers".into(),
                xdotool_key.to_owned(),
            ],
        )
    }
}

/// Scripts and punctuation blocks that do not separate words with spaces.
fn is_unspaced_script(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x303F   // CJK symbols and punctuation (。、「」)
        | 0x3040..=0x309F // Hiragana
        | 0x30A0..=0x30FF // Katakana
        | 0x3400..=0x4DBF // CJK unified ideographs extension A
        | 0x4E00..=0x9FFF // CJK unified ideographs
        | 0xF900..=0xFAFF // CJK compatibility ideographs
        | 0xFE10..=0xFE1F // vertical forms
        | 0xFE30..=0xFE4F // CJK compatibility forms
        | 0xFF00..=0xFF65 // fullwidth / halfwidth forms
    )
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

    /// A transcript beginning with a dash must be typed, not parsed as options.
    /// This is the reason `--` is in the argument list at all.
    #[test]
    fn linux_text_is_separated_from_the_options() {
        for wayland in [false, true] {
            let (_, args) = linux_type_command(wayland, "-- as I was saying");
            let sep = args.iter().position(|a| a == "--").expect("no -- separator");
            assert_eq!(
                args.last().unwrap(),
                "-- as I was saying",
                "the text must survive verbatim"
            );
            assert_eq!(sep, args.len() - 2, "-- must sit immediately before the text");
        }
    }

    /// The whole transcript is one argument. Splitting it would turn a spoken
    /// sentence into a series of flags.
    #[test]
    fn linux_text_is_a_single_argument() {
        let (_, args) = linux_type_command(false, "hello there friend");
        assert_eq!(args.iter().filter(|a| a.contains("hello")).count(), 1);
        assert_eq!(args.last().unwrap(), "hello there friend");
    }

    #[test]
    fn linux_picks_the_tool_that_matches_the_display_server() {
        assert_eq!(linux_type_command(false, "x").0, "xdotool");
        assert_eq!(linux_type_command(true, "x").0, "ydotool");
        assert_eq!(linux_chord_command(false, 47, "ctrl+v").0, "xdotool");
        assert_eq!(linux_chord_command(true, 47, "ctrl+v").0, "ydotool");
    }

    /// The paste chord must press Ctrl, press and release the key, then release
    /// Ctrl. Leaving Ctrl held would wedge the user's keyboard.
    #[test]
    fn linux_wayland_chord_presses_and_releases_ctrl_around_the_key() {
        let (_, args) = linux_chord_command(true, 47, "ctrl+v");
        assert_eq!(args, vec!["key", "29:1", "47:1", "47:0", "29:0"]);
    }

    #[test]
    fn linux_x11_chord_uses_the_key_name() {
        let (_, args) = linux_chord_command(false, 47, "ctrl+v");
        assert_eq!(args, vec!["key", "--clearmodifiers", "ctrl+v"]);
    }
    #[test]
    fn deliver_type_routes_to_keystrokes() {
        let spy = SpyInjector::default();
        deliver(&spy, "hello", false, 1).unwrap();
        assert!(spy.typed.load(Ordering::SeqCst));
        assert!(!spy.pasted.load(Ordering::SeqCst));
    }
}
