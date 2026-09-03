//! Hotkeys that are a modifier on its own — Ctrl, Alt, Shift, or Cmd/Win.
//!
//! The global-shortcut plugin cannot express these. It wraps `RegisterHotKey`
//! on Windows and its equivalents elsewhere, and in all of them a modifier is a
//! *flag* attached to a real key, never the key itself. So watching one means
//! reading the keyboard directly.
//!
//! This polls key state rather than installing a keyboard hook. A hook receives
//! every keystroke you type; polling only ever asks "is this key down?", and
//! only asks about keys *other* than the watched one while that one is being
//! held. Cost is one short scan every [`POLL`].
//!
//! Fn is deliberately absent: on nearly all laptops it is resolved by the
//! keyboard's own firmware and never becomes a key event the OS can see.

mod platform;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use platform::KeyReader;

/// How often key state is sampled. Fast enough that a deliberate tap is never
/// missed, slow enough to be free.
const POLL: Duration = Duration::from_millis(20);

/// How long the modifier must be held alone before hold-to-talk starts. Short
/// enough to feel immediate, long enough that the first half of a Ctrl+C never
/// reaches it.
const HOLD_THRESHOLD: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierKey {
    Control,
    Alt,
    Shift,
    Meta,
}

impl ModifierKey {
    /// Parse a stored accelerator that names a modifier and nothing else.
    /// Anything with a `+` in it is a real chord and belongs to the plugin.
    pub fn parse(accelerator: &str) -> Option<Self> {
        match accelerator.trim() {
            "Control" | "Ctrl" | "CommandOrControl" | "CmdOrCtrl" => Some(Self::Control),
            "Alt" | "Option" => Some(Self::Alt),
            "Shift" => Some(Self::Shift),
            "Meta" | "Super" | "Command" | "Cmd" | "Win" => Some(Self::Meta),
            _ => None,
        }
    }
}

/// How the watched modifier starts and stops dictation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// Press and release it alone: one clean tap fires once.
    Tap,
    /// Hold it alone: starts after [`HOLD_THRESHOLD`], stops on release.
    Hold,
}

/// A running watcher. Dropping it stops the polling thread.
pub struct ModTapWatcher {
    stop: Arc<AtomicBool>,
}

impl ModTapWatcher {
    /// Watch `key`, calling `on_start` (and, for [`Activation::Hold`],
    /// `on_stop`) as it is used.
    ///
    /// Returns `None` when the platform cannot report key state — Wayland, or a
    /// session with no X display. The caller should say so rather than leave a
    /// hotkey that silently does nothing.
    pub fn start<F, G>(
        key: ModifierKey,
        activation: Activation,
        on_start: F,
        on_stop: G,
    ) -> Option<Self>
    where
        F: Fn() + Send + 'static,
        G: Fn() + Send + 'static,
    {
        // Opened on this thread only to fail fast; the watcher opens its own.
        KeyReader::open(key)?;

        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();

        std::thread::spawn(move || {
            let Some(reader) = KeyReader::open(key) else { return };
            let mut state = TapState::default();

            while !flag.load(Ordering::Relaxed) {
                let down = reader.modifier_down(key);
                // Only look at the rest of the keyboard while the modifier is
                // actually held, and stop looking once something has already
                // disqualified this press.
                let interrupted = down
                    && state.pressed_at.is_some()
                    && !state.cancelled
                    && reader.other_key_down(key);

                match state.step(down, interrupted, activation) {
                    Some(Edge::Start) => on_start(),
                    Some(Edge::Stop) => on_stop(),
                    None => {}
                }
                std::thread::sleep(POLL);
            }
        });

        Some(Self { stop })
    }
}

impl Drop for ModTapWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// What a single poll concluded.
#[derive(Debug, PartialEq, Eq)]
enum Edge {
    Start,
    Stop,
}

/// The press-tracking state machine, kept separate from the polling loop and
/// the OS so it can be tested.
#[derive(Default)]
struct TapState {
    /// When the modifier went down, if it is down.
    pressed_at: Option<Instant>,
    /// Another key was pressed during this press, so it is a chord, not a tap.
    cancelled: bool,
    /// Hold-to-talk has begun and still owes a stop.
    holding: bool,
}

