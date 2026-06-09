use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::{
    core::{
        asr::TranscriptSegment,
        events::AppEvent,
        vad::EnergyVad,
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

    let mut audio_rx = state.audio.start_capture(device_name.as_deref())?;
    let (transcript_tx, mut transcript_rx) = mpsc::channel::<TranscriptSegment>(32);

    // VAD gating stage: sits between raw audio capture and the ASR pipeline.
    // It forwards only speech chunks and emits an empty-vec sentinel at each
    // speech→silence transition so the ASR provider knows an utterance ended.
    // The VAD instance belongs entirely to this task (see architectural rule 8).
    let (vad_tx, vad_rx) = mpsc::channel::<Vec<f32>>(256);
    tokio::spawn(async move {
        let mut vad = EnergyVad::new(0.01);
        let mut was_speaking = false;
        while let Some(chunk) = audio_rx.recv().await {
            if chunk.is_empty() {
                // Audio error/stop sentinel from the capture layer — flush and exit.
                let _ = vad_tx.send(Vec::new()).await;
                break;
            }
            if vad.is_speech(&chunk) {
                was_speaking = true;
                if vad_tx.send(chunk).await.is_err() {
                    break;
                }
            } else if was_speaking {
                // Speech just ended: signal end of utterance.
                was_speaking = false;
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
    let (auto_inject, inject_delay_ms) = {
        let conn = state.db.lock().unwrap();
        let auto = crate::storage::repositories::get_setting(&conn, "auto_inject")
            .unwrap_or(None)
            .map(|v| v != "false")
            .unwrap_or(true);
        let delay = crate::storage::repositories::get_setting(&conn, "inject_delay_ms")
            .unwrap_or(None)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        (auto, delay)
    };

    let app_clone = app.clone();
    tokio::spawn(async move {
        while let Some(segment) = transcript_rx.recv().await {
            if segment.is_final {
                // Apply dictionary replacements to the final transcript.
                let processed = dictionary.read().await.process(&segment.text);
                let event = AppEvent::TranscriptFinal {
                    text: processed.clone(),
                    language: segment.language,
                };
                if let Err(e) = app_clone.emit(event.event_name(), &event) {
                    error!("Failed to emit transcript event: {e}");
                }

                // Inject into the focused application if enabled.
                if auto_inject && !processed.is_empty() {
                    if inject_delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(inject_delay_ms)).await;
                    }
                    let inj = injector.clone();
                    let result =
                        tokio::task::spawn_blocking(move || inj.inject_text(&processed)).await;
                    match result {
                        Ok(Err(e)) => error!("Text injection failed: {e}"),
                        Err(e) => error!("Injection task panicked: {e}"),
                        Ok(Ok(())) => {}
                    }
                }
            } else {
                let event = AppEvent::TranscriptPartial { text: segment.text };
                if let Err(e) = app_clone.emit(event.event_name(), &event) {
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

    Ok(())
}

#[tauri::command]
pub fn is_recording(state: State<'_, AppState>) -> bool {
    *state.recording.lock().unwrap()
}
