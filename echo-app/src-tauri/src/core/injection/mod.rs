use std::sync::{Mutex, MutexGuard};

use unicode_segmentation::UnicodeSegmentation;

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

    /// Send the OS "undo" shortcut to the focused app.
    ///
    /// Undoing our own injection is delegated to the target app's undo stack
    /// rather than counting backspaces, because a caret that moved after the
    /// injection makes a character count delete the *user's* text. See
    /// [`crate::core::undo`].
    fn send_undo(&self) -> Result<()>;

    /// Delete the `n` characters immediately before the caret.
    ///
    /// Only safe where the caller knows it wrote those characters itself and
    /// nothing has intervened — which is true within one streaming utterance
    /// (see the partial-injection path in `commands::recording`) and is not
    /// true for undo.
    fn send_backspace(&self, n: usize) -> Result<()>;
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

/// Above this many characters, synthesized keystrokes stop being the better
/// choice: they take visibly longer, and the longer they run the more chance
/// something steals focus partway through and the rest lands elsewhere.
///
/// ponytail: one threshold rather than a measurement. Typing speed varies by
/// platform and target app, but the failure it guards against is qualitative —
/// nobody wants to watch 400 characters appear one key at a time.
const AUTO_PASTE_CHARS: usize = 160;

/// Whether to paste rather than type, for a user who has not pinned either.
///
/// Newlines decide it on their own: a multi-line transcript typed as
/// keystrokes sends Return into the target app, which submits chat boxes,
/// search fields and forms rather than inserting a line.
pub fn should_paste(text: &str) -> bool {
    text.contains('\n') || text.chars().count() > AUTO_PASTE_CHARS
}

/// Resolve the configured `injection_method` against the text about to be
/// delivered. `"auto"` — and anything unrecognised — defers to
/// [`should_paste`]; an explicit choice is always honoured.
pub fn use_paste_for(setting: Option<&str>, text: &str) -> bool {
    match setting {
        Some("paste") => true,
        Some("type") => false,
        _ => should_paste(text),
    }
}

/// Rewrite the tail of text Echo already typed so that it reads `next`.
///
/// Only the differing tail moves: a partial that grows types the new words and
/// deletes nothing. `shown` must be exactly what this process last typed and
/// nothing else — see the safety note on [`TextInjector::send_backspace`].
pub fn rewrite(inj: &dyn TextInjector, shown: &str, next: &str) -> Result<()> {
    let (delete, add) = partial_edit(shown, next);
    inj.send_backspace(delete)?;
    if !add.is_empty() {
        inj.inject_text(&add)?;
    }
    Ok(())
}

/// Close out a streamed utterance: whatever partial text is on screen becomes
/// the finished transcript, spaced like any other delivery.
///
/// Returns the text now standing in the target app, which is what an undo of
/// this delivery has to account for.
pub fn finish_streamed(inj: &dyn TextInjector, shown: &str, final_text: &str) -> Result<String> {
    let next = smart_spacing(final_text);
    rewrite(inj, shown, &next)?;
    Ok(next)
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

/// A bare key press with no modifier, repeated `count` times.
///
/// Separate from [`linux_chord_command`] because that one always wraps the key
/// in Ctrl — sending Ctrl+Backspace would delete a whole word per press instead
/// of a character.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn linux_key_command(
    wayland: bool,
    keycode: u8,
    xdotool_key: &str,
    count: usize,
) -> (&'static str, Vec<String>) {
    if wayland {
        let mut args = vec!["key".to_string()];
        for _ in 0..count {
            args.push(format!("{keycode}:1"));
            args.push(format!("{keycode}:0"));
        }
        ("ydotool", args)
    } else {
        (
            "xdotool",
            vec![
                "key".into(),
                "--clearmodifiers".into(),
                "--repeat".into(),
                count.to_string(),
                xdotool_key.to_owned(),
            ],
        )
    }
}

