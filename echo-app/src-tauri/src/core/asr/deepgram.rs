use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use super::wav::pcm_f32_to_wav;
use super::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

/// Deepgram speech-to-text. `transcribe` uses the pre-recorded `/v1/listen`
/// HTTP API (one request per utterance); `transcribe_stream` upgrades to the
/// real-time WebSocket API for true word-by-word interim results.
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

/// A `Results` message from the Deepgram streaming WebSocket API.
#[derive(Debug, Deserialize)]
struct DgStreamResult {
    #[serde(default)]
    channel: Option<DgChannel>,
    #[serde(default)]
    is_final: bool,
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

    /// True streaming over Deepgram's real-time WebSocket. Raw 16 kHz PCM is
    /// streamed up as it arrives; interim hypotheses come back as non-final
    /// segments and endpointed results as final segments. The upstream VAD's
    /// empty-vec sentinels are ignored here — Deepgram does its own endpointing.
    async fn transcribe_stream(
        &self,
        mut audio_rx: mpsc::Receiver<Vec<f32>>,
        tx: mpsc::Sender<TranscriptSegment>,
        language: Option<&str>,
    ) -> Result<()> {
        let mut url = "wss://api.deepgram.com/v1/listen?model=nova-2&smart_format=true\
            &interim_results=true&encoding=linear16&sample_rate=16000&channels=1"
            .to_string();
        if let Some(lang) = language {
            url.push_str(&format!("&language={lang}"));
        }

        let mut request = url
            .into_client_request()
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
        request.headers_mut().insert(
            "Authorization",
            format!("Token {}", self.api_key)
                .parse()
                .map_err(|_| EchoError::AsrProvider("invalid Deepgram key header".into()))?,
        );

        let (ws, _resp) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| EchoError::AsrProvider(format!("deepgram ws connect: {e}")))?;
        let (mut write, mut read) = ws.split();

        // Writer: forward PCM (f32 → little-endian i16) until capture stops, then
        // close the stream so Deepgram flushes any trailing transcript.
        let writer = tokio::spawn(async move {
            while let Some(chunk) = audio_rx.recv().await {
                if chunk.is_empty() {
                    continue; // VAD utterance boundary — Deepgram endpoints itself
                }
                let mut pcm = Vec::with_capacity(chunk.len() * 2);
                for s in chunk {
                    let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                    pcm.extend_from_slice(&v.to_le_bytes());
                }
                if write.send(Message::Binary(pcm)).await.is_err() {
                    break;
                }
            }
            let _ = write.send(Message::Close(None)).await;
        });

        // Reader: surface interim (non-final) and final hypotheses.
        while let Some(msg) = read.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => return Err(EchoError::AsrProvider(format!("deepgram ws: {e}"))),
            };
            let text = match msg {
                Message::Text(t) => t,
                Message::Close(_) => break,
                _ => continue,
            };
            let Ok(result) = serde_json::from_str::<DgStreamResult>(text.as_str()) else {
                continue;
            };
            let Some(channel) = result.channel else { continue };
            let Some(alt) = channel.alternatives.into_iter().next() else {
                continue;
            };
            let transcript = alt.transcript.trim().to_string();
            if transcript.is_empty() {
                continue;
            }
            let _ = tx
                .send(TranscriptSegment {
                    text: transcript,
                    is_final: result.is_final,
                    language: channel.detected_language,
                    confidence: alt.confidence,
                })
                .await;
        }

        let _ = writer.await;
        Ok(())
    }

    fn supports_streaming(&self) -> bool {
        true
    }
}
