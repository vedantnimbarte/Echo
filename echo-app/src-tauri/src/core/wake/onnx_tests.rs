//! Tests against the actual openWakeWord models.
//!
//! The chain in `onnx.rs` was written from upstream's reference implementation
//! and had never been run against real model files, so none of it was known to
//! work: the tensor shapes, the `/10 + 2` feature transform and the streaming
//! buffer bookkeeping were all assumptions. These check the assumptions, and
//! the first run of them found a real defect — see
//! [`a_freshly_armed_spotter_hears_the_phrase_immediately`].
//!
//! The models are a ~3.6 MB download rather than a repository file, so every
//! test here skips when they are absent (as on CI) rather than failing. Get
//! them by enabling the wake word in the app, or point `ECHO_WAKE_MODELS` at a
//! directory holding `melspectrogram.onnx`, `embedding_model.onnx` and
//! `hey_jarvis_v0.1.onnx`.
//!
//! The speech fixtures are synthesised. openWakeWord is itself trained largely
//! on synthetic speech, so text-to-speech is a fair positive — and, unlike a
//! recording of somebody talking, a reproducible one.

use super::*;
use std::path::PathBuf;
use std::sync::Arc;

/// Where the downloaded models live, if they have been downloaded at all.
fn model_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("ECHO_WAKE_MODELS") {
        let dir = PathBuf::from(dir);
        return dir.is_dir().then_some(dir);
    }
    let base = std::env::var("APPDATA").ok()?;
    let dir = PathBuf::from(base).join("com.echo.app").join("wake");
    dir.is_dir().then_some(dir)
}

fn load() -> Option<Arc<WakeModel>> {
    let dir = model_dir()?;
    let melspec = dir.join("melspectrogram.onnx");
    let embedding = dir.join("embedding_model.onnx");
    let classifier = dir.join("hey_jarvis_v0.1.onnx");
    if !(melspec.exists() && embedding.exists() && classifier.exists()) {
        return None;
    }
    Some(Arc::new(
        WakeModel::load(&melspec, &embedding, &classifier).expect("the models failed to load"),
    ))
}

fn clip(name: &str) -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("wake")
        .join(name);
    let mut reader = hound::WavReader::open(&path)
        .unwrap_or_else(|e| panic!("missing fixture {}: {e}", path.display()));
    reader
        .samples::<i16>()
        .filter_map(|s| s.ok())
        .map(|s| s as f32 / 32768.0)
        .collect()
}

/// `lead_ms` of quiet followed by the clip — what a microphone actually
/// delivers to a listener that has been running for a while.
fn with_lead(lead_ms: usize, name: &str) -> Vec<f32> {
    let mut audio = vec![0.0f32; 16 * lead_ms];
    audio.extend_from_slice(&clip(name));
    audio
}

/// Drive the spotter the way the capture task does, in microphone-sized
/// pieces rather than one slice, returning the first detection.
fn fires(model: Arc<WakeModel>, threshold: f32, audio: &[f32]) -> Option<f32> {
    let mut spotter = WakeSpotter::new(model, threshold);
    let mut first = None;
    for piece in audio.chunks(1_600) {
        if let Some(score) = spotter.detect(piece) {
            first = first.or(Some(score));
        }
    }
    first
}

fn skip() -> bool {
    if model_dir().is_none() {
        eprintln!("skipped: openWakeWord models are not downloaded");
        return true;
    }
    false
}

/// The whole point of the feature: the phrase is heard, and nothing else is.
#[test]
fn the_phrase_fires_and_other_speech_stays_quiet() {
    if skip() {
        return;
    }
    let Some(model) = load() else { return };

    let score = fires(
        model.clone(),
        crate::core::wake::DEFAULT_THRESHOLD,
        &with_lead(1_000, "hey_jarvis.wav"),
    )
    .expect("'hey jarvis' was not detected at the shipped default threshold");
    assert!(score >= 0.5, "detected, but only at {score:.4}");

    // A different wake phrase must not fire this classifier, and ordinary
    // conversation must not either. A wake word that triggers on speech is
    // worse than no wake word at all, because it starts recording unasked.
    for name in ["alexa.wav", "unrelated_speech.wav"] {
        let spurious = fires(
            model.clone(),
            crate::core::wake::DEFAULT_THRESHOLD,
            &with_lead(1_000, name),
        );
        assert!(spurious.is_none(), "{name} falsely fired at {spurious:?}");
    }
}

