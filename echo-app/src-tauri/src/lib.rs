mod commands;
mod core;
mod error;
mod platform;
mod state;
mod storage;

#[cfg(test)]
mod pipeline_tests;

use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::ShortcutState;
use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::EnvFilter;

use core::{
    asr::binary_manager::BinaryManager,
    asr::manager::AsrManager,
    asr::model_manager::ModelManager,
    audio::AudioService,
    dictionary::DictionaryEngine,
    injection::platform_injector,
};
use state::AppState;
use storage::db;

/// Log to stdout *and* to `echo.log` beside the database.
///
/// Everything downstream of capture — VAD, ASR, injection — reports failure by
/// logging and carrying on, which is invisible in a packaged build and in any
/// dev run whose console has scrolled away. The file is the only record a user
/// can actually send us.
fn init_tracing(data_dir: &std::path::Path) {
    use tracing_subscriber::fmt::writer::MakeWriterExt;

    let filter = || {
        EnvFilter::from_default_env().add_directive("echo=debug".parse().unwrap())
    };

    // Best effort: if the log file can't be opened, stdout alone still works.
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(data_dir.join("echo.log"))
        .ok();

    match file {
        Some(file) => tracing_subscriber::fmt()
            .with_env_filter(filter())
            .with_ansi(false)
            .with_writer(std::io::stdout.and(std::sync::Mutex::new(file)))
            .init(),
        None => tracing_subscriber::fmt().with_env_filter(filter()).init(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // Hold-to-talk brackets one utterance between the key going
                    // down and coming back up; every other mode acts on the
                    // press alone and ignores the release.
                    let hold = app
                        .try_state::<AppState>()
                        .map(|s| {
                            let conn = s.db.lock().unwrap();
                            storage::repositories::get_setting(&conn, "recording_mode")
                                .unwrap_or(None)
                                .as_deref()
                                == Some("hold")
                        })
                        .unwrap_or(false);

                    let _ = match (hold, event.state) {
                        (false, ShortcutState::Pressed) => app.emit("echo://hotkey-toggle", ()),
                        (true, ShortcutState::Pressed) => app.emit("echo://hotkey-press", ()),
                        (true, ShortcutState::Released) => app.emit("echo://hotkey-release", ()),
                        _ => Ok(()),
                    };
                })
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("Could not resolve app data directory");

            std::fs::create_dir_all(&data_dir)?;
            init_tracing(&data_dir);
            let db_path = data_dir.join("echo.db");

            info!("Opening database at {}", db_path.display());
            let conn = db::open(&db_path).expect("Failed to open database");

            let entries = storage::repositories::list_dictionary_entries(&conn)
                .unwrap_or_default()
                .into_iter()
                .map(|e| core::dictionary::DictionaryEntry {
                    id: e.id,
                    phrase: e.phrase,
                    replacement: e.replacement,
                    enabled: e.enabled,
                    profile_id: e.profile_id,
                })
                .collect();

            // Built before the ASR providers: the local engine holds a handle
            // to it so dictionary terms can bias whisper's decoder, not just
            // patch its output afterwards.
            let dictionary = Arc::new(RwLock::new(DictionaryEngine::new(entries)));

            // Apply the history retention policy at startup. Doing it here
            // rather than on a timer means it also runs for someone who just
            // shortened the window, instead of waiting for the next tick.
            match storage::repositories::get_setting(&conn, "history_retention_days")
                .unwrap_or(None)
                .and_then(|v| v.parse::<i64>().ok())
            {
                Some(days) if days > 0 => {
                    match storage::repositories::trim_history_older_than(&conn, days) {
                        Ok(0) => {}
                        Ok(n) => info!("Removed {n} transcript(s) older than {days} days"),
                        Err(e) => tracing::warn!("History retention pass failed: {e}"),
                    }
                }
                _ => {}
            }

            // Default to the local (offline) Whisper engine on first run.
            let active_provider = storage::repositories::get_setting(&conn, "asr_provider")
                .unwrap_or(None)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "local".into());

            let asr_manager = Arc::new(AsrManager::new(active_provider.clone()));

            let models_dir = data_dir.join("models");
            std::fs::create_dir_all(&models_dir)?;
            let model_manager = Arc::new(ModelManager::new(models_dir));

            // whisper.cpp CLI binary: a copy bundled in the installer's
            // resources is preferred; otherwise it is downloaded on first run
            // or found on PATH.
            let bin_dir = data_dir.join("bin");
            std::fs::create_dir_all(&bin_dir)?;
            let bundled_bin = core::runtime_deps::bundled_whisper_dir(app.handle());
            // Probed once here rather than per transcription: it costs a
            // process spawn, and the answer cannot change while we run.
            let gpu = core::gpu::detect();
            info!("Compute backend: {}", gpu.label());
            let binary_manager = Arc::new(
                BinaryManager::new(bin_dir)
                    .with_bundled_dir(bundled_bin)
                    .with_gpu(gpu),
            );
            let whisper_server = Arc::new(core::asr::whisper_server::WhisperServer::new());

            // Selected local model (defaults to base.en).
            let whisper_model = storage::repositories::get_setting(&conn, "whisper_model")
                .unwrap_or(None)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| core::asr::model_manager::DEFAULT_MODEL.to_string());

            // If the local engine is fully provisioned, register it now. Until
            // then `asr_provider` may be "local" with no provider registered —
            // onboarding downloads the binary + model and calls set_asr_provider.
            if let Some(binary) = binary_manager.resolve() {
                if model_manager.is_downloaded(&whisper_model) {
                    let _ = binary; // presence check only; the provider resolves per call
                    let (threads, gpu_allowed) = commands::asr::local_decode_settings(&conn);
                    let provider = core::asr::local::LocalWhisperProvider::new(
                        binary_manager.clone(),
                        whisper_server.clone(),
                        model_manager.model_path(&whisper_model),
                        whisper_model.clone(),
                    )
                    .with_dictionary(dictionary.clone())
                    .with_threads(threads)
                    .with_gpu_allowed(gpu_allowed);
                    let asr = asr_manager.clone();
                    tauri::async_runtime::block_on(async move {
                        asr.register(Arc::new(provider)).await;
                    });
                }
            }

            // Wake-word models live beside the Whisper models; nothing is
            // fetched until the user enables the feature.
            let wake_dir = data_dir.join("wake");
            std::fs::create_dir_all(&wake_dir)?;
            let wake_models = Arc::new(core::wake::WakeModelManager::new(wake_dir));

            // Load the Silero VAD model; energy VAD is the fallback on failure.
            // (ONNX Runtime is statically linked into the binary, so there is
            // nothing to bundle or locate for this.)
            let silero = match core::vad::SileroModel::load() {
                Ok(m) => Some(Arc::new(m)),
                Err(e) => {
                    tracing::warn!("Silero VAD unavailable, using energy VAD: {e}");
                    None
                }
            };

            // Register any cloud ASR providers whose API keys are in the keychain.
            for provider_name in ["openai", "groq", "deepgram"] {
                if let Ok(Some(key)) = storage::keychain::get_api_key(provider_name) {
                    if let Ok(provider) = commands::providers::build_provider(provider_name, key) {
                        let asr = asr_manager.clone();
                        tauri::async_runtime::block_on(async move {
                            asr.register(provider).await;
                        });
                    }
                }
            }

            // Telemetry (local-only). Mirror the persisted opt-in flag, default on.
            let telemetry_enabled = storage::repositories::get_setting(&conn, "telemetry_enabled")
                .unwrap_or(None)
                .map(|v| v != "false")
                .unwrap_or(true);
            let telemetry = core::telemetry::TelemetryService::new(telemetry_enabled);
            telemetry.record(
                &conn,
                "app_started",
                Some(serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "os": std::env::consts::OS,
                    "arch": std::env::consts::ARCH,
                })),
            );

            // Plugins: ensure the directory exists and load any enabled plugins.
            let plugins_dir = data_dir.join("plugins");
            std::fs::create_dir_all(&plugins_dir)?;
            let mut plugin_loader = core::plugins::loader::PluginLoader::new();
            if let Ok(rows) = storage::repositories::list_plugins(&conn) {
                let ctx = core::plugins::PluginContext {
                    data_dir: plugins_dir.clone(),
                    settings: Arc::new(|_| None),
                };
                for (name, _version, enabled, manifest_str) in rows {
                    if !enabled {
                        continue;
                    }
                    if let Ok(manifest) =
                        serde_json::from_str::<core::plugins::PluginManifest>(&manifest_str)
                    {
                        let lib = plugins_dir.join(&name).join(&manifest.entry);
                        if let Err(e) = plugin_loader.load(&lib, &ctx) {
                            tracing::error!("Failed to load plugin '{name}': {e}");
                        }
                    }
                }
            }

            // First run shows the settings window with the onboarding wizard.
            let onboarding_done = storage::repositories::get_setting(&conn, "onboarding_complete")
                .unwrap_or(None)
                .map(|v| v == "true")
                .unwrap_or(false);

            let app_state = AppState {
                db: Mutex::new(conn),
                audio: Arc::new(AudioService::new().expect("Failed to initialize audio")),
                asr: asr_manager,
                models: model_manager,
                binaries: binary_manager,
                whisper_server,
                silero,
                wake_models,
                wake_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                dictionary,
                injector: Arc::from(platform_injector()),
                telemetry,
                plugins: Mutex::new(plugin_loader),
                plugins_dir,
                recording: Mutex::new(false),
                modtap: Mutex::new(None),
            };

            app.manage(app_state);

            // Bind the global hotkey now that state is available. Which
            // mechanism gets used depends on the shortcut itself.
            let handle = app.handle().clone();
            if let Err(e) = commands::hotkey::apply(&handle, &handle.state::<AppState>()) {
                tracing::warn!("Couldn't bind the global hotkey: {e}");
            }

            // Drain the egress channel into the database. Requests are logged
            // fire-and-forget so a network call never waits on SQLite.
            {
                let (tx, mut rx) =
                    tokio::sync::mpsc::unbounded_channel::<core::egress::Egress>();
                core::egress::init(tx);

                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    // Trim occasionally rather than on every insert; this is a
                    // rolling log, not an audit trail.
                    let mut since_trim = 0u32;
                    while let Some(e) = rx.recv().await {
                        let state = handle.state::<AppState>();
                        let conn = state.db.lock().unwrap();
                        if let Err(err) =
                            storage::repositories::insert_egress(&conn, &e.host, &e.purpose)
                        {
                            tracing::warn!("Failed to log egress: {err}");
                        }
                        since_trim += 1;
                        if since_trim >= 50 {
                            since_trim = 0;
                            let _ = storage::repositories::trim_egress(&conn, 1000);
                        }
                    }
                });
            }

            // Arm the wake-word listener if the user enabled it last session.
            commands::wake::rearm(app.handle());

            // Surface the settings window on first launch so onboarding can run.
            if !onboarding_done {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::quit,
            commands::audio::get_audio_devices,
            commands::asr::list_models,
            commands::asr::download_model,
            commands::asr::set_asr_provider,
            commands::asr::set_whisper_model,
            commands::asr::delete_model,
            commands::asr::whisper_ready,
            commands::asr::download_whisper_binary,
            commands::asr::gpu_status,
            commands::asr::download_gpu_pack,
            commands::asr::set_gpu_enabled,
            commands::asr::set_whisper_threads,
            commands::asr::installed_packs,
            commands::import::transcribe_file,
            commands::import::supported_import_formats,
            commands::recording::start_recording,
            commands::recording::stop_recording,
            commands::recording::is_recording,
            commands::recording::warm_microphone,
            commands::dictionary::list_dictionary,
            commands::dictionary::add_dictionary_entry,
            commands::dictionary::delete_dictionary_entry,
            commands::dictionary::toggle_dictionary_entry,
            commands::dictionary::export_dictionary,
            commands::dictionary::import_dictionary,
            commands::dictionary::learn_from_correction,
            commands::history::get_history,
            commands::history::clear_history,
            commands::injection::check_accessibility_permission,
            commands::injection::inject_text,
            commands::hotkey::get_hotkey,
            commands::hotkey::register_hotkey,
            commands::hotkey::hotkey_support,
            commands::hotkey::set_recording_mode,
            commands::providers::set_api_key,
            commands::providers::get_api_key_set,
            commands::providers::remove_api_key,
            commands::telemetry::get_telemetry_summary,
            commands::telemetry::clear_telemetry,
            commands::telemetry::set_telemetry_enabled,
            commands::telemetry::record_telemetry_event,
            commands::plugins::list_plugins,
            commands::plugins::inspect_plugin,
            commands::plugins::install_plugin,
            commands::plugins::enable_plugin,
            commands::plugins::disable_plugin,
            commands::plugins::uninstall_plugin,
            commands::wake::set_wake_word_enabled,
            commands::wake::set_wake_word_model,
            commands::wake::set_wake_word_sensitivity,
            commands::wake::list_wake_words,
            commands::wake::download_wake_model,
            commands::wake::import_wake_model,
            commands::wake::wake_word_ready,
            commands::wake::wake_word_active,
            commands::wake::wake_word_status,
            commands::history::export_history,
            commands::profiles::get_foreground_app,
            commands::profiles::list_app_profiles,
            commands::profiles::save_app_profile,
            commands::profiles::delete_app_profile,
            commands::profiles::list_profiles,
            commands::profiles::add_profile,
            commands::profiles::delete_profile,
            commands::profiles::set_dictionary_entry_profile,
            commands::egress::get_egress_log,
            commands::egress::clear_egress_log,
            commands::egress::get_egress_status,
            commands::settings::get_setting,
            commands::settings::set_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running echo");
}
