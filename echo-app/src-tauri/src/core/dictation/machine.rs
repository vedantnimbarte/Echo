/*!
 * SOURCE OF TRUTH KEYWORDS: DictationMachine, DictationState, DictationEvent,
 *   Transition, Effect, TransitionError, handle, CANCEL_COUNTDOWN_MS
 * WHAT:  The finite state machine governing one dictation. Pure: it takes an
 *        event, returns the new state plus a list of effects, and performs none
 *        of them.
 * WHY:   Recording state lives in exactly one place, so illegal states are
 *        unrepresentable and every transition is logged. Echo's state was a
 *        `Mutex<bool>` that each caller had to remember to consult in the right
 *        order — which is why `begin_recording` opens by claiming it and then
 *        has to hand it back by hand on three separate error paths, and why a
 *        capture failure once left the flag set and the hotkey silently dead
 *        from then on.
 *
 *        Purity is the point. The machine touches no database, no audio device
 *        and no clock, which is what makes "send these events in this order and
 *        assert the state" something a unit test can express rather than
 *        something you find out about from a user. Effects are DATA, returned
 *        for the caller to perform.
 *
 *        THE ONE ASYMMETRY WORTH KNOWING: cancellation does NOT stop capture.
 *        Audio keeps flowing through CancelArmed, because nothing being torn
 *        down is exactly what lets a second Escape resume with no gap and
 *        nothing lost. A machine that stopped capture on the first Escape could
 *        offer a countdown but not a resume, and the countdown is only worth
 *        having because the resume is real.
 * WHERE: Driven by commands/recording.rs. Its state is pushed to the pill as a
 *        typed event.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

/**
 * SOURCE OF TRUTH KEYWORDS: CANCEL_COUNTDOWN_MS
 * WHAT:  How long Escape leaves before the recording is actually discarded.
 * WHY:   Three seconds. Long enough to notice the pill has changed and press
 *        Escape again, short enough that someone who meant to cancel is not
 *        left watching a progress line. The value is here rather than in the
 *        UI because the machine is what expires it — a countdown the frontend
 *        owned would keep running in a webview that had been closed.
 * WHERE: Read by the actor driving this machine, and by the pill to animate the
 *        draining line.
 */
pub const CANCEL_COUNTDOWN_MS: u64 = 3_000;

/**
 * SOURCE OF TRUTH KEYWORDS: DictationState
 * WHAT:  Every state one dictation can be in.
 * WHY:   A closed enum rather than a set of booleans, which is what makes
 *        "recording and cancelling at the same time" unrepresentable instead of
 *        merely unlikely.
 * WHERE: Serialised to the pill so it can draw the right thing.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DictationState {
    /// Nothing happening. The only state a new dictation may start from.
    Idle,
    /// The hotkey landed; the capture device is being opened.
    Arming,
    /// Audio is flowing.
    Recording,
    /// Escape was pressed. AUDIO IS STILL FLOWING — see the module WHY.
    CancelArmed,
    /// Capture has stopped and the transcript is being produced.
    Finalizing,
}

/**
 * SOURCE OF TRUTH KEYWORDS: DictationEvent
 * WHAT:  Everything that can happen to a dictation.
 * WHERE: Sent by commands/recording.rs, the hotkey handler and the countdown
 *        timer.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationEvent {
    /// The user asked to start.
    Start,
    /// The capture device is open and delivering audio.
    CaptureReady,
    /// The capture device could not be opened, or died mid-recording.
    CaptureFailed,
    /// The user asked to stop, by hotkey or by the VAD deciding they had
    /// finished.
    Stop,
    /// Escape. Arms the countdown, or — pressed again — calls it off.
    Escape,
    /// The countdown reached zero with no second Escape.
    CancelExpired,
    /// The transcript has been delivered, or failed to be.
    Finalized,
}

/**
 * SOURCE OF TRUTH KEYWORDS: Effect
 * WHAT:  Something the caller must do as a result of a transition.
 * WHY:   Returned as data rather than performed, so the machine stays testable
 *        and so the ORDER is inspectable: a test can assert that the audio is
 *        discarded before the state goes Idle, which is the property that stops
 *        a cancelled recording being delivered by a race.
 * WHERE: Performed by commands/recording.rs.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    OpenCapture,
    StopCapture,
    /// Begin the countdown that will send `CancelExpired`.
    StartCountdown,
    /// The second Escape arrived; the countdown must not fire.
    CancelCountdown,
    /// Throw the audio away. Emitted only on a cancel that ran to completion.
    DiscardAudio,
    /// Hand the buffered audio to the decoder.
    Transcribe,
    PlayCue(CueKind),
    /// Tell the pill what state it is in now.
    NotifyState(DictationState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CueKind {
    Start,
    Stop,
    Failed,
}

/// An event that makes no sense in the current state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionError {
    pub state: DictationState,
    pub event: DictationEvent,
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} is not valid while {:?}", self.event, self.state)
    }
}

#[derive(Debug, Clone)]
pub struct Transition {
    pub state: DictationState,
    pub effects: Vec<Effect>,
}

/**
 * SOURCE OF TRUTH KEYWORDS: DictationMachine
 * WHAT:  Holds the current state and nothing else.
 * WHERE: One per app, owned by AppState.
 */
