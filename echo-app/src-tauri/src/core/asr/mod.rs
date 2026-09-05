#[allow(unused_imports)]
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

pub mod binary_manager;
pub mod decode_opts;
pub mod fallback;
pub mod local;
pub mod manager;
pub mod model_manager;
pub mod wav;
pub mod whisper_cli;
pub mod whisper_server;

pub mod deepgram;
pub mod openai;

#[cfg(feature = "whisper")]
pub mod whisper;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub text: String,
    pub is_final: bool,
    pub language: Option<String>,
    pub confidence: Option<f32>,
}

/// Trait all ASR providers must implement.
#[async_trait]
pub trait AsrProvider: Send + Sync {
    fn name(&self) -> &str;

    /// Transcribe a complete PCM audio buffer (f32, 16kHz mono).
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> crate::error::Result<TranscriptSegment>;

    /// Streaming transcription — sends partial and final segments via channel.
    ///
    /// The default implementation accumulates speech and transcribes one
    /// utterance at a time: the upstream VAD sends an empty chunk to mark the
    /// end of each utterance. Providers with true streaming APIs (e.g. via
    /// WebSocket) can override this.
    async fn transcribe_stream(
        &self,
        audio_rx: mpsc::Receiver<Vec<f32>>,
        tx: mpsc::Sender<TranscriptSegment>,
        language: Option<&str>,
    ) -> crate::error::Result<()> {
        default_transcribe_stream(self, audio_rx, tx, language).await
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}

/// The buffered streaming loop: accumulate speech, transcribe one utterance at
/// a time, where the upstream VAD marks each end with an empty chunk.
///
/// A free function rather than only a trait default so a wrapper — see
/// [`fallback::FallbackProvider`] — can delegate to it and still have the
/// per-utterance calls come back through its own `transcribe`.
pub(crate) async fn default_transcribe_stream<P>(
    provider: &P,
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    tx: mpsc::Sender<TranscriptSegment>,
    language: Option<&str>,
) -> crate::error::Result<()>
where
    P: AsrProvider + ?Sized,
{
    let mut buffer: Vec<f32> = Vec::new();
    while let Some(chunk) = audio_rx.recv().await {
        if chunk.is_empty() {
            let utterance = std::mem::take(&mut buffer);
            if let Some(seg) = transcribe_utterance(provider, utterance, language).await? {
                let _ = tx.send(seg).await;
            }
            continue;
        }
        buffer.extend_from_slice(&chunk);
    }
    if let Some(seg) = transcribe_utterance(provider, buffer, language).await? {
        let _ = tx.send(seg).await;
    }
    Ok(())
}

/// Transcribe one buffered utterance, or `None` if it should not be sent to the
/// decoder at all.
///
/// The speech gate runs here rather than in the VAD stage on purpose. This is
/// the point where a whole utterance already exists in memory, so the check
/// costs one pass over a buffer we are holding anyway — and gating here leaves
/// providers with real streaming APIs (which override `transcribe_stream` and
/// do their own endpointing) completely untouched.
async fn transcribe_utterance<P>(
    provider: &P,
    audio: Vec<f32>,
    language: Option<&str>,
) -> crate::error::Result<Option<TranscriptSegment>>
where
    P: AsrProvider + ?Sized,
{
    if audio.is_empty() {
        return Ok(None);
    }

    let decision = crate::core::vad::gate::speech_gate(&audio);
    if !decision.should_transcribe() {
        // Worth a log line: "I spoke and nothing happened" is otherwise
        // indistinguishable from a broken pipeline.
        tracing::debug!(
            reason = decision.reason(),
            samples = audio.len(),
            "Utterance gated out before transcription"
        );
        return Ok(None);
    }

    let seg = provider.transcribe(audio, language).await?;
    Ok((!seg.text.is_empty()).then_some(seg))
}
