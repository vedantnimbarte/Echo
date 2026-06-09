use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::{
    core::{
        asr::model_manager::{is_whisper_model, ModelInfo},
        events::AppEvent,
    },
    error::{EchoError, Result},
    state::AppState,
};

/// List the Whisper model catalog with local download status.
#[tauri::command]
pub fn list_models(state: State<'_, AppState>) -> Vec<ModelInfo> {
    state.models.list()
}

/// Download a model, emitting `echo://model-download-progress` updates and a
/// final `echo://model-download-complete` event.
#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    let models = state.models.clone();

    let (tx, mut rx) = mpsc::channel::<f32>(32);
    let app_progress = app.clone();
    let progress_name = name.clone();
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let event = AppEvent::ModelDownloadProgress {
                name: progress_name.clone(),
                progress,
            };
            let _ = app_progress.emit(event.event_name(), &event);
        }
    });

    models.download(&name, tx).await?;

    let event = AppEvent::ModelDownloadComplete { name: name.clone() };
    app.emit(event.event_name(), &event)
        .map_err(|e| EchoError::Plugin(e.to_string()))?;
    Ok(())
}

/// Switch the active ASR provider, persisting the choice. For local Whisper
/// models this loads and registers the provider on demand.
#[tauri::command]
pub async fn set_asr_provider(state: State<'_, AppState>, name: String) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::set_setting(&conn, "asr_provider", &name)?;
    }

    if name == "none" {
        return Ok(());
    }

    if is_whisper_model(&name) {
        load_whisper_provider(state.inner(), &name).await?;
    }

    state.asr.set_active(&name).await
}

#[cfg(feature = "whisper")]
async fn load_whisper_provider(state: &AppState, name: &str) -> Result<()> {
    use crate::core::asr::whisper::WhisperProvider;
    use std::sync::Arc;

    if !state.models.is_downloaded(name) {
        return Err(EchoError::NotFound(format!(
            "Model '{name}' is not downloaded"
        )));
    }
    let path = state.models.model_path(name);
    let provider = Arc::new(WhisperProvider::load(&path, name)?);
    state.asr.register(provider).await;
    Ok(())
}

#[cfg(not(feature = "whisper"))]
async fn load_whisper_provider(_state: &AppState, _name: &str) -> Result<()> {
    Err(EchoError::Config(
        "Local Whisper support was not compiled in. Rebuild with `cargo build --features whisper` \
         (requires cmake + libclang)."
            .into(),
    ))
}
