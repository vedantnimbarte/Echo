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
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::binary_manager::BinaryManager;
use super::decode_opts::DecodeConfig;
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
    /// Thread count, and whether the user has opted out of GPU use entirely.
    threads: usize,
    gpu_allowed: bool,
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
            threads: super::decode_opts::auto_threads(),
            gpu_allowed: true,
        }
    }

    pub fn with_dictionary(mut self, dictionary: Arc<RwLock<DictionaryEngine>>) -> Self {
        self.dictionary = Some(dictionary);
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
        let prompt = initial_prompt(self.dictionary.as_ref()).await;
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
