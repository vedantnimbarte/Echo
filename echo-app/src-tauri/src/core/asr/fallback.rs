//! Second-chance transcription when the chosen provider fails.
//!
//! A cloud provider fails for reasons that have nothing to do with the user:
//! the wifi dropped, the API is rate-limiting, a key expired. Losing a
//! dictation to that is avoidable — the offline engine is sitting right there.
//!
//! **Fallback only ever moves toward more privacy.** Local can never fall back
//! to a cloud provider, however it fails. The whole promise of choosing the
//! offline engine is that the audio does not leave the machine, and an error
//! path that quietly uploads it anyway would break that promise at exactly the
//! moment nobody is watching. [`is_local`] is what enforces this, and it is the
//! reason this wrapper decides the direction rather than taking any pair of
//! providers it is handed.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::{AsrProvider, TranscriptSegment};
use crate::error::Result;

/// The only provider audio may be diverted *to*.
const LOCAL_PROVIDER: &str = "local";

/// Whether `name` identifies the on-device engine.
pub fn is_local(name: &str) -> bool {
    name == LOCAL_PROVIDER
}

/// Wraps a provider so a failed utterance is retried on the offline engine.
pub struct FallbackProvider {
    primary: Arc<dyn AsrProvider>,
    local: Arc<dyn AsrProvider>,
}

impl FallbackProvider {
    /// Wrap `primary` with a local fallback, or return it unchanged when a
    /// fallback would be pointless or would leak audio.
    ///
    /// Returns `primary` untouched when it *is* the local engine (there is
    /// nothing safer to fall back to) or when no local engine is registered.
    pub fn wrap(
        primary: Arc<dyn AsrProvider>,
        local: Option<Arc<dyn AsrProvider>>,
    ) -> Arc<dyn AsrProvider> {
        if is_local(primary.name()) {
            return primary;
        }
        match local {
            Some(local) => Arc::new(Self { primary, local }),
            None => primary,
        }
    }
}

#[async_trait]
impl AsrProvider for FallbackProvider {
    /// Reports the primary's name so history and telemetry keep attributing
    /// transcripts to the engine the user actually chose.
    fn name(&self) -> &str {
        self.primary.name()
    }

    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        // The buffer has to be cloned up front: a failed attempt consumes it,
        // and by the time we know we need the retry the original is gone.
        let retry = audio.clone();
        match self.primary.transcribe(audio, language).await {
            Ok(segment) => Ok(segment),
            Err(e) => {
                tracing::warn!(
                    provider = self.primary.name(),
                    "Transcription failed, retrying on the offline engine: {e}"
                );
                self.local.transcribe(retry, language).await
            }
        }
    }

    async fn transcribe_stream(
        &self,
        audio_rx: mpsc::Receiver<Vec<f32>>,
        tx: mpsc::Sender<TranscriptSegment>,
        language: Option<&str>,
    ) -> Result<()> {
        if self.primary.supports_streaming() {
            // A true streaming provider holds the socket and the audio; there
            // is nothing left to retry once it fails mid-utterance, so let it
            // own the stream rather than pretending we can cover for it.
            return self.primary.transcribe_stream(audio_rx, tx, language).await;
        }
        // The default implementation buffers each utterance and calls
        // `transcribe`, which is ours — so every utterance gets the retry.
        super::default_transcribe_stream(self, audio_rx, tx, language).await
    }

    fn supports_streaming(&self) -> bool {
        self.primary.supports_streaming()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Stub {
        name: &'static str,
        fail: bool,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AsrProvider for Stub {
        fn name(&self) -> &str {
            self.name
        }
        async fn transcribe(&self, _: Vec<f32>, _: Option<&str>) -> Result<TranscriptSegment> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(crate::error::EchoError::AsrProvider("offline".into()));
            }
            Ok(TranscriptSegment {
                text: self.name.to_string(),
                is_final: true,
                language: None,
                confidence: None,
            })
        }
    }

    fn stub(name: &'static str, fail: bool) -> (Arc<dyn AsrProvider>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let p = Arc::new(Stub { name, fail, calls: calls.clone() });
        (p, calls)
    }

    #[tokio::test]
    async fn a_failed_cloud_utterance_is_retried_locally() {
        let (cloud, _) = stub("openai", true);
        let (local, local_calls) = stub("local", false);

        let provider = FallbackProvider::wrap(cloud, Some(local));
        let out = provider.transcribe(vec![0.1; 16], None).await.unwrap();

        assert_eq!(out.text, "local");
        assert_eq!(local_calls.load(Ordering::SeqCst), 1);
        // History should still credit the engine the user picked.
        assert_eq!(provider.name(), "openai");
    }

    #[tokio::test]
    async fn a_working_provider_never_reaches_the_fallback() {
        let (cloud, _) = stub("openai", false);
        let (local, local_calls) = stub("local", false);

        let provider = FallbackProvider::wrap(cloud, Some(local));
        assert_eq!(provider.transcribe(vec![0.1; 16], None).await.unwrap().text, "openai");
        assert_eq!(local_calls.load(Ordering::SeqCst), 0);
    }

    /// The privacy rule: a local failure must never send audio to the cloud.
    #[tokio::test]
    async fn local_is_never_wrapped_so_audio_cannot_be_diverted() {
        let (local, _) = stub("local", true);
        let (cloud, cloud_calls) = stub("openai", false);

        let provider = FallbackProvider::wrap(local, Some(cloud));
        assert!(provider.transcribe(vec![0.1; 16], None).await.is_err());
        assert_eq!(
            cloud_calls.load(Ordering::SeqCst),
            0,
            "a local failure must never be retried off-machine"
        );
    }

    #[tokio::test]
    async fn without_a_local_engine_the_primary_is_returned_unchanged() {
        let (cloud, _) = stub("openai", true);
        let provider = FallbackProvider::wrap(cloud, None);
        assert!(provider.transcribe(vec![0.1; 16], None).await.is_err());
    }
}
