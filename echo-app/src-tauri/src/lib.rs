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

            let app_state = AppState {
                db: Mutex::new(conn),
                audio: Arc::new(AudioService::new().expect("Failed to initialize audio")),
                asr: asr_manager,
                models: model_manager,
                dictionary: Arc::new(RwLock::new(DictionaryEngine::new(entries))),
                injector: Arc::from(platform_injector()),
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
            commands::settings::get_setting,
            commands::settings::set_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running echo");
}