/// The edit that turns the text currently on screen into `next`: how many
/// backspaces to send, and what to type in their place.
///
/// Used by streaming injection, where each partial transcript replaces the one
/// before it. Only the differing tail is rewritten, so a partial that merely
/// grows — the common case — types the new words and deletes nothing.
///
/// Counted in grapheme clusters, because that is what a backspace deletes. A
/// `char` count agrees for ASCII and disagrees exactly where it is most
/// visible: "é" written as e + combining accent is two chars and one backspace,
/// a family emoji is seven chars and one backspace, and a flag is two. Counting
/// chars there under-deletes and leaves debris on screen that the next partial
/// builds on top of.
pub(crate) fn partial_edit(shown: &str, next: &str) -> (usize, String) {
    let shown: Vec<&str> = shown.graphemes(true).collect();
    let next: Vec<&str> = next.graphemes(true).collect();
    let common = shown
        .iter()
        .zip(next.iter())
        .take_while(|(a, b)| a == b)
        .count();
    (shown.len() - common, next[common..].concat())
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

/// The richest form of the clipboard's prior contents that we managed to read.
///
/// An enum rather than a struct of `Option`s because putting it back is a single
/// `set` operation — each of arboard's setters replaces the clipboard wholesale —
/// so exactly one of these can be restored, and saying so in the type stops the
/// restore from pretending otherwise.
enum Saved {
    /// HTML with its plain-text alternative, so formatting survives the trip.
    Html { html: String, text: String },
    Text(String),
    Image(arboard::ImageData<'static>),
    Files(Vec<std::path::PathBuf>),
    /// Either genuinely empty, or holding a format arboard cannot represent.
    Unknown,
}

/// The clipboard, borrowed — whatever was on it goes back when this is dropped.
///
/// Echo uses the clipboard as a transport: paste injection puts a transcript on
/// it, and reading a selection puts a sentinel on it. Both borrow something that
/// belongs to the user, and before this existed both could fail to give it back.
///
/// 1. **Only text was saved.** `get_text` fails on an image, a screenshot or a
///    copied file, so the restore was skipped and the user's content was simply
///    replaced by a transcript — no error, nothing pointing at Echo. An empty
///    clipboard took the same path, which left the transcript sitting there for
///    whatever they pasted into next.
/// 2. **The restore was a statement, not a guarantee.** It came after a `?` on
///    the shortcut send, so a failed paste returned early with Echo's data still
///    on the clipboard. That failure is routine rather than exotic: Wayland
///    without `ydotoold`, macOS before Accessibility is granted. For
///    `copy_selection` what was left behind was the internal sentinel.
///
/// `Drop` answers both at once, and on a panic too, which is why the restore
/// lives here rather than at the end of each function.
struct Borrowed {
    /// Held for the whole borrow, so no other thread can open the clipboard
    /// between the snapshot and the restore. See [`CLIPBOARD`].
    _lock: MutexGuard<'static, ()>,
    clipboard: arboard::Clipboard,
    saved: Saved,
}

/// Serialises clipboard access across threads.
///
/// Not defensive programming. Two threads opening the clipboard at once is a
/// **heap corruption** crash on Windows — `STATUS_HEAP_CORRUPTION`, reproduced
/// by letting this module's clipboard tests run in parallel. The Win32 clipboard
/// is a single global object with no concurrency story, and `clipboard-win`
/// opens a process-wide handle to it.
///
/// Echo can arrive here from two directions — delivering a transcript, and
/// reading a selection for command mode — and a second utterance can begin while
/// the first is still being delivered. So the lock sits here, at the one
/// chokepoint both paths now route through, rather than being repeated at each
/// call site where the next one added would forget it.
static CLIPBOARD: Mutex<()> = Mutex::new(());

impl Borrowed {
    /// Take the clipboard lock, open the clipboard, and snapshot it.
    fn take() -> Result<Self> {
        // Poisoning is recovered from rather than propagated: the guarded value
        // is `()`, so a thread that panicked holding this broke no invariant,
        // and refusing every later paste because of it would turn one failed
        // dictation into a permanently broken clipboard.
        let lock = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|e| EchoError::Injection(format!("clipboard unavailable: {e}")))?;
        let saved = snapshot(&mut clipboard);
        Ok(Self { _lock: lock, clipboard, saved })
    }
}

/// Read the clipboard into the richest form we can put back.
///
/// Ordered by cost as much as by priority. Text is both the common case and the
/// cheap one; an image is read only when there is no text to find, because a
/// screenshot can be tens of megabytes and paying that on every dictation — to
/// discover that a sentence was on the clipboard — would be a poor trade.
fn snapshot(clipboard: &mut arboard::Clipboard) -> Saved {
    if let Ok(text) = clipboard.get_text() {
        // HTML rides along with text wherever it exists. Restoring the plain
        // text alone is exactly how formatting copied out of Word or a browser
        // quietly disappears.
        return match clipboard.get().html() {
            Ok(html) => Saved::Html { html, text },
            Err(_) => Saved::Text(text),
        };
    }
    if let Ok(image) = clipboard.get_image() {
        return Saved::Image(image);
    }
    match clipboard.get().file_list() {
        Ok(files) if !files.is_empty() => Saved::Files(files),
        _ => Saved::Unknown,
    }
}

impl Drop for Borrowed {
    fn drop(&mut self) {
        // Best effort throughout: a clipboard that refuses the restore is not
        // something a finished dictation can be failed over, and `Drop` has
        // nowhere to report it to anyway.
        let _ = match &self.saved {
            Saved::Html { html, text } => {
                self.clipboard.set().html(html.as_str(), Some(text.as_str()))
            }
            Saved::Text(text) => self.clipboard.set_text(text.clone()),
            Saved::Image(image) => self.clipboard.set_image(image.clone()),
            Saved::Files(files) => self.clipboard.set().file_list(files),
            // Nothing readable was there, so leave nothing of ours behind.
            // Clearing beats leaving the transcript: the alternative is a user
            // pasting their own dictation into the next document without
            // knowing where it came from.
            //
            // ponytail: this also clears a format arboard cannot represent — an
            // Excel cell range carrying no text fallback, say. Such formats
            // nearly always ship text or HTML alongside, so the case is rare;
            // telling "empty" apart from "unreadable" needs per-platform format
            // enumeration, which is a lot of code for that margin.
            Saved::Unknown => self.clipboard.clear(),
        };
    }
}

/// Clipboard-paste injection: borrow the clipboard, set it to `text`, send the
/// paste shortcut, and let [`Borrowed`] put the user's content back.
fn paste_text(inj: &dyn TextInjector, text: &str, settle_ms: u64) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }

    let mut borrowed = Borrowed::take()?;

    borrowed
        .clipboard
        .set_text(text.to_owned())
        .map_err(|e| EchoError::Injection(format!("failed to set clipboard: {e}")))?;

    inj.send_paste()?;

    std::thread::sleep(std::time::Duration::from_millis(settle_ms.max(1)));
    Ok(())
    // `borrowed` restores here — and on either `?` above.
}

