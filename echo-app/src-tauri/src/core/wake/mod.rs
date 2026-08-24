//! Wake-word detection: "say the phrase, start dictating".
//!
//! Models are openWakeWord's ONNX releases, downloaded on first enable rather
//! than committed — the two shared feature models are reused by every phrase,
//! so switching phrases only fetches a ~1 MB classifier.
//!
//! Custom phrases are supported by pointing `wake_word_model` at an imported
//! `.onnx` file (see `docs/WAKE_WORD.md` for how to train one).

mod onnx;

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::core::download::download_file;
use crate::error::{EchoError, Result};

pub use onnx::{WakeModel, WakeSpotter};

/// openWakeWord's pinned model release. Bump deliberately: the feature models
/// and the phrase classifiers are trained together and must stay in step.
const RELEASE: &str =
    "https://github.com/dscripka/openWakeWord/releases/download/v0.5.1";

/// Feature models shared by every phrase — downloaded once.
const MELSPEC_FILE: &str = "melspectrogram.onnx";
const EMBEDDING_FILE: &str = "embedding_model.onnx";

/// Id used for a user-supplied phrase model imported from disk.
pub const CUSTOM_ID: &str = "custom";
/// Filename a custom model is copied to inside the wake-model directory.
const CUSTOM_FILE: &str = "custom.onnx";

/// The phrase used when none is configured.
pub const DEFAULT_PHRASE: &str = "hey_jarvis";

/// Detection score above which the phrase counts as spoken. Lower catches more
/// (and misfires more); the `wake_word_sensitivity` setting overrides it.
pub const DEFAULT_THRESHOLD: f32 = 0.5;

struct PhraseSpec {
    id: &'static str,
    label: &'static str,
    file: &'static str,
}

/// Pretrained phrases shipped by openWakeWord. None of these is "Hey Echo" —
/// training that model is a separate, documented step (`docs/WAKE_WORD.md`);
/// until it exists "Hey Jarvis" is the most reliable default.
const PHRASE_CATALOG: &[PhraseSpec] = &[
    PhraseSpec {
        id: "hey_jarvis",
        label: "Hey Jarvis",
        file: "hey_jarvis_v0.1.onnx",
    },
    PhraseSpec {
        id: "alexa",
        label: "Alexa",
        file: "alexa_v0.1.onnx",
    },
    PhraseSpec {
        id: "hey_mycroft",
        label: "Hey Mycroft",
        file: "hey_mycroft_v0.1.onnx",
    },
    PhraseSpec {
        id: "hey_rhasspy",
        label: "Hey Rhasspy",
        file: "hey_rhasspy_v0.1.onnx",
    },
];

/// A wake phrase and its local availability, for the settings UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakePhraseInfo {
    pub id: String,
    pub label: String,
    pub downloaded: bool,
    /// True for a user-imported model rather than a catalog entry.
    pub custom: bool,
}

/// Manages the local wake-model files: listing, download, import, and loading.
pub struct WakeModelManager {
    dir: PathBuf,
}

