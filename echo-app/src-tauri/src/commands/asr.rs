use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::{
    core::{
        asr::binary_manager::Pack,
        asr::decode_opts,
        asr::local::LocalWhisperProvider,
        asr::model_manager::{Engine, ModelInfo, DEFAULT_MODEL, DEFAULT_NEMO_MODEL},
        asr::nemo::{NemoPack, NemoProvider},
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
    let binary = state
        .binaries
        .resolve()
        .ok_or_else(|| EchoError::NotFound("The whisper-cli binary is not installed yet".into()))?;
    let _ = binary; // presence check only; the provider re-resolves per call
    let (threads, gpu_allowed) = {
        let conn = state.db.lock().unwrap();
        local_decode_settings(&conn)
    };
    let provider = build_local_provider(
        state.binaries.clone(),
        state.whisper_server.clone(),
        state.models.clone(),
        &model,
        state.dictionary.clone(),
        state.prompt_ctx.clone(),
        threads,
        gpu_allowed,
    );
    state.asr.register(Arc::new(provider)).await;
    Ok(())
}

/// Assemble the local provider with everything it should be holding.
///
/// One function because there are two callers — startup, before `AppState`
/// exists, and every later re-registration — and they had already drifted: the
/// re-registration path never attached the prompt context, so choosing a
/// different model silently switched off per-app decoder prompting until the
/// next restart. Nothing about that failure is visible, which is exactly why it
/// survived.
#[allow(clippy::too_many_arguments)]
pub fn build_local_provider(
    binaries: Arc<crate::core::asr::binary_manager::BinaryManager>,
    server: Arc<crate::core::asr::whisper_server::WhisperServer>,
    models: Arc<crate::core::asr::model_manager::ModelManager>,
    model: &str,
    dictionary: Arc<tokio::sync::RwLock<crate::core::dictionary::DictionaryEngine>>,
    prompt_ctx: Arc<crate::core::asr::prompt::PromptContext>,
    threads: usize,
    gpu_allowed: bool,
) -> LocalWhisperProvider {
    let mut provider = LocalWhisperProvider::new(
        binaries,
        server,
        models.model_path(model),
        model.to_string(),
    )
    .with_dictionary(dictionary)
    .with_prompt_context(prompt_ctx)
    .with_threads(threads)
    .with_gpu_allowed(gpu_allowed);

    // Streaming partials are discarded within a second, so they only have to be
    // roughly right. Decoding them on a smaller model is what lets the text
    // keep up with the speaker on a machine without a GPU.
    if let Some(small) = models.smaller_downloaded(model) {
        tracing::info!(partial_model = %small.name, "Using a smaller model for live partials");
        provider = provider.with_partial_model(models.model_path(&small.name), small.name);
    }
    provider
}

/// The selected local model name, defaulting to [`DEFAULT_MODEL`].
fn current_whisper_model(state: &AppState) -> String {
    let conn = state.db.lock().unwrap();
    crate::storage::repositories::get_setting(&conn, "whisper_model")
        .unwrap_or(None)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string())
}

/// The selected NeMo model, defaulting to the one model that engine has.
fn current_nemo_model(state: &AppState) -> String {
    let conn = state.db.lock().unwrap();
    crate::storage::repositories::get_setting(&conn, "nemo_model")
        .unwrap_or(None)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_NEMO_MODEL.to_string())
}

/// Build the NeMo provider from the selected model and the installed binary,
/// and register it under the `"nemo"` id.
pub async fn register_nemo_provider(state: &AppState) -> Result<()> {
    let model = current_nemo_model(state);
    if !state.models.is_downloaded(&model) {
        return Err(EchoError::NotFound(format!(
            "The '{model}' model is not downloaded yet"
        )));
    }
    if !state.nemo_binaries.is_installed() {
        return Err(EchoError::NotFound(
            "The NeMo-Speech engine is not installed yet".into(),
        ));
    }
    let gpu_allowed = {
        let conn = state.db.lock().unwrap();
        local_decode_settings(&conn).1
    };
    let provider = NemoProvider::new(
        state.nemo_binaries.clone(),
        state.nemo_server.clone(),
        state.models.model_path(&model),
    )
    .with_gpu_allowed(gpu_allowed);
    state.asr.register(Arc::new(provider)).await;
    Ok(())
}

