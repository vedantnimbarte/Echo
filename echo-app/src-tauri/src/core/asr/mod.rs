#[allow(unused_imports)]
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

pub mod binary_manager;
pub mod catalog;
pub mod decode_opts;
pub mod fallback;
pub mod http;
pub mod languages;
pub mod local;
pub mod manager;
pub mod model_manager;
pub mod prompt;

#[cfg(test)]
mod pack_tests;
pub mod wav;
pub mod whisper_cli;
pub mod whisper_server;

pub mod assemblyai;
pub mod azure;
pub mod deepgram;
pub mod elevenlabs;
pub mod google;
pub mod locale;
pub mod openai;
pub mod speechmatics;

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

    /// Tell the provider whether partial results will actually be used before
    /// a streaming session starts.
    ///
    /// Producing partials from a local model means re-decoding the utterance
    /// as it grows, which costs real CPU or GPU. A provider that streams for
    /// free (a cloud WebSocket) can ignore this; one that pays for it should
    /// not pay when nobody is reading.
    ///
    /// Additive with a default so an existing plugin keeps compiling: the
    /// contract is "an unimplemented provider behaves as it always did".
    fn set_partials_wanted(&self, _wanted: bool) {}

    /// Transcribe a whole recording and say who spoke each part.
    ///
    /// Takes the file's own bytes and MIME type rather than 16 kHz PCM: every
    /// provider that diarizes also decodes mp3, ogg and flac itself, so there
    /// is nothing gained by decoding a twenty-minute file locally only to
    /// upload a wav several times its size.
    ///
    /// Returns turns in spoken order as `(speaker, text)`. The speaker is the
    /// provider's own id — `0`, `"A"`, `"S1"`, `"speaker_0"` — and naming them
    /// "Speaker 1, 2, …" is left to [`crate::commands::import`], so the ids
    /// never have to agree with each other. Consecutive turns may share a
    /// speaker; merging them is the caller's job too.
    ///
    /// Only for importing a file. Live dictation never asks for this: typing
    /// "Speaker 1:" into someone's email mid-sentence helps nobody.
    ///
    /// Additive with a default for the same reason as `set_partials_wanted`:
    /// a provider that cannot do it keeps compiling and says so plainly.
    async fn transcribe_speakers(
        &self,
        _audio: Vec<u8>,
        _mime: &str,
        _language: Option<&str>,
    ) -> crate::error::Result<Vec<(String, String)>> {
        Err(crate::error::EchoError::Config(format!(
            "{} does not label speakers.",
            self.name()
        )))
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
