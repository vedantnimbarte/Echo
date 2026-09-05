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

/// Query string for the pre-recorded API.
///
/// With no language Deepgram is asked to detect one; naming it is both more
/// accurate and cheaper, so the two cases must not be confused.
fn listen_url(language: Option<&str>) -> String {
    let mut url = "https://api.deepgram.com/v1/listen?model=nova-2&smart_format=true".to_string();
    match language {
        Some(lang) => url.push_str(&format!("&language={lang}")),
        None => url.push_str("&detect_language=true"),
    }
    url
}

/// Query string for the realtime WebSocket API.
///
/// The encoding parameters must match what the writer actually sends: 16-bit
/// little-endian PCM, 16 kHz, mono. A mismatch is not rejected — Deepgram
/// decodes the bytes as whatever it was told and returns confident nonsense.
fn stream_url(language: Option<&str>) -> String {
    let mut url = "wss://api.deepgram.com/v1/listen?model=nova-2&smart_format=true        &interim_results=true&encoding=linear16&sample_rate=16000&channels=1"
        .to_string();
    if let Some(lang) = language {
        url.push_str(&format!("&language={lang}"));
    }
    url
}

/// Convert a chunk of f32 samples to the little-endian 16-bit PCM the socket
/// expects.
///
/// The clamp is load-bearing. A sample above 1.0 — which resampling and gain
/// can produce — multiplied out and cast without clamping wraps to a large
/// negative value, so the loudest moment of a sentence returns as a crackle
/// rather than as clipping.
fn pcm_le_bytes(chunk: &[f32]) -> Vec<u8> {
    let mut pcm = Vec::with_capacity(chunk.len() * 2);
    for s in chunk {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
    }
    pcm
}

