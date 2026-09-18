//! Decoder tuning shared by the local whisper providers (CLI and server).
//!
//! Kept in one place because the two providers must stay identical: switching
//! between them is a performance decision, and it would be a bad surprise if it
//! also silently changed transcription quality.

/// Entropy threshold for "decoder failed" (whisper.cpp default: 2.40).
///
/// The stock thresholds let a mostly-silent decode window pass whisper.cpp's
/// repetition check and emit training-data boilerplate — "Thank you for
/// watching", "Продолжение следует...". Raising both thresholds makes the
/// decoder declare failure on those windows instead of shipping the
/// hallucination.
///
/// The cost is real: a higher entropy threshold sends more windows into
/// whisper.cpp's temperature-fallback loop, which re-decodes a window up to ~6
/// times. That is affordable here because dictation is one short utterance at a
/// time — a continuous transcription load would not be able to pay it.
pub const ENTROPY_THOLD: &str = "2.8";

/// Log-probability threshold for "decoder failed" (whisper.cpp default: -1.00).
/// Raised alongside [`ENTROPY_THOLD`]; the two work as a pair.
pub const LOGPROB_THOLD: &str = "-1.25";

/// Ceiling on auto-selected decode threads.
///
/// Whisper's encoder stops scaling well past this, and on a many-core machine
/// an unbounded thread count starves the rest of the desktop during the decode
/// — which the user notices as the whole system stuttering when they dictate.
const MAX_AUTO_THREADS: usize = 12;

/// Leave a quarter of the machine for everything else (the browser or editor
/// the transcript is about to be typed into, most importantly).
const AUTO_THREAD_RATIO: f64 = 0.75;

/// Decode threads to use when the user has not pinned a value.
pub fn auto_threads() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let scaled = (cores as f64 * AUTO_THREAD_RATIO).floor() as usize;
    scaled.clamp(1, MAX_AUTO_THREADS)
}

/// Resolve the thread count, honouring an explicit user setting.
///
/// `setting` is the raw `whisper_threads` value: absent, empty or `"auto"` all
/// mean "decide for me". A garbage value is treated the same way rather than
/// failing the transcription — a typo in a settings box should not stop
/// dictation working.
pub fn resolve_threads(setting: Option<&str>) -> usize {
    match setting.map(str::trim) {
        None | Some("") | Some("auto") => auto_threads(),
        Some(raw) => raw
            .parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .map(|n| n.min(64))
            .unwrap_or_else(auto_threads),
    }
}

/// Mel frames whisper's encoder always runs over: 30 s at one frame per 20 ms.
///
/// The encoder pads every input to this length, so a three-second dictation
/// costs the same as half a minute of speech unless the context is clamped.
const FULL_AUDIO_CTX: u32 = 1500;
const FRAMES_PER_SECOND: u32 = FULL_AUDIO_CTX / 30;

/// Slack above the utterance's own length, so a clamp never truncates the tail
/// of a word. Two and a half seconds.
const AUDIO_CTX_MARGIN: u32 = 128;

/// Floor, because below this the encoder's own overheads dominate and the
/// margin stops being meaningful.
const MIN_AUDIO_CTX: u32 = 256;

/// Encoder context for an utterance of `audio_seconds`.
///
/// Measured on a mid-range NVIDIA GPU with `base.en`: a 2 s utterance decoded
/// in 346 ms at full context and 180 ms clamped, with an identical transcript.
/// The win grows with how short the utterance is, which for dictation is most
/// of them.
pub fn audio_ctx_for(audio_seconds: u32) -> u32 {
    audio_seconds
        .saturating_mul(FRAMES_PER_SECOND)
        .saturating_add(AUDIO_CTX_MARGIN)
        .clamp(MIN_AUDIO_CTX, FULL_AUDIO_CTX)
}

