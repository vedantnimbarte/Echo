//! openWakeWord inference: a three-stage ONNX chain run through the same
//! statically-linked ONNX Runtime that already serves the Silero VAD, so wake
//! word detection adds no new runtime dependency and nothing to bundle.
//!
//! ```text
//! audio (16 kHz f32) → melspectrogram.onnx → 32-bin mel frames
//!                    → embedding_model.onnx (76 mel frames → 96-d vector)
//!                    → <phrase>.onnx        (16 vectors → score 0..1)
//! ```
//!
//! Each model has exactly one input, so they are fed positionally and we never
//! have to guess ONNX input names.

use std::path::Path;
use std::sync::Mutex;

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;

use crate::error::{EchoError, Result};

/// Mel bins produced per frame by the melspectrogram model.
pub const MEL_BINS: usize = 32;
/// Mel frames the embedding model consumes per 96-d vector.
pub const EMBED_WINDOW: usize = 76;
/// Width of one speech-embedding vector.
pub const EMBED_DIM: usize = 96;
/// Embedding vectors the phrase classifier consumes per score.
pub const CLASSIFIER_FRAMES: usize = 16;
/// Audio consumed per streaming step: 1280 samples = 80 ms at 16 kHz.
pub const CHUNK: usize = 1280;
/// Extra trailing samples re-fed to the melspectrogram model each step so the
/// windowed transform has the context it needs at the chunk boundary
/// (3 hops of 160 samples, matching openWakeWord's streaming implementation).
pub const MEL_PAD: usize = 480;

/// The loaded three-stage model chain for one wake phrase.
///
/// Sessions sit behind `Mutex` only because `Session::run` needs `&mut self`;
/// at most one listener runs at a time so there is no real contention.
pub struct WakeModel {
    melspec: Mutex<Session>,
    embedding: Mutex<Session>,
    classifier: Mutex<Session>,
}

fn build_session(path: &Path) -> Result<Session> {
    Session::builder()
        .map_err(|e| EchoError::Config(format!("ort session builder: {e}")))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| EchoError::Config(format!("ort opt level: {e}")))?
        .commit_from_file(path)
        .map_err(|e| {
            EchoError::Config(format!("ort load wake model {}: {e}", path.display()))
        })
}

impl WakeModel {
    /// Load the shared feature models plus one phrase classifier from disk.
    pub fn load(melspec: &Path, embedding: &Path, classifier: &Path) -> Result<Self> {
        Ok(Self {
            melspec: Mutex::new(build_session(melspec)?),
            embedding: Mutex::new(build_session(embedding)?),
            classifier: Mutex::new(build_session(classifier)?),
        })
    }

    /// Run raw audio through the melspectrogram model, returning flattened mel
    /// frames (`MEL_BINS` values each).
    ///
    /// The `/10 + 2` rescale is part of openWakeWord's feature definition — the
    /// embedding model was trained on transformed mels, so omitting it produces
    /// silently useless embeddings rather than an error.
    fn melspectrogram(&self, samples: &[f32]) -> Result<Vec<f32>> {
        let input = Tensor::from_array(([1usize, samples.len()], samples.to_vec()))
            .map_err(|e| EchoError::AsrProvider(format!("wake mel input: {e}")))?;

        let mut session = self
            .melspec
            .lock()
            .map_err(|e| EchoError::AsrProvider(format!("wake mel lock: {e}")))?;
        let outputs = session
            .run(ort::inputs![input])
            .map_err(|e| EchoError::AsrProvider(format!("wake mel run: {e}")))?;

        let (_, mel) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| EchoError::AsrProvider(format!("wake mel output: {e}")))?;

        Ok(mel.iter().map(|v| v / 10.0 + 2.0).collect())
    }

    /// Turn a window of `EMBED_WINDOW * MEL_BINS` mel values into one 96-d
    /// speech embedding.
    fn embed(&self, mel_window: &[f32]) -> Result<Vec<f32>> {
        let input = Tensor::from_array((
            [1usize, EMBED_WINDOW, MEL_BINS, 1],
            mel_window.to_vec(),
        ))
        .map_err(|e| EchoError::AsrProvider(format!("wake embed input: {e}")))?;

        let mut session = self
            .embedding
            .lock()
            .map_err(|e| EchoError::AsrProvider(format!("wake embed lock: {e}")))?;
        let outputs = session
            .run(ort::inputs![input])
            .map_err(|e| EchoError::AsrProvider(format!("wake embed run: {e}")))?;

        let (_, emb) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| EchoError::AsrProvider(format!("wake embed output: {e}")))?;

        Ok(emb.to_vec())
    }

    /// Score `CLASSIFIER_FRAMES` stacked embeddings against the wake phrase.
    fn classify(&self, features: &[f32]) -> Result<f32> {
        let input = Tensor::from_array((
            [1usize, CLASSIFIER_FRAMES, EMBED_DIM],
            features.to_vec(),
        ))
        .map_err(|e| EchoError::AsrProvider(format!("wake score input: {e}")))?;

        let mut session = self
            .classifier
            .lock()
            .map_err(|e| EchoError::AsrProvider(format!("wake score lock: {e}")))?;
        let outputs = session
            .run(ort::inputs![input])
            .map_err(|e| EchoError::AsrProvider(format!("wake score run: {e}")))?;

        let (_, score) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| EchoError::AsrProvider(format!("wake score output: {e}")))?;

        Ok(score.first().copied().unwrap_or(0.0))
    }
}

/// Mel frames produced per `CHUNK` of audio (the melspectrogram hop is 160
/// samples). The buffer bookkeeping below depends on this being exact.
const MEL_FRAMES_PER_CHUNK: usize = CHUNK / 160;