/// Read the focused app's current selection via the clipboard, restoring the
/// user's clipboard contents afterwards.
///
/// Returns `None` when nothing is selected. A sentinel is written first so an
/// app that ignores the copy shortcut is distinguishable from one that copied
/// text identical to what was already on the clipboard.
pub fn copy_selection(inj: &dyn TextInjector) -> Result<Option<String>> {
    const SENTINEL: &str = "\u{0}echo-no-selection\u{0}";

    let mut borrowed = Borrowed::take()?;
    let _ = borrowed.clipboard.set_text(SENTINEL);

    inj.send_copy()?;

    // Poll rather than sleep: a fast app answers in ~20ms and a slow one gets
    // the full budget, instead of everyone paying the same fixed wait and slow
    // apps still losing the race.
    let copied = poll_clipboard_change(&mut borrowed.clipboard, SENTINEL);

    // `borrowed` restores on the way out, including via the `?` above — the path
    // that used to leave SENTINEL sitting on the user's clipboard.
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Default)]
    struct SpyInjector {
        typed: AtomicBool,
        pasted: AtomicBool,
        copied: AtomicBool,
        undone: AtomicBool,
        backspaces: AtomicUsize,
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
        fn send_undo(&self) -> Result<()> {
            self.undone.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn send_backspace(&self, n: usize) -> Result<()> {
            self.backspaces.fetch_add(n, Ordering::SeqCst);
            Ok(())
        }
    }

    /// An injector whose paste shortcut always fails — Wayland without
    /// `ydotoold`, or macOS before Accessibility is granted.
    struct FailingPaste;
    impl TextInjector for FailingPaste {
        fn inject_text(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn send_paste(&self) -> Result<()> {
            Err(EchoError::Injection("no paste here".into()))
        }
        fn send_copy(&self) -> Result<()> {
            Err(EchoError::Injection("no copy here".into()))
        }
        fn send_undo(&self) -> Result<()> {
            Ok(())
        }
        fn send_backspace(&self, _: usize) -> Result<()> {
            Ok(())
        }
    }

    /// The defect this guards against: the restore used to sit *after* a `?` on
    /// the shortcut send, so a failed shortcut returned early and left Echo's own
    /// data on the user's clipboard — the transcript for a paste, and the internal
    /// `SENTINEL` for a selection read.
    ///
    /// One test for both paths on purpose. The clipboard is a single global OS
    /// object, and the setup and assertions here run outside [`CLIPBOARD`], so
    /// splitting this in two would let the halves race each other.
    ///
    /// Needs a real clipboard, which a headless CI container does not have, so it
    /// returns rather than fails there instead of reporting a problem that has
    /// nothing to do with this code.
    #[test]
    fn a_failed_shortcut_still_gives_the_clipboard_back() {
        const MINE: &str = "something the user copied themselves";

        let Ok(mut cb) = arboard::Clipboard::new() else { return };
        if cb.set_text(MINE.to_owned()).is_err() {
            return;
        }
        drop(cb);

        let pasted = paste_text(&FailingPaste, "a dictated sentence", 1);
        assert!(pasted.is_err(), "a paste that could not be sent must be reported");
        assert_eq!(current_text().as_deref(), Some(MINE), "a failed paste kept what it borrowed");

        let copied = copy_selection(&FailingPaste);
        assert!(copied.is_err(), "a copy that could not be sent must be reported");
        let now = current_text().unwrap_or_default();
        assert!(
            !now.contains("echo-no-selection"),
            "the internal sentinel escaped onto the clipboard: {now:?}"
        );
        assert_eq!(now, MINE, "a failed copy did not restore the user's text");
    }

    fn current_text() -> Option<String> {
        arboard::Clipboard::new().ok()?.get_text().ok()
    }

    // `deliver(_, _, false)` types keystrokes and never sends a paste shortcut.

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
    /// A newline is the decisive case: typed as keystrokes it becomes Return,
    /// which submits the chat box instead of breaking the line.
    #[test]
    fn auto_pastes_multi_line_text_however_short() {
        assert!(should_paste("one\ntwo"));
        assert!(!should_paste("one two"));
    }

    #[test]
    fn auto_types_short_text_and_pastes_long_text() {
        assert!(!should_paste(&"a".repeat(AUTO_PASTE_CHARS)));
        assert!(should_paste(&"a".repeat(AUTO_PASTE_CHARS + 1)));
    }

    /// An explicit choice is never second-guessed, whatever the text looks
    /// like — that is the difference between a setting and a suggestion.
    #[test]
    fn an_explicit_method_always_wins() {
        let long = "a".repeat(AUTO_PASTE_CHARS + 1);
        assert!(!use_paste_for(Some("type"), &long));
        assert!(use_paste_for(Some("paste"), "hi"));
        assert!(use_paste_for(Some("auto"), &long));
        assert!(!use_paste_for(Some("auto"), "hi"));
    }

    #[test]
    fn deliver_type_routes_to_keystrokes() {
        let spy = SpyInjector::default();
        deliver(&spy, "hello", false, 1).unwrap();
        assert!(spy.typed.load(Ordering::SeqCst));
        assert!(!spy.pasted.load(Ordering::SeqCst));
    }

    /// Backspace must be a bare key. Ctrl+Backspace deletes a whole word, which
    /// would eat text a partial rewrite never wrote.
    #[test]
    fn linux_backspace_holds_no_modifier() {
        let (_, args) = linux_key_command(true, 14, "BackSpace", 2);
        assert_eq!(args, vec!["key", "14:1", "14:0", "14:1", "14:0"]);
        assert!(!args.iter().any(|a| a.starts_with("29:")), "ctrl was held");

        let (_, args) = linux_key_command(false, 14, "BackSpace", 3);
        assert_eq!(args, vec!["key", "--clearmodifiers", "--repeat", "3", "BackSpace"]);
    }

    /// The common case: a partial only grows, so nothing is deleted and only
    /// the new words are typed.
    #[test]
    fn a_growing_partial_types_only_the_new_tail() {
        assert_eq!(
            partial_edit("the quick", "the quick brown"),
            (0, " brown".to_string())
        );
    }

    /// A re-decode can revise what it already emitted; only the differing tail
    /// is rewritten, never the whole line.
    #[test]
    fn a_revised_partial_deletes_only_back_to_the_divergence() {
        assert_eq!(
            partial_edit("the quick brown", "the quick brawn"),
            (3, "awn".to_string())
        );
        // Shorter is a legal revision too.
        assert_eq!(partial_edit("hello there", "hello"), (6, String::new()));
        // Nothing changed: no keystrokes at all.
        assert_eq!(partial_edit("same", "same"), (0, String::new()));
        // Nothing on screen yet.
        assert_eq!(partial_edit("", "hello"), (0, "hello".to_string()));
    }

    /// Counting in `char`s, not bytes: a multi-byte character is one backspace,
    /// and getting this wrong would delete a byte at a time through the user's
    /// text.
    #[test]
    fn partial_edit_counts_characters_not_bytes() {
        assert_eq!(partial_edit("café", "cafe"), (1, "e".to_string()));
    }

    /// One backspace deletes one grapheme cluster, so that is the unit here.
    /// Counting `char`s under-deletes and leaves debris on screen that the next
    /// partial then builds on top of.
    #[test]
    fn partial_edit_counts_what_a_backspace_deletes() {
        // Decomposed "e" + combining acute is two chars and one cluster, so
        // the accented letter is deleted whole and the plain one retyped.
        // Counting chars found a four-char common prefix and asked for one
        // backspace with nothing to type — which deletes the cluster and
        // leaves "caf".
        let decomposed = "cafe\u{301}";
        assert_eq!(decomposed.chars().count(), 5);
        assert_eq!(partial_edit(decomposed, "cafe"), (1, "e".to_string()));

        // A ZWJ family is one cluster however many code points build it.
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        assert!(family.chars().count() > 1);
        assert_eq!(partial_edit(&format!("hi {family}"), "hi "), (1, String::new()));

        // A regional-indicator flag is two code points and one backspace.
        assert_eq!(partial_edit("go \u{1F1EE}\u{1F1F3}", "go "), (1, String::new()));

        // Growing by an emoji types it whole and deletes nothing.
        assert_eq!(
            partial_edit("hi ", &format!("hi {family}")),
            (0, family.to_string())
        );
    }
}