/// Runtime knobs shared by the CLI and server local backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeConfig {
    pub threads: usize,
    /// Whether the backend may use the GPU. `false` passes whisper.cpp's
    /// `--no-gpu`; it is not the same as "no GPU is present" — a CPU-only build
    /// ignores the flag either way.
    pub use_gpu: bool,
}

impl Default for DecodeConfig {
    fn default() -> Self {
        Self {
            threads: auto_threads(),
            // GPU-first: a build without GPU support simply ignores this, and a
            // build with it is why the user installed the pack.
            use_gpu: true,
        }
    }
}

impl DecodeConfig {
    /// The decoder arguments both whisper.cpp front-ends accept, in the order
    /// they appear in `--help`. Kept as owned strings because the numeric ones
    /// are formatted at runtime.
    pub fn args(&self) -> Vec<String> {
        let mut args = vec![
            "-t".into(),
            self.threads.to_string(),
            "-et".into(),
            ENTROPY_THOLD.into(),
            "-lpt".into(),
            LOGPROB_THOLD.into(),
            // Suppress non-speech tokens: whisper otherwise transcribes room
            // noise as bracketed stage directions ("(keyboard clacking)"),
            // which is never what someone dictating into a text field wants.
            "-sns".into(),
            // Flash attention. Measured at ~25% off the encoder on a mid-range
            // NVIDIA GPU, and the default from whisper.cpp 1.8 on.
            "-fa".into(),
        ];
        if !self.use_gpu {
            args.push("-ng".into());
        }
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_ctx_tracks_the_utterance_and_stays_in_range() {
        // A short dictation is clamped well under the full 30 s window.
        assert_eq!(audio_ctx_for(2), MIN_AUDIO_CTX);
        assert_eq!(audio_ctx_for(10), 10 * FRAMES_PER_SECOND + AUDIO_CTX_MARGIN);
        // Never past what the encoder actually has, however long the recording.
        assert_eq!(audio_ctx_for(30), FULL_AUDIO_CTX);
        assert_eq!(audio_ctx_for(600), FULL_AUDIO_CTX);
    }

    /// The margin is what keeps a clamp from cutting off the end of a word.
    #[test]
    fn audio_ctx_always_covers_the_audio_it_is_given() {
        for seconds in 1..=29 {
            assert!(
                audio_ctx_for(seconds) >= seconds * FRAMES_PER_SECOND,
                "{seconds}s would be truncated"
            );
        }
    }

    #[test]
    fn args_carry_the_raised_thresholds() {
        let args = DecodeConfig {
            threads: 4,
            use_gpu: true,
        }
        .args();
        let joined = args.join(" ");
        assert!(joined.contains("-et 2.8"), "{joined}");
        assert!(joined.contains("-lpt -1.25"), "{joined}");
        assert!(args.iter().any(|a| a == "-fa"), "{joined}");
        // GPU is the default, so nothing opts out of it.
        assert!(!args.iter().any(|a| a == "-ng"), "{joined}");
    }

    #[test]
    fn cpu_only_config_disables_gpu() {
        let args = DecodeConfig {
            threads: 2,
            use_gpu: false,
        }
        .args();
        assert!(args.iter().any(|a| a == "-ng"), "{args:?}");
    }

    #[test]
    fn auto_threads_stays_in_range() {
        let n = auto_threads();
        assert!((1..=MAX_AUTO_THREADS).contains(&n), "got {n}");
    }

    #[test]
    fn explicit_thread_setting_wins() {
        assert_eq!(resolve_threads(Some("3")), 3);
        // Clamped, not rejected: a huge value is a user mistake, not a failure.
        assert_eq!(resolve_threads(Some("999")), 64);
    }

    #[test]
    fn blank_or_broken_settings_fall_back_to_auto() {
        let auto = auto_threads();
        for raw in [
            None,
            Some(""),
            Some("auto"),
            Some("many"),
            Some("0"),
            Some("-4"),
        ] {
            assert_eq!(resolve_threads(raw), auto, "input {raw:?}");
        }
    }
}
