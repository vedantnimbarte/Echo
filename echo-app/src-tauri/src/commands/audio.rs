use serde::Serialize;
use tauri::State;

use crate::{core::audio::AudioDevice, error::Result, state::AppState};

#[tauri::command]
#[specta::specta]
pub fn get_audio_devices(state: State<'_, AppState>) -> Result<Vec<AudioDevice>> {
    state.audio.list_input_devices()
}

/// Whether the neural voice-activity model loaded at startup.
///
/// Settings offers a choice between it and the energy detector, and the choice
/// is a lie if the model never loaded — `start_recording` falls back to energy
/// whatever the setting says. So the picker asks, and says so when the answer
/// is no, rather than letting someone select a detector that is not running.
#[tauri::command]
#[specta::specta]
pub fn silero_available(state: State<'_, AppState>) -> bool {
    state.silero.is_some()
}

/// How long the microphone test listens.
const TEST_DURATION: std::time::Duration = std::time::Duration::from_millis(3_000);

/// What a level test found.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct InputTest {
    /// Loudest sample heard, 0.0..=1.0.
    pub peak: f32,
    /// The same figure in dBFS, which is the scale every OS meter uses.
    pub peak_dbfs: f32,
    /// `"good"`, `"quiet"`, `"faint"` or `"silent"`.
    pub verdict: &'static str,
    pub advice: &'static str,
}

/// Levels, in raw peak amplitude, that separate the verdicts.
///
/// Deliberately judged *before* the capture AGC: the meter in the pill shows
/// lifted audio, so it now looks lively on a microphone that is barely working.
/// This test is the one place that reports what the device actually delivers,
/// which is what tells "your input is fine" from "your mic is in the wrong
/// jack and Echo is carrying it on gain alone".
const GOOD_PEAK: f32 = 0.05;
const QUIET_PEAK: f32 = 0.01;
const FAINT_PEAK: f32 = 0.002;

/// Classify a measured peak.
fn classify(peak: f32) -> (&'static str, &'static str) {
    if peak >= GOOD_PEAK {
        ("good", "Your microphone sounds healthy.")
    } else if peak >= QUIET_PEAK {
        (
            "quiet",
            "Quiet, but Echo can work with it — dictation is amplified automatically.",
        )
    } else if peak >= FAINT_PEAK {
        (
            "faint",
            "Very quiet. Echo will amplify it, but raising the input level in your \
             system sound settings (or setting the jack to Microphone rather than \
             Line In) will transcribe more accurately.",
        )
    } else {
        (
            "silent",
            "Echo heard nothing. Check that this is the right input, that it is not \
             muted, and that the microphone is plugged in.",
        )
    }
}

/// Listen for a few seconds and report how loud the input actually is.
///
/// Refuses while a dictation is running rather than stealing the stream from
/// it: the device is shared, and the words being spoken belong to the
/// recording, not to a test.
#[tauri::command]
#[specta::specta]
pub async fn test_input_level(
    state: State<'_, AppState>,
    device: Option<String>,
) -> Result<InputTest> {
    if *state.recording.lock().unwrap() {
        return Err(crate::error::EchoError::AudioDevice(
            "Echo is recording right now. Stop the dictation and test again.".into(),
        ));
    }

    let device = device.filter(|d| !d.is_empty());
    let mut rx = state.audio.start_capture(device.as_deref())?;

    let deadline = tokio::time::Instant::now() + TEST_DURATION;
    let mut peak = 0.0_f32;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(chunk)) => {
                for s in chunk {
                    peak = peak.max(s.abs());
                }
            }
            // The device stopped delivering, which is itself an answer.
            Ok(None) => break,
            Err(_) => break,
        }
    }

    // Leave the microphone warm rather than closed: someone who just tested it
    // is usually about to dictate.
    state.audio.stop_capture_warm(device.as_deref());

    let (verdict, advice) = classify(peak);
    Ok(InputTest {
        peak,
        peak_dbfs: 20.0 * peak.max(1e-6).log10(),
        verdict,
        advice,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_healthy_microphone_is_not_complained_about() {
        assert_eq!(classify(0.3).0, "good");
        assert_eq!(classify(GOOD_PEAK).0, "good");
    }

    /// The case this whole test exists for: a mono mic in a stereo Line In jack
    /// measured 0.014 peak. Echo transcribes it, but the user deserves to know
    /// why it is being carried on 20 dB of gain.
    #[test]
    fn a_line_level_input_is_reported_as_quiet_not_broken() {
        assert_eq!(classify(0.014).0, "quiet");
        assert_eq!(classify(0.004).0, "faint");
    }

    #[test]
    fn a_dead_input_is_called_dead() {
        assert_eq!(classify(0.0).0, "silent");
        assert_eq!(classify(0.0005).0, "silent");
    }
}