impl WakeModelManager {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn spec(id: &str) -> Result<&'static PhraseSpec> {
        PHRASE_CATALOG
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| EchoError::NotFound(format!("Unknown wake phrase '{id}'")))
    }

    /// Path of the classifier for `id` (the imported file for [`CUSTOM_ID`]).
    pub fn phrase_path(&self, id: &str) -> PathBuf {
        if id == CUSTOM_ID {
            self.dir.join(CUSTOM_FILE)
        } else {
            match Self::spec(id) {
                Ok(spec) => self.dir.join(spec.file),
                // Unknown ids resolve to a path that simply won't exist, so
                // callers report "not downloaded" instead of erroring.
                Err(_) => self.dir.join(format!("{id}.onnx")),
            }
        }
    }

    /// True once both shared feature models are on disk.
    pub fn shared_ready(&self) -> bool {
        self.dir.join(MELSPEC_FILE).exists() && self.dir.join(EMBEDDING_FILE).exists()
    }

    /// True when `id` can actually be loaded (features + classifier present).
    pub fn is_ready(&self, id: &str) -> bool {
        self.shared_ready() && self.phrase_path(id).exists()
    }

    /// The catalog plus, if one has been imported, the custom entry.
    pub fn list(&self) -> Vec<WakePhraseInfo> {
        let mut out: Vec<WakePhraseInfo> = PHRASE_CATALOG
            .iter()
            .map(|p| WakePhraseInfo {
                id: p.id.to_string(),
                label: p.label.to_string(),
                downloaded: self.dir.join(p.file).exists(),
                custom: false,
            })
            .collect();

        if self.dir.join(CUSTOM_FILE).exists() {
            out.push(WakePhraseInfo {
                id: CUSTOM_ID.to_string(),
                label: "Custom phrase".to_string(),
                downloaded: true,
                custom: true,
            });
        }
        out
    }

    /// Fetch whatever `id` still needs: the shared feature models on first use,
    /// then the phrase classifier. Progress is reported across the whole set so
    /// the UI shows one continuous bar.
    pub async fn download(&self, id: &str, progress_tx: mpsc::Sender<f32>) -> Result<()> {
        if id == CUSTOM_ID {
            return Err(EchoError::NotFound(
                "A custom wake phrase is imported from a file, not downloaded".into(),
            ));
        }
        let spec = Self::spec(id)?;

        let mut jobs: Vec<(&str, PathBuf)> = Vec::new();
        for shared in [MELSPEC_FILE, EMBEDDING_FILE] {
            let dest = self.dir.join(shared);
            if !dest.exists() {
                jobs.push((shared, dest));
            }
        }
        let phrase_dest = self.dir.join(spec.file);
        if !phrase_dest.exists() {
            jobs.push((spec.file, phrase_dest));
        }

        let total = jobs.len();
        for (index, (file, dest)) in jobs.into_iter().enumerate() {
            // Rescale each file's 0..1 into its slice of the overall bar.
            let (sub_tx, mut sub_rx) = mpsc::channel::<f32>(32);
            let outer = progress_tx.clone();
            let relay = tokio::spawn(async move {
                while let Some(p) = sub_rx.recv().await {
                    let overall = (index as f32 + p) / total as f32;
                    let _ = outer.send(overall.clamp(0.0, 1.0)).await;
                }
            });

            let url = format!("{RELEASE}/{file}");
            let result = download_file(&url, &dest, sub_tx).await;
            let _ = relay.await;
            result?;
        }

        let _ = progress_tx.send(1.0).await;
        Ok(())
    }

    /// Copy a user-trained `.onnx` classifier into the wake-model directory.
    /// The shared feature models must already be present — a custom classifier
    /// is useless without them.
    pub async fn import_custom(&self, source: &std::path::Path) -> Result<()> {
        if source.extension().and_then(|e| e.to_str()) != Some("onnx") {
            return Err(EchoError::Config(
                "A custom wake word model must be an .onnx file".into(),
            ));
        }
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|e| EchoError::Config(e.to_string()))?;
        tokio::fs::copy(source, self.dir.join(CUSTOM_FILE))
            .await
            .map_err(|e| EchoError::Config(format!("failed to import wake model: {e}")))?;
        Ok(())
    }

    /// Ensure the shared feature models exist, downloading them if a custom
    /// classifier was imported before any catalog phrase was fetched.
    pub async fn ensure_shared(&self, progress_tx: mpsc::Sender<f32>) -> Result<()> {
        for (index, file) in [MELSPEC_FILE, EMBEDDING_FILE].into_iter().enumerate() {
            let dest = self.dir.join(file);
            if dest.exists() {
                continue;
            }
            let (sub_tx, mut sub_rx) = mpsc::channel::<f32>(32);
            let outer = progress_tx.clone();
            let relay = tokio::spawn(async move {
                while let Some(p) = sub_rx.recv().await {
                    let _ = outer.send(((index as f32 + p) / 2.0).clamp(0.0, 1.0)).await;
                }
            });
            let result = download_file(&format!("{RELEASE}/{file}"), &dest, sub_tx).await;
            let _ = relay.await;
            result?;
        }
        let _ = progress_tx.send(1.0).await;
        Ok(())
    }

    /// Build the inference chain for `id`.
    pub fn load(&self, id: &str) -> Result<Arc<WakeModel>> {
        if !self.shared_ready() {
            return Err(EchoError::NotFound(
                "Wake word feature models are not downloaded yet".into(),
            ));
        }
        let phrase = self.phrase_path(id);
        if !phrase.exists() {
            return Err(EchoError::NotFound(format!(
                "Wake phrase '{id}' is not installed"
            )));
        }
        Ok(Arc::new(WakeModel::load(
            &self.dir.join(MELSPEC_FILE),
            &self.dir.join(EMBEDDING_FILE),
            &phrase,
        )?))
    }
}
