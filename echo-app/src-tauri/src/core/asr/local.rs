//! The `"local"` ASR provider: offline whisper.cpp, fastest path first.
//!
//! Three things vary underneath, and this type is where they are decided so the
//! rest of the app never has to care:
//!
//! - **Server before CLI.** A resident model answers in the time it takes to
//!   decode; the CLI reloads the model first. The CLI is kept as the fallback
//!   because it needs no port and no supervised child process, so it works in
//!   the situations that break the server.
//! - **GPU before CPU.** Whichever binary the machine can actually accelerate,
//!   falling back permanently for the session the first time one fails.
//! - **Dictionary before decoding.** Known vocabulary is passed to whisper as
//!   an initial prompt so the decoder is biased toward the right spelling while
//!   it still has a choice.
//!
//! A degradation is never silent: each one logs, so "why did it get slower"
//! has an answer in `echo.log`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::binary_manager::BinaryManager;
use super::decode_opts::DecodeConfig;
use super::prompt::PromptContext;
use super::wav::pcm_f32_to_wav;
use super::whisper_cli::{initial_prompt, is_english_only, resolve_language, run_cli};
use super::whisper_server::{DecodeConfigKey, Signature, WhisperServer};
use super::{AsrProvider, TranscriptSegment};
use crate::core::dictionary::DictionaryEngine;
use crate::error::{EchoError, Result};

pub struct LocalWhisperProvider {
    binaries: Arc<BinaryManager>,
    server: Arc<WhisperServer>,
    model_path: PathBuf,
    model_name: String,
    dictionary: Option<Arc<RwLock<DictionaryEngine>>>,
    /// What the focused app implies and what was just said — see
    /// [`super::prompt`]. `None` outside the dictation pipeline (imports).
    context: Option<Arc<PromptContext>>,
    /// Thread count, and whether the user has opted out of GPU use entirely.
    threads: usize,
    gpu_allowed: bool,
    /// Whether anything is reading partial results. Set per session by the
    /// pipeline; when false this provider behaves exactly as it always has.
    partials_wanted: AtomicBool,
    /// A smaller model to decode partials with, if one is installed.
    ///
    /// A partial is discarded and rewritten within a second, so it only has to
    /// be roughly right — and on CPU the difference between `tiny.en` and
    /// `base.en` is the difference between words that keep up with the speaker
    /// and words that trail two seconds behind. The final transcript is always
    /// decoded with the real model.
    partial_model: Option<(PathBuf, String)>,
}

impl LocalWhisperProvider {
    pub fn new(
        binaries: Arc<BinaryManager>,
        server: Arc<WhisperServer>,
        model_path: PathBuf,
        model_name: impl Into<String>,
    ) -> Self {
        Self {
            binaries,
            server,
            model_path,
            model_name: model_name.into(),
            dictionary: None,
            context: None,
            threads: super::decode_opts::auto_threads(),
            gpu_allowed: true,
            partials_wanted: AtomicBool::new(false),
            partial_model: None,
        }
    }

    pub fn with_dictionary(mut self, dictionary: Arc<RwLock<DictionaryEngine>>) -> Self {
        self.dictionary = Some(dictionary);
        self
    }

    /// Decode partials with a smaller model than the finals.
    pub fn with_partial_model(mut self, path: PathBuf, name: impl Into<String>) -> Self {
        self.partial_model = Some((path, name.into()));
        self
    }

    pub fn with_prompt_context(mut self, context: Arc<PromptContext>) -> Self {
        self.context = Some(context);
        self
    }

    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }

    /// Builder: let the user force CPU decoding even on a capable machine.
    pub fn with_gpu_allowed(mut self, allowed: bool) -> Self {
        self.gpu_allowed = allowed;
        self
    }

    /// Resolve the decode configuration for *this* attempt.
    ///
    /// Recomputed per utterance rather than cached, because
    /// [`BinaryManager::mark_gpu_failed`] can flip it mid-session — that is
    /// exactly how the CPU fallback takes effect without a restart.
    fn decode_config(&self) -> DecodeConfig {
        let accelerated = self
            .binaries
            .active_dir()
            .map(|(_, accel)| accel)
            .unwrap_or(false);
        DecodeConfig {
            threads: self.threads,
            use_gpu: accelerated && self.gpu_allowed && !self.binaries.gpu_failed(),
        }
    }
}

#[async_trait]
impl AsrProvider for LocalWhisperProvider {
    fn name(&self) -> &str {
        "local"
    }

    fn supports_streaming(&self) -> bool {
        // True now that partials are produced by re-decoding — but only ever
        // acted on when `set_partials_wanted(true)` says somebody is reading.
        true
    }

