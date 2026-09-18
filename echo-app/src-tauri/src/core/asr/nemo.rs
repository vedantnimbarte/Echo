//! The `"nemo"` ASR provider: NVIDIA's NeMo-Speech.cpp running a Nemotron
//! transducer locally.
//!
//! Why a second local engine rather than a bigger whisper model: whisper's
//! encoder always runs over a padded 30-second window, so a short dictation
//! costs the same as a long one, and its decoder invents fluent sentences when
//! handed near-silence. A transducer processes what it is given, punctuates and
//! capitalises natively, and has nothing to hallucinate *from* — the three
//! things that separate "a transcript arrived" from "dictation feels instant".
//!
//! whisper.cpp stays. It is the 99-language fallback, the smallest download,
//! and the engine that works when this one has no binary for the platform.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use super::nemo_server::{Device, NemoServer, Signature};
use super::prompt::PromptContext;
use super::wav::pcm_f32_to_wav;
use super::{AsrProvider, TranscriptSegment};
use crate::core::dictionary::DictionaryEngine;
use crate::core::gpu::GpuBackend;
use crate::error::{EchoError, Result};

/// Pinned NeMo-Speech.cpp release. Keep the checksums in [`NemoPack::sha256`]
/// in sync when bumping this.
const NEMO_RELEASE_TAG: &str = "v0.1.0";

#[cfg(target_os = "windows")]
pub(crate) const BINARY_NAME: &str = "nemo-speech.exe";
#[cfg(not(target_os = "windows"))]
pub(crate) const BINARY_NAME: &str = "nemo-speech";

/// A build of the NeMo-Speech binaries against one compute backend.
///
/// Each pack lands in its own directory for the same reason the whisper packs
/// do: they ship conflicting copies of the same `ggml*` libraries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NemoPack {
    Cpu,
    Cuda,
}

impl NemoPack {
    pub fn subdir(self) -> &'static str {
        match self {
            NemoPack::Cpu => "nemo",
            NemoPack::Cuda => "nemo-cuda",
        }
    }

    /// Release asset for this pack on Windows.
    fn asset(self) -> &'static str {
        match self {
            NemoPack::Cpu => "nemo-speech-0.1.0-windows-x86_64-cpu.zip",
            NemoPack::Cuda => "nemo-speech-0.1.0-windows-x86_64-cuda.zip",
        }
    }

    /// SHA-256 of the archive, lowercase hex, checked before anything is
    /// unpacked — what comes out is an executable Echo runs.
    ///
    /// Recorded from the GitHub release API's own `digest` field:
    ///
    /// ```text
    /// gh api repos/NVIDIA/NeMo-Speech.cpp/releases/tags/<tag> \
    ///   --jq '.assets[] | "\(.name) \(.digest)"'
    /// ```
    fn sha256(self) -> &'static str {
        match self {
            NemoPack::Cpu => "5e4ea81046012edcd77fd8848de8eefb5a4ba38cc26f52eb544ab184695a75d6",
            NemoPack::Cuda => "ba024204e76ca2fa4eefa8787506c3c49e418147f627f60cf9206a582b60089c",
        }
    }

    pub fn url(self) -> String {
        format!(
            "https://github.com/NVIDIA/NeMo-Speech.cpp/releases/download/{NEMO_RELEASE_TAG}/{}",
            self.asset()
        )
    }

    /// Download size in megabytes, rounded — shown before the click.
    pub fn download_mb(self) -> u32 {
        match self {
            NemoPack::Cpu => 5,
            NemoPack::Cuda => 101,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NemoPack::Cpu => "CPU",
            NemoPack::Cuda => "NVIDIA CUDA",
        }
    }

    /// The pack to install for a detected GPU.
    ///
    /// One CUDA build covers both driver majors here, unlike whisper.cpp's
    /// separate 11.8 and 12.4 packs.
    pub fn for_gpu(gpu: GpuBackend) -> Self {
        match gpu {
            GpuBackend::Cuda { .. } => NemoPack::Cuda,
            _ => NemoPack::Cpu,
        }
    }
}

