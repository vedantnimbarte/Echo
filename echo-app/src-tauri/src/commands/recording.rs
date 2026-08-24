use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
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
            Some(serde_json::json!({ "provider": provider })),
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

    let mut audio_rx = state.audio.start_capture(device_name.as_deref())?;
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
        let mut vad: Box<dyn Vad> = match silero_model {
            Some(model) if vad_engine != "energy" => Box::new(SileroVad::new(model)),
            _ => Box::new(EnergyVad::new(0.01)),
        };
        let mut was_speaking = false;
        while let Some(chunk) = audio_rx.recv().await {
            if chunk.is_empty() {
                // Audio error/stop sentinel from the capture layer — flush and exit.
                let _ = vad_tx.send(Vec::new()).await;
                break;
            }
            // Emit a per-chunk RMS level so the floating pill can render a live
            // waveform that reflects the audio actually being captured. Computed
            // before the VAD gate so the visualization stays responsive in near-
            // silence. Payload is the bare f32 (read as `event.payload` in JS).
            let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
            let _ = level_app.emit("echo://audio-level", rms);
            if vad.is_speech(&chunk) {
                if !was_speaking {
                    // Rising edge: the user just started talking. Drives the
                    // pill's listening state in voice-activated mode.
                    was_speaking = true;
                    let _ = level_app.emit("echo://speech-started", ());
                }
                if vad_tx.send(chunk).await.is_err() {
                    break;
                }
            } else if was_speaking {
                // Falling edge: speech ended — flush the utterance to ASR and
                // tell the pill we're now transcribing.
                was_speaking = false;
                let _ = level_app.emit("echo://speech-ended", ());
                if vad_tx.send(Vec::new()).await.is_err() {
                    break;
                }
            }
        }
        // Capture closed (recording stopped): flush any trailing utterance.
        let _ = vad_tx.send(Vec::new()).await;
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
    let (auto_inject, inject_delay_ms, use_paste) = {
        let conn = state.db.lock().unwrap();
        let auto = crate::storage::repositories::get_setting(&conn, "auto_inject")
            .unwrap_or(None)
            .map(|v| v != "false")
            .unwrap_or(true);
        let delay = crate::storage::repositories::get_setting(&conn, "inject_delay_ms")
            .unwrap_or(None)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let paste = crate::storage::repositories::get_setting(&conn, "injection_method")
            .unwrap_or(None)
            .map(|v| v == "paste")
            .unwrap_or(false);
        (auto, delay, paste)
    };

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
                // Apply dictionary replacements to the final transcript.
                let processed = dictionary.read().await.process(&segment.text);
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
                if auto_inject && !to_inject.is_empty() {
                    if inject_delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(inject_delay_ms)).await;
                    }
                    let inj = injector.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        crate::core::injection::deliver(inj.as_ref(), &to_inject, use_paste)
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