    fn set_partials_wanted(&self, wanted: bool) {
        self.partials_wanted.store(wanted, Ordering::Relaxed);
    }

    /// Buffered by default; rolling re-decode when partials are wanted.
    async fn transcribe_stream(
        &self,
        audio_rx: tokio::sync::mpsc::Receiver<Vec<f32>>,
        tx: tokio::sync::mpsc::Sender<TranscriptSegment>,
        language: Option<&str>,
    ) -> Result<()> {
        if self.partials_wanted.load(Ordering::Relaxed) {
            self.stream_with_partials(audio_rx, tx, language).await
        } else {
            super::default_transcribe_stream(self, audio_rx, tx, language).await
        }
    }

    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        if audio.is_empty() {
            return Ok(TranscriptSegment {
                text: String::new(),
                is_final: true,
                language: None,
                confidence: None,
            });
        }

        let decode = self.decode_config();
        let lang = resolve_language(&self.model_name, language);
        let prompt = initial_prompt(self.dictionary.as_ref(), self.context.as_ref()).await;
        let audio_seconds = (audio.len() / 16_000) as u32;
        let wav = pcm_f32_to_wav(&audio, 16_000)?;

        let text = match self.try_server(&decode, &wav, audio_seconds, lang, &prompt).await {
            Some(Ok(text)) => text,
            Some(Err(e)) => {
                // The server failed. If it was the accelerated one, latch that
                // so every later utterance — including the CLI retry below —
                // resolves to the CPU binary instead of failing the same way.
                if decode.use_gpu {
                    self.binaries.mark_gpu_failed();
                }
                tracing::warn!("whisper-server failed, falling back to whisper-cli: {e}");
                self.run_cli_fallback(&wav, lang, &prompt).await?
            }
            None => self.run_cli_fallback(&wav, lang, &prompt).await?,
        };

        Ok(TranscriptSegment {
            text,
            is_final: true,
            language: is_english_only(&self.model_name).then(|| "en".to_string()),
            confidence: None,
        })
    }
}

/// How much new speech has to arrive before another partial is worth decoding.
///
/// Under this and the re-decodes cost more than the extra words are worth;
/// much over it and the text visibly lags the voice. 0.8s is roughly a spoken
/// phrase.
const PARTIAL_INTERVAL_SAMPLES: usize = (0.8 * 16_000.0) as usize;

/// Past this much audio in one utterance, partials stop.
///
/// Each one re-decodes the whole utterance so far, so the cost grows with the
/// square of how long someone talks. Somebody still going at twenty seconds is
/// monologuing, not dictating a sentence, and the final transcript still
/// arrives in full.
const MAX_PARTIAL_SAMPLES: usize = 20 * 16_000;

/// Whether another partial decode should start right now.
///
/// Three rules, and each one is there to stop a specific failure: enough new
/// speech to be worth the work, an utterance short enough that re-decoding it
/// is still cheap, and never more than one decode in flight — a queue of them
/// would fall further behind the speaker with every one it started.
fn should_decode_partial(buffered: usize, decoded_at: usize, busy: bool) -> bool {
    !busy
        && buffered <= MAX_PARTIAL_SAMPLES
        && buffered.saturating_sub(decoded_at) >= PARTIAL_INTERVAL_SAMPLES
}

/// Everything one decode needs, owned, so it can be handed to a spawned task.
///
/// The alternative is borrowing the provider inside `tokio::select!`, which
/// works and is considerably harder to read. All of this is `Arc` or small.
struct DecodeJob {
    binaries: Arc<BinaryManager>,
    server: Arc<WhisperServer>,
    model_path: PathBuf,
    decode: DecodeConfig,
    prompt: Option<String>,
    /// Already resolved against the model, so the job does not need to know
    /// which model it is running.
    language: String,
}

impl DecodeJob {
    /// Decode one buffer, server first and CLI as the fallback — the same
    /// order [`LocalWhisperProvider::transcribe`] uses, and for the same
    /// reasons.
    async fn run(self, audio: Vec<f32>) -> Result<String> {
        let audio_seconds = (audio.len() / 16_000) as u32;
        let wav = pcm_f32_to_wav(&audio, 16_000)?;

        if let Some(binary) = self.binaries.resolve_server() {
            let sig = Signature {
                binary,
                model: self.model_path.clone(),
                decode: DecodeConfigKey::from(self.decode),
            };
            match self
                .server
                .transcribe(&sig, wav.clone(), audio_seconds, &self.language, self.prompt.clone())
                .await
            {
                Ok(text) => return Ok(text),
                Err(e) => {
                    if self.decode.use_gpu {
                        self.binaries.mark_gpu_failed();
                    }
                    tracing::warn!("whisper-server failed during streaming: {e}");
                }
            }
        }

        let binary = self
            .binaries
            .resolve()
            .ok_or_else(|| EchoError::NotFound("No whisper-cli binary is installed".into()))?;
        run_cli(&binary, &self.model_path, &wav, &self.language, self.decode, self.prompt.as_deref())
            .await
    }
}

