use async_trait::async_trait;
use serde::Deserialize;

use super::wav::pcm_f32_to_wav;
use super::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

/// Deepgram speech-to-text via the pre-recorded `/v1/listen` HTTP API.
///
/// Deepgram also offers true streaming over WebSocket; this provider uses the
/// batch endpoint, which fits the VAD-segmented utterance pipeline (one request
/// per utterance). A streaming path can be added later by overriding
/// `transcribe_stream` and setting `supports_streaming` to true.
pub struct DeepgramProvider {
    api_key: String,
    client: reqwest::Client,
}

impl DeepgramProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct DeepgramResponse {
    results: DgResults,
}

#[derive(Debug, Deserialize)]
struct DgResults {
    channels: Vec<DgChannel>,
}

#[derive(Debug, Deserialize)]
struct DgChannel {
    alternatives: Vec<DgAlternative>,
    #[serde(default)]
    detected_language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DgAlternative {
    transcript: String,
    #[serde(default)]
    confidence: Option<f32>,
}

#[async_trait]
impl AsrProvider for DeepgramProvider {
    fn name(&self) -> &str {
        "deepgram"
    }

    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        let wav = pcm_f32_to_wav(&audio, 16_000)?;

        let mut url =
            "https://api.deepgram.com/v1/listen?model=nova-2&smart_format=true".to_string();
        match language {
            Some(lang) => url.push_str(&format!("&language={lang}")),
            None => url.push_str("&detect_language=true"),
        }

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Token {}", self.api_key))
            .header("Content-Type", "audio/wav")
            .body(wav)
            .send()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(EchoError::AsrProvider(format!(
                "deepgram API error {status}: {body}"
            )));
        }

        let parsed: DeepgramResponse = resp
            .json()
            .await
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

        let channel = parsed
            .results
            .channels
            .into_iter()
            .next()
            .ok_or_else(|| EchoError::AsrProvider("deepgram returned no channels".into()))?;
        let language = channel.detected_language;
        let alt = channel
            .alternatives
            .into_iter()
            .next()
            .ok_or_else(|| EchoError::AsrProvider("deepgram returned no alternatives".into()))?;

        Ok(TranscriptSegment {
            text: alt.transcript.trim().to_string(),
            is_final: true,
            language,
            confidence: alt.confidence,
        })
    }
}
