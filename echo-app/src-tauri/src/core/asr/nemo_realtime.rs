//! The NeMo-Speech realtime socket: words on screen while you are still
//! talking.
//!
//! whisper cannot do this. Its encoder runs over a fixed 30-second window, so
//! "live whisper" is the batch model re-run on overlapping buffers — which is
//! what [`super::local`] does, and it costs a full re-decode per partial. A
//! transducer consumes audio as it arrives and emits tokens as it goes, so a
//! partial here is the model's actual running output rather than a guess Echo
//! paid for twice.
//!
//! The protocol: connect, optionally send one `session.update`, then push
//! little-endian PCM16 frames; `input_audio_buffer.commit` closes an utterance.
//! The server answers with `…transcription.delta` while it is unsure and
//! `…transcription.completed` when it is not.

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::TranscriptSegment;
use crate::error::{EchoError, Result};

/// Sample rate of everything in the pipeline; the capture layer resamples to it.
const SAMPLE_RATE: u32 = 16_000;

/// Convert the pipeline's f32 samples to the little-endian PCM16 the socket
/// expects.
fn pcm16(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        let clamped = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&clamped.to_le_bytes());
    }
    out
}

/// The text carried by a server event, whichever field it arrived in.
///
/// Deliberately permissive: the event names are documented but the payload key
/// is not, and a partial that is silently dropped because it came as `text`
/// rather than `delta` would look exactly like an engine that does not stream.
fn event_text(event: &serde_json::Value) -> Option<String> {
    ["delta", "transcript", "text"]
        .iter()
        .find_map(|key| event.get(*key).and_then(|v| v.as_str()))
        .map(str::to_string)
        .filter(|t| !t.is_empty())
}

/// What one server event means for the pipeline.
enum Incoming {
    Partial(String),
    Final(String),
    Failed(String),
    Ignored,
}

fn classify(raw: &str) -> Incoming {
    let Ok(event) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Incoming::Ignored;
    };
    let kind = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if kind == "error" {
        let message = event
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .or_else(|| event.get("message").and_then(|m| m.as_str()))
            .unwrap_or("the realtime socket reported an error");
        return Incoming::Failed(message.to_string());
    }
    match (kind.ends_with(".completed"), kind.ends_with(".delta")) {
        (true, _) => event_text(&event).map_or(Incoming::Ignored, Incoming::Final),
        (_, true) => event_text(&event).map_or(Incoming::Ignored, Incoming::Partial),
        _ => Incoming::Ignored,
    }
}