impl LocalWhisperProvider {
    /// Stream partial transcripts by re-decoding the utterance as it grows.
    ///
    /// whisper has no incremental mode: it decodes a buffer and returns a
    /// transcript. So a partial is the whole utterance-so-far decoded again,
    /// which is what whisper.cpp's own streaming example does. The waste is
    /// real and bounded by two things — one decode in flight at a time, and
    /// [`MAX_PARTIAL_SAMPLES`].
    ///
    /// The decode runs in a spawned task and is *polled*, never awaited, in
    /// the receive loop. Awaiting it would stop reading audio for the length
    /// of a decode, and the capture layer would start dropping chunks — losing
    /// the user's words to make the display prettier, which is the wrong trade
    /// in every case.
    async fn stream_with_partials(
        &self,
        mut audio_rx: tokio::sync::mpsc::Receiver<Vec<f32>>,
        tx: tokio::sync::mpsc::Sender<TranscriptSegment>,
        language: Option<&str>,
    ) -> Result<()> {
        let mut buffer: Vec<f32> = Vec::new();
        let mut decoded_at = 0usize;
        let mut in_flight: Option<tokio::task::JoinHandle<Result<String>>> = None;

        while let Some(chunk) = audio_rx.recv().await {
            // Collect a finished partial before anything else, so the text on
            // screen is never more than one chunk behind the decoder.
            if in_flight.as_ref().is_some_and(|h| h.is_finished()) {
                let handle = in_flight.take().expect("just checked");
                if let Ok(Ok(text)) = handle.await {
                    if !text.is_empty() {
                        let _ = tx
                            .send(TranscriptSegment {
                                text,
                                is_final: false,
                                language: None,
                                confidence: None,
                            })
                            .await;
                    }
                }
            }

            if chunk.is_empty() {
                // Utterance boundary. A partial still running describes less
                // audio than the final decode is about to, so its answer is
                // already obsolete.
                if let Some(handle) = in_flight.take() {
                    handle.abort();
                }
                let utterance = std::mem::take(&mut buffer);
                decoded_at = 0;
                if let Some(segment) =
                    super::transcribe_utterance(self, utterance, language).await?
                {
                    let _ = tx.send(segment).await;
                }
                continue;
            }

            buffer.extend_from_slice(&chunk);

            if should_decode_partial(buffer.len(), decoded_at, in_flight.is_some()) {
                decoded_at = buffer.len();
                let job = self.partial_job(language).await;
                let audio = buffer.clone();
                in_flight = Some(tokio::spawn(job.run(audio)));
            }
        }

        if let Some(handle) = in_flight.take() {
            handle.abort();
        }
        // Capture closed mid-utterance: what was said still deserves a
        // transcript.
        if let Some(segment) = super::transcribe_utterance(self, buffer, language).await? {
            let _ = tx.send(segment).await;
        }
        Ok(())
    }

    /// Snapshot everything a partial decode needs, resolved now rather than
    /// cached — a GPU failure latched a moment ago must already route this to
    /// CPU — and pointed at the small model when one is configured.
    async fn partial_job(&self, language: Option<&str>) -> DecodeJob {
        let (model_path, model_name) = match &self.partial_model {
            Some((path, name)) => (path.clone(), name.as_str()),
            None => (self.model_path.clone(), self.model_name.as_str()),
        };
        DecodeJob {
            binaries: self.binaries.clone(),
            server: self.server.clone(),
            model_path,
            decode: self.decode_config(),
            prompt: initial_prompt(self.dictionary.as_ref(), self.context.as_ref()).await,
            language: resolve_language(model_name, language).to_string(),
        }
    }
}

impl LocalWhisperProvider {
    /// Try the resident server. `None` means there is no server binary to try,
    /// which is the normal case for a PATH install that ships only the CLI.
    async fn try_server(
        &self,
        decode: &DecodeConfig,
        wav: &[u8],
        audio_seconds: u32,
        language: &str,
        prompt: &Option<String>,
    ) -> Option<Result<String>> {
        let binary = self.binaries.resolve_server()?;
        let sig = Signature {
            binary,
            model: self.model_path.clone(),
            decode: DecodeConfigKey::from(*decode),
        };
        Some(
            self.server
                .transcribe(&sig, wav.to_vec(), audio_seconds, language, prompt.clone())
                .await,
        )
    }

