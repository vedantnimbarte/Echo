/*!
 * SOURCE OF TRUTH KEYWORDS: is_hallucination, normalise_for_match, rms_dbfs,
 *   is_digital_silence, SILENCE_PEAK_FLOOR, LIKELY_SILENCE_RMS_DBFS
 * WHAT:  The guard that stands between a near-silent buffer and the words
 *        "Thanks for watching!" appearing in the user's document.
 * WHY:   Whisper invents text from silence, and the inventions are a small,
 *        well-known set of subtitle strings baked in by training-data
 *        contamination.
 *
 *        ECHO ALREADY HAD THE UPSTREAM HALF of this. `core/vad/gate.rs` refuses
 *        to send a silent utterance to the decoder at all, and its own comment
 *        names this exact failure. That gate catches the common case and this
 *        does not replace it. What it cannot catch is the case where the buffer
 *        genuinely CONTAINS speech — so the gate passes it — and the decoder
 *        still returns boilerplate for it: a trailing fragment that is mostly
 *        breath, a chunk clipped mid-pause, a hotkey released a beat late.
 *
 *        THE RULE THAT MAKES A BLOCKLIST SAFE rather than destructive is that a
 *        phrase is dropped ONLY when it is the entire segment. "Thank you"
 *        mid-sentence is a real thing people say, and an editor that deletes it
 *        has broken dictation to fix a cosmetic bug.
 *
 *        The riskiest entries are short ones a person might genuinely utter
 *        alone, so those carry a second condition: the audio must also have
 *        been too quiet to be speech. That qualifier is measured from the AUDIO
 *        rather than taken from the decoder's own confidence, because the
 *        confidence fields are not comparable across the engines Echo supports
 *        — whisper.cpp via CLI, a resident server, and NeMo all report them
 *        differently or not at all, and a guard that reads one of them is a
 *        guard that silently does nothing on the other two.
 * WHERE: Applied by core/asr/mod.rs::transcribe_utterance to the finished
 *        segment, after the VAD gate has already had its say. The phrase table
 *        lives in blocklist.rs.
 */

use super::blocklist::{blocklist_for, DropRule};

/**
 * SOURCE OF TRUTH KEYWORDS: SILENCE_PEAK_FLOOR, is_digital_silence
 * WHAT:  Peak amplitude below which a buffer is treated as containing nothing.
 * WHY:   Deliberately set at digital silence rather than at a noise floor. This
 *        is a last-resort backstop, not the VAD: a real microphone in a quiet
 *        room sits far above it, so this can only ever reject a buffer that is
 *        genuinely empty — a dropped device, a muted input, a zero-filled
 *        fragment. Raising it to something that "works better" would start
 *        eating quiet speech, which is the failure this guard is supposed to be
 *        too dumb to cause.
 * WHERE: Checked by `is_digital_silence`.
 */
pub const SILENCE_PEAK_FLOOR: f32 = 1.0e-4;

/**
 * SOURCE OF TRUTH KEYWORDS: LIKELY_SILENCE_RMS_DBFS
 * WHAT:  The loudness below which a buffer is too quiet to have been speech.
 * WHY:   -50 dBFS, and the number is chosen against Echo's own capture path
 *        rather than in the abstract. Audio here has already been through
 *        `core::audio::Agc`, which puts speech near 0.05 RMS — about -26 dBFS —
 *        whatever the microphone. A boosted room floor sits an order of
 *        magnitude below that. -50 leaves a wide margin under the quietest real
 *        speech the AGC produces while still being far above digital silence,
 *        so the `WhenLikelySilence` entries only fire on buffers that really
 *        had nothing in them.
 * WHERE: Used by `is_hallucination` to qualify the riskier blocklist entries.
 */
pub const LIKELY_SILENCE_RMS_DBFS: f32 = -50.0;

/// Peak-based emptiness. See SILENCE_PEAK_FLOOR.
pub fn is_digital_silence(audio: &[f32]) -> bool {
    audio.iter().fold(0.0f32, |peak, s| peak.max(s.abs())) < SILENCE_PEAK_FLOOR
}

/// Loudness in dBFS. Returns a very negative number for an empty buffer rather
/// than negative infinity, so callers can compare without special-casing.
pub fn rms_dbfs(audio: &[f32]) -> f32 {
    if audio.is_empty() {
        return -120.0;
    }
    let sum: f64 = audio.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    let rms = (sum / audio.len() as f64).sqrt() as f32;
    if rms <= 0.0 {
        return -120.0;
    }
    20.0 * rms.log10()
}

/**
 * SOURCE OF TRUTH KEYWORDS: normalise_for_match
 * WHAT:  Lowercases, strips punctuation and collapses whitespace, so a segment
 *        can be compared against the already-normalised blocklist.
 * WHY:   Whisper's boilerplate arrives punctuated and capitalised, and differs
 *        run to run — "Thanks for watching!" and "thanks for watching." are the
 *        same invention. Comparing raw text would match neither.
 *
 *        Non-alphanumeric characters are dropped rather than replaced with a
 *        space, which is what makes "amara.org" normalise to "amaraorg" — the
 *        form the table stores.
 * WHERE: Called by is_hallucination on the segment text; the table in
 *        blocklist.rs is written pre-normalised.
 */
