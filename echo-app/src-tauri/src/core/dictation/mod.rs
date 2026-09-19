/*!
 * SOURCE OF TRUTH KEYWORDS: dictation, DictationMachine, DictationState
 * WHAT:  One dictation's lifecycle: the state machine that governs it.
 * WHY:   Separate from core/session.rs, which despite the name is about the
 *        DESKTOP session — Wayland versus X11 and whether a global hotkey can
 *        be bound there. Two different meanings of "session" in one crate is
 *        exactly the sort of collision that makes a grep for it useless, so
 *        this one is named for what it governs.
 * WHERE: Driven by commands/recording.rs.
 */

pub mod machine;

// Only what is used outside this module. CueKind, Transition and
// TransitionError are part of the machine's own vocabulary and are reached for
// as `machine::X` on the rare occasion something outside needs them.
pub use machine::{
    DictationEvent, DictationMachine, DictationState, Effect, CANCEL_COUNTDOWN_MS,
};