    /// One-shot CLI decode. Resolved at call time so that a GPU failure latched
    /// moments ago already routes this to the CPU binary.
    async fn run_cli_fallback(
        &self,
        wav: &[u8],
        language: &str,
        prompt: &Option<String>,
    ) -> Result<String> {
        let binary = self.binaries.resolve().ok_or_else(|| {
            EchoError::NotFound("No whisper-cli binary is installed".into())
        })?;
        run_cli(
            &binary,
            &self.model_path,
            wav,
            language,
            self.decode_config(),
            prompt.as_deref(),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gpu::GpuBackend;

    fn provider_with(dir: PathBuf, gpu: GpuBackend) -> LocalWhisperProvider {
        let binaries = Arc::new(BinaryManager::new(dir).with_gpu(gpu));
        LocalWhisperProvider::new(
            binaries,
            Arc::new(WhisperServer::new()),
            PathBuf::from("model.bin"),
            "base.en",
        )
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("echo-local-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(target_os = "windows")]
    const CLI: &str = "whisper-cli.exe";
    #[cfg(not(target_os = "windows"))]
    const CLI: &str = "whisper-cli";

    /// A partial costs a full re-decode, so it waits for enough new speech to
    /// be worth one.
    #[test]
    fn a_partial_waits_for_enough_new_speech() {
        assert!(!should_decode_partial(PARTIAL_INTERVAL_SAMPLES - 1, 0, false));
        assert!(should_decode_partial(PARTIAL_INTERVAL_SAMPLES, 0, false));
    }

    /// One decode at a time. Queueing them would put the display further
    /// behind the speaker with every extra one started.
    #[test]
    fn a_decode_already_running_blocks_another() {
        assert!(!should_decode_partial(PARTIAL_INTERVAL_SAMPLES * 4, 0, true));
    }

    /// Each partial re-decodes everything said so far, so the cost grows with
    /// the square of the utterance. Past the cap, partials stop — the final
    /// transcript still arrives whole.
    #[test]
    fn partials_stop_on_a_long_monologue() {
        assert!(should_decode_partial(MAX_PARTIAL_SAMPLES, 0, false));
        assert!(!should_decode_partial(MAX_PARTIAL_SAMPLES + 1, 0, false));
    }

    /// Progress is measured from the last decode, not from the start, or a
    /// long utterance would re-decode on every single chunk.
    #[test]
    fn progress_is_measured_since_the_last_decode() {
        let decoded_at = PARTIAL_INTERVAL_SAMPLES * 3;
        assert!(!should_decode_partial(decoded_at + 1, decoded_at, false));
        assert!(should_decode_partial(
            decoded_at + PARTIAL_INTERVAL_SAMPLES,
            decoded_at,
            false
        ));
    }

    #[test]
    fn gpu_is_used_when_an_accelerated_pack_is_installed() {
        let base = scratch("on");
        std::fs::write(base.join(CLI), b"cpu").unwrap();
        let cuda = base.join("cuda12");
        std::fs::create_dir_all(&cuda).unwrap();
        std::fs::write(cuda.join(CLI), b"gpu").unwrap();

        let p = provider_with(base.clone(), GpuBackend::Cuda { major: 12 });
        assert!(p.decode_config().use_gpu);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_failed_gpu_run_switches_later_utterances_to_cpu() {
        let base = scratch("fallback");
        std::fs::write(base.join(CLI), b"cpu").unwrap();
        let cuda = base.join("cuda12");
        std::fs::create_dir_all(&cuda).unwrap();
        std::fs::write(cuda.join(CLI), b"gpu").unwrap();

        let p = provider_with(base.clone(), GpuBackend::Cuda { major: 12 });
        assert!(p.decode_config().use_gpu);

        // This is the whole fallback contract: no restart, no reconstruction.
        p.binaries.mark_gpu_failed();
        assert!(!p.decode_config().use_gpu);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_user_who_opts_out_never_gets_gpu() {
        let base = scratch("optout");
        std::fs::write(base.join(CLI), b"cpu").unwrap();
        let cuda = base.join("cuda12");
        std::fs::create_dir_all(&cuda).unwrap();
        std::fs::write(cuda.join(CLI), b"gpu").unwrap();

        let p = provider_with(base.clone(), GpuBackend::Cuda { major: 12 }).with_gpu_allowed(false);
        assert!(!p.decode_config().use_gpu);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_cpu_only_machine_asks_for_no_gpu() {
        let base = scratch("cpuonly");
        std::fs::write(base.join(CLI), b"cpu").unwrap();

        let p = provider_with(base.clone(), GpuBackend::None);
        assert!(!p.decode_config().use_gpu);

        let _ = std::fs::remove_dir_all(&base);
    }
}
