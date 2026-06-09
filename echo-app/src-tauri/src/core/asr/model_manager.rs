use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::error::{EchoError, Result};

/// Catalog of downloadable Whisper models (ggml format, from Hugging Face).
/// `size_mb` is approximate and used only for display in the UI.
const MODEL_CATALOG: &[ModelSpec] = &[
    ModelSpec {
        name: "tiny",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        size_mb: 75,
    },
    ModelSpec {
        name: "base",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        size_mb: 142,
    },
    ModelSpec {
        name: "small",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        size_mb: 466,
    },
    ModelSpec {
        name: "medium",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        size_mb: 1500,
    },
];

struct ModelSpec {
    name: &'static str,
    url: &'static str,
    size_mb: u32,
}

/// Information about a model returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub downloaded: bool,
    pub size_mb: u32,
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
            })
            .collect()
    }

    fn spec(name: &str) -> Result<&'static ModelSpec> {
        MODEL_CATALOG
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| EchoError::NotFound(format!("Unknown model '{name}'")))
    }

    /// Download a model, streaming to a temp file and emitting fractional
    /// progress (0.0..1.0) on `progress_tx`. The file is renamed into place
    /// only after a complete download so partial files never look valid.
    pub async fn download(&self, name: &str, progress_tx: mpsc::Sender<f32>) -> Result<PathBuf> {
        let spec = Self::spec(name)?;
        tokio::fs::create_dir_all(&self.models_dir)
            .await
            .map_err(|e| EchoError::Config(e.to_string()))?;

        let final_path = self.model_path(name);
        let tmp_path = final_path.with_extension("part");

        let resp = reqwest::get(spec.url)
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?
            .error_for_status()
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        let total = resp.content_length();
        let mut downloaded: u64 = 0;
        let mut last_emitted = -1.0_f32;

        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| EchoError::Config(e.to_string()))?;
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| EchoError::AsrProvider(e.to_string()))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| EchoError::Config(e.to_string()))?;
            downloaded += chunk.len() as u64;

            if let Some(total) = total {
                let progress = (downloaded as f32 / total as f32).clamp(0.0, 1.0);
                // Throttle: only emit on ~1% changes to avoid flooding the bus.
                if progress - last_emitted >= 0.01 {
                    last_emitted = progress;
                    let _ = progress_tx.send(progress).await;
                }
            }
        }

        file.flush()
            .await
            .map_err(|e| EchoError::Config(e.to_string()))?;
        drop(file);

        tokio::fs::rename(&tmp_path, &final_path)
            .await
            .map_err(|e| EchoError::Config(e.to_string()))?;

        let _ = progress_tx.send(1.0).await;
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