pub fn normalise_for_match(text: &str) -> String {
    let stripped: String = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    stripped
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/**
 * SOURCE OF TRUTH KEYWORDS: is_hallucination
 * WHAT:  Whether this segment is invented boilerplate that should be dropped.
 * WHY:   The two rules of the module WHY, in order: the phrase must be the
 *        WHOLE segment, and a phrase a person might plausibly say alone must
 *        also have come from audio too quiet to be speech.
 * WHERE: core/asr/mod.rs::transcribe_utterance.
 */
pub fn is_hallucination(text: &str, language: Option<&str>, audio: &[f32]) -> bool {
    is_invented(text, language, is_digital_silence(audio), rms_dbfs(audio))
}

/**
 * SOURCE OF TRUTH KEYWORDS: is_invented
 * WHAT:  The same decision, taking the two audio measurements rather than the
 *        audio.
 * WHY:   The caller in core/asr/mod.rs hands its buffer to the decoder and no
 *        longer owns it by the time there is a transcript to judge. Measuring
 *        first and passing two floats is what avoids cloning an utterance —
 *        which for a minute of speech is several megabytes copied to answer a
 *        question that needs eight bytes.
 * WHERE: core/asr/mod.rs::transcribe_utterance; `is_hallucination` wraps it.
 */
pub fn is_invented(
    text: &str,
    language: Option<&str>,
    was_digital_silence: bool,
    rms_dbfs: f32,
) -> bool {
    let normalised = normalise_for_match(text);
    if normalised.is_empty() {
        return false;
    }

    // Nothing came in, so nothing real can have come out. This is the one case
    // that does not need the phrase table at all.
    if was_digital_silence {
        return true;
    }

    let likely_silence = rms_dbfs < LIKELY_SILENCE_RMS_DBFS;

    blocklist_for(language).any(|phrase| {
        // WHOLE SEGMENT ONLY. See the module WHY — this is the condition that
        // makes the table safe to have at all.
        phrase.text == normalised
            && match phrase.rule {
                DropRule::Always => true,
                DropRule::WhenLikelySilence => likely_silence,
            }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Speech-level audio, so the `WhenLikelySilence` qualifier does not fire.
    fn loud() -> Vec<f32> {
        (0..16_000)
            .map(|i| ((i as f32) * 0.01).sin() * 0.05)
            .collect()
    }

    /// Audible but far below speech.
    fn quiet() -> Vec<f32> {
        (0..16_000)
            .map(|i| ((i as f32) * 0.01).sin() * 0.0005)
            .collect()
    }

    #[test]
    fn subtitle_credits_go_whatever_the_audio_was() {
        assert!(is_hallucination(
            "Thanks for watching!",
            Some("en"),
            &loud()
        ));
        assert!(is_hallucination(
            "Subtitles by the Amara.org community",
            Some("en"),
            &loud()
        ));
        // Punctuation and case differ run to run; normalisation is what makes
        // one table entry cover all of them.
        assert!(is_hallucination(
            "THANKS FOR WATCHING.",
            Some("en"),
            &loud()
        ));
    }

    /// The rule the whole guard rests on. If this ever fails, the blocklist has
    /// become an editor that deletes things people said.
    #[test]
    fn a_blocked_phrase_inside_a_sentence_is_kept() {
        assert!(!is_hallucination(
            "Thank you for the update, I'll take a look this afternoon.",
            Some("en"),
            &loud()
        ));
        assert!(!is_hallucination(
            "Please subscribe to the mailing list and let me know.",
            Some("en"),
            &loud()
        ));
    }

    /// "Thank you" alone is a real thing to dictate, so it needs the audio to
    /// have been too quiet to be speech before it is touched.
    #[test]
    fn a_plausible_phrase_survives_when_the_audio_was_loud() {
        assert!(!is_hallucination("Thank you", Some("en"), &loud()));
        assert!(!is_hallucination("Okay", Some("en"), &loud()));
    }

    #[test]
    fn a_plausible_phrase_goes_when_the_audio_was_near_silent() {
        assert!(is_hallucination("Thank you", Some("en"), &quiet()));
        assert!(is_hallucination("You", Some("en"), &quiet()));
    }

    /// A buffer with nothing in it cannot have produced words, so the phrase
    /// table is not consulted at all.
    #[test]
    fn anything_decoded_from_an_empty_buffer_is_invented() {
        let silence = vec![0.0f32; 16_000];
        assert!(is_hallucination("Hello there", Some("en"), &silence));
    }

    /// A language Echo has no table for falls back to the universal list rather
    /// than to English — applying English single-word entries to another
    /// language would be deleting words on a guess.
    #[test]
    fn an_unlisted_language_only_gets_the_universal_entries() {
        // Universal still applies.
        assert!(is_hallucination("[MUSIC]", Some("sv"), &loud()));
        // The English single-word entries must not reach it.
        assert!(!is_hallucination("you", Some("sv"), &quiet()));
    }

    #[test]
    fn ordinary_dictation_is_untouched() {
        for text in [
            "Let's move the meeting to Thursday.",
            "git commit -m fix the parser",
            "Thanks",
        ] {
            assert!(
                !is_hallucination(text, Some("en"), &loud()),
                "dropped {text:?}"
            );
        }
    }

    #[test]
    fn normalisation_folds_punctuation_and_case() {
        assert_eq!(normalise_for_match("Amara.org"), "amaraorg");
        assert_eq!(
            normalise_for_match("  Thanks   for watching!  "),
            "thanks for watching"
        );
        assert_eq!(normalise_for_match("[BLANK_AUDIO]"), "blankaudio");
    }

    /// The floor is a backstop, not a detector: real quiet speech must clear it
    /// comfortably or this guard starts eating words.
    #[test]
    fn the_silence_floor_is_far_below_real_speech() {
        assert!(!is_digital_silence(&quiet()));
        assert!(is_digital_silence(&vec![0.0; 100]));
        assert!(rms_dbfs(&loud()) > LIKELY_SILENCE_RMS_DBFS);
    }
}
