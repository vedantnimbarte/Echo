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
    #[serde(default)]
    phrases: Vec<AzurePhrase>,
}

/// Used for both `combinedPhrases` and `phrases`; only the latter ever has a
/// `speaker`, and only with diarization on.
#[derive(Debug, Deserialize)]
struct AzurePhrase {
    #[serde(default)]
    text: String,
    #[serde(default)]
    speaker: Option<u32>,
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
        let parsed = self.send(part, &locale, false).await?;
        Ok(segment_from_response(parsed, &locale))
    }

    async fn transcribe_speakers(
        &self,
        audio: Vec<u8>,
        mime: &str,
        language: Option<&str>,
    ) -> Result<Vec<(String, String)>> {
        let part = multipart::Part::bytes(audio)
            .file_name("audio")
            .mime_str(mime)
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
        turns_from_response(self.send(part, &to_locale(language), true).await?)
    }
}

impl AzureSpeechProvider {
    /// One fast-transcription request. `speakers` is the import path: it turns
    /// diarization on and swaps the utterance-sized timeout for
    /// [`super::http::IMPORT_TIMEOUT`], because Azure answers a whole recording
    /// in this same single request.
    async fn send(
        &self,
        part: multipart::Part,
        locale: &str,
        speakers: bool,
    ) -> Result<AzureResponse> {
        let form = multipart::Form::new()
            .text("definition", definition(locale, speakers))
            .part("audio", part);

        crate::core::egress::record(&self.endpoint, "cloud transcription");

        let mut request = super::http::client()
            .post(&self.endpoint)
            .header("Ocp-Apim-Subscription-Key", &self.api_key)
            .multipart(form);
        if speakers {
            request = request.timeout(super::http::IMPORT_TIMEOUT);
        }
        let resp = request
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

        resp.json()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))
    }
}

/// The request's `definition` JSON.
///
/// For dictation, `channels: [0]` keeps Azure from splitting the mono capture
/// into a per-channel result set, which would arrive as an empty second phrase.
///
/// For speakers it is left out entirely. Azure does not diarize more than one
/// channel, and without `channels` it merges a stereo recording down to one
/// rather than transcribing each side — which is what a two-person call
/// recorded as stereo needs. `maxSpeakers` is a ceiling, not a count; Azure's
/// default of 2 would fold a third voice into one of the other two, so it is
/// raised to cover a meeting (Azure accepts 2–35).
fn definition(locale: &str, speakers: bool) -> String {
    let mut definition = serde_json::json!({
        "locales": [locale],
        "profanityFilterMode": "None",
    });
    if speakers {
        definition["diarization"] = serde_json::json!({ "enabled": true, "maxSpeakers": 10 });
    } else {
        definition["channels"] = serde_json::json!([0]);
    }
    definition.to_string()
}

/// Speaker turns out of a diarized response, in spoken order.
///
/// `phrases` is Azure's per-phrase list; with diarization on each carries an
/// integer `speaker`. A phrase without one means diarization did not run,
/// which is an error rather than a transcript attributed to nobody.
fn turns_from_response(parsed: AzureResponse) -> Result<Vec<(String, String)>> {
    parsed
        .phrases
        .into_iter()
        .filter(|p| !p.text.trim().is_empty())
        .map(|p| {
            let speaker = p
                .speaker
                .ok_or_else(|| EchoError::AsrProvider("azure returned no speaker labels".into()))?;
            Ok((speaker.to_string(), p.text.trim().to_string()))
        })
        .collect()
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
    fn diarization_replaces_the_channel_pin_only_for_an_import() {
        let import: serde_json::Value = serde_json::from_str(&definition("en-US", true)).unwrap();
        assert_eq!(import["diarization"]["enabled"], true);
        assert!(
            import.get("channels").is_none(),
            "Azure refuses to diarize separated channels"
        );

        let dictation: serde_json::Value =
            serde_json::from_str(&definition("en-US", false)).unwrap();
        assert!(dictation.get("diarization").is_none());
        assert_eq!(dictation["channels"], serde_json::json!([0]));
    }

    /// Shaped like the documented fast-transcription answer with diarization
    /// on: `phrases` in spoken order, each with an integer `speaker`.
    #[test]
    fn speaker_turns_are_read_out_of_the_phrases() {
        let parsed: AzureResponse = serde_json::from_str(
            r#"{
              "durationMilliseconds": 182439,
              "combinedPhrases": [{"text": "Good afternoon. Hi there."}],
              "phrases": [
                {"speaker": 1, "offsetMilliseconds": 960, "durationMilliseconds": 640,
                 "text": "Good afternoon.",
                 "words": [{"text": "Good", "offsetMilliseconds": 960, "durationMilliseconds": 240},
                           {"text": "afternoon.", "offsetMilliseconds": 1200, "durationMilliseconds": 400}],
                 "locale": "en-US", "confidence": 0.93616915},
                {"speaker": 0, "offsetMilliseconds": 5040, "durationMilliseconds": 400,
                 "text": "Hi there.",
                 "words": [{"text": "Hi", "offsetMilliseconds": 5040, "durationMilliseconds": 240},
                           {"text": "there.", "offsetMilliseconds": 5280, "durationMilliseconds": 160}],
                 "locale": "en-US", "confidence": 0.93616915}
              ]
            }"#,
        )
        .unwrap();
        assert_eq!(
            turns_from_response(parsed).unwrap(),
            vec![
                ("1".into(), "Good afternoon.".into()),
                ("0".into(), "Hi there.".into()),
            ]
        );
    }

    #[test]
    fn phrases_without_speakers_are_an_error_not_one_long_monologue() {
        let parsed: AzureResponse =
            serde_json::from_str(r#"{"phrases":[{"text":"hello","locale":"en-US"}]}"#).unwrap();
        assert!(turns_from_response(parsed).is_err());
    }

    #[test]
    fn silence_comes_back_as_empty_rather_than_as_an_error() {
        let seg = parse(r#"{"combinedPhrases":[]}"#);
        assert_eq!(seg.text, "");
    }
}