impl TapState {
    /// Advance one poll. `interrupted` means another key is currently down.
    fn step(&mut self, down: bool, interrupted: bool, activation: Activation) -> Option<Edge> {
        match (self.pressed_at, down) {
            // Press begins.
            (None, true) => {
                // A press that already has company is a chord from the start.
                self.pressed_at = Some(Instant::now());
                self.cancelled = interrupted;
                self.holding = false;
                None
            }

            // Still held.
            (Some(since), true) => {
                if interrupted {
                    self.cancelled = true;
                }
                let ready = activation == Activation::Hold
                    && !self.cancelled
                    && !self.holding
                    && since.elapsed() >= HOLD_THRESHOLD;
                if ready {
                    self.holding = true;
                    return Some(Edge::Start);
                }
                None
            }

            // Released.
            (Some(_), false) => {
                let edge = if self.holding {
                    Some(Edge::Stop)
                } else if !self.cancelled && activation == Activation::Tap {
                    // No upper bound on how long a tap may last: in tap mode a
                    // slow, deliberate press is still a tap, and holding the
                    // key alone means nothing else.
                    Some(Edge::Start)
                } else {
                    None
                };
                *self = Self::default();
                edge
            }

            (None, false) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Press and release alone: one activation, no matter the mode's threshold.
    #[test]
    fn a_clean_tap_fires_once_in_tap_mode() {
        let mut s = TapState::default();
        assert_eq!(s.step(true, false, Activation::Tap), None);
        assert_eq!(s.step(true, false, Activation::Tap), None);
        assert_eq!(s.step(false, false, Activation::Tap), Some(Edge::Start));
        // And nothing more once it is over.
        assert_eq!(s.step(false, false, Activation::Tap), None);
    }

    /// The whole point: Ctrl+C must not start dictation. This is the case that
    /// makes a bare modifier usable at all.
    #[test]
    fn another_key_during_the_press_cancels_it() {
        let mut s = TapState::default();
        s.step(true, false, Activation::Tap);
        s.step(true, true, Activation::Tap); // the "C" of Ctrl+C
        assert_eq!(s.step(false, false, Activation::Tap), None);
    }

    /// A press that is already accompanied when first seen is a chord too — the
    /// poll can land after both keys are down.
    #[test]
    fn a_press_that_arrives_with_company_is_cancelled() {
        let mut s = TapState::default();
        s.step(true, true, Activation::Tap);
        assert_eq!(s.step(false, false, Activation::Tap), None);
    }

    /// Hold mode starts once past the threshold and stops on release.
    #[test]
    fn hold_starts_after_the_threshold_and_stops_on_release() {
        let mut s = TapState::default();
        assert_eq!(s.step(true, false, Activation::Hold), None);
        s.pressed_at = Some(Instant::now() - HOLD_THRESHOLD);
        assert_eq!(s.step(true, false, Activation::Hold), Some(Edge::Start));
        // Still held: no repeat.
        assert_eq!(s.step(true, false, Activation::Hold), None);
        assert_eq!(s.step(false, false, Activation::Hold), Some(Edge::Stop));
    }

    /// A hold that never reaches the threshold does nothing in hold mode —
    /// otherwise a brush of the key would open the microphone.
    #[test]
    fn a_brief_press_does_nothing_in_hold_mode() {
        let mut s = TapState::default();
        s.step(true, false, Activation::Hold);
        assert_eq!(s.step(false, false, Activation::Hold), None);
    }

    /// Interrupting a hold that has already started still stops it cleanly,
    /// rather than leaving the microphone open with no stop owed.
    #[test]
    fn an_interrupted_hold_still_stops() {
        let mut s = TapState::default();
        s.step(true, false, Activation::Hold);
        s.pressed_at = Some(Instant::now() - HOLD_THRESHOLD);
        assert_eq!(s.step(true, false, Activation::Hold), Some(Edge::Start));
        s.step(true, true, Activation::Hold);
        assert_eq!(s.step(false, false, Activation::Hold), Some(Edge::Stop));
    }

    #[test]
    fn parse_accepts_bare_modifiers_only() {
        assert_eq!(ModifierKey::parse("Control"), Some(ModifierKey::Control));
        assert_eq!(ModifierKey::parse("Alt"), Some(ModifierKey::Alt));
        assert_eq!(ModifierKey::parse("Shift"), Some(ModifierKey::Shift));
        assert_eq!(ModifierKey::parse("Super"), Some(ModifierKey::Meta));
        assert_eq!(ModifierKey::parse("Control+Shift+Space"), None);
        assert_eq!(ModifierKey::parse("F9"), None);
    }
}

/// End-to-end check of the Windows key-state reading, which no amount of
/// reasoning about the state machine can cover: synthesise a real Ctrl tap and
/// confirm the watcher sees it. Ctrl on its own does nothing to the focused
/// app, so this is safe to run anywhere.
#[cfg(all(test, target_os = "windows"))]
mod windows_key_reading {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn send_ctrl(up: bool) {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
        };
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0x11), // VK_CONTROL
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    }

    #[test]
    fn a_synthesised_ctrl_tap_reaches_the_watcher() {
        let taps = Arc::new(AtomicUsize::new(0));
        let counter = taps.clone();

        let _watcher = ModTapWatcher::start(
            ModifierKey::Control,
            Activation::Tap,
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
            },
            || {},
        )
        .expect("Windows can always report key state");

        // Let the poll loop settle, then tap. The sleeps are generous multiples
        // of POLL so a loaded machine does not turn this into a flaky test.
        std::thread::sleep(POLL * 5);
        send_ctrl(false);
        std::thread::sleep(POLL * 5);
        send_ctrl(true);
        std::thread::sleep(POLL * 5);

        assert_eq!(
            taps.load(Ordering::SeqCst),
            1,
            "a clean Ctrl tap should activate exactly once"
        );
    }
}
