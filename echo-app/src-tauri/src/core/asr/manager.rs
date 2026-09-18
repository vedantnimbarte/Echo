use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use super::fallback::FallbackNotify;
use super::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

pub struct AsrManager {
    providers: RwLock<HashMap<String, Arc<dyn AsrProvider>>>,
    active_provider: RwLock<String>,
    /// Set once at startup by whoever can reach a window. `None` in tests and
    /// in the CLI, where there is no screen to tell.
    on_fallback: RwLock<Option<FallbackNotify>>,
}

impl AsrManager {
    pub fn new(default_provider: String) -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            active_provider: RwLock::new(default_provider),
            on_fallback: RwLock::new(None),
        }
    }

    /// Register the sink told whenever an utterance is diverted to the offline
    /// engine.
    pub async fn set_fallback_notify(&self, notify: FallbackNotify) {
        *self.on_fallback.write().await = Some(notify);
    }

    pub async fn register(&self, provider: Arc<dyn AsrProvider>) {
        let name = provider.name().to_string();
        self.providers.write().await.insert(name, provider);
    }

    /// Drop every provider whose name starts with `prefix`.
    ///
    /// Exists for plugin engines, which must stop being selectable when their
    /// plugin is disabled. If one of them is active, the active name is left
    /// alone — the next recording then reports it missing rather than quietly
    /// transcribing with something the user did not pick.
    pub async fn unregister_prefixed(&self, prefix: &str) {
        self.providers
            .write()
            .await
            .retain(|name, _| !name.starts_with(prefix));
    }

    pub async fn set_active(&self, name: &str) -> Result<()> {
        let providers = self.providers.read().await;
        if !providers.contains_key(name) {
            return Err(EchoError::NotFound(format!(
                "ASR provider '{name}' not registered"
            )));
        }
        drop(providers);
        *self.active_provider.write().await = name.to_string();
        Ok(())
    }

    pub async fn active_provider_name(&self) -> String {
        self.active_provider.read().await.clone()
    }

    pub async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        self.active().await?.transcribe(audio, language).await
    }

    /// Transcribe with a *named* provider rather than the active one.
    ///
    /// Deliberately unwrapped by [`super::fallback::FallbackProvider`]: this
    /// exists so a retry can be decoded by something other than whatever just
    /// got it wrong, and silently falling back to the local engine would hand
    /// back the same answer again.
    pub async fn transcribe_with(
        &self,
        name: &str,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        let provider = self
            .providers
            .read()
            .await
            .get(name)
            .cloned()
            .ok_or_else(|| EchoError::NotFound(format!("ASR provider '{name}' not registered")))?;
        provider.transcribe(audio, language).await
    }

    /// Names of every registered provider, for a settings picker.
    pub async fn registered(&self) -> Vec<String> {
        let mut names: Vec<String> = self.providers.read().await.keys().cloned().collect();
        names.sort();
        names
    }

    /// Whether the active provider produces partial results as you speak.
    ///
    /// Gates partial injection: under the buffered default there is one segment
    /// per utterance and nothing to stream, so streaming would add its risk
    /// without its benefit.
    pub async fn supports_streaming(&self) -> bool {
        let name = self.active_provider.read().await.clone();
        self.providers
            .read()
            .await
            .get(&name)
            .map(|p| p.supports_streaming())
            .unwrap_or(false)
    }

    /// Pass the partials-wanted hint to the active provider before a session.
    pub async fn set_partials_wanted(&self, wanted: bool) {
        let name = self.active_provider.read().await.clone();
        if let Some(provider) = self.providers.read().await.get(&name) {
            provider.set_partials_wanted(wanted);
        }
    }

    pub async fn transcribe_stream(
        &self,
        audio_rx: mpsc::Receiver<Vec<f32>>,
        tx: mpsc::Sender<TranscriptSegment>,
        language: Option<&str>,
    ) -> Result<()> {
        self.active()
            .await?
            .transcribe_stream(audio_rx, tx, language)
            .await
    }

    /// Warm the active provider, if it is registered and wants warming.
    ///
    /// Silent about a provider that is not registered yet: during onboarding
    /// that is the normal state, not a fault worth reporting.
    pub async fn preload_active(&self) {
        let Ok(provider) = self.active().await else {
            return;
        };
        if let Err(e) = provider.preload().await {
            tracing::debug!("Could not warm {}: {e}", provider.name());
        }
    }

    /// The provider to transcribe with, already wrapped in its fallback.
    ///
    /// Resolved per call rather than cached at registration, because the point
    /// of the fallback is to cover failures that appear later — the offline
    /// engine may have finished downloading since the last utterance.
    async fn active(&self) -> Result<Arc<dyn AsrProvider>> {
        let name = self.active_provider.read().await.clone();
        let providers = self.providers.read().await;
        let provider = providers
            .get(&name)
            .ok_or_else(|| EchoError::NotFound(format!("Active ASR provider '{name}' not found")))?
            .clone();
        let local = providers.get("local").cloned();
        drop(providers);

        let notify = self.on_fallback.read().await.clone();
        Ok(super::fallback::FallbackProvider::wrap(
            provider, local, notify,
        ))
    }
}
