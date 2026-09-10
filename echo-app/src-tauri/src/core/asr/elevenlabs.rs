//! ElevenLabs Scribe.
//!
//! Shaped almost like the OpenAI endpoint — one multipart POST, one JSON answer
//! — but different in the three places that matter: the key rides in an
//! `xi-api-key` header rather than a bearer token, the model field is
//! `model_id`, and the detected language comes back as `language_code`. Close
//! enough to look reusable, different enough that folding it into
//! [`super::openai::WhisperApiProvider`] would mean three conditionals in a
//! struct that currently has none.

use async_trait::async_trait;
use reqwest::multipart;
use serde::Deserialize;

use super::openai::join_url;
use super::wav::pcm_f32_to_wav;
use super::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

pub struct ElevenLabsProvider {
    endpoint: String,
    model: String,
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct ScribeResponse {
    text: String,
    #[serde(default)]
    language_code: Option<String>,
    #[serde(default)]
    language_probability: Option<f32>,
}

impl ElevenLabsProvider {
    pub fn new(base_url: &str, model: impl Into<String>, api_key: String) -> Self {
        Self {
            endpoint: join_url(base_url, "speech-to-text"),
            model: model.into(),
            api_key,
        }
    }
}

#[async_trait]
impl AsrProvider for ElevenLabsProvider {
    fn name(&self) -> &str {
        "elevenlabs"
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
            .text("model_id", self.model.clone())
            .part("file", part);
        // Omitting the field entirely is what asks Scribe to detect the
        // language; sending an empty one is a validation error.
        if let Some(lang) = language {
            form = form.text("language_code", lang.to_string());
        }

        crate::core::egress::record(&self.endpoint, "cloud transcription");

        let resp = super::http::client()
            .post(&self.endpoint)
            .header("xi-api-key", &self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(EchoError::AsrProvider(format!(
                "elevenlabs API error {status}: {body}"
            )));
        }

        let parsed: ScribeResponse = resp
            .json()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        Ok(segment_from_response(parsed))
    }
}

fn segment_from_response(parsed: ScribeResponse) -> TranscriptSegment {
    TranscriptSegment {
        text: parsed.text.trim().to_string(),
        is_final: true,
        language: parsed.language_code,
        confidence: parsed.language_probability,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> TranscriptSegment {
        segment_from_response(serde_json::from_str(json).expect("valid json"))
    }

    #[test]
    fn a_transcript_is_read_out_of_the_scribe_shape() {
        // `language_code`, not `language` — the field name is the whole reason
        // this provider is not the OpenAI one.
        let seg = parse(r#"{"text":"  hello there  ","language_code":"eng","language_probability":0.98}"#);
        assert_eq!(seg.text, "hello there");
        assert_eq!(seg.language.as_deref(), Some("eng"));
        assert_eq!(seg.confidence, Some(0.98));
        assert!(seg.is_final);
    }

    #[test]
    fn the_optional_fields_really_are_optional() {
        let seg = parse(r#"{"text":"hi"}"#);
        assert_eq!(seg.text, "hi");
        assert!(seg.language.is_none());
        assert!(seg.confidence.is_none());
    }

    #[test]
    fn the_endpoint_is_built_from_the_configured_base() {
        let p = ElevenLabsProvider::new("https://api.elevenlabs.io/v1/", "scribe_v2", "k".into());
        assert_eq!(p.endpoint, "https://api.elevenlabs.io/v1/speech-to-text");
    }
}
