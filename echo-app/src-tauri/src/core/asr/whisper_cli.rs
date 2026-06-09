use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::Command;

use super::wav::pcm_f32_to_wav;
use super::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

/// Offline transcription provider that shells out to a bundled `whisper-cli`
/// binary (whisper.cpp). Unlike the in-process `whisper-rs` path this needs no
/// cmake/libclang at build time — the binary is downloaded on first run by
/// [`crate::core::asr::binary_manager::BinaryManager`].
///
/// This is the default local engine. It is always compiled in.
pub struct WhisperCliProvider {
    binary: PathBuf,
    model_path: PathBuf,
    /// Catalog name of the loaded model (e.g. `base.en`). English-only models
    /// (`*.en`) are pinned to English; others auto-detect.
    model_name: String,
}

impl WhisperCliProvider {
    pub fn new(binary: PathBuf, model_path: PathBuf, model_name: impl Into<String>) -> Self {
        Self {
            binary,
            model_path,
            model_name: model_name.into(),
        }
    }

    fn is_english_only(&self) -> bool {
        self.model_name.ends_with(".en")
    }
}

#[async_trait]
impl AsrProvider for WhisperCliProvider {
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

        // whisper-cli reads 16 kHz mono WAV from disk. Stage the utterance in a
        // unique temp file so concurrent utterances never collide.
        let wav = pcm_f32_to_wav(&audio, 16_000)?;
        let tmp = std::env::temp_dir().join(format!("echo-{}.wav", uuid::Uuid::new_v4()));
        tokio::fs::write(&tmp, &wav)
            .await
            .map_err(|e| EchoError::Config(e.to_string()))?;

        // English-only models must run in `en`; multilingual models honour the
        // caller's language or auto-detect.
        let lang = if self.is_english_only() {
            "en"
        } else {
            language.unwrap_or("auto")
        };

        let output = Command::new(&self.binary)
            .arg("-m")
            .arg(&self.model_path)
            .arg("-f")
            .arg(&tmp)
            .args(["-l", lang])
            .arg("-nt") // no timestamps — stdout is plain transcript text
            .arg("-np") // no progress / system-info prints
            .output()
            .await
            .map_err(|e| {
                EchoError::AsrProvider(format!(
                    "failed to launch whisper-cli at {}: {e}",
                    self.binary.display()
                ))
            })?;

        let _ = tokio::fs::remove_file(&tmp).await;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EchoError::AsrProvider(format!(
                "whisper-cli exited with {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        let text = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && *l != "[BLANK_AUDIO]")
            .collect::<Vec<_>>()
            .join(" ");

        Ok(TranscriptSegment {
            text: text.trim().to_string(),
            is_final: true,
            language: self.is_english_only().then(|| "en".to_string()),
            confidence: None,
        })
    }

    // The default `transcribe_stream` (accumulate per utterance, transcribe on
    // the VAD end-sentinel) is exactly the "local chunked" behaviour we want.
}
