//! Google Cloud Speech-to-Text, synchronous `speech:recognize`.
//!
//! The odd one out in two ways, both of which the catalog note warns about.
//!
//! **The key travels in the query string.** That is Google's design for REST
//! API-key auth, not a choice made here — but it means the key can land in the
//! logs of any proxy between Echo and Google. [`crate::core::egress`] records
//! hosts only, so Echo itself never stores it.
//!
//! **The sync endpoint caps audio at 60 seconds.** That is enforced here, up
//! front, because the alternative is Google truncating the request and
//! returning a confident partial sentence that looks like a complete one.

use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;

use super::locale::to_locale;
use super::openai::join_url;
use super::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

/// Google's own limit on the synchronous endpoint.
const MAX_SECONDS: usize = 60;
const SAMPLE_RATE: usize = 16_000;

pub struct GoogleSttProvider {
    endpoint: String,
    model: String,
    api_key: String,
}

#[derive(Debug, Deserialize)]
struct GoogleResponse {
    #[serde(default)]
    results: Vec<GoogleResult>,
}

#[derive(Debug, Deserialize)]
struct GoogleResult {
    #[serde(default)]
    alternatives: Vec<GoogleAlternative>,
}

#[derive(Debug, Deserialize)]
struct GoogleAlternative {
    #[serde(default)]
    transcript: String,
    #[serde(default)]
    confidence: Option<f32>,
}

impl GoogleSttProvider {
    pub fn new(base_url: &str, model: impl Into<String>, api_key: String) -> Self {
        Self {
            endpoint: join_url(base_url, "speech:recognize"),
            model: model.into(),
            api_key,
        }
    }
}

/// Convert f32 samples to the 16-bit little-endian PCM `LINEAR16` means.
///
/// The clamp matters for the same reason it does in the Deepgram writer: a
/// sample past full scale, cast without clamping, wraps to a large negative
/// value and the loudest word of a sentence returns as a crackle.
fn pcm_le_bytes(audio: &[f32]) -> Vec<u8> {
    let mut pcm = Vec::with_capacity(audio.len() * 2);
    for s in audio {
        pcm.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    pcm
}

/// Flatten Google's per-result alternatives into one transcript.
fn segment_from_response(parsed: GoogleResponse, locale: &str) -> TranscriptSegment {
    let mut parts: Vec<String> = Vec::new();
    let mut confidences: Vec<f32> = Vec::new();

    for result in parsed.results {
        // Alternatives are ranked; anything past the first is a worse guess at
        // the same audio, so taking more than one would duplicate the sentence.
        if let Some(alt) = result.alternatives.into_iter().next() {
            let text = alt.transcript.trim().to_string();
            if !text.is_empty() {
                parts.push(text);
            }
            if let Some(c) = alt.confidence {
                confidences.push(c);
            }
        }
    }

    let confidence = (!confidences.is_empty())
        .then(|| confidences.iter().sum::<f32>() / confidences.len() as f32);

    TranscriptSegment {
        text: parts.join(" "),
        is_final: true,
        language: Some(locale.to_string()),
        confidence,
    }
}

#[async_trait]
impl AsrProvider for GoogleSttProvider {
    fn name(&self) -> &str {
        "google"
    }

    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        if audio.len() > MAX_SECONDS * SAMPLE_RATE {
            return Err(EchoError::AsrProvider(format!(
                "Google's synchronous API accepts at most {MAX_SECONDS} seconds of audio; \
                 this was {}s. Use a different provider for longer recordings.",
                audio.len() / SAMPLE_RATE
            )));
        }

        let locale = to_locale(language);
        let content = base64::engine::general_purpose::STANDARD.encode(pcm_le_bytes(&audio));

        let body = serde_json::json!({
            "config": {
                "encoding": "LINEAR16",
                "sampleRateHertz": SAMPLE_RATE,
                "languageCode": locale,
                "model": self.model,
                "enableAutomaticPunctuation": true,
            },
            "audio": { "content": content },
        });

        // Recorded without the query string, so the key is not written down.
        crate::core::egress::record(&self.endpoint, "cloud transcription");

        let resp = super::http::client()
            .post(&self.endpoint)
            .query(&[("key", &self.api_key)])
            .json(&body)
            .send()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(EchoError::AsrProvider(format!(
                "google API error {status}: {body}"
            )));
        }

        let parsed: GoogleResponse = resp
            .json()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        Ok(segment_from_response(parsed, &locale))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> TranscriptSegment {
        segment_from_response(serde_json::from_str(json).expect("valid json"), "en-US")
    }

    #[test]
    fn consecutive_results_are_joined_into_one_transcript() {
        let seg = parse(
            r#"{"results":[
                {"alternatives":[{"transcript":"hello there","confidence":0.9}]},
                {"alternatives":[{"transcript":"how are you","confidence":0.7}]}
            ]}"#,
        );
        assert_eq!(seg.text, "hello there how are you");
        // Averaged, not compared exactly: (0.9 + 0.7) / 2 is 0.79999995 in f32.
        assert!((seg.confidence.unwrap() - 0.8).abs() < 1e-6, "{:?}", seg.confidence);
    }

    #[test]
    fn only_the_best_alternative_is_used() {
        // Alternatives are competing guesses at the *same* audio. Taking both
        // would type the sentence twice.
        let seg = parse(
            r#"{"results":[{"alternatives":[
                {"transcript":"recognise speech"},
                {"transcript":"wreck a nice beach"}
            ]}]}"#,
        );
        assert_eq!(seg.text, "recognise speech");
    }

    #[test]
    fn silence_returns_empty_rather_than_erroring() {
        // Google omits `results` entirely for silence.
        assert_eq!(parse(r#"{}"#).text, "");
        assert_eq!(parse(r#"{"results":[]}"#).text, "");
    }

    #[test]
    fn samples_become_little_endian_pairs_and_clip_at_full_scale() {
        let bytes = pcm_le_bytes(&[0.0, 1.0, -1.0, 2.0]);
        assert_eq!(bytes.len(), 8, "two bytes per sample");
        assert_eq!(&bytes[0..2], &0i16.to_le_bytes());
        assert_eq!(&bytes[2..4], &32767i16.to_le_bytes());
        assert_eq!(&bytes[4..6], &(-32767i16).to_le_bytes());
        // Past full scale must clip, not wrap to a large negative.
        assert_eq!(&bytes[6..8], &32767i16.to_le_bytes());
    }

    #[tokio::test]
    async fn audio_past_the_sixty_second_cap_is_refused_before_it_is_uploaded() {
        // Google would otherwise truncate and return a confident partial
        // sentence, which reads as a complete one.
        let p = GoogleSttProvider::new("https://speech.googleapis.com/v1", "latest_short", "k".into());
        let too_long = vec![0.0f32; (MAX_SECONDS + 1) * SAMPLE_RATE];
        let err = p.transcribe(too_long, Some("en")).await;
        assert!(err.is_err());
        assert!(format!("{}", err.err().unwrap()).contains("60 seconds"));
    }

    #[test]
    fn the_endpoint_is_built_from_the_configured_base() {
        let p = GoogleSttProvider::new("https://speech.googleapis.com/v1/", "default", "k".into());
        assert_eq!(p.endpoint, "https://speech.googleapis.com/v1/speech:recognize");
    }
}
