#[allow(unused_imports)]
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

pub mod manager;
pub mod model_manager;

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
    async fn transcribe_stream(
        &self,
        audio_rx: mpsc::Receiver<Vec<f32>>,
        tx: mpsc::Sender<TranscriptSegment>,
        language: Option<&str>,
    ) -> crate::error::Result<()>;

    fn supports_streaming(&self) -> bool {
        false
    }
}