/// Chunks to ignore after a detection, so one spoken phrase fires once rather
/// than on every overlapping window. ~19 chunks ≈ 1.5 s.
const COOLDOWN_CHUNKS: u32 = 19;

/// Streaming wake-phrase detector over a loaded [`WakeModel`].
///
/// Feed it arbitrary-length audio; it buffers internally to the fixed `CHUNK`
/// the feature models expect and reports a score when the phrase fires.
pub struct WakeSpotter {
    model: std::sync::Arc<WakeModel>,
    /// Samples not yet consumed into a whole `CHUNK`.
    pending: Vec<f32>,
    /// Recent raw audio kept only for the melspectrogram overlap window.
    raw: Vec<f32>,
    /// Flattened mel frames (`MEL_BINS` values each).
    mel: Vec<f32>,
    /// Flattened embedding vectors (`EMBED_DIM` values each).
    feats: Vec<f32>,
    threshold: f32,
    cooldown: u32,
}

impl WakeSpotter {
    pub fn new(model: std::sync::Arc<WakeModel>, threshold: f32) -> Self {
        Self {
            model,
            pending: Vec::with_capacity(CHUNK * 2),
            raw: Vec::with_capacity(CHUNK + MEL_PAD),
            mel: Vec::with_capacity(EMBED_WINDOW * 2 * MEL_BINS),
            feats: Vec::with_capacity(CLASSIFIER_FRAMES * 2 * EMBED_DIM),
            threshold: threshold.clamp(0.05, 0.99),
            cooldown: 0,
        }
    }

    /// Feed audio. Returns the score of a detection, or `None`.
    ///
    /// Inference errors are logged and swallowed: a hiccup in the ONNX chain
    /// must never wedge the capture task, it just means no detection this step.
    pub fn detect(&mut self, samples: &[f32]) -> Option<f32> {
        self.pending.extend_from_slice(samples);
        let mut fired = None;

        while self.pending.len() >= CHUNK {
            let chunk: Vec<f32> = self.pending.drain(..CHUNK).collect();
            if let Some(score) = self.step(&chunk) {
                // Keep draining so `pending` never grows unbounded, but report
                // only the first detection in this batch.
                fired = fired.or(Some(score));
            }
        }
        fired
    }

    /// Advance the chain by exactly one `CHUNK`.
    fn step(&mut self, chunk: &[f32]) -> Option<f32> {
        self.raw.extend_from_slice(chunk);
        trim_front(&mut self.raw, CHUNK + MEL_PAD);

        match self.model.melspectrogram(&self.raw) {
            Ok(frames) => {
                // The melspec ran over the overlap window too, so keep only the
                // frames belonging to this chunk.
                let keep = MEL_FRAMES_PER_CHUNK * MEL_BINS;
                let start = frames.len().saturating_sub(keep);
                self.mel.extend_from_slice(&frames[start..]);
                trim_front(&mut self.mel, EMBED_WINDOW * 2 * MEL_BINS);
            }
            Err(e) => {
                tracing::debug!("wake melspectrogram failed: {e}");
                return None;
            }
        }

        if self.mel.len() < EMBED_WINDOW * MEL_BINS {
            return None;
        }
        let window_start = self.mel.len() - EMBED_WINDOW * MEL_BINS;
        match self.model.embed(&self.mel[window_start..]) {
            Ok(emb) => {
                self.feats.extend_from_slice(&emb);
                trim_front(&mut self.feats, CLASSIFIER_FRAMES * 2 * EMBED_DIM);
            }
            Err(e) => {
                tracing::debug!("wake embedding failed: {e}");
                return None;
            }
        }

        if self.cooldown > 0 {
            self.cooldown -= 1;
            return None;
        }
        if self.feats.len() < CLASSIFIER_FRAMES * EMBED_DIM {
            return None;
        }

        let feat_start = self.feats.len() - CLASSIFIER_FRAMES * EMBED_DIM;
        match self.model.classify(&self.feats[feat_start..]) {
            Ok(score) if score >= self.threshold => {
                // Drop the features that fired so the same utterance cannot be
                // re-scored as the window slides forward.
                self.feats.clear();
                self.cooldown = COOLDOWN_CHUNKS;
                Some(score)
            }
            Ok(_) => None,
            Err(e) => {
                tracing::debug!("wake classifier failed: {e}");
                None
            }
        }
    }

}

/// Keep only the last `max` elements, dropping from the front.
fn trim_front(buf: &mut Vec<f32>, max: usize) {
    if buf.len() > max {
        buf.drain(..buf.len() - max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The buffer bookkeeping in `step` assumes a chunk yields exactly 8 mel
    /// frames and that a window is a whole number of frames. If someone retunes
    /// CHUNK or MEL_PAD without re-deriving these, detection silently degrades
    /// instead of failing loudly — so pin the invariants here.
    #[test]
    fn frame_math_is_consistent() {
        assert_eq!(CHUNK % 160, 0, "chunk must be a whole number of mel hops");
        assert_eq!(MEL_FRAMES_PER_CHUNK, 8);
        assert_eq!(MEL_PAD % 160, 0, "overlap must be a whole number of hops");
        // A classifier window must be reachable from the mel buffer we retain.
        assert!(EMBED_WINDOW * 2 >= EMBED_WINDOW + MEL_FRAMES_PER_CHUNK);
    }

    #[test]
    fn trim_front_keeps_the_tail() {
        let mut buf: Vec<f32> = (0..10).map(|i| i as f32).collect();
        trim_front(&mut buf, 4);
        assert_eq!(buf, vec![6.0, 7.0, 8.0, 9.0]);

        // Under the cap it is left alone.
        let mut small = vec![1.0, 2.0];
        trim_front(&mut small, 4);
        assert_eq!(small, vec![1.0, 2.0]);
    }
}