/// Finds the installed NeMo-Speech binary, preferring the accelerated pack.
pub struct NemoBinaries {
    dir: PathBuf,
    gpu: GpuBackend,
}

impl NemoBinaries {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            gpu: GpuBackend::None,
        }
    }

    pub fn with_gpu(mut self, gpu: GpuBackend) -> Self {
        self.gpu = gpu;
        self
    }

    pub fn pack_dir(&self, pack: NemoPack) -> PathBuf {
        self.dir.join(pack.subdir())
    }

    /// The binary to run, and whether it is the accelerated build.
    ///
    /// Prefers the pack matching the detected GPU and falls back to the CPU
    /// pack, so a half-finished CUDA download never leaves the engine dead.
    pub fn resolve(&self) -> Option<(PathBuf, bool)> {
        let preferred = NemoPack::for_gpu(self.gpu);
        for pack in [preferred, NemoPack::Cpu] {
            if let Some(binary) = super::nemo_server::binary_in(&self.pack_dir(pack)) {
                return Some((binary, pack == NemoPack::Cuda));
            }
        }
        None
    }

    pub fn is_installed(&self) -> bool {
        self.resolve().is_some()
    }

    /// Download and unpack one pack, reporting fractional progress.
    pub async fn download_pack(
        &self,
        pack: NemoPack,
        progress_tx: tokio::sync::mpsc::Sender<f32>,
    ) -> Result<PathBuf> {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (pack, progress_tx);
            return Err(EchoError::Config(format!(
                "NeMo-Speech publishes prebuilt binaries Echo can install on Windows only. \
                 Install one named '{BINARY_NAME}' on your PATH to use this engine here."
            )));
        }

        #[cfg(target_os = "windows")]
        {
            let dest = self.pack_dir(pack);
            super::binary_manager::download_zip_into(
                &pack.url(),
                pack.sha256(),
                &dest,
                "nemo-speech binary download",
                progress_tx,
            )
            .await?;

            let installed = dest.join(BINARY_NAME);
            if !installed.exists() {
                return Err(EchoError::Config(format!(
                    "The NeMo-Speech {} archive did not contain '{BINARY_NAME}'",
                    pack.label()
                )));
            }
            Ok(installed)
        }
    }
}

/// Local transcription through a resident `nemo-speech serve`.
pub struct NemoProvider {
    binaries: Arc<NemoBinaries>,
    server: Arc<NemoServer>,
    model_path: PathBuf,
    /// Whether the user allows the GPU at all. A machine with no CUDA pack
    /// installed runs on CPU regardless.
    gpu_allowed: bool,
    /// Custom vocabulary, boosted while the decoder is still choosing. The
    /// whisper provider feeds the same terms in as an initial prompt; without
    /// this the dictionary would only repair a name after it was misheard.
    dictionary: Option<Arc<tokio::sync::RwLock<DictionaryEngine>>>,
    /// Which app is focused, which decides the profile the dictionary is
    /// scoped to.
    context: Option<Arc<PromptContext>>,
}

impl NemoProvider {
    pub fn new(binaries: Arc<NemoBinaries>, server: Arc<NemoServer>, model_path: PathBuf) -> Self {
        Self {
            binaries,
            server,
            model_path,
            gpu_allowed: true,
            dictionary: None,
            context: None,
        }
    }

    pub fn with_gpu_allowed(mut self, allowed: bool) -> Self {
        self.gpu_allowed = allowed;
        self
    }

    pub fn with_dictionary(
        mut self,
        dictionary: Arc<tokio::sync::RwLock<DictionaryEngine>>,
    ) -> Self {
        self.dictionary = Some(dictionary);
        self
    }

    pub fn with_prompt_context(mut self, context: Arc<PromptContext>) -> Self {
        self.context = Some(context);
        self
    }

