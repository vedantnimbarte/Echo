use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use super::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

pub struct AsrManager {
    providers: RwLock<HashMap<String, Arc<dyn AsrProvider>>>,
    active_provider: RwLock<String>,
}

impl AsrManager {
    pub fn new(default_provider: String) -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            active_provider: RwLock::new(default_provider),
        }
    }

    pub async fn register(&self, provider: Arc<dyn AsrProvider>) {
        let name = provider.name().to_string();
        self.providers.write().await.insert(name, provider);
    }

    pub async fn set_active(&self, name: &str) -> Result<()> {
        let providers = self.providers.read().await;
        if !providers.contains_key(name) {
            return Err(EchoError::NotFound(format!("ASR provider '{name}' not registered")));
        }
        drop(providers);
        *self.active_provider.write().await = name.to_string();
        Ok(())
    }

    pub async fn active_provider_name(&self) -> String {
        self.active_provider.read().await.clone()
    }

    pub async fn transcribe(&self, audio: Vec<f32>, language: Option<&str>) -> Result<TranscriptSegment> {
        let name = self.active_provider.read().await.clone();
        let providers = self.providers.read().await;
        let provider = providers
            .get(&name)
            .ok_or_else(|| EchoError::NotFound(format!("Active ASR provider '{name}' not found")))?;
        provider.transcribe(audio, language).await
    }

    pub async fn transcribe_stream(
        &self,
        audio_rx: mpsc::Receiver<Vec<f32>>,
        tx: mpsc::Sender<TranscriptSegment>,
        language: Option<&str>,
    ) -> Result<()> {
        let name = self.active_provider.read().await.clone();
        let providers = self.providers.read().await;
        let provider = providers
            .get(&name)
            .ok_or_else(|| EchoError::NotFound(format!("Active ASR provider '{name}' not found")))?
            .clone();
        drop(providers);
        provider.transcribe_stream(audio_rx, tx, language).await
    }
}
