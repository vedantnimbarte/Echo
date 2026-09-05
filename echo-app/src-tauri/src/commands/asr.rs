use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::{
    core::{
        asr::binary_manager::Pack,
        asr::decode_opts,
        asr::local::LocalWhisperProvider,
        asr::model_manager::{ModelInfo, DEFAULT_MODEL},
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
    let _ = binary; // presence check only; the provider re-resolves per call
    let (threads, gpu_allowed) = {
        let conn = state.db.lock().unwrap();
        local_decode_settings(&conn)
    };
    let provider = LocalWhisperProvider::new(
        state.binaries.clone(),
        state.whisper_server.clone(),
        state.models.model_path(&model),
        model,
    )
    .with_dictionary(state.dictionary.clone())
    .with_threads(threads)
    .with_gpu_allowed(gpu_allowed);
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

/// Delete a downloaded model's weights, freeing the disk they occupy.
///
/// Refuses to remove the model the local engine is set to use: that would leave
/// transcription silently broken with nothing on screen explaining why.
#[tauri::command]
pub fn delete_model(state: State<'_, AppState>, name: String) -> Result<()> {
    if current_whisper_model(state.inner()) == name {
        return Err(EchoError::Config(format!(
            "{name} is the model Echo is set to use. Pick another model first."
        )));
    }
    state.models.delete(&name)
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

/// Read the decode knobs for the local engine: thread count and whether the
/// GPU may be used at all.
///
/// Defaults are "auto" and "yes" — Echo prefers the GPU whenever the machine
/// has one it can drive, and only a deliberate opt-out or a runtime failure
/// takes it back to the CPU.
pub fn local_decode_settings(conn: &rusqlite::Connection) -> (usize, bool) {
    use crate::storage::repositories::get_setting;
    let threads = decode_opts::resolve_threads(
        get_setting(conn, "whisper_threads")
            .unwrap_or(None)
            .as_deref(),
    );
    let gpu_allowed = get_setting(conn, "gpu_enabled")
        .unwrap_or(None)
        .map(|v| v != "false")
        .unwrap_or(true);
    (threads, gpu_allowed)
}

/// What the settings UI needs to show about compute: what was detected, what is
/// installed, and what is actually in use right now.
#[derive(serde::Serialize)]
pub struct GpuStatus {
    /// Human-readable detected backend, e.g. "NVIDIA CUDA 12.x".
    pub detected: String,
    /// Id of the accelerated pack this machine could run, if any.
    pub available_pack: Option<String>,
    /// Whether that pack is downloaded.
    pub pack_installed: bool,
    /// Whether acceleration is actually being used for the next utterance.
    pub active: bool,
    /// True once an accelerated run failed and we latched to CPU.
    pub failed: bool,
    /// The user's opt-out.
    pub enabled: bool,
    pub threads: usize,
}

#[tauri::command]
pub fn gpu_status(state: State<'_, AppState>) -> GpuStatus {
    let (threads, enabled) = {
        let conn = state.db.lock().unwrap();
        local_decode_settings(&conn)
    };
    let available = state.binaries.available_gpu_pack();
    GpuStatus {
        detected: state.binaries.gpu().label(),
        available_pack: available.map(|p| p.id().to_string()),
        pack_installed: available.map(|p| state.binaries.pack_installed(p)).unwrap_or(false),
        active: enabled && state.binaries.active_gpu_pack().is_some(),
        failed: state.binaries.gpu_failed(),
        enabled,
        threads,
    }
}

/// Download the accelerated whisper.cpp build this machine can run, emitting
/// `echo://whisper-binary-progress` (bare f32, 0..1).
#[tauri::command]
pub async fn download_gpu_pack(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let pack = state.binaries.available_gpu_pack().ok_or_else(|| {
        EchoError::NotFound(
            "No accelerated whisper build is available for this machine".into(),
        )
    })?;

    let binaries = state.binaries.clone();
    let (tx, mut rx) = mpsc::channel::<f32>(32);
    let app_progress = app.clone();
    tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            let _ = app_progress.emit("echo://whisper-binary-progress", p);
        }
    });
    binaries.download_pack(pack, tx).await?;
    let _ = app.emit("echo://whisper-binary-progress", 1.0_f32);

    // The pack changes which binary we resolve, which changes the server's
    // signature — restart it so the next utterance runs on the GPU rather than
    // being served by the CPU process still holding the model.
    state.whisper_server.shutdown().await;
    if state.asr.active_provider_name().await == "local" {
        register_local_provider(state.inner()).await?;
    }
    Ok(())
}

/// Turn GPU decoding on or off. Also clears a latched failure, so this doubles
/// as the "try the GPU again" control after fixing a driver.
#[tauri::command]
pub async fn set_gpu_enabled(state: State<'_, AppState>, enabled: bool) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::set_setting(
            &conn,
            "gpu_enabled",
            if enabled { "true" } else { "false" },
        )?;
    }
    state.whisper_server.shutdown().await;
    if state.asr.active_provider_name().await == "local" {
        register_local_provider(state.inner()).await?;
    }
    Ok(())
}

/// Pin the decode thread count, or pass "auto" to let Echo choose.
#[tauri::command]
pub async fn set_whisper_threads(state: State<'_, AppState>, threads: String) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::set_setting(&conn, "whisper_threads", &threads)?;
    }
    state.whisper_server.shutdown().await;
    if state.asr.active_provider_name().await == "local" {
        register_local_provider(state.inner()).await?;
    }
    Ok(())
}

/// Ids of every binary pack currently installed on disk. Lets the settings UI
/// offer to reclaim the disk a superseded pack is using.
#[tauri::command]
pub fn installed_packs(state: State<'_, AppState>) -> Vec<String> {
    [Pack::Cpu, Pack::Cuda11, Pack::Cuda12]
        .into_iter()
        .filter(|p| state.binaries.pack_installed(*p))
        .map(|p| p.id().to_string())
        .collect()
}
