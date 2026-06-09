//! Silero VAD (v5) inference via ONNX Runtime.
//!
//! The v5 model processes fixed 512-sample frames at 16 kHz and carries a
//! recurrent `state` tensor between frames. [`SileroModel`] holds the loaded
//! ONNX session (shared read-only across sessions); [`SileroVad`] holds the
//! per-session ring buffer, recurrent state, and silence-debounce so it can
//! satisfy the same [`Vad`](super::Vad) contract as the energy detector.

use std::sync::Mutex;

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;

use super::Vad;
use crate::error::{EchoError, Result};

/// The Silero v5 ONNX model, embedded so VAD always works offline.
static MODEL_BYTES: &[u8] = include_bytes!("../../../resources/silero_vad.onnx");

/// 512 samples = one 32 ms frame at 16 kHz (the only window v5 accepts at 16k).
const FRAME: usize = 512;
const SAMPLE_RATE: i64 = 16_000;

/// Probability above which a frame counts as speech (rising edge).
const SPEECH_THRESHOLD: f32 = 0.5;
/// Frames of sub-threshold audio tolerated before declaring silence.
/// ~24 frames * 32 ms ≈ 770 ms of trailing pad before an utterance ends.
const SILENCE_FRAMES: usize = 24;

/// Loaded Silero ONNX session. Shared read-only behind an `Arc`; the inner
/// `Mutex` exists only because `Session::run` needs `&mut self` — there is no
/// real contention since at most one recording runs at a time.
pub struct SileroModel {
    session: Mutex<Session>,
}

impl SileroModel {
    /// Build the ONNX session from the embedded model. Returns an error if the
    /// ONNX Runtime fails to initialise (caller falls back to energy VAD).
    pub fn load() -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| EchoError::Config(format!("ort session builder: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| EchoError::Config(format!("ort opt level: {e}")))?
            .commit_from_memory(MODEL_BYTES)
            .map_err(|e| EchoError::Config(format!("ort load silero model: {e}")))?;
        Ok(Self {
            session: Mutex::new(session),
        })
    }

    /// Run one 512-sample frame, returning the speech probability and the next
    /// recurrent state. `state` is the [2,1,128] tensor (zeros to start).
    fn infer(&self, frame: &[f32], state: &[f32]) -> Result<(f32, Vec<f32>)> {
        let input = Tensor::from_array(([1usize, FRAME], frame.to_vec()))
            .map_err(|e| EchoError::AsrProvider(format!("vad input: {e}")))?;
        let state_t = Tensor::from_array(([2usize, 1, 128], state.to_vec()))
            .map_err(|e| EchoError::AsrProvider(format!("vad state: {e}")))?;
        let sr_t = Tensor::from_array(([1usize], vec![SAMPLE_RATE]))
            .map_err(|e| EchoError::AsrProvider(format!("vad sr: {e}")))?;

        let mut session = self
            .session
            .lock()
            .map_err(|e| EchoError::AsrProvider(format!("vad lock: {e}")))?;
        let outputs = session
            .run(ort::inputs![
                "input" => input,
                "state" => state_t,
                "sr" => sr_t,
            ])
            .map_err(|e| EchoError::AsrProvider(format!("vad run: {e}")))?;

        // Output 0 = speech probability, output 1 = next recurrent state.
        let (_, prob) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| EchoError::AsrProvider(format!("vad prob: {e}")))?;
        let (_, next_state) = outputs[1]
            .try_extract_tensor::<f32>()
            .map_err(|e| EchoError::AsrProvider(format!("vad next state: {e}")))?;

        Ok((prob.first().copied().unwrap_or(0.0), next_state.to_vec()))
    }
}

/// Per-session Silero detector.
pub struct SileroVad {
    model: std::sync::Arc<SileroModel>,
    /// Recurrent state carried between frames ([2,1,128] flattened).
    state: Vec<f32>,
    /// Samples not yet consumed into a full 512-frame.
    buffer: Vec<f32>,
    triggered: bool,
    silent_count: usize,
}

impl SileroVad {
    pub fn new(model: std::sync::Arc<SileroModel>) -> Self {
        Self {
            model,
            state: vec![0.0; 2 * 128],
            buffer: Vec::with_capacity(FRAME * 2),
            triggered: false,
            silent_count: 0,
        }
    }

    /// Fold one frame's probability into the trigger/silence state machine.
    fn ingest_prob(&mut self, prob: f32) {
        if prob >= SPEECH_THRESHOLD {
            self.triggered = true;
            self.silent_count = 0;
        } else if self.triggered {
            self.silent_count += 1;
            if self.silent_count > SILENCE_FRAMES {
                self.triggered = false;
            }
        }
    }
}

impl Vad for SileroVad {
    fn is_speech(&mut self, samples: &[f32]) -> bool {
        self.buffer.extend_from_slice(samples);
        while self.buffer.len() >= FRAME {
            let frame: Vec<f32> = self.buffer.drain(..FRAME).collect();
            match self.model.infer(&frame, &self.state) {
                Ok((prob, next_state)) => {
                    self.state = next_state;
                    self.ingest_prob(prob);
                }
                Err(_) => {
                    // Inference hiccup — fall back to a crude energy decision for
                    // this frame so capture never wedges.
                    let rms =
                        (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
                    self.ingest_prob(if rms > 0.01 { 1.0 } else { 0.0 });
                }
            }
        }
        self.triggered
    }

    fn reset(&mut self) {
        self.state = vec![0.0; 2 * 128];
        self.buffer.clear();
        self.triggered = false;
        self.silent_count = 0;
    }
}
