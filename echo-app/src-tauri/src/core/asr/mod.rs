#[allow(unused_imports)]
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

pub mod manager;
pub mod model_manager;
pub mod wav;

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
        mut audio_rx: mpsc::Receiver<Vec<f32>>,
        tx: mpsc::Sender<TranscriptSegment>,
        language: Option<&str>,
    ) -> crate::error::Result<()> {
        let mut buffer: Vec<f32> = Vec::new();
        while let Some(chunk) = audio_rx.recv().await {
            if chunk.is_empty() {
                if !buffer.is_empty() {
                    let seg = self.transcribe(std::mem::take(&mut buffer), language).await?;
                    if !seg.text.is_empty() {
                        let _ = tx.send(seg).await;
                    }
                }
                continue;
            }
            buffer.extend_from_slice(&chunk);
        }
        if !buffer.is_empty() {
            let seg = self.transcribe(buffer, language).await?;
            if !seg.text.is_empty() {
                let _ = tx.send(seg).await;
            }
        }
        Ok(())
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}
