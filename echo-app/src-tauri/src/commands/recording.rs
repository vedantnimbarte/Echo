use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::{
    core::{
        asr::TranscriptSegment,
        command::CommandConfig,
        events::AppEvent,
        injection::TextInjector,
        vad::{EnergyVad, SileroVad, Vad},
    },
    error::{EchoError, Result},
    state::AppState,
};

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    device_name: Option<String>,
    language: Option<String>,
) -> Result<()> {
    begin_recording(app, state.inner(), device_name, language).await
}

/// Start a capture session.
///
/// Split out of the command so the wake-word listener can start recording
/// directly, without bouncing a request through the frontend.
pub async fn begin_recording(
    app: AppHandle,
    state: &AppState,
    device_name: Option<String>,
    language: Option<String>,
) -> Result<()> {
    {
        let mut recording = state.recording.lock().unwrap();
        if *recording {
            return Ok(());
        }
        *recording = true;
    }

    app.emit(AppEvent::RecordingStarted.event_name(), AppEvent::RecordingStarted)
        .map_err(|e| EchoError::Plugin(e.to_string()))?;
    info!("Recording started");

    let provider = state.asr.active_provider_name().await;
    {
        let conn = state.db.lock().unwrap();
        state.telemetry.record(
            &conn,
            "recording_started",
            Some(serde_json::json!({ "provider": provider.clone() })),
        );
    }

    // Fall back to the device configured in Settings when the caller doesn't
    // pin one (the floating pill triggers recording without knowing the device).
    let device_name = device_name.filter(|s| !s.is_empty()).or_else(|| {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::get_setting(&conn, "audio_device")
            .unwrap_or(None)
            .filter(|s| !s.is_empty())
    });

    // Same fallback shape as the device above: the pill triggers recording
    // without knowing the configured language. "auto" means let the model
    // detect it, which is what the providers already do for None.
    let language = language.filter(|s| !s.is_empty()).or_else(|| {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::get_setting(&conn, "language")
            .unwrap_or(None)
            .filter(|s| !s.is_empty() && s != "auto")
    });

    let audio_rx = state.audio.start_capture(device_name.as_deref())?;
    let (transcript_tx, mut transcript_rx) = mpsc::channel::<TranscriptSegment>(32);

    // VAD gating stage: sits between raw audio capture and the ASR pipeline.
    // It forwards only speech chunks and emits an empty-vec sentinel at each
    // speech→silence transition so the ASR provider knows an utterance ended.
    // The VAD instance belongs entirely to this task (see architectural rule 8).
    // Pick the VAD engine: Silero (neural, ignores keyboard/fan noise) when its
    // model loaded, else the energy fallback. `vad_engine` setting can force it.
    let silero_model = state.silero.clone();
    let vad_engine = {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::get_setting(&conn, "vad_engine")
            .unwrap_or(None)
            .unwrap_or_else(|| "silero".into())
    };

    let (vad_tx, vad_rx) = mpsc::channel::<Vec<f32>>(256);
    let level_app = app.clone();
    tokio::spawn(async move {
        let vad: Box<dyn Vad> = match silero_model {
            Some(model) if vad_engine != "energy" => Box::new(SileroVad::new(model)),
            _ => Box::new(EnergyVad::new(0.01)),
        };
        vad_gate(audio_rx, vad, vad_tx, move |event| match event {
            // Payload is the bare f32 (read as `event.payload` in JS).
            VadEvent::Level(rms) => {
                let _ = level_app.emit("echo://audio-level", rms);
            }
            // Rising edge: drives the pill's listening state in voice-activated mode.
            VadEvent::SpeechStarted => {
                let _ = level_app.emit("echo://speech-started", ());
            }
            // Falling edge: the pill switches to "transcribing".
            VadEvent::SpeechEnded => {
                let _ = level_app.emit("echo://speech-ended", ());
            }
        })
        .await;
    });

    let asr = state.asr.clone();
    let lang = language.clone();

    tokio::spawn(async move {
        if let Err(e) = asr.transcribe_stream(vad_rx, transcript_tx, lang.as_deref()).await {
            error!("ASR stream error: {e}");
        }
    });

    // Capture shared handles before the spawn — `state` is not 'static.
    let dictionary = state.dictionary.clone();
    let injector = state.injector.clone();

    // Command mode: a transcript opening with the prefix word is an instruction
    // for the LLM rather than text to type. Read once per session so a settings
    // change applies to the next recording without a restart.
    let command_cfg = command_config(state);
    let command_key = if command_cfg.enabled && command_cfg.provider == "openai" {
        crate::storage::keychain::get_api_key("openai").unwrap_or(None)
    } else {
        None
    };

    let app_clone = app.clone();
    tokio::spawn(async move {
        while let Some(segment) = transcript_rx.recv().await {
            if segment.is_final {
                // Which app is focused decides how the text is delivered and
                // which dictionary entries apply, so resolve it now rather than
                // at recording start — focus can move while you talk.
                // The macOS/Linux lookups shell out, so keep them off the runtime.
                let focused =
                    tokio::task::spawn_blocking(crate::core::appcontext::foreground_app)
                        .await
                        .ok()
                        .flatten();

                let delivery = {
                    let state = app_clone.state::<AppState>();
                    let conn = state.db.lock().unwrap();
                    resolve_delivery(&conn, focused.as_deref())
                };

                // Apply dictionary replacements to the final transcript.
                let processed = dictionary
                    .read()
                    .await
                    .process_for(&segment.text, delivery.dictionary_profile);

                // Record it before command mode rewrites anything: history is a
                // log of what you said, not of what the model replied.
                if delivery.record_history && !processed.is_empty() {
                    let state = app_clone.state::<AppState>();
                    let conn = state.db.lock().unwrap();
                    let record = crate::storage::models::TranscriptionRecord {
                        id: None,
                        text: processed.clone(),
                        language: segment.language.clone(),
                        provider: provider.clone(),
                        created_at: String::new(),
                    };
                    if let Err(e) = crate::storage::repositories::insert_history(&conn, &record) {
                        error!("Failed to record history: {e}");
                    }
                }
                // Emit the transcript fields directly (not the tagged AppEvent
                // wrapper) so the frontend reads `event.payload.text` naturally.
                if let Err(e) = app_clone.emit(
                    "echo://transcript-final",
                    serde_json::json!({ "text": processed, "language": segment.language }),
                ) {
                    error!("Failed to emit transcript event: {e}");
                }

                // Command mode intercepts before injection: the text to deliver
                // becomes the model's reply, not the transcript itself.
                let instruction = command_cfg
                    .enabled
                    .then(|| crate::core::command::parse_command(&processed, &command_cfg.prefix))
                    .flatten();

                let to_inject = match instruction {
                    None => processed,
                    Some(instruction) => {
                        match run_command(
                            &command_cfg,
                            command_key.as_deref(),
                            instruction,
                            &injector,
                        )
                        .await
                        {
                            Ok(reply) => reply,
                            Err(e) => {
                                error!("Command mode failed: {e}");
                                let _ = app_clone.emit(
                                    AppEvent::ErrorOccurred {
                                        message: String::new(),
                                    }
                                    .event_name(),
                                    serde_json::json!({ "message": e.to_string() }),
                                );
                                continue;
                            }
                        }
                    }
                };

                // Inject into the focused application if enabled.
                if delivery.auto_inject && !to_inject.is_empty() {
                    if delivery.delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(delivery.delay_ms))
                            .await;
                    }
                    let inj = injector.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        crate::core::injection::deliver(
                            inj.as_ref(),
                            &to_inject,
                            delivery.use_paste,
                            delivery.settle_ms,
                        )
                    })
                    .await;
                    match result {
                        Ok(Err(e)) => error!("Text injection failed: {e}"),
                        Err(e) => error!("Injection task panicked: {e}"),
                        Ok(Ok(())) => {}
                    }
                }
            } else {
                if let Err(e) = app_clone.emit(
                    "echo://transcript-partial",
                    serde_json::json!({ "text": segment.text }),
                ) {
                    error!("Failed to emit transcript event: {e}");
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    end_recording(app, state.inner()).await
}

/// Stop the capture session and, if wake-word listening is enabled, hand the
/// microphone back to the listener so the next phrase is heard.
pub async fn end_recording(app: AppHandle, state: &AppState) -> Result<()> {
    {
        let mut recording = state.recording.lock().unwrap();
        if !*recording {
            return Ok(());
        }
        *recording = false;
    }

    state.audio.stop_capture();
    app.emit(AppEvent::RecordingStopped.event_name(), AppEvent::RecordingStopped)
        .map_err(|e| EchoError::Plugin(e.to_string()))?;
    info!("Recording stopped");

    crate::commands::wake::rearm(&app);

    Ok(())
}

#[tauri::command]
pub fn is_recording(state: State<'_, AppState>) -> bool {
    *state.recording.lock().unwrap()
}

/// Signals the VAD stage produces for the UI.
pub(crate) enum VadEvent {
    /// Per-chunk RMS of the captured audio, for the live waveform.
    Level(f32),
    SpeechStarted,
    SpeechEnded,
}

/// The VAD gating stage: sits between raw audio capture and the ASR pipeline,
/// forwarding only speech chunks and emitting an empty-vec sentinel at each
/// speech→silence transition so the ASR provider knows an utterance ended.
///
/// Split out of [`begin_recording`] so the pipeline can be driven in tests
/// without a Tauri app — `events` receives exactly what the app forwards to the
/// frontend. The VAD instance belongs entirely to this task (architectural
/// rule 8).
pub(crate) async fn vad_gate<F>(
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    mut vad: Box<dyn Vad>,
    vad_tx: mpsc::Sender<Vec<f32>>,
    events: F,
) where
    F: Fn(VadEvent),
{
    let mut was_speaking = false;

    while let Some(chunk) = audio_rx.recv().await {
        if chunk.is_empty() {
            // Audio error/stop sentinel from the capture layer — flush and exit.
            let _ = vad_tx.send(Vec::new()).await;
            return;
        }

        // Computed before the VAD gate so the visualization stays responsive in
        // near-silence.
        let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
        events(VadEvent::Level(rms));

        if vad.is_speech(&chunk) {
            if !was_speaking {
                was_speaking = true;
                events(VadEvent::SpeechStarted);
            }
            if vad_tx.send(chunk).await.is_err() {
                return;
            }
        } else if was_speaking {
            was_speaking = false;
            events(VadEvent::SpeechEnded);
            if vad_tx.send(Vec::new()).await.is_err() {
                return;
            }
        }
    }

    // Capture closed (recording stopped): flush any trailing utterance.
    let _ = vad_tx.send(Vec::new()).await;
}

/// How one finished transcript should be delivered. Resolved per utterance so
/// a per-app profile — and any settings change — takes effect immediately.
pub(crate) struct Delivery {
    pub auto_inject: bool,
    pub use_paste: bool,
    pub delay_ms: u64,
    /// Dictionary profile to scope replacements to, if the focused app selects one.
    pub dictionary_profile: Option<i64>,
    pub record_history: bool,
    /// How long to let the target read the clipboard before restoring it.
    pub settle_ms: u64,
}

/// Resolve delivery settings for the focused app.
///
/// Global settings are the baseline; a matching per-app profile overrides only
/// the fields it actually sets (a `NULL` column means "inherit"). With no
/// focused app or no profile, this is exactly the old global behaviour.
pub(crate) fn resolve_delivery(
    conn: &rusqlite::Connection,
    focused: Option<&str>,
) -> Delivery {
    use crate::storage::repositories as repo;

    let get = |key: &str| repo::get_setting(conn, key).unwrap_or(None);

    let mut delivery = Delivery {
        auto_inject: get("auto_inject").map(|v| v != "false").unwrap_or(true),
        use_paste: get("injection_method").map(|v| v == "paste").unwrap_or(false),
        delay_ms: get("inject_delay_ms")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0),
        dictionary_profile: None,
        // Defaults on, matching the Privacy toggle's own default.
        record_history: get("history_enabled").map(|v| v != "false").unwrap_or(true),
        settle_ms: get("clipboard_settle_ms")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(crate::core::injection::DEFAULT_SETTLE_MS),
    };

    if let Some(app) = focused {
        if let Ok(Some(profile)) = repo::find_app_profile(conn, app) {
            if let Some(auto) = profile.auto_inject {
                delivery.auto_inject = auto;
            }
            if let Some(method) = profile.injection_method {
                delivery.use_paste = method == "paste";
            }
            delivery.dictionary_profile = profile.profile_id;
        }
    }

    delivery
}

