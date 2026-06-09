use async_trait::async_trait;
use reqwest::multipart;
use serde::Deserialize;

use super::wav::pcm_f32_to_wav;
use super::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

/// Shared implementation for OpenAI-compatible `/audio/transcriptions` endpoints.
/// OpenAI and Groq use the same multipart request shape, differing only in base
/// URL, model name, and the provider key used to look up the API key.
pub struct WhisperApiProvider {
    provider_name: String,
    endpoint: String,
    model: String,
    api_key: String,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
    #[serde(default)]
    language: Option<String>,
}

impl WhisperApiProvider {
    fn new(
        provider_name: impl Into<String>,
        endpoint: impl Into<String>,
        model: impl Into<String>,
        api_key: String,
    ) -> Self {
        Self {
            provider_name: provider_name.into(),
            endpoint: endpoint.into(),
            model: model.into(),
            api_key,
            client: reqwest::Client::new(),
        }
    }

    /// OpenAI Whisper API (`whisper-1`).
    pub fn openai(api_key: String) -> Self {
        Self::new(
            "openai",
            "https://api.openai.com/v1/audio/transcriptions",
            "whisper-1",
            api_key,
        )
    }

    /// Groq's OpenAI-compatible transcription endpoint (`whisper-large-v3`).
    pub fn groq(api_key: String) -> Self {
        Self::new(
            "groq",
            "https://api.groq.com/openai/v1/audio/transcriptions",
            "whisper-large-v3",
            api_key,
        )
    }
}

#[async_trait]
impl AsrProvider for WhisperApiProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        let wav = pcm_f32_to_wav(&audio, 16_000)?;

        let part = multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        let mut form = multipart::Form::new()
            .text("model", self.model.clone())
            .text("response_format", "verbose_json")
            .part("file", part);
        if let Some(lang) = language {
            form = form.text("language", lang.to_string());
        }

        let resp = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(EchoError::AsrProvider(format!(
                "{} API error {status}: {body}",
                self.provider_name
            )));
        }

        let parsed: TranscriptionResponse = resp
            .json()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        Ok(TranscriptSegment {
            text: parsed.text.trim().to_string(),
            is_final: true,
            language: parsed.language,
            confidence: None,
        })
    }
}
