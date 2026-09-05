//! Whole-utterance speech gate.
//!
//! The VAD decides, chunk by chunk, whether audio is worth *capturing*. This
//! gate decides whether a finished utterance is worth *transcribing* — a
//! separate question, and the reason it exists is that whisper does something
//! actively harmful with near-silence: it emits training-data boilerplate
//! ("Thank you for watching", "Продолжение следует...") with high confidence.
//! Raised decoder thresholds (see [`crate::core::asr::decode_opts`]) catch most
//! of that; refusing to send silence to the decoder at all catches the rest.
//!
//! It runs at the point where the audio is already buffered — the default
//! `transcribe_stream` — so it costs one pass over a buffer we are holding
//! anyway, and it protects cloud providers too, where a skipped call is also a
//! saved round-trip and a saved API charge.

/// Below this peak RMS the buffer is silence: a muted mic, a hotkey pressed and
/// released, a wake word that fired on nothing.
const SILENCE_RMS: f32 = 0.002;

/// A 30 ms window counts as speech only if it clears both of these. Requiring
/// peak *and* RMS rejects steady low-level noise (fan, mains hum) that can
/// carry a respectable RMS without any of the transients speech always has.
const WINDOW_RMS: f32 = 0.003;
const WINDOW_PEAK: f32 = 0.02;

/// A buffer this loud is speech regardless of the window count — it protects a
/// short, sharp utterance ("yes", "stop") from being gated out.
const STRONG_RMS: f32 = 0.006;

/// 30 ms at 16 kHz. Long enough to contain a syllable's energy, short enough
/// that a one-word utterance still produces several windows.
const WINDOW_SAMPLES: usize = 480;

/// Why an utterance was, or was not, sent to the decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateDecision {
    /// Send it to ASR.
    Speech,
    /// Nothing above the noise floor.
    Silence,
    /// Audible, but without the shape of speech.
    InsufficientSpeech,
}

impl GateDecision {
    pub fn should_transcribe(self) -> bool {
        matches!(self, GateDecision::Speech)
    }

    /// Stable label for logs.
    pub fn reason(self) -> &'static str {
        match self {
            GateDecision::Speech => "speech_detected",
            GateDecision::Silence => "silence",
            GateDecision::InsufficientSpeech => "insufficient_speech",
        }
    }
}

/// Decide whether `samples` (16 kHz mono f32) is worth transcribing.
///
/// An empty buffer returns [`GateDecision::Silence`]. A buffer shorter than one
/// window is judged on its own peak/RMS rather than being rejected outright:
/// the caller's VAD already thought it was speech, and truncating that decision
/// on a length technicality would drop legitimate one-word utterances.
pub fn speech_gate(samples: &[f32]) -> GateDecision {
    if samples.is_empty() {
        return GateDecision::Silence;
    }

    let mut peak_rms = 0.0_f32;
    let mut speech_windows = 0usize;

    for window in samples.chunks(WINDOW_SAMPLES) {
        let (sum_sq, peak) = window.iter().fold((0.0_f32, 0.0_f32), |(s, p), v| {
            (s + v * v, p.max(v.abs()))
        });
        let rms = (sum_sq / window.len() as f32).sqrt();
        peak_rms = peak_rms.max(rms);

        if rms >= WINDOW_RMS && peak >= WINDOW_PEAK {
            speech_windows += 1;
        }
    }

    if peak_rms < SILENCE_RMS {
        return GateDecision::Silence;
    }
    if speech_windows >= 1 || peak_rms >= STRONG_RMS {
        return GateDecision::Speech;
    }
    GateDecision::InsufficientSpeech
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sine at `amp`, one second long at 16 kHz.
    fn tone(amp: f32) -> Vec<f32> {
        (0..16_000)
            .map(|i| (i as f32 * 0.05).sin() * amp)
            .collect()
    }

    #[test]
    fn empty_and_digital_silence_are_gated_out() {
        assert_eq!(speech_gate(&[]), GateDecision::Silence);
        assert_eq!(speech_gate(&vec![0.0; 16_000]), GateDecision::Silence);
    }

    #[test]
    fn a_normal_utterance_passes() {
        assert_eq!(speech_gate(&tone(0.2)), GateDecision::Speech);
    }

    #[test]
    fn room_tone_below_the_floor_is_gated_out() {
        // Audible to a meter, nothing like speech.
        assert_eq!(speech_gate(&tone(0.0005)), GateDecision::Silence);
    }

    #[test]
    fn a_short_loud_word_still_passes() {
        // ~90 ms, the "yes"/"stop" case a window-count-only rule would drop.
        assert_eq!(speech_gate(&tone(0.3)[..1_440]), GateDecision::Speech);
    }

    #[test]
    fn decisions_carry_a_reason_for_logs() {
        assert_eq!(GateDecision::Silence.reason(), "silence");
        assert!(GateDecision::Speech.should_transcribe());
        assert!(!GateDecision::InsufficientSpeech.should_transcribe());
    }
}