/// Read command-mode settings, falling back to the local-first defaults.
fn command_config(state: &AppState) -> CommandConfig {
    let defaults = CommandConfig::default();
    let conn = state.db.lock().unwrap();
    let get = |key: &str| {
        crate::storage::repositories::get_setting(&conn, key)
            .unwrap_or(None)
            .filter(|s| !s.is_empty())
    };

    CommandConfig {
        enabled: get("command_mode_enabled").map(|v| v == "true").unwrap_or(false),
        prefix: get("command_prefix").unwrap_or(defaults.prefix),
        provider: get("command_llm_provider").unwrap_or(defaults.provider),
        model: get("command_llm_model").unwrap_or(defaults.model),
        endpoint: get("ollama_endpoint").unwrap_or(defaults.endpoint),
    }
}

/// Grab whatever the focused app has selected, then run the instruction against
/// it. With no selection the model answers the instruction on its own.
async fn run_command(
    cfg: &CommandConfig,
    api_key: Option<&str>,
    instruction: &str,
    injector: &Arc<dyn TextInjector>,
) -> Result<String> {
    // Reading the selection synthesizes a copy shortcut and touches the OS
    // clipboard, so it must not run on the async runtime.
    let inj = injector.clone();
    let selection = tokio::task::spawn_blocking(move || {
        crate::core::injection::copy_selection(inj.as_ref())
    })
    .await
    .map_err(|e| EchoError::Injection(format!("selection task panicked: {e}")))??;

    crate::core::command::run(cfg, api_key, instruction, selection.as_deref()).await
}
