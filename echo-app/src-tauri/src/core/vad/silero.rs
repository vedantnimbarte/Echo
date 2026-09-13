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
/// v5 prepends the tail of the previous frame to each window: the graph is fed
/// `CONTEXT + FRAME` samples, not `FRAME`. The `input` dim is dynamic, so ONNX
/// Runtime accepts a bare 512-sample frame and silently returns ~0 speech
/// probabilities instead of erroring.
const CONTEXT: usize = 64;
const SAMPLE_RATE: i64 = 16_000;

/// Probability above which a frame counts as speech (rising edge).
const SPEECH_THRESHOLD: f32 = 0.5;
/// Frames of sub-threshold audio tolerated before declaring silence.
/// ~24 frames * 32 ms ≈ 770 ms of trailing pad before an utterance ends.
const SILENCE_FRAMES: usize = 24;

/// Load the ONNX Runtime dylib on Intel macOS, before the first session.
///
/// Every other target links ONNX Runtime statically and this does nothing.
/// `x86_64-apple-darwin` has no static build to link (see the `ort` note in
/// `Cargo.toml`), so there `ort` is built with `load-dynamic` and needs telling
/// where the library is. Left to itself it looks for `libonnxruntime.dylib`
/// beside the executable, in `Contents/MacOS`; the release stages it into the
/// same `resources/bin` as whisper-cli instead, which lands in
/// `Contents/Resources/bin`, which is found from the executable's directory so
/// it holds wherever the app is installed.
///
/// `ORT_DYLIB_PATH` overrides it, because a `cargo run`/`cargo test` binary is
/// not inside a bundle: point it at an unpacked `onnxruntime-osx-x86_64-1.23.2`.
///
/// **Everything that can fail is checked before `ort` is called**, and that is
/// not belt and braces. In `ort` 2.0.0-rc.12 a failed `init_from` does not
/// return its error, it deadlocks: building the error calls ONNX Runtime's
/// `CreateStatus`, which needs the library that just failed to load, which
/// re-enters the `std::sync::Once` still running the first load. (Reproduced
/// with a missing path; the thread never returns.) `SileroModel::load` runs in
/// app setup, so that would be a launch that hangs forever. Past the checks,
/// `ort`'s own load does the same dlopen and finds the same symbol, and cannot
/// fail — except by version, and 1.23.2 is exactly the API this target asks
/// for (a stray older `ORT_DYLIB_PATH` would still hang; it is a developer
/// override).
///
/// The result is computed once. The checks spawn `sw_vers`, and a failed load
/// must not be retried by the wake word later either.
///
/// Callers already fall back on an error (energy VAD; wake word reported
/// unavailable). The realistic cause is macOS 12 or 13.0-13.3: the 1.23.2
/// dylib is built for 13.4 and up, while Echo itself supports 12, and whether
/// dyld refuses such a dylib cleanly or loads it and crashes later is not
/// something to find out on a user's Mac — hence the explicit version gate.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub(crate) fn load_onnx_runtime() -> Result<()> {
    static LOADED: std::sync::OnceLock<std::result::Result<(), String>> =
        std::sync::OnceLock::new();
    LOADED
        .get_or_init(|| {
            // `sw_vers` over a syscall crate: this runs once, and a missing
            // or odd answer just skips the gate rather than failing it.
            let version = std::process::Command::new("/usr/bin/sw_vers")
                .arg("-productVersion")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|v| {
                    let mut parts = v.trim().split('.').map(|p| p.parse::<u32>().ok());
                    Some((parts.next()??, parts.next().flatten().unwrap_or(0)))
                });
            if let Some((major, minor)) = version {
                if (major, minor) < (13, 4) {
                    return Err(format!(
                        "the bundled ONNX Runtime needs macOS 13.4 or later, this is {major}.{minor}"
                    ));
                }
            }

            let path = match std::env::var_os("ORT_DYLIB_PATH") {
                Some(p) => std::path::PathBuf::from(p),
                None => std::env::current_exe()
                    .map_err(|e| format!("locating the executable: {e}"))?
                    .parent()
                    .ok_or("the executable has no parent directory")?
                    .join("../Resources/bin/libonnxruntime.dylib"),
            };
            // The plugin loader's `libloading` (0.8), not `ort`'s (0.9) — same
            // dlopen either way, and no second version to compile.
            // SAFETY: loading ONNX Runtime runs only its static initialisers,
            // which `ort` is about to run anyway; the handle is dropped again
            // (a refcount, so `ort`'s own dlopen below keeps it loaded).
            unsafe {
                let lib = libloading::Library::new(&path)
                    .map_err(|e| format!("loading {}: {e}", path.display()))?;
                lib.get::<unsafe extern "C" fn()>(b"OrtGetApiBase")
                    .map_err(|e| format!("{} is not ONNX Runtime: {e}", path.display()))?;
            }

            ort::init_from(&path)
                .map_err(|e| format!("loading {}: {e}", path.display()))?
                .commit();
            Ok(())
        })
        .clone()
        .map_err(|why| EchoError::Config(format!("ONNX Runtime unavailable: {why}")))
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
pub(crate) fn load_onnx_runtime() -> Result<()> {
    Ok(())
}

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
        load_onnx_runtime()?;
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

    /// Run one `CONTEXT + FRAME` window, returning the speech probability and
    /// the next recurrent state. `state` is the [2,1,128] tensor (zeros to
    /// start).
    fn infer(&self, window: &[f32], state: &[f32]) -> Result<(f32, Vec<f32>)> {
        let input = Tensor::from_array(([1usize, window.len()], window.to_vec()))
            .map_err(|e| EchoError::AsrProvider(format!("vad input: {e}")))?;
        let state_t = Tensor::from_array(([2usize, 1, 128], state.to_vec()))
            .map_err(|e| EchoError::AsrProvider(format!("vad state: {e}")))?;
        let sr_t = Tensor::from_array(([0usize; 0], vec![SAMPLE_RATE]))
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
    /// Tail of the previous frame, prepended to the next window (see [`CONTEXT`]).
    context: Vec<f32>,
    triggered: bool,
    silent_count: usize,
}