/// Pull the transcript out of a pre-recorded response.
fn segment_from_response(parsed: DeepgramResponse) -> Result<TranscriptSegment> {
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

/// Turn one streaming message into a segment, or `None` when there is nothing
/// worth emitting.
///
/// Deepgram sends keep-alives, metadata and empty interim hypotheses on the
/// same socket. Forwarding an empty transcript would blank the live preview
/// mid-sentence, so those are dropped rather than passed on.
fn segment_from_stream(result: DgStreamResult) -> Option<TranscriptSegment> {
    let channel = result.channel?;
    let language = channel.detected_language;
    let alt = channel.alternatives.into_iter().next()?;
    let transcript = alt.transcript.trim().to_string();
    if transcript.is_empty() {
        return None;
    }
    Some(TranscriptSegment {
        text: transcript,
        is_final: result.is_final,
        language,
        confidence: alt.confidence,
    })
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

        let url = listen_url(language);
        crate::core::egress::record(&url, "cloud transcription");

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

        segment_from_response(parsed)
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
        let url = stream_url(language);
        let mut request = url
            .into_client_request()
            .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
        request.headers_mut().insert(
            "Authorization",
            format!("Token {}", self.api_key)
                .parse()
                .map_err(|_| EchoError::AsrProvider("invalid Deepgram key header".into()))?,
        );

        crate::core::egress::record_host("api.deepgram.com", "streaming transcription");

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
                if write.send(Message::Binary(pcm_le_bytes(&chunk))).await.is_err() {
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
            let Some(segment) = segment_from_stream(result) else {
                continue;
            };
            let _ = tx.send(segment).await;
        }

        let _ = writer.await;
        Ok(())
    }

    fn supports_streaming(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Result<TranscriptSegment> {
        segment_from_response(serde_json::from_str(json).expect("valid json"))
    }

    #[test]
    fn a_named_language_is_requested_and_detection_is_not() {
        let named = listen_url(Some("fr"));
        assert!(named.contains("&language=fr"), "{named}");
        assert!(!named.contains("detect_language"), "{named}");

        let auto = listen_url(None);
        assert!(auto.contains("&detect_language=true"), "{auto}");
        assert!(!auto.contains("&language="), "{auto}");
    }

    /// These must agree with what `pcm_le_bytes` produces. Deepgram does not
    /// validate them — it decodes the bytes as whatever it was told, so a
    /// mismatch returns fluent nonsense rather than an error.
    #[test]
    fn the_stream_url_describes_the_audio_actually_sent() {
        let url = stream_url(None);
        assert!(url.starts_with("wss://"), "{url}");
        assert!(url.contains("encoding=linear16"), "{url}");
        assert!(url.contains("sample_rate=16000"), "{url}");
        assert!(url.contains("channels=1"), "{url}");
        assert!(url.contains("interim_results=true"), "{url}");
        assert!(stream_url(Some("de")).contains("&language=de"));
    }

    #[test]
    fn samples_become_little_endian_pairs() {
        let bytes = pcm_le_bytes(&[0.0, 1.0, -1.0]);
        assert_eq!(bytes.len(), 6, "two bytes per sample");
        assert_eq!(&bytes[0..2], &0i16.to_le_bytes());
        assert_eq!(&bytes[2..4], &32767i16.to_le_bytes());
        assert_eq!(&bytes[4..6], &(-32767i16).to_le_bytes());
    }

    /// Without the clamp, 1.5 * 32767 overflows the cast and wraps to a large
    /// negative sample — the loudest moment of a sentence returned as a
    /// crackle. Resampling and gain really do produce samples past 1.0.
    #[test]
    fn samples_beyond_full_scale_clip_rather_than_wrapping() {
        for (input, expected) in [(1.5f32, 32767i16), (-1.5, -32767), (9.0, 32767)] {
            let bytes = pcm_le_bytes(&[input]);
            let got = i16::from_le_bytes([bytes[0], bytes[1]]);
            assert_eq!(got, expected, "{input} should clip, got {got}");
        }
    }

    #[test]
    fn a_transcript_is_read_out_of_a_response() {
        let seg = parse(
            r#"{"results":{"channels":[{"detected_language":"en",
               "alternatives":[{"transcript":"  hello there  ","confidence":0.97}]}]}}"#,
        )
        .unwrap();
        assert_eq!(seg.text, "hello there", "surrounding space must be trimmed");
        assert_eq!(seg.language.as_deref(), Some("en"));
        assert_eq!(seg.confidence, Some(0.97));
        assert!(seg.is_final, "the pre-recorded API only returns finals");
    }

    /// Optional fields really are optional; Deepgram omits them when language
    /// detection is off.
    #[test]
    fn a_response_without_language_or_confidence_still_parses() {
        let seg = parse(r#"{"results":{"channels":[{"alternatives":[{"transcript":"hi"}]}]}}"#)
            .unwrap();
        assert_eq!(seg.text, "hi");
        assert_eq!(seg.language, None);
        assert_eq!(seg.confidence, None);
    }

    /// An empty response must be an error rather than an empty transcript,
    /// which would look to the pipeline like the user said nothing.
    #[test]
    fn an_empty_response_is_reported_rather_than_returned_as_silence() {
        assert!(parse(r#"{"results":{"channels":[]}}"#).is_err());
        assert!(parse(r#"{"results":{"channels":[{"alternatives":[]}]}}"#).is_err());
    }

    #[test]
    fn streaming_marks_interim_and_final_hypotheses() {
        let interim: DgStreamResult = serde_json::from_str(
            r#"{"is_final":false,"channel":{"alternatives":[{"transcript":"hel"}]}}"#,
        )
        .unwrap();
        let seg = segment_from_stream(interim).unwrap();
        assert_eq!(seg.text, "hel");
        assert!(!seg.is_final);

        let final_msg: DgStreamResult = serde_json::from_str(
            r#"{"is_final":true,"channel":{"alternatives":[{"transcript":"hello"}]}}"#,
        )
        .unwrap();
        assert!(segment_from_stream(final_msg).unwrap().is_final);
    }

    /// Keep-alives, metadata and empty interim hypotheses share the socket.
    /// Emitting an empty transcript would blank the live preview mid-sentence.
    #[test]
    fn streaming_ignores_messages_with_nothing_to_say() {
        for json in [
            r#"{"type":"Metadata"}"#,
            r#"{"is_final":false,"channel":{"alternatives":[]}}"#,
            r#"{"is_final":false,"channel":{"alternatives":[{"transcript":"   "}]}}"#,
        ] {
            let msg: DgStreamResult = serde_json::from_str(json).unwrap();
            assert!(segment_from_stream(msg).is_none(), "should ignore {json}");
        }
    }
}
