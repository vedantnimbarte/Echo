use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

use crate::core::{
    asr::binary_manager::BinaryManager,
    asr::manager::AsrManager,
    asr::model_manager::ModelManager,
    audio::AudioService,
    dictionary::DictionaryEngine,
    injection::TextInjector,
    modtap::ModTapWatcher,
    plugins::loader::PluginLoader,
    telemetry::TelemetryService,
    vad::SileroModel,
    wake::WakeModelManager,
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
    pub binaries: Arc<BinaryManager>,
    /// Loaded Silero VAD model, shared read-only across recording sessions.
    /// `None` if the ONNX model failed to load (falls back to energy VAD).
    pub silero: Option<Arc<SileroModel>>,
    /// Downloadable wake-word models and the loader for them.
    pub wake_models: Arc<WakeModelManager>,
    /// True while the idle wake-word listener holds the microphone. Also gates
    /// `rearm` so only one listener runs at a time.
    pub wake_active: Arc<AtomicBool>,
    pub dictionary: Arc<RwLock<DictionaryEngine>>,
    pub injector: Arc<dyn TextInjector>,
    pub telemetry: TelemetryService,
    pub plugins: Mutex<PluginLoader>,
    pub plugins_dir: PathBuf,
    pub recording: Mutex<bool>,
    /// Live watcher when the hotkey is a bare modifier, which the
    /// global-shortcut plugin cannot express. Exactly one of the two
    /// mechanisms is bound at a time; dropping this one unbinds it.
    pub modtap: Mutex<Option<ModTapWatcher>>,
}

// rusqlite::Connection is not Send by default; we wrap it in Mutex<> and
// guarantee single-threaded access via the lock.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}
