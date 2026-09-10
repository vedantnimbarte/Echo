use std::sync::Arc;

use async_trait::async_trait;
use reqwest::multipart;
use serde::Deserialize;
use tokio::sync::RwLock;

use super::prompt::PromptContext;
use super::wav::pcm_f32_to_wav;
use super::whisper_cli::initial_prompt;
use super::{AsrProvider, TranscriptSegment};
use crate::core::dictionary::DictionaryEngine;
use crate::error::{EchoError, Result};

/// Shared implementation for OpenAI-compatible `/audio/transcriptions` endpoints.
///
/// OpenAI, Groq, Mistral and every self-hosted clone (vLLM, LiteLLM, faster-whisper
/// servers, OpenRouter) accept the same multipart request, differing only in base
/// URL and model name. That is why this takes both as data rather than hardcoding
/// them: the "custom endpoint" provider is this struct with user-supplied strings,
/// not a separate implementation.
pub struct WhisperApiProvider {
    provider_name: String,
    endpoint: String,
    model: String,
    api_key: String,
    /// Custom-dictionary terms and the preceding sentence, biasing the decoder
    /// toward the right spellings — the same context the local engine gets.
    /// `None` outside the dictation pipeline (file imports, key tests).
    dictionary: Option<Arc<RwLock<DictionaryEngine>>>,
    context: Option<Arc<PromptContext>>,
}

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
    #[serde(default)]
    language: Option<String>,
}

/// Join a base URL and a path without doubling or dropping the separator.
///
/// Users type base URLs both ways, and `http://localhost:8000/v1/` +
/// `audio/transcriptions` must not become a 404 over a slash.
pub fn join_url(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

impl WhisperApiProvider {
    /// Build a provider against any OpenAI-compatible base URL.
    ///
    /// `base_url` is the API root (`https://api.openai.com/v1`), not the full
    /// transcription path — that suffix is fixed by the API shape.
    pub fn new(
        provider_name: impl Into<String>,
        base_url: &str,
        model: impl Into<String>,
        api_key: String,
    ) -> Self {
        Self {
            provider_name: provider_name.into(),
            endpoint: join_url(base_url, "audio/transcriptions"),
            model: model.into(),
            api_key,
            dictionary: None,
            context: None,
        }
    }

    /// Give the provider the same dictionary the offline engine uses.
    pub fn with_dictionary(mut self, dictionary: Arc<RwLock<DictionaryEngine>>) -> Self {
        self.dictionary = Some(dictionary);
        self
    }

    /// Give the provider the per-app prompt context (carried sentence, profile).
    pub fn with_prompt_context(mut self, context: Arc<PromptContext>) -> Self {
        self.context = Some(context);
        self
    }

    /// Build the multipart body for one request.
    ///
    /// Rebuilt per attempt rather than cloned: a `multipart::Form` is consumed
    /// by `send`, so the retry needs its own.
    fn form(&self, wav: Vec<u8>, language: Option<&str>, prompt: Option<&str>) -> Result<multipart::Form> {
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
        if let Some(prompt) = prompt {
            form = form.text("prompt", prompt.to_string());
        }
        Ok(form)
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
        let prompt = initial_prompt(self.dictionary.as_ref(), self.context.as_ref()).await;

        crate::core::egress::record(&self.endpoint, "cloud transcription");

        // One retry, then give up so the offline fallback can have a turn.
        // Looping longer would keep someone staring at an empty cursor while a
        // working local engine sits idle.
        let mut last_err = None;
        for attempt in 0..2 {
            if attempt > 0 {
                tokio::time::sleep(super::http::RETRY_DELAY).await;
            }

            let form = self.form(wav.clone(), language, prompt.as_deref())?;
            let resp = super::http::client()
                .post(&self.endpoint)
                .bearer_auth(&self.api_key)
                .multipart(form)
                .send()
                .await
                .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                let err = EchoError::AsrProvider(format!(
                    "{} API error {status}: {body}",
                    self.provider_name
                ));
                if super::http::is_retryable(status) && attempt == 0 {
                    tracing::warn!(provider = %self.provider_name, %status, "Retrying once");
                    last_err = Some(err);
                    continue;
                }
                return Err(err);
            }

            let parsed: TranscriptionResponse = resp
                .json()
                .await
                .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

            return Ok(TranscriptSegment {
                text: parsed.text.trim().to_string(),
                is_final: true,
                language: parsed.language,
                confidence: None,
            });
        }

        Err(last_err
            .unwrap_or_else(|| EchoError::AsrProvider("transcription failed".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trailing_slash_does_not_produce_a_double_one() {
        // Both spellings are what people actually paste into a settings field,
        // and the difference used to be a 404 nobody could explain.
        assert_eq!(
            join_url("https://api.openai.com/v1", "audio/transcriptions"),
            "https://api.openai.com/v1/audio/transcriptions"
        );
        assert_eq!(
            join_url("https://api.openai.com/v1/", "audio/transcriptions"),
            "https://api.openai.com/v1/audio/transcriptions"
        );
        assert_eq!(
            join_url("http://localhost:8000/v1/", "/audio/transcriptions"),
            "http://localhost:8000/v1/audio/transcriptions"
        );
    }

    #[test]
    fn the_endpoint_is_derived_from_whatever_base_the_user_gave() {
        let p = WhisperApiProvider::new("custom", "http://localhost:8000/v1", "whisper-1", "k".into());
        assert_eq!(p.endpoint, "http://localhost:8000/v1/audio/transcriptions");
        assert_eq!(p.name(), "custom");
    }
}