#[derive(Debug, Default)]
pub struct DictationMachine {
    state: DictationState,
}

impl Default for DictationState {
    fn default() -> Self {
        DictationState::Idle
    }
}

impl DictationMachine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> DictationState {
        self.state
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: handle
     * WHAT:  Applies one event, returning the new state and what to do about it.
     * WHY:   Every arm is written out rather than falling through a catch-all,
     *        so adding a state forces every event to be reconsidered. The
     *        catch-all at the end is only for combinations that are genuinely
     *        errors, and it reports which pair it rejected rather than silently
     *        doing nothing — an ignored event is how a stuck recording happens.
     * WHERE: Called by commands/recording.rs for every event.
     */
    pub fn handle(
        &mut self,
        event: DictationEvent,
    ) -> std::result::Result<Transition, TransitionError> {
        use DictationEvent as E;
        use DictationState as S;

        let (next, mut effects) = match (self.state, event) {
            // A second Start while one is already running is the double-fired
            // hotkey, and doing nothing is the correct response — not an error,
            // because nothing is wrong.
            (S::Arming | S::Recording | S::CancelArmed | S::Finalizing, E::Start) => {
                return Ok(Transition {
                    state: self.state,
                    effects: Vec::new(),
                })
            }

            (S::Idle, E::Start) => (
                S::Arming,
                // The cue comes before the device is opened: it answers "did my
                // keypress register", and opening a cold microphone can take
                // long enough that a later sound would not be an answer.
                vec![Effect::PlayCue(CueKind::Start), Effect::OpenCapture],
            ),

            (S::Arming, E::CaptureReady) => (S::Recording, vec![]),
            (S::Arming | S::Recording | S::CancelArmed, E::CaptureFailed) => (
                S::Idle,
                vec![Effect::DiscardAudio, Effect::PlayCue(CueKind::Failed)],
            ),

            // Stopping while still arming is a tap so short the device never
            // opened. There is nothing to transcribe, and saying so beats
            // delivering an empty transcript.
            (S::Arming, E::Stop) => (S::Idle, vec![Effect::StopCapture]),
            (S::Recording, E::Stop) => (
                S::Finalizing,
                vec![
                    Effect::StopCapture,
                    Effect::PlayCue(CueKind::Stop),
                    Effect::Transcribe,
                ],
            ),

            // FIRST ESCAPE. Note what is NOT here: no StopCapture. See the
            // module WHY — the resume depends on nothing having been torn down.
            (S::Recording, E::Escape) => (S::CancelArmed, vec![Effect::StartCountdown]),

            // SECOND ESCAPE. Straight back to recording, with the audio that
            // kept arriving throughout still in the buffer.
            (S::CancelArmed, E::Escape) => (S::Recording, vec![Effect::CancelCountdown]),

            // The countdown ran out. Only now is anything thrown away.
            (S::CancelArmed, E::CancelExpired) => {
                (S::Idle, vec![Effect::StopCapture, Effect::DiscardAudio])
            }

            // Stopping while the cancel is armed resolves it the user's way:
            // they reached for the stop key, so they want the words.
            (S::CancelArmed, E::Stop) => (
                S::Finalizing,
                vec![
                    Effect::CancelCountdown,
                    Effect::StopCapture,
                    Effect::PlayCue(CueKind::Stop),
                    Effect::Transcribe,
                ],
            ),

            (S::Finalizing, E::Finalized) => (S::Idle, vec![]),

            // A countdown that expires after the user already stopped is the
            // timer losing a race it was never going to win. Harmless, and
            // explicitly ignored so it is not reported as an error.
            (S::Idle | S::Recording | S::Finalizing, E::CancelExpired) => {
                return Ok(Transition {
                    state: self.state,
                    effects: Vec::new(),
                })
            }

            (state, event) => return Err(TransitionError { state, event }),
        };

        self.state = next;
        // Always last, so the pill is told only after everything that had to
        // happen for the new state to be true is already in the list.
        effects.push(Effect::NotifyState(next));

        Ok(Transition {
            state: next,
            effects,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DictationEvent as E;
    use DictationState as S;

    fn run(machine: &mut DictationMachine, events: &[DictationEvent]) -> Vec<Effect> {
        let mut all = Vec::new();
        for event in events {
            all.extend(machine.handle(*event).expect("valid transition").effects);
        }
        all
    }

    #[test]
    fn the_ordinary_dictation_runs_start_to_finish() {
        let mut m = DictationMachine::new();
        assert_eq!(m.state(), S::Idle);

        run(&mut m, &[E::Start]);
        assert_eq!(m.state(), S::Arming);
        run(&mut m, &[E::CaptureReady]);
        assert_eq!(m.state(), S::Recording);

        let effects = run(&mut m, &[E::Stop]);
        assert_eq!(m.state(), S::Finalizing);
        assert!(effects.contains(&Effect::Transcribe));

        run(&mut m, &[E::Finalized]);
        assert_eq!(m.state(), S::Idle);
    }

    /// The property the whole cancel design rests on: the first Escape must not
    /// stop capture, because the resume has nothing to resume otherwise.
    #[test]
    fn the_first_escape_does_not_stop_capture() {
        let mut m = DictationMachine::new();
        run(&mut m, &[E::Start, E::CaptureReady]);

        let effects = run(&mut m, &[E::Escape]);
        assert_eq!(m.state(), S::CancelArmed);
        assert!(effects.contains(&Effect::StartCountdown));
        assert!(
            !effects.contains(&Effect::StopCapture),
            "capture was torn down on the first Escape, so a resume would lose audio"
        );
        assert!(!effects.contains(&Effect::DiscardAudio));
    }

    #[test]
    fn a_second_escape_resumes_with_nothing_discarded() {
        let mut m = DictationMachine::new();
        run(&mut m, &[E::Start, E::CaptureReady, E::Escape]);

        let effects = run(&mut m, &[E::Escape]);
        assert_eq!(m.state(), S::Recording);
        assert!(effects.contains(&Effect::CancelCountdown));
        assert!(
            !effects.contains(&Effect::DiscardAudio),
            "resuming threw the audio away"
        );
    }

    /// And only when it runs to completion is anything actually lost.
    #[test]
    fn letting_the_countdown_expire_discards_the_audio() {
        let mut m = DictationMachine::new();
        run(&mut m, &[E::Start, E::CaptureReady, E::Escape]);

        let effects = run(&mut m, &[E::CancelExpired]);
        assert_eq!(m.state(), S::Idle);
        assert!(effects.contains(&Effect::StopCapture));
        assert!(effects.contains(&Effect::DiscardAudio));
        assert!(
            !effects.contains(&Effect::Transcribe),
            "a cancelled recording was sent to the decoder"
        );
    }

    /// Reaching for stop while the cancel is armed means they want the words.
    #[test]
    fn stopping_while_cancel_is_armed_keeps_the_transcript() {
        let mut m = DictationMachine::new();
        run(&mut m, &[E::Start, E::CaptureReady, E::Escape]);

        let effects = run(&mut m, &[E::Stop]);
        assert_eq!(m.state(), S::Finalizing);
        assert!(effects.contains(&Effect::CancelCountdown));
        assert!(effects.contains(&Effect::Transcribe));
        assert!(!effects.contains(&Effect::DiscardAudio));
    }

    /// The bug this machine exists to make impossible: a failure that leaves
    /// the state claimed, so every later hotkey press does nothing.
    #[test]
    fn a_capture_failure_always_returns_to_idle() {
        for reached in [
            vec![E::Start],
            vec![E::Start, E::CaptureReady],
            vec![E::Start, E::CaptureReady, E::Escape],
        ] {
            let mut m = DictationMachine::new();
            run(&mut m, &reached);
            let effects = run(&mut m, &[E::CaptureFailed]);
            assert_eq!(m.state(), S::Idle, "stuck after {reached:?}");
            assert!(effects.contains(&Effect::PlayCue(CueKind::Failed)));
        }
    }

    /// A hotkey that fires twice is a fact of life, not an error.
    #[test]
    fn a_repeated_start_is_ignored_rather_than_rejected() {
        let mut m = DictationMachine::new();
        run(&mut m, &[E::Start, E::CaptureReady]);

        let transition = m.handle(E::Start).expect("a second start is not an error");
        assert_eq!(transition.state, S::Recording);
        assert!(transition.effects.is_empty());
    }

    /// The timer can always lose its race; that must not be reported as a bug.
    #[test]
    fn a_late_countdown_is_harmless() {
        let mut m = DictationMachine::new();
        run(&mut m, &[E::Start, E::CaptureReady, E::Escape, E::Escape]);
        assert!(m.handle(E::CancelExpired).is_ok());
        assert_eq!(m.state(), S::Recording);
    }

    /// Escape with nothing running is the user pressing Escape at their editor.
    #[test]
    fn escape_outside_a_recording_is_rejected_not_swallowed() {
        let mut m = DictationMachine::new();
        let err = m.handle(E::Escape).expect_err("escape while idle");
        assert_eq!(err.state, S::Idle);
        assert_eq!(m.state(), S::Idle);
    }

    /// The pill is told last, so it never renders a state whose effects have
    /// not been listed yet.
    #[test]
    fn the_state_notification_comes_after_everything_else() {
        let mut m = DictationMachine::new();
        let effects = run(&mut m, &[E::Start]);
        assert_eq!(
            effects.last(),
            Some(&Effect::NotifyState(S::Arming)),
            "the pill was notified before the transition's own effects"
        );
    }
}
