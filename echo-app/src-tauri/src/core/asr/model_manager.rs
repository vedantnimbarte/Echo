use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::{EchoError, Result};

/// Which local engine runs a model. The file name on disk follows from it, and
/// so does which provider can load it: a GGUF transducer means nothing to
/// whisper.cpp and a ggml whisper model means nothing to NeMo-Speech.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    Whisper,
    Nemo,
}

/// Catalog of downloadable local models (from Hugging Face).
/// `size_mb` is approximate and used only for display in the UI.
///
/// `sha256` is checked before a download is accepted — see
/// [`crate::core::download::verify`] for what that does and does not prove. The
/// digests came from Hugging Face's LFS object ids, which are the SHA-256 of the
/// file content; `ggml-base.en.bin` was additionally hashed on disk to confirm
/// that the two agree. Re-record them with:
///
/// ```text
/// curl -sIL <url> | grep -i x-linked-etag
/// ```
const MODEL_CATALOG: &[ModelSpec] = &[
    // NVIDIA Nemotron, run by NeMo-Speech.cpp rather than whisper.cpp. A
    // transducer: it punctuates and capitalises natively, decodes only the
    // audio it is given rather than whisper's padded 30 s window, and has no
    // temperature-fallback loop to hallucinate a sentence out of silence.
    ModelSpec {
        name: "nemotron-streaming-0.6b",
        sha256: "3fc991d3badad7277c11030a7519832cddaf2057aafed6d4b25147e953a070b1",
        url: "https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b/resolve/main/nemotron-3.5-asr-streaming-0.6b.q8_0.gguf",
        size_mb: 708,
        english_only: false,
        engine: Engine::Nemo,
    },
    // English-only models — smaller and more accurate for English speech.
    ModelSpec {
        name: "tiny.en",
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        size_mb: 75,
        english_only: true,
        engine: Engine::Whisper,
    },
    ModelSpec {
        name: "base.en",
        sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
        size_mb: 142,
        english_only: true,
        engine: Engine::Whisper,
    },
    ModelSpec {
        name: "small.en",
        sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        size_mb: 466,
        english_only: true,
        engine: Engine::Whisper,
    },
    // Multilingual models.
    ModelSpec {
        name: "tiny",
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        size_mb: 75,
        english_only: false,
        engine: Engine::Whisper,
    },
    ModelSpec {
        name: "base",
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        size_mb: 142,
        english_only: false,
        engine: Engine::Whisper,
    },
    ModelSpec {
        name: "small",
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        size_mb: 466,
        english_only: false,
        engine: Engine::Whisper,
    },
    ModelSpec {
        name: "medium",
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        size_mb: 1500,
        english_only: false,
        engine: Engine::Whisper,
    },
];

/// The model fetched on first run and used by default.
pub const DEFAULT_MODEL: &str = "base.en";

/// The model the NeMo engine runs. One entry today, so this is also the only
/// value `nemo_model` ever holds unless the catalog grows.
pub const DEFAULT_NEMO_MODEL: &str = "nemotron-streaming-0.6b";

struct ModelSpec {
    name: &'static str,
    url: &'static str,
    /// SHA-256 the downloaded weights must have, lowercase hex.
    sha256: &'static str,
    size_mb: u32,
    english_only: bool,
    engine: Engine,
}

impl ModelSpec {
    /// File name on disk. whisper models keep the `ggml-<name>.bin` shape they
    /// have always had, so nothing already downloaded has to move.
    fn file_name(&self) -> String {
        match self.engine {
            Engine::Whisper => format!("ggml-{}.bin", self.name),
            Engine::Nemo => format!("{}.gguf", self.name),
        }
    }
}

/// Information about a model returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub downloaded: bool,
    pub size_mb: u32,
    pub english_only: bool,
    /// Which local engine runs it, so the UI can say what a model needs
    /// installed before it can be selected.
    pub engine: Engine,
}

/// Manages local Whisper model files: listing, download, and path resolution.
pub struct ModelManager {
    models_dir: PathBuf,
}

impl ModelManager {
    pub fn new(models_dir: PathBuf) -> Self {
        Self { models_dir }
    }

    pub fn model_path(&self, name: &str) -> PathBuf {
        // An unknown name is assumed to be a whisper model: that is what every
        // stored setting from before the catalog gained a second engine is.
        let file = Self::spec(name)
            .map(|s| s.file_name())
            .unwrap_or_else(|_| format!("ggml-{name}.bin"));
        self.models_dir.join(file)
    }

    /// Which engine loads `name`.
    pub fn engine_of(name: &str) -> Engine {
        Self::spec(name)
            .map(|s| s.engine)
            .unwrap_or(Engine::Whisper)
    }

    pub fn is_downloaded(&self, name: &str) -> bool {
        self.model_path(name).exists()
    }