/// Stream one dictation session over the realtime socket.
///
/// Returns when the audio channel closes. Each empty chunk from the VAD stage
/// is an utterance boundary and becomes a `commit`.
pub(crate) async fn stream(
    port: u16,
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    tx: mpsc::Sender<TranscriptSegment>,
    language: Option<&str>,
    boost: &[String],
) -> Result<()> {
    let url = format!("ws://127.0.0.1:{port}/v1/realtime");
    let (socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| EchoError::AsrProvider(format!("nemo-speech realtime connect failed: {e}")))?;
    let (mut writer, mut reader) = socket.split();

    let mut session = serde_json::json!({
        "sample_rate": SAMPLE_RATE,
        "automatic_punctuation": true,
    });
    if let Some(language) = language {
        session["language"] = serde_json::json!(language);
    }
    if !boost.is_empty() {
        session["speech_contexts"] = serde_json::json!([{ "phrases": boost, "boost": 3.0 }]);
    }
    let update = serde_json::json!({ "type": "session.update", "session": session });
    writer
        .send(Message::Text(update.to_string()))
        .await
        .map_err(|e| EchoError::AsrProvider(format!("nemo-speech realtime setup failed: {e}")))?;

    // The last final text sent on, so a `completed` that merely repeats the
    // partial does not deliver the sentence twice.
    let mut failure: Option<String> = None;

    loop {
        tokio::select! {
            // Audio first: the socket is the thing waiting on us, not the
            // other way round.
            biased;

            chunk = audio_rx.recv() => match chunk {
                // Capture ended. Commit what is buffered and let the reader
                // drain the last final below.
                None => {
                    let _ = writer
                        .send(Message::Text(
                            serde_json::json!({ "type": "input_audio_buffer.commit" }).to_string(),
                        ))
                        .await;
                    break;
                }
                Some(chunk) if chunk.is_empty() => {
                    writer
                        .send(Message::Text(
                            serde_json::json!({ "type": "input_audio_buffer.commit" }).to_string(),
                        ))
                        .await
                        .map_err(|e| {
                            EchoError::AsrProvider(format!("nemo-speech realtime commit: {e}"))
                        })?;
                }
                Some(chunk) => {
                    writer
                        .send(Message::Binary(pcm16(&chunk)))
                        .await
                        .map_err(|e| {
                            EchoError::AsrProvider(format!("nemo-speech realtime send: {e}"))
                        })?;
                }
            },

            event = reader.next() => match event {
                None => break,
                Some(Err(e)) => {
                    failure = Some(format!("nemo-speech realtime socket failed: {e}"));
                    break;
                }
                Some(Ok(Message::Text(raw))) => {
                    match classify(&raw) {
                        Incoming::Partial(text) => {
                            let _ = tx.send(TranscriptSegment {
                                text,
                                is_final: false,
                                language: None,
                                confidence: None,
                            }).await;
                        }
                        Incoming::Final(text) => {
                            let _ = tx.send(TranscriptSegment {
                                text,
                                is_final: true,
                                language: language.map(str::to_string),
                                confidence: None,
                            }).await;
                        }
                        Incoming::Failed(message) => {
                            failure = Some(message);
                            break;
                        }
                        Incoming::Ignored => {}
                    }
                }
                Some(Ok(_)) => {}
            },
        }
    }

    // Whatever the server still owes us after the last commit. Bounded, so a
    // server that never answers cannot hold the session open.
    let drain = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(Ok(Message::Text(raw))) = reader.next().await {
            match classify(&raw) {
                Incoming::Final(text) => {
                    let _ = tx
                        .send(TranscriptSegment {
                            text,
                            is_final: true,
                            language: language.map(str::to_string),
                            confidence: None,
                        })
                        .await;
                    break;
                }
                Incoming::Failed(message) => return Err(message),
                _ => {}
            }
        }
        Ok(())
    })
    .await;

    let _ = writer.send(Message::Close(None)).await;

    match (failure, drain) {
        (Some(message), _) | (None, Ok(Err(message))) => Err(EchoError::AsrProvider(message)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_become_little_endian_pcm16() {
        assert_eq!(pcm16(&[0.0]), vec![0, 0]);
        assert_eq!(pcm16(&[1.0]), i16::MAX.to_le_bytes().to_vec());
        assert_eq!(pcm16(&[-1.0]), (-i16::MAX).to_le_bytes().to_vec());
        // Past full scale is clamped rather than wrapped — wrapping would turn
        // a loud syllable into a burst of noise.
        assert_eq!(pcm16(&[2.0]), i16::MAX.to_le_bytes().to_vec());
    }

    #[test]
    fn partials_finals_and_errors_are_told_apart() {
        let partial =
            r#"{"type":"conversation.item.input_audio_transcription.delta","delta":"hello"}"#;
        assert!(matches!(classify(partial), Incoming::Partial(t) if t == "hello"));

        let final_event = r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":"hello there"}"#;
        assert!(matches!(classify(final_event), Incoming::Final(t) if t == "hello there"));

        let failed = r#"{"type":"error","error":{"message":"model not loaded"}}"#;
        assert!(matches!(classify(failed), Incoming::Failed(m) if m == "model not loaded"));

        // Housekeeping events carry no text and must not be delivered as one.
        assert!(matches!(
            classify(r#"{"type":"session.created"}"#),
            Incoming::Ignored
        ));
        assert!(matches!(
            classify(r#"{"type":"input_audio_buffer.committed"}"#),
            Incoming::Ignored
        ));
    }

    /// The payload key is undocumented, so every plausible one is accepted.
    #[test]
    fn text_is_read_from_whichever_field_carries_it() {
        for key in ["delta", "transcript", "text"] {
            let raw = format!(
                r#"{{"type":"conversation.item.input_audio_transcription.delta","{key}":"hi"}}"#
            );
            assert!(
                matches!(classify(&raw), Incoming::Partial(t) if t == "hi"),
                "{key} was not read"
            );
        }
        // An empty string is not a partial worth showing.
        let empty = r#"{"type":"conversation.item.input_audio_transcription.delta","delta":""}"#;
        assert!(matches!(classify(empty), Incoming::Ignored));
    }
}