/// Regression: the spotter was deaf for its first ~2.1 seconds.
///
/// The classifier cannot run until the mel buffer holds a full embedding
/// window and the feature buffer a full classifier window — 26 chunks of
/// audio. Nothing reported this; the spotter simply returned no detection.
/// Because the listener is re-armed after every dictation, the phrase was
/// silently missed at exactly the moment someone was most likely to say it
/// again. `WakeSpotter::new` now primes the buffers with silence.
#[test]
fn a_freshly_armed_spotter_hears_the_phrase_immediately() {
    if skip() {
        return;
    }
    let Some(model) = load() else { return };

    for lead_ms in [0, 250, 500] {
        let audio = with_lead(lead_ms, "hey_jarvis.wav");
        assert!(
            fires(model.clone(), crate::core::wake::DEFAULT_THRESHOLD, &audio).is_some(),
            "missed the phrase with only {lead_ms}ms of lead-in"
        );
    }
}

/// The listener does not feed the spotter continuously: `commands/wake.rs`
/// gates it behind the VAD and skips every chunk that is not speech, so the
/// three-stage chain only runs on frames that might contain a phrase.
///
/// That is a real risk worth pinning down, because openWakeWord is a streaming
/// model: dropping frames splices non-contiguous audio together, and the chain
/// reports nothing when it is confused — it simply stops detecting. It turns
/// out to be fine, because what gets dropped is silence either way, but
/// "it turned out fine" is exactly the kind of thing that quietly stops being
/// true when the VAD is retuned.
#[test]
fn the_vad_gate_the_listener_uses_does_not_break_detection() {
    use crate::core::vad::{EnergyVad, Vad};

    if skip() {
        return;
    }
    let Some(model) = load() else { return };

    // Long enough that the VAD has settled into reporting silence — the state
    // a listener is actually in when somebody finally speaks. A short lead-in
    // does not discriminate, because `EnergyVad` passes its first 15 chunks
    // regardless of what they contain.
    let audio = with_lead(6_000, "hey_jarvis.wav");

    let continuous = fires(model.clone(), crate::core::wake::DEFAULT_THRESHOLD, &audio)
        .expect("the phrase must be detected when fed continuously");

    let mut spotter = WakeSpotter::new(model, crate::core::wake::DEFAULT_THRESHOLD);
    let mut vad: Box<dyn Vad> = Box::new(EnergyVad::new(0.01));
    let mut gated = None;
    let mut dropped = 0;
    for piece in audio.chunks(1_600) {
        if !vad.is_speech(piece) {
            dropped += 1;
            continue;
        }
        if let Some(score) = spotter.detect(piece) {
            gated = gated.or(Some(score));
        }
    }

    assert!(dropped > 0, "the VAD dropped nothing, so this proved nothing");
    let gated = gated.expect("the phrase was missed once the VAD gate was applied");
    assert!(
        (gated - continuous).abs() < 0.2,
        "gating changed the score materially: {gated:.4} vs {continuous:.4}"
    );
}

/// The streaming buffers must stay faithful to the obvious batch computation.
///
/// Nothing else would notice them drifting apart: a misaligned window does not
/// error, it just quietly stops detecting — which is how the chain sat unnoticed
/// in an unverified state for so long.
#[test]
fn streaming_stays_close_to_a_batch_reference() {
    if skip() {
        return;
    }
    let Some(model) = load() else { return };
    let audio = with_lead(3_000, "hey_jarvis.wav");

    // Reference: one melspectrogram over everything, then slide both windows.
    let mel = model.melspectrogram(&audio).unwrap();
    let frames = mel.len() / MEL_BINS;
    let mut embeddings = Vec::new();
    let mut i = 0;
    while i + EMBED_WINDOW <= frames {
        let window = &mel[i * MEL_BINS..(i + EMBED_WINDOW) * MEL_BINS];
        embeddings.extend_from_slice(&model.embed(window).unwrap());
        i += MEL_FRAMES_PER_CHUNK;
    }
    let vectors = embeddings.len() / EMBED_DIM;
    let mut batch_best = 0.0f32;
    let mut j = 0;
    while j + CLASSIFIER_FRAMES <= vectors {
        let window = &embeddings[j * EMBED_DIM..(j + CLASSIFIER_FRAMES) * EMBED_DIM];
        batch_best = batch_best.max(model.classify(window).unwrap());
        j += 1;
    }
    assert!(
        batch_best > 0.9,
        "the reference itself failed to detect the phrase ({batch_best:.4}); that \
         points at the models or the feature transform, not at the buffering"
    );

    // The streaming path, scored at every step so it can be compared.
    let mut spotter = WakeSpotter::new(model.clone(), 0.99);
    let mut stream_best = 0.0f32;
    for piece in audio.chunks(1_600) {
        spotter.detect(piece);
        if spotter.feats.len() >= CLASSIFIER_FRAMES * EMBED_DIM {
            let start = spotter.feats.len() - CLASSIFIER_FRAMES * EMBED_DIM;
            stream_best = stream_best.max(model.classify(&spotter.feats[start..]).unwrap());
        }
    }
    assert!(
        stream_best > 0.8,
        "streaming peaked at {stream_best:.4} against a batch reference of \
         {batch_best:.4} — the incremental buffers have drifted"
    );
}
