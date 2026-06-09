use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::{
    core::{
        asr::model_manager::{ModelInfo, DEFAULT_MODEL},
        asr::whisper_cli::WhisperCliProvider,
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

/// Build the local Whisper provider from the currently-selected model and the
/// resolved whisper-cli binary, and register it under the `"local"` id.
pub async fn register_local_provider(state: &AppState) -> Result<()> {
    let model = current_whisper_model(state);
    if !state.models.is_downloaded(&model) {
        return Err(EchoError::NotFound(format!(
            "Whisper model '{model}' is not downloaded yet"
        )));
    }
    let binary = state.binaries.resolve().ok_or_else(|| {
        EchoError::NotFound("The whisper-cli binary is not installed yet".into())
    })?;
    let provider = WhisperCliProvider::new(binary, state.models.model_path(&model), model);
    state.asr.register(Arc::new(provider)).await;
    Ok(())
}

/// The selected local model name, defaulting to [`DEFAULT_MODEL`].
fn current_whisper_model(state: &AppState) -> String {
    let conn = state.db.lock().unwrap();
    crate::storage::repositories::get_setting(&conn, "whisper_model")
        .unwrap_or(None)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string())
}

/// Switch the active ASR provider, persisting the choice. Selecting `"local"`
/// (re)builds the Whisper provider from the bundled binary + selected model.
#[tauri::command]
pub async fn set_asr_provider(state: State<'_, AppState>, name: String) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::set_setting(&conn, "asr_provider", &name)?;
    }

    // "none" disables transcription; leave the manager's active provider as-is.
    if name == "none" {
        return Ok(());
    }

    if name == "local" {
        register_local_provider(state.inner()).await?;
    }

    state.asr.set_active(&name).await
}

/// Change the local Whisper model and, if local is active, reload the provider.
#[tauri::command]
pub async fn set_whisper_model(state: State<'_, AppState>, name: String) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::set_setting(&conn, "whisper_model", &name)?;
    }
    if state.asr.active_provider_name().await == "local" {
        register_local_provider(state.inner()).await?;
    }
    Ok(())
}

/// Whether the local engine is ready to transcribe (binary + selected model
/// both present). Used by onboarding and settings to gate the local option.
#[tauri::command]
pub fn whisper_ready(state: State<'_, AppState>) -> bool {
    let model = current_whisper_model(state.inner());
    state.binaries.is_installed() && state.models.is_downloaded(&model)
}

/// Download the whisper-cli binary for this platform, emitting
/// `echo://whisper-binary-progress` (bare f32, 0..1). On platforms without a
/// prebuilt release this errors with guidance to install one on PATH.
#[tauri::command]
pub async fn download_whisper_binary(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let binaries = state.binaries.clone();
    let (tx, mut rx) = mpsc::channel::<f32>(32);
    let app_progress = app.clone();
    tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            let _ = app_progress.emit("echo://whisper-binary-progress", p);
        }
    });
    binaries.download(tx).await?;
    let _ = app.emit("echo://whisper-binary-progress", 1.0_f32);
    Ok(())
}
