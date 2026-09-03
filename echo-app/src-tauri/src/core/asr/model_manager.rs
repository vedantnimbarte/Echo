use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::{EchoError, Result};

/// Catalog of downloadable Whisper models (ggml format, from Hugging Face).
/// `size_mb` is approximate and used only for display in the UI.
const MODEL_CATALOG: &[ModelSpec] = &[
    // English-only models — smaller and more accurate for English speech.
    ModelSpec {
        name: "tiny.en",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        size_mb: 75,
        english_only: true,
    },
    ModelSpec {
        name: "base.en",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
        size_mb: 142,
        english_only: true,
    },
    ModelSpec {
        name: "small.en",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        size_mb: 466,
        english_only: true,
    },
    // Multilingual models.
    ModelSpec {
        name: "tiny",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        size_mb: 75,
        english_only: false,
    },
    ModelSpec {
        name: "base",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        size_mb: 142,
        english_only: false,
    },
    ModelSpec {
        name: "small",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        size_mb: 466,
        english_only: false,
    },
    ModelSpec {
        name: "medium",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        size_mb: 1500,
        english_only: false,
    },
];

/// The model fetched on first run and used by default.
pub const DEFAULT_MODEL: &str = "base.en";

struct ModelSpec {
    name: &'static str,
    url: &'static str,
    size_mb: u32,
    english_only: bool,
}

/// Information about a model returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub downloaded: bool,
    pub size_mb: u32,
    pub english_only: bool,
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
        self.models_dir.join(format!("ggml-{name}.bin"))
    }

    pub fn is_downloaded(&self, name: &str) -> bool {
        self.model_path(name).exists()
    }

    /// List the catalog with each model's local download status.
    pub fn list(&self) -> Vec<ModelInfo> {
        MODEL_CATALOG
            .iter()
            .map(|m| ModelInfo {
                name: m.name.to_string(),
                downloaded: self.is_downloaded(m.name),
                size_mb: m.size_mb,
                english_only: m.english_only,
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
        crate::core::download::download_file(spec.url, &final_path, progress_tx).await?;
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
