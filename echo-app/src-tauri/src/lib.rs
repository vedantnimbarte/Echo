mod commands;
mod core;
mod error;
mod platform;
mod state;
mod storage;

use std::sync::{Arc, Mutex};

use tauri::Manager;
use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::EnvFilter;

use core::{
    asr::manager::AsrManager,
    asr::model_manager::ModelManager,
    audio::AudioService,
    dictionary::DictionaryEngine,
    injection::platform_injector,
};
use state::AppState;
use storage::db;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("echo=debug".parse().unwrap()))
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("Could not resolve app data directory");

            std::fs::create_dir_all(&data_dir)?;
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
                })
                .collect();

            // Resolve the configured ASR provider (defaults to "none").
            let active_provider = storage::repositories::get_setting(&conn, "asr_provider")
                .unwrap_or(None)
                .unwrap_or_else(|| "none".into());

            let asr_manager = Arc::new(AsrManager::new(active_provider.clone()));

            let models_dir = data_dir.join("models");
            std::fs::create_dir_all(&models_dir)?;
            let model_manager = Arc::new(ModelManager::new(models_dir));

            // If a local Whisper model is selected and present, load it now.
            #[cfg(feature = "whisper")]
            if core::asr::model_manager::is_whisper_model(&active_provider)
                && model_manager.is_downloaded(&active_provider)
            {
                let path = model_manager.model_path(&active_provider);
                match core::asr::whisper::WhisperProvider::load(&path, &active_provider) {
                    Ok(provider) => {
                        let asr = asr_manager.clone();
                        let name = active_provider.clone();
                        tauri::async_runtime::block_on(async move {
                            asr.register(Arc::new(provider)).await;
                            let _ = asr.set_active(&name).await;
                        });
                    }
                    Err(e) => tracing::error!("Failed to load Whisper model '{active_provider}': {e}"),
                }
            }

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

            let app_state = AppState {
                db: Mutex::new(conn),
                audio: Arc::new(AudioService::new().expect("Failed to initialize audio")),
                asr: asr_manager,
                models: model_manager,
                dictionary: Arc::new(RwLock::new(DictionaryEngine::new(entries))),
                injector: Arc::from(platform_injector()),
                telemetry,
                plugins: Mutex::new(plugin_loader),
                plugins_dir,
                recording: Mutex::new(false),
            };

            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::audio::get_audio_devices,
            commands::asr::list_models,
            commands::asr::download_model,
            commands::asr::set_asr_provider,
            commands::recording::start_recording,
            commands::recording::stop_recording,
            commands::recording::is_recording,
            commands::dictionary::list_dictionary,
            commands::dictionary::add_dictionary_entry,
            commands::dictionary::delete_dictionary_entry,
            commands::dictionary::toggle_dictionary_entry,
            commands::dictionary::export_dictionary,
            commands::dictionary::import_dictionary,
            commands::history::get_history,
            commands::history::clear_history,
            commands::injection::check_accessibility_permission,
            commands::providers::set_api_key,
            commands::providers::get_api_key_set,
            commands::providers::remove_api_key,
            commands::telemetry::get_telemetry_summary,
            commands::telemetry::clear_telemetry,
            commands::telemetry::set_telemetry_enabled,
            commands::telemetry::record_telemetry_event,
            commands::plugins::list_plugins,
            commands::plugins::install_plugin,
            commands::plugins::enable_plugin,
            commands::plugins::disable_plugin,
            commands::plugins::uninstall_plugin,
            commands::settings::get_setting,
            commands::settings::set_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running echo");
}
