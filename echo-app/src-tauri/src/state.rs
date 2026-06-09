use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

use crate::core::{
    asr::manager::AsrManager,
    asr::model_manager::ModelManager,
    audio::AudioService,
    dictionary::DictionaryEngine,
    injection::TextInjector,
};

/// Shared application state — stored in Tauri's managed state.
///
/// Note: the VAD is intentionally not stored here. It is created fresh inside
/// the audio-capture task per recording session, keeping latency stages
/// separate (architectural rule 8).
pub struct AppState {
    pub db: Mutex<Connection>,
    pub audio: Arc<AudioService>,
    pub asr: Arc<AsrManager>,
    pub models: Arc<ModelManager>,
    pub dictionary: RwLock<DictionaryEngine>,
    pub injector: Arc<dyn TextInjector>,
    pub recording: Mutex<bool>,
}

// rusqlite::Connection is not Send by default; we wrap it in Mutex<> and
// guarantee single-threaded access via the lock.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}
