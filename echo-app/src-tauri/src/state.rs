use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

use crate::core::{
    asr::manager::AsrManager,
    audio::AudioService,
    dictionary::DictionaryEngine,
    injection::TextInjector,
    vad::EnergyVad,
};

/// Shared application state — stored in Tauri's managed state.
pub struct AppState {
    pub db: Mutex<Connection>,
    pub audio: Arc<AudioService>,
    pub asr: Arc<AsrManager>,
    pub dictionary: RwLock<DictionaryEngine>,
    pub vad: Mutex<EnergyVad>,
    pub injector: Arc<dyn TextInjector>,
    pub recording: Mutex<bool>,
}

// rusqlite::Connection is not Send by default; we wrap it in Mutex<> and
// guarantee single-threaded access via the lock.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}
