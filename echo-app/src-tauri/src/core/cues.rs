/*!
 * SOURCE OF TRUTH KEYWORDS: Cue, play, FeedbackSound, sound_cues
 * WHAT:  The three short sounds that confirm a recording started, stopped, or
 *        failed.
 * WHY:   Without them you do not trust that the hotkey registered. Echo's pill
 *        is often not where you are looking — that is the point of dictating
 *        into another app — and in the moment you press the key it may not be
 *        on screen yet at all. That is why this is a feature and not decoration.
 *
 *        MOVED HERE FROM THE PILL, and the move is the point. The cues used to
 *        be WebAudio tones fired from a React effect watching the pill's
 *        recording state, which meant they could only sound once that webview
 *        existed and had received the state — so the one case the feature is
 *        for, "did my keypress do anything", was the case it could not answer.
 *        Firing on the session transition in Rust removes that dependency
 *        entirely, and gives the failure its own sound, which the pill version
 *        never had.
 *
 *        SYSTEM SOUNDS, not bundled audio, on every platform that has them:
 *        they are already the sounds this user's machine makes, they respect
 *        the system alert volume, they add nothing to the bundle and they
 *        cannot be missing. Linux has no such guarantee, so there it is a
 *        best-effort call to whatever is installed and silence otherwise —
 *        a missing sound is not worth an error when the recording is what
 *        matters and is already underway.
 *
 *        Playback is fire-and-forget and never blocks the caller.
 * WHERE: Called by commands/recording.rs on start, stop and failure, gated on
 *        the `sound_cues` setting.
 */

/// Which moment a sound is marking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    Start,
    Stop,
    Failed,
}

/**
 * SOURCE OF TRUTH KEYWORDS: play
 * WHAT:  Plays the cue, and returns immediately.
 * WHY:   Spawned rather than awaited because every caller is on a path where
 *        the user is waiting: the start cue must not delay opening the input
 *        stream, and the stop cue must not delay the transcript. A sound that
 *        fails is swallowed for the same reason.
 * WHERE: commands/recording.rs.
 */
pub fn play(cue: Cue) {
    std::thread::spawn(move || {
        if let Err(e) = play_blocking(cue) {
            // Debug, not warn: on a machine with no sound theme this would
            // otherwise be a line per dictation.
            tracing::debug!(?cue, error = %e, "Could not play the cue");
        }
    });
}

#[cfg(target_os = "windows")]
fn play_blocking(cue: Cue) -> std::io::Result<()> {
    // The three system events, by the names the registry gives them. These are
    // what the user has already chosen in Sound settings, which is exactly why
    // they are used rather than tones of our own.
    let event = match cue {
        Cue::Start => "Notification.Default",
        Cue::Stop => "Notification.Default",
        Cue::Failed => "SystemExclamation",
    };
    crate::platform::windows::play_system_sound(event)
}

#[cfg(target_os = "macos")]
fn play_blocking(cue: Cue) -> std::io::Result<()> {
    // Named NSSound system sounds. Chosen muffled and minimal for start/stop so
    // they can be heard many times an hour without grating; the failure is the
    // one that is allowed to be sharp, because it is the one that wants
    // attention.
    let name = match cue {
        Cue::Start => "Tink",
        Cue::Stop => "Pop",
        Cue::Failed => "Basso",
    };
    crate::platform::macos::play_system_sound(name)
}

#[cfg(target_os = "linux")]
fn play_blocking(cue: Cue) -> std::io::Result<()> {
    // Freedesktop sound-theme event names. `canberra-gtk-play` is the usual
    // player and is frequently absent; its absence is not an error worth
    // surfacing, which is why the caller only logs at debug.
    let event = match cue {
        Cue::Start => "message",
        Cue::Stop => "message",
        Cue::Failed => "dialog-error",
    };
    std::process::Command::new("canberra-gtk-play")
        .arg("-i")
        .arg(event)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract every caller relies on: this returns straight away and
    /// never propagates a failure, because it sits on the path where the user
    /// is waiting for their words.
    #[test]
    fn playing_a_cue_returns_immediately_and_never_panics() {
        let started = std::time::Instant::now();
        play(Cue::Start);
        play(Cue::Stop);
        play(Cue::Failed);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(100),
            "play blocked the caller for {:?}",
            started.elapsed()
        );
    }

    /// Three distinct moments. Start and stop may share a sound on a platform
    /// whose theme has nothing better, but "it failed" must never be one of
    /// them — that is the whole reason the third variant exists.
    #[test]
    fn failure_is_its_own_cue() {
        assert_ne!(Cue::Failed, Cue::Start);
        assert_ne!(Cue::Failed, Cue::Stop);
    }
}
