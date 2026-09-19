use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

use crate::core::{
    asr::binary_manager::BinaryManager, asr::manager::AsrManager, asr::model_manager::ModelManager,
    asr::nemo::NemoBinaries, asr::nemo_server::NemoServer, asr::whisper_server::WhisperServer,
    audio::AudioService, dictionary::DictionaryEngine, injection::TextInjector,
    modtap::ModTapWatcher, plugins::loader::PluginLoader, telemetry::TelemetryService,
    vad::SileroModel, wake::WakeModelManager,
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
    /// The resident whisper.cpp model. Shared so switching models or
    /// engines reuses the same supervised process rather than leaking one.
    pub whisper_server: Arc<WhisperServer>,
    /// The NeMo-Speech engine: its binaries, and the resident model process.
    pub nemo_binaries: Arc<NemoBinaries>,
    pub nemo_server: Arc<NemoServer>,
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
    /// The dictation state machine. THE ONLY PLACE RECORDING STATE LIVES — see
    /// [`crate::core::dictation::machine`]. It replaced a bare `Mutex<bool>`
    /// that every caller had to remember to claim and hand back in the right
    /// order, and that a capture failure could leave set, killing the hotkey
    /// until restart.
    pub dictation: Mutex<crate::core::dictation::DictationMachine>,
    /// Bumped every time a cancel countdown is armed or called off.
    ///
    /// A timer that wakes holding a stale generation does nothing. That is what
    /// stops a countdown armed during one dictation from discarding the next
    /// one, which is a real sequence: Escape, Escape, stop, start again, all
    /// inside three seconds.
    pub cancel_generation: std::sync::atomic::AtomicU64,
    /// When the user last stopped talking.
    ///
    /// The start of the one measurement the product is judged on — stop to text
    /// on screen. It lives on the shared state rather than in the delivery task
    /// because the two ends are set in different functions: `end_recording`
    /// knows when the user stopped, and the delivery task knows when the text
    /// landed, and neither can see the other's locals.
    pub last_stop_at: Mutex<Option<std::time::Instant>>,
    /// What Echo last typed into another app, so it can be taken back or
    /// re-transcribed. Cleared once used — see [`crate::core::undo`].
    pub last_delivery: Mutex<Option<crate::core::undo::LastDelivery>>,
    /// PCM of the most recent utterance, retained so a retry can re-decode it
    /// on a stronger model instead of asking the user to say it again.
    /// Memory only, capped, and dropped when retry is disabled.
    pub last_utterance: Arc<Mutex<Option<crate::commands::recording::Retained>>>,
    /// Live decoder-prompt context: focused app, its dictionary profile, and
    /// the sentence just spoken. Shared with the local whisper provider.
    pub prompt_ctx: Arc<crate::core::asr::prompt::PromptContext>,
    /// Live watcher when the hotkey is a bare modifier, which the
    /// global-shortcut plugin cannot express. Exactly one of the two
    /// mechanisms is bound at a time; dropping this one unbinds it.
    pub modtap: Mutex<Option<ModTapWatcher>>,
    /// Which capabilities are mid-way through an exclusive command. Claimed and
    /// released by the command factory, never by a handler — see
    /// [`crate::ipc::factory`].
    pub exclusive: crate::ipc::factory::ExclusiveRegistry,
}

// rusqlite::Connection is not Send by default; we wrap it in Mutex<> and
// guarantee single-threaded access via the lock.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

impl AppState {
    /// Whether audio is flowing right now.
    ///
    /// True in CancelArmed as well as Recording, because capture genuinely is
    /// still running there — that is what makes a second Escape able to resume
    /// with nothing lost. A caller asking "is the microphone busy" must get
    /// yes.
    pub fn is_capturing(&self) -> bool {
        use crate::core::dictation::DictationState as S;
        matches!(
            self.dictation_state(),
            S::Arming | S::Recording | S::CancelArmed
        )
    }

    pub fn dictation_state(&self) -> crate::core::dictation::DictationState {
        self.dictation
            .lock()
            .map(|m| m.state())
            .unwrap_or(crate::core::dictation::DictationState::Idle)
    }
}
