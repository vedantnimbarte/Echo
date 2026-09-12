//! Azure AI Speech, via the fast-transcription REST API.
//!
//! Two things make this provider unlike the others. The host is built from the
//! user's **region** rather than being fixed, so a key alone is not enough to
//! reach it — see `needs_region` in [`super::catalog`]. And there is no
//! auto-detect: Azure wants explicit BCP-47 locales, which is why
//! [`super::locale::to_locale`] exists.

use async_trait::async_trait;
use reqwest::multipart;
use serde::Deserialize;

use super::locale::to_locale;
use super::wav::pcm_f32_to_wav;
use super::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

/// Pinned rather than tracking the newest preview: Azure changes response
/// shapes between versions, and a silent upgrade would break parsing in the
/// field rather than in CI.
const API_VERSION: &str = "2024-11-15";

pub struct AzureSpeechProvider {
    endpoint: String,
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct AzureResponse {
    #[serde(default, rename = "combinedPhrases")]
    combined_phrases: Vec<AzurePhrase>,
    #[serde(default)]
    duration: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AzurePhrase {
    #[serde(default)]
    text: String,
}

/// The transcription endpoint for a Speech resource in `region`.
pub fn transcribe_url(region: &str) -> String {
    format!(
        "https://{}.api.cognitive.microsoft.com/speechtotext/transcriptions:transcribe?api-version={API_VERSION}",
        region.trim()
    )
}

impl AzureSpeechProvider {
    pub fn new(region: &str, api_key: String) -> Self {
        Self {
            endpoint: transcribe_url(region),
            api_key,
        }
    }
}

#[async_trait]
impl AsrProvider for AzureSpeechProvider {
    fn name(&self) -> &str {
        "azure"
    }

    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        let wav = pcm_f32_to_wav(&audio, 16_000)?;
        let locale = to_locale(language);

        let part = multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        // `channels: [0]` keeps Azure from splitting the mono capture into a
        // per-channel result set, which would arrive as an empty second phrase.
        let definition = serde_json::json!({
            "locales": [locale],
            "profanityFilterMode": "None",
            "channels": [0],
        })
        .to_string();

        let form = multipart::Form::new()
            .text("definition", definition)
            .part("audio", part);

        crate::core::egress::record(&self.endpoint, "cloud transcription");

        let resp = super::http::client()
            .post(&self.endpoint)
            .header("Ocp-Apim-Subscription-Key", &self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(EchoError::AsrProvider(format!(
                "azure API error {status}: {body}"
            )));
        }

        let parsed: AzureResponse = resp
            .json()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        Ok(segment_from_response(parsed, &locale))
    }
}

/// Flatten Azure's phrase list into one transcript.
///
/// Azure reports the locale it was *told* to use rather than one it detected,
/// so the locale is passed back in rather than read out of the response — there
/// is nothing to read.
fn segment_from_response(parsed: AzureResponse, locale: &str) -> TranscriptSegment {
    let text = parsed
        .combined_phrases
        .iter()
        .map(|p| p.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let _ = parsed.duration;

    TranscriptSegment {
        text,
        is_final: true,
        language: Some(locale.to_string()),
        confidence: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> TranscriptSegment {
        segment_from_response(serde_json::from_str(json).expect("valid json"), "en-US")
    }

    #[test]
    fn the_host_is_built_from_the_region() {
        let url = transcribe_url("westeurope");
        assert!(
            url.starts_with("https://westeurope.api.cognitive.microsoft.com/"),
            "{url}"
        );
        assert!(url.contains("api-version="), "{url}");
    }

    #[test]
    fn a_region_pasted_with_whitespace_still_produces_a_valid_host() {
        assert_eq!(transcribe_url("  eastus  "), transcribe_url("eastus"));
    }

    #[test]
    fn phrases_are_joined_into_one_transcript() {
        let seg = parse(r#"{"combinedPhrases":[{"text":"hello there"}],"duration":1200}"#);
        assert_eq!(seg.text, "hello there");
        assert_eq!(seg.language.as_deref(), Some("en-US"));
    }

    #[test]
    fn empty_phrases_do_not_become_stray_spaces() {
        // Azure emits an empty phrase for a silent channel; joining blindly
        // would prefix the transcript with a space and shift every insert.
        let seg = parse(r#"{"combinedPhrases":[{"text":""},{"text":"second"}]}"#);
        assert_eq!(seg.text, "second");
    }

    #[test]
    fn silence_comes_back_as_empty_rather_than_as_an_error() {
        let seg = parse(r#"{"combinedPhrases":[]}"#);
        assert_eq!(seg.text, "");
    }
}
