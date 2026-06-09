use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::mpsc;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
};

use super::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

/// Offline transcription provider backed by whisper.cpp (via whisper-rs).
///
/// Compiled only when the `whisper` Cargo feature is enabled, because building
/// whisper.cpp requires cmake + libclang on the build machine.
pub struct WhisperProvider {
    ctx: Arc<Mutex<WhisperContext>>,
    model_name: String,
}

impl WhisperProvider {
    pub fn load(model_path: &Path, model_name: &str) -> Result<Self> {
        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .ok_or_else(|| EchoError::Config("Model path is not valid UTF-8".into()))?,
            WhisperContextParameters::default(),
        )
        .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
        Ok(Self {
            ctx: Arc::new(Mutex::new(ctx)),
            model_name: model_name.into(),
        })
    }
}

#[async_trait]
impl AsrProvider for WhisperProvider {
    fn name(&self) -> &str {
        &self.model_name
    }

    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        let ctx = self.ctx.clone();
        let lang = language.map(str::to_string);

        // whisper inference is CPU-bound: run it on the blocking pool.
        tokio::task::spawn_blocking(move || {
            let ctx = ctx.lock().unwrap();
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            if let Some(l) = &lang {
                params.set_language(Some(l));
            }
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_special(false);

            let mut state = ctx
                .create_state()
                .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
            state
                .full(params, &audio)
                .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

            let n = state
                .full_n_segments()
                .map_err(|e| EchoError::AsrProvider(e.to_string()))?;
            let text: String = (0..n)
                .filter_map(|i| state.full_get_segment_text(i).ok())
                .collect::<Vec<_>>()
                .join(" ");

            Ok(TranscriptSegment {
                text: text.trim().to_string(),
                is_final: true,
                language: None,
                confidence: None,
            })
        })
        .await
        .map_err(|e| EchoError::AsrProvider(e.to_string()))?
    }

    async fn transcribe_stream(
        &self,
        mut audio_rx: mpsc::Receiver<Vec<f32>>,
        tx: mpsc::Sender<TranscriptSegment>,
        language: Option<&str>,
    ) -> Result<()> {
        // The VAD stage upstream sends an empty chunk to mark end-of-utterance.
        // Accumulate speech until that sentinel, transcribe, then continue for
        // the next utterance until the channel closes.
        let mut buffer: Vec<f32> = Vec::new();
        while let Some(chunk) = audio_rx.recv().await {
            if chunk.is_empty() {
                if !buffer.is_empty() {
                    let segment = self.transcribe(std::mem::take(&mut buffer), language).await?;
                    if !segment.text.is_empty() {
                        let _ = tx.send(segment).await;
                    }
                }
                continue;
            }
            buffer.extend_from_slice(&chunk);
        }

        // Flush any trailing audio when the stream ends.
        if !buffer.is_empty() {
            let segment = self.transcribe(buffer, language).await?;
            if !segment.text.is_empty() {
                let _ = tx.send(segment).await;
            }
        }
        Ok(())
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}
