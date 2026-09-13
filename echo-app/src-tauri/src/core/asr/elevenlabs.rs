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
    #[serde(default)]
    words: Vec<ScribeWord>,
}

/// One entry of Scribe's `words` list, which holds spacing and audio events
/// ("(laughter)") alongside the words themselves.
#[derive(Debug, Deserialize)]
struct ScribeWord {
    #[serde(default)]
    text: String,
    #[serde(default, rename = "type")]
    kind: String,
    /// "speaker_0", "speaker_1", …; only present with `diarize` on.
    #[serde(default)]
    speaker_id: Option<String>,
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
        Ok(segment_from_response(
            self.send(part, language, false).await?,
        ))
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
        turns_from_response(self.send(part, language, true).await?)
    }
}

impl ElevenLabsProvider {
    /// One request to Scribe. `speakers` is the import path: it sets `diarize`
    /// and swaps the utterance-sized timeout for
    /// [`super::http::IMPORT_TIMEOUT`], because Scribe answers a whole
    /// recording in the same single request.
    async fn send(
        &self,
        part: multipart::Part,
        language: Option<&str>,
        speakers: bool,
    ) -> Result<ScribeResponse> {
        let mut form = multipart::Form::new()
            .text("model_id", self.model.clone())
            .part("file", part);
        // Omitting the field entirely is what asks Scribe to detect the
        // language; sending an empty one is a validation error.
        if let Some(lang) = language {
            form = form.text("language_code", lang.to_string());
        }
        if speakers {
            form = form.text("diarize", "true");
        }

        crate::core::egress::record(&self.endpoint, "cloud transcription");

        let mut request = super::http::client()
            .post(&self.endpoint)
            .header("xi-api-key", &self.api_key)
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
                "elevenlabs API error {status}: {body}"
            )));
        }

        resp.json()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))
    }
}

/// Speaker turns out of a diarized response, in spoken order.
///
/// Scribe labels each word, not each sentence, so a turn is a run of words
/// with the same `speaker_id`. Spacing entries are kept (they are the only
/// spaces there are — the words carry none) and audio events are dropped: a
/// "(laughter)" in the middle of someone's paragraph reads as something they
/// said. A word with no speaker means diarization did not run, which is an
/// error rather than a transcript attributed to nobody.
fn turns_from_response(parsed: ScribeResponse) -> Result<Vec<(String, String)>> {
    let mut turns: Vec<(String, String)> = Vec::new();
    for word in parsed.words {
        match word.kind.as_str() {
            "word" => {
                let speaker = word.speaker_id.ok_or_else(|| {
                    EchoError::AsrProvider("elevenlabs returned no speaker labels".into())
                })?;
                match turns.last_mut() {
                    Some((current, text)) if *current == speaker => text.push_str(&word.text),
                    _ => turns.push((speaker, word.text)),
                }
            }
            "spacing" => {
                if let Some((_, text)) = turns.last_mut() {
                    text.push_str(&word.text);
                }
            }
            _ => {}
        }
    }
    Ok(turns
        .into_iter()
        .map(|(speaker, text)| (speaker, text.trim().to_string()))
        .collect())
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
        let seg = parse(
            r#"{"text":"  hello there  ","language_code":"eng","language_probability":0.98}"#,
        );
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

    /// Shaped like the documented `POST /v1/speech-to-text` answer with
    /// `diarize` on: words, spacing and audio events share one list, each
    /// word carrying a `speaker_id`.
    #[test]
    fn speaker_turns_are_rebuilt_from_labelled_words() {
        let parsed: ScribeResponse = serde_json::from_str(
            r#"{
              "language_code": "en",
              "language_probability": 0.98,
              "text": "Hello there. (laughter) Hi!",
              "words": [
                {"text": "Hello", "start": 0.0, "end": 0.5, "type": "word", "speaker_id": "speaker_0", "logprob": -0.12, "characters": []},
                {"text": " ", "start": 0.5, "end": 0.52, "type": "spacing", "speaker_id": "speaker_0", "logprob": 0.0},
                {"text": "there.", "start": 0.52, "end": 0.9, "type": "word", "speaker_id": "speaker_0", "logprob": -0.2},
                {"text": " ", "start": 0.9, "end": 1.0, "type": "spacing", "speaker_id": "speaker_0", "logprob": 0.0},
                {"text": "(laughter)", "start": 1.0, "end": 1.6, "type": "audio_event", "speaker_id": "speaker_1", "logprob": -0.4},
                {"text": " ", "start": 1.6, "end": 1.7, "type": "spacing", "speaker_id": "speaker_1", "logprob": 0.0},
                {"text": "Hi!", "start": 1.7, "end": 2.0, "type": "word", "speaker_id": "speaker_1", "logprob": -0.1}
              ]
            }"#,
        )
        .unwrap();
        assert_eq!(
            turns_from_response(parsed).unwrap(),
            vec![
                ("speaker_0".into(), "Hello there.".into()),
                ("speaker_1".into(), "Hi!".into()),
            ]
        );
    }

    #[test]
    fn words_without_speakers_are_an_error_not_one_long_monologue() {
        let parsed: ScribeResponse = serde_json::from_str(
            r#"{"text":"hi","words":[{"text":"hi","type":"word","start":0,"end":0.2}]}"#,
        )
        .unwrap();
        assert!(turns_from_response(parsed).is_err());
    }

    #[test]
    fn the_endpoint_is_built_from_the_configured_base() {
        let p = ElevenLabsProvider::new("https://api.elevenlabs.io/v1/", "scribe_v2", "k".into());
        assert_eq!(p.endpoint, "https://api.elevenlabs.io/v1/speech-to-text");
    }
}