    /// Dictionary spellings to boost for this utterance, scoped to the focused
    /// app's profile.
    async fn boost_terms(&self) -> Vec<String> {
        let Some(dictionary) = &self.dictionary else {
            return Vec::new();
        };
        let profile = self.context.as_ref().and_then(|c| c.profile());
        dictionary
            .read()
            .await
            .hint_terms(profile)
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    /// Resolve the binary and the device for *this* attempt.
    fn signature(&self) -> Result<Signature> {
        let (binary, accelerated) = self.binaries.resolve().ok_or_else(|| {
            EchoError::NotFound("The NeMo-Speech engine is not installed.".into())
        })?;
        Ok(Signature {
            binary,
            model: self.model_path.clone(),
            // "auto" lets the runtime pick the best device the pack supports;
            // pinning "cpu" is how the user's opt-out actually takes effect,
            // since the accelerated pack would otherwise use the GPU.
            device: if accelerated && self.gpu_allowed {
                Device::Auto
            } else {
                Device::Cpu
            },
        })
    }
}

#[async_trait]
impl AsrProvider for NemoProvider {
    fn name(&self) -> &str {
        "nemo"
    }

    async fn preload(&self) -> Result<()> {
        self.server.warm(&self.signature()?).await
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

        let sig = self.signature()?;
        let audio_seconds = audio.len().div_ceil(16_000) as u32;
        let wav = pcm_f32_to_wav(&audio, 16_000)?;

        let text = self
            .server
            .transcribe(
                &sig,
                wav,
                audio_seconds,
                language,
                &self.boost_terms().await,
            )
            .await?;

        Ok(TranscriptSegment {
            text,
            is_final: true,
            language: language.map(str::to_string),
            confidence: None,
        })
    }
}

/// Where the NeMo binaries live under the app's data directory.
pub fn binaries_dir_of(base: &Path) -> PathBuf {
    base.join("bin")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("echo-nemo-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(BINARY_NAME), b"x").unwrap();
    }

    #[test]
    fn a_cuda_machine_prefers_the_accelerated_pack() {
        let base = scratch("cuda");
        touch(&base.join("nemo"));
        touch(&base.join("nemo-cuda"));

        let b = NemoBinaries::new(base.clone()).with_gpu(GpuBackend::Cuda { major: 12 });
        let (path, accelerated) = b.resolve().unwrap();
        assert!(accelerated);
        assert!(path.to_string_lossy().contains("nemo-cuda"));

        let _ = std::fs::remove_dir_all(&base);
    }

    /// The CPU pack is the floor: a GPU machine whose CUDA download never
    /// finished still has a working engine rather than a dead one.
    #[test]
    fn a_missing_cuda_pack_falls_back_to_cpu() {
        let base = scratch("fallback");
        touch(&base.join("nemo"));

        let b = NemoBinaries::new(base.clone()).with_gpu(GpuBackend::Cuda { major: 12 });
        let (path, accelerated) = b.resolve().unwrap();
        assert!(!accelerated);
        assert!(path.to_string_lossy().contains("nemo"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn nothing_installed_resolves_to_nothing() {
        let base = scratch("empty");
        assert!(!NemoBinaries::new(base.clone()).is_installed());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Opting out of the GPU has to reach the server's command line, or the
    /// accelerated build quietly uses the GPU anyway.
    #[test]
    fn opting_out_of_the_gpu_pins_the_device_to_cpu() {
        let base = scratch("optout");
        touch(&base.join("nemo-cuda"));

        let binaries =
            Arc::new(NemoBinaries::new(base.clone()).with_gpu(GpuBackend::Cuda { major: 12 }));
        let provider = NemoProvider::new(
            binaries.clone(),
            Arc::new(NemoServer::new()),
            PathBuf::from("m.gguf"),
        );
        assert_eq!(provider.signature().unwrap().device, Device::Auto);

        let provider = NemoProvider::new(
            binaries,
            Arc::new(NemoServer::new()),
            PathBuf::from("m.gguf"),
        )
        .with_gpu_allowed(false);
        assert_eq!(provider.signature().unwrap().device, Device::Cpu);

        let _ = std::fs::remove_dir_all(&base);
    }

    /// The whole engine against the real binary and the real weights: download
    /// the pack if it is missing, start the server, decode the speech fixture.
    ///
    /// Ignored by default — it needs a ~100 MB download and a 708 MB model, so
    /// it is a thing you run deliberately:
    ///
    /// ```text
    /// cargo test --lib nemo_end_to_end -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore]
    async fn nemo_end_to_end_decodes_real_speech() {
        let data_dir = dirs_data_dir().join("com.echo.app");
        let model = data_dir.join("models").join("nemotron-streaming-0.6b.gguf");
        if !model.exists() {
            eprintln!("skipping: {} is not downloaded", model.display());
            return;
        }

        let gpu = crate::core::gpu::detect();
        let binaries = Arc::new(NemoBinaries::new(binaries_dir_of(&data_dir)).with_gpu(gpu));
        if !binaries.is_installed() {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<f32>(32);
            tokio::spawn(async move { while rx.recv().await.is_some() {} });
            binaries
                .download_pack(NemoPack::for_gpu(gpu), tx)
                .await
                .expect("the engine pack should install");
        }

        let provider = NemoProvider::new(binaries, Arc::new(NemoServer::new()), model);
        let audio = fixture_pcm();

        // What the pipeline does on app start and microphone warm, so that the
        // first dictation does not pay for loading 708 MB of weights.
        let started = std::time::Instant::now();
        provider.preload().await.expect("the engine should warm");
        let warmup = started.elapsed();

        let started = std::time::Instant::now();
        let first = provider
            .transcribe(audio.clone(), Some("en"))
            .await
            .unwrap();
        let cold = started.elapsed();
        eprintln!("nemo: preload {warmup:?}, first decode after preload {cold:?}");
        assert!(
            cold < std::time::Duration::from_secs(5),
            "a preloaded engine must not reload the model: took {cold:?}"
        );

        // The model is resident now, so this is what a dictation actually pays.
        let started = std::time::Instant::now();
        let second = provider.transcribe(audio, Some("en")).await.unwrap();
        eprintln!(
            "nemo: cold {cold:?}, warm {:?} -> {:?}",
            started.elapsed(),
            second.text
        );

        assert!(
            first.text.to_lowercase().contains("testing"),
            "got {:?}",
            first.text
        );
        assert_eq!(first.text, second.text, "the same audio must decode alike");
    }

    /// The bundled 16 kHz mono speech clip as f32 samples.
    fn fixture_pcm() -> Vec<f32> {
        static WAV: &[u8] = include_bytes!("../../../resources/test_speech_16k.wav");
        let mut i = 12; // skip "RIFF" + size + "WAVE"
        while i + 8 <= WAV.len() {
            let len = u32::from_le_bytes(WAV[i + 4..i + 8].try_into().unwrap()) as usize;
            if &WAV[i..i + 4] == b"data" {
                return WAV[i + 8..(i + 8 + len).min(WAV.len())]
                    .chunks_exact(2)
                    .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
                    .collect();
            }
            i += 8 + len + (len & 1);
        }
        panic!("no data chunk in fixture");
    }

    /// Where the app keeps its data, for the end-to-end test only.
    fn dirs_data_dir() -> PathBuf {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
    }

    #[tokio::test]
    async fn transcribing_without_the_engine_installed_says_so() {
        let base = scratch("noengine");
        let provider = NemoProvider::new(
            Arc::new(NemoBinaries::new(base.clone())),
            Arc::new(NemoServer::new()),
            PathBuf::from("m.gguf"),
        );
        let err = provider
            .transcribe(vec![0.1; 16_000], None)
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("not installed"), "{err}");

        let _ = std::fs::remove_dir_all(&base);
    }
}