impl SileroVad {
    pub fn new(model: std::sync::Arc<SileroModel>) -> Self {
        Self {
            model,
            state: vec![0.0; 2 * 128],
            buffer: Vec::with_capacity(FRAME * 2),
            context: vec![0.0; CONTEXT],
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
            let mut window = Vec::with_capacity(CONTEXT + FRAME);
            window.extend_from_slice(&self.context);
            window.extend_from_slice(&frame);
            self.context = frame[FRAME - CONTEXT..].to_vec();
            match self.model.infer(&window, &self.state) {
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
        self.context = vec![0.0; CONTEXT];
        self.triggered = false;
        self.silent_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// The ONNX Runtime is statically linked into the binary, so building a
    /// session and running inference must work with no external DLL/dylib
    /// present. This test would fail to link/load if that were not the case —
    /// which is exactly the property we rely on to *not* bundle ONNX Runtime.
    #[test]
    fn silero_loads_and_runs_inference_without_external_runtime() {
        let model = Arc::new(SileroModel::load().expect("Silero session should build"));

        // Silence is reliably below the speech threshold across several frames.
        let mut vad = SileroVad::new(model);
        let silence = vec![0.0f32; FRAME * 4];
        assert!(
            !vad.is_speech(&silence),
            "pure silence must not trigger speech"
        );
    }

    /// A 16 kHz mono clip of clear speech, so the detector is pinned from both
    /// sides. The silence test alone passes even when the model is fed a
    /// malformed window and returns ~0 for everything — which is exactly how
    /// the missing [`CONTEXT`] prefix went unnoticed and gated every utterance
    /// out of the ASR pipeline.
    #[test]
    fn silero_detects_real_speech() {
        static SPEECH_WAV: &[u8] = include_bytes!("../../../resources/test_speech_16k.wav");

        let model = Arc::new(SileroModel::load().expect("Silero session should build"));
        let mut vad = SileroVad::new(model);

        // Fed in ~10 ms chunks, the way the capture task delivers them.
        let triggered = pcm_s16_mono(SPEECH_WAV)
            .chunks(160)
            .any(|chunk| vad.is_speech(chunk));

        assert!(triggered, "speech must trigger the detector");
    }

    /// Decode a 16-bit PCM mono WAV into the normalised f32 samples the VAD
    /// expects.
    fn pcm_s16_mono(bytes: &[u8]) -> Vec<f32> {
        let mut i = 12; // skip "RIFF" + size + "WAVE"
        while i + 8 <= bytes.len() {
            let len = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().unwrap()) as usize;
            if &bytes[i..i + 4] == b"data" {
                return bytes[i + 8..(i + 8 + len).min(bytes.len())]
                    .chunks_exact(2)
                    .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
                    .collect();
            }
            i += 8 + len + (len & 1); // chunks are word-aligned
        }
        panic!("no data chunk in fixture");
    }
}