    /// List the catalog with each model's local download status.
    /// The smallest downloaded model that is smaller than `than`, for decoding
    /// streaming partials.
    ///
    /// Matched within the same family: an English-only model must not be
    /// paired with a multilingual one, or the partials would come out in a
    /// different language from the final and the on-screen text would thrash
    /// between the two.
    ///
    /// `None` when nothing smaller is installed, in which case partials use the
    /// main model exactly as before.
    pub fn smaller_downloaded(&self, than: &str) -> Option<ModelInfo> {
        let target = Self::spec(than).ok()?;
        let english_only = than.ends_with(".en");
        self.list()
            .into_iter()
            .filter(|m| {
                m.downloaded
                    && m.size_mb < target.size_mb
                    && m.name.ends_with(".en") == english_only
                    // Partials are decoded by the whisper provider itself, so a
                    // model another engine owns is not a candidate however
                    // small it is.
                    && m.engine == Engine::Whisper
            })
            .min_by_key(|m| m.size_mb)
    }

    pub fn list(&self) -> Vec<ModelInfo> {
        MODEL_CATALOG
            .iter()
            .map(|m| ModelInfo {
                name: m.name.to_string(),
                downloaded: self.is_downloaded(m.name),
                size_mb: m.size_mb,
                english_only: m.english_only,
                engine: m.engine,
            })
            .collect()
    }

    fn spec(name: &str) -> Result<&'static ModelSpec> {
        MODEL_CATALOG
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| EchoError::NotFound(format!("Unknown model '{name}'")))
    }

    /// Remove a downloaded model's weights from disk.
    ///
    /// Deleting a model that isn't there succeeds: the caller asked for it to
    /// be gone, and it is. Resolving `name` through the catalog first keeps an
    /// arbitrary string from being joined onto the models directory.
    pub fn delete(&self, name: &str) -> Result<()> {
        Self::spec(name)?;
        let path = self.model_path(name);
        if !path.exists() {
            return Ok(());
        }
        std::fs::remove_file(&path)
            .map_err(|e| EchoError::Config(format!("couldn't remove '{name}': {e}")))
    }

    /// Download a model into the models directory, emitting fractional
    /// progress (0.0..1.0) on `progress_tx`.
    pub async fn download(&self, name: &str, progress_tx: mpsc::Sender<f32>) -> Result<PathBuf> {
        let spec = Self::spec(name)?;
        let final_path = self.model_path(name);
        crate::core::download::download_file(spec.url, &final_path, spec.sha256, progress_tx)
            .await?;
        Ok(final_path)
    }
}

/// True if `name` is a known local Whisper model in the catalog.
pub fn is_whisper_model(name: &str) -> bool {
    MODEL_CATALOG.iter().any(|m| m.name == name)
}

#[allow(dead_code)]
pub fn models_dir_of(base: &Path) -> PathBuf {
    base.join("models")
}

#[cfg(test)]
mod tests {

    /// The point of the pairing: a smaller model for partials, so live text
    /// keeps up on a machine without a GPU.
    #[test]
    fn the_smallest_installed_model_is_chosen_for_partials() {
        let dir = scratch();
        let m = ModelManager::new(dir.clone());
        for name in ["tiny.en", "base.en", "small.en"] {
            std::fs::write(m.model_path(name), b"x").unwrap();
        }

        let picked = m
            .smaller_downloaded("small.en")
            .expect("something smaller exists");
        assert_eq!(picked.name, "tiny.en", "the smallest installed should win");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Families must not cross. Pairing an English-only partial model with a
    /// multilingual final would make the on-screen text flip language mid
    /// sentence as partials were replaced.
    #[test]
    fn partial_and_final_models_stay_in_the_same_family() {
        let dir = scratch();
        let m = ModelManager::new(dir.clone());
        std::fs::write(m.model_path("tiny.en"), b"x").unwrap();

        assert!(
            m.smaller_downloaded("small").is_none(),
            "an English-only model must not be paired with a multilingual one"
        );

        std::fs::write(m.model_path("tiny"), b"x").unwrap();
        assert_eq!(m.smaller_downloaded("small").unwrap().name, "tiny");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nothing smaller installed means partials use the main model, exactly as
    /// they did before.
    #[test]
    fn no_smaller_model_means_no_pairing() {
        let dir = scratch();
        let m = ModelManager::new(dir.clone());
        std::fs::write(m.model_path("tiny.en"), b"x").unwrap();
        assert!(m.smaller_downloaded("tiny.en").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("echo-models-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn delete_removes_the_weights_and_is_idempotent() {
        let dir = scratch();
        let mm = ModelManager::new(dir.clone());
        std::fs::write(mm.model_path("base.en"), b"weights").unwrap();
        assert!(mm.is_downloaded("base.en"));

        mm.delete("base.en").unwrap();
        assert!(!mm.is_downloaded("base.en"));
        // Deleting again is not an error â the end state is what was asked for.
        mm.delete("base.en").unwrap();

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_rejects_a_name_outside_the_catalog() {
        let dir = scratch();
        let mm = ModelManager::new(dir.clone());
        // Would otherwise be joined straight onto the models directory.
        assert!(mm.delete("../../echo.db").is_err());
        assert!(mm.delete("not-a-model").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