/// Download the NeMo-Speech binaries for this machine.
#[tauri::command]
pub async fn download_nemo_engine(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let pack = NemoPack::for_gpu(state.binaries.gpu());
    let binaries = state.nemo_binaries.clone();

    let (tx, mut rx) = mpsc::channel::<f32>(32);
    let app_progress = app.clone();
    tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            let _ = app_progress.emit("echo://nemo-engine-progress", p);
        }
    });
    binaries.download_pack(pack, tx).await?;
    let _ = app.emit("echo://nemo-engine-progress", 1.0_f32);

    // A newly installed pack changes which binary resolves, and the running
    // server was started from the old one.
    state.nemo_server.shutdown().await;
    if state.asr.active_provider_name().await == "nemo" {
        register_nemo_provider(state.inner()).await?;
    }
    Ok(())
}

/// What the UI needs to offer the NeMo engine: whether each half is present,
/// and what installing the missing half would cost.
#[derive(serde::Serialize)]
pub struct NemoStatus {
    /// Binaries and the selected model are both present.
    pub ready: bool,
    pub engine_installed: bool,
    pub model_downloaded: bool,
    pub model: String,
    /// Size of the engine download for this machine, in megabytes.
    pub engine_mb: u32,
    /// Which build would be installed ("CPU" / "NVIDIA CUDA").
    pub engine_label: &'static str,
}

#[tauri::command]
pub fn nemo_status(state: State<'_, AppState>) -> NemoStatus {
    let model = current_nemo_model(&state);
    let pack = NemoPack::for_gpu(state.binaries.gpu());
    NemoStatus {
        ready: state.nemo_binaries.is_installed() && state.models.is_downloaded(&model),
        engine_installed: state.nemo_binaries.is_installed(),
        model_downloaded: state.models.is_downloaded(&model),
        model,
        engine_mb: pack.download_mb(),
        engine_label: pack.label(),
    }
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
    if name == "nemo" {
        register_nemo_provider(state.inner()).await?;
    }

    // Only one local engine holds a model in memory at a time: whichever the
    // user just walked away from should give the RAM back.
    match name.as_str() {
        "local" => state.nemo_server.shutdown().await,
        "nemo" => state.whisper_server.shutdown().await,
        _ => {}
    }

    state.asr.set_active(&name).await
}

/// Change the selected local model and reload whichever engine owns it.
///
/// One command for both engines because the picker is one list: the catalog
/// says which engine a model belongs to, so choosing a NeMo model switches the
/// engine rather than asking the user to do it in two places.
#[tauri::command]
pub async fn set_whisper_model(state: State<'_, AppState>, name: String) -> Result<()> {
    let engine = crate::core::asr::model_manager::ModelManager::engine_of(&name);
    {
        let conn = state.db.lock().unwrap();
        let key = match engine {
            Engine::Whisper => "whisper_model",
            Engine::Nemo => "nemo_model",
        };
        crate::storage::repositories::set_setting(&conn, key, &name)?;
    }
    match (engine, state.asr.active_provider_name().await.as_str()) {
        (Engine::Whisper, "local") => register_local_provider(state.inner()).await?,
        (Engine::Nemo, "nemo") => register_nemo_provider(state.inner()).await?,
        _ => {}
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
    /// Its download size in MB, so the user knows what they are agreeing to.
    pub available_pack_mb: Option<u32>,
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
        available_pack_mb: available.map(|p| p.download_mb()),
        pack_installed: available
            .map(|p| state.binaries.pack_installed(p))
            .unwrap_or(false),
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
        EchoError::NotFound("No accelerated whisper build is available for this machine".into())
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
