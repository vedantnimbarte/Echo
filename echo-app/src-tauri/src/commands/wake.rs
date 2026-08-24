//! Wake-word listening: the idle microphone loop that arms dictation hands-free.
//!
//! Only one CPAL stream exists at a time (`AudioService::start_capture` stops
//! the previous one), so the listener *owns* the microphone while idle and
//! hands it to the recording task on detection. The recording task's own
//! `start_capture` call closes the listener's channel, which is how the loop
//! learns to exit; `end_recording` then calls [`rearm`] to start it again.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::{
    core::{
        events::AppEvent,
        vad::{EnergyVad, SileroVad, Vad},
        wake::{WakePhraseInfo, WakeSpotter, DEFAULT_PHRASE, DEFAULT_THRESHOLD},
    },
    error::Result,
    state::AppState,
    storage::repositories,
};

/// Longest a wake-triggered dictation runs before it is force-stopped, in case
/// the transcript never arrives (ASR stall, no speech after the phrase).
const MAX_UTTERANCE_SECS: u64 = 20;

/// Read the persisted wake settings: (enabled, phrase id, threshold).
fn wake_settings(state: &AppState) -> (bool, String, f32) {
    let conn = state.db.lock().unwrap();
    let enabled = repositories::get_setting(&conn, "wake_word_enabled")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);
    let phrase = repositories::get_setting(&conn, "wake_word_model")
        .unwrap_or(None)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_PHRASE.to_string());
    let threshold = repositories::get_setting(&conn, "wake_word_sensitivity")
        .unwrap_or(None)
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(DEFAULT_THRESHOLD);
    (enabled, phrase, threshold)
}

/// Start the idle listener if wake word is enabled and nothing else holds the
/// microphone. Safe to call repeatedly — a second call while a listener is
/// already running is a no-op.
pub fn rearm(app: &AppHandle) {
    let state = app.state::<AppState>();

    let (enabled, phrase, threshold) = wake_settings(state.inner());
    if !enabled {
        return;
    }
    if *state.recording.lock().unwrap() {
        return;
    }
    if !state.wake_models.is_ready(&phrase) {
        warn!("Wake word enabled but model '{phrase}' is not installed");
        return;
    }

    // ponytail: a compare-exchange is enough here. Two racing rearms could in
    // principle both spawn, but the second's `start_capture` closes the first's
    // channel and that task exits — it self-heals. Promote to a proper handle
    // if listeners ever need to outlive a single capture.
    if state
        .wake_active
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    let model = match state.wake_models.load(&phrase) {
        Ok(m) => m,
        Err(e) => {
            warn!("Failed to load wake model '{phrase}': {e}");
            state.wake_active.store(false, Ordering::SeqCst);
            return;
        }
    };

    let device_name = {
        let conn = state.db.lock().unwrap();
        repositories::get_setting(&conn, "audio_device")
            .unwrap_or(None)
            .filter(|s| !s.is_empty())
    };

    let audio_rx = match state.audio.start_capture(device_name.as_deref()) {
        Ok(rx) => rx,
        Err(e) => {
            warn!("Wake listener could not open the microphone: {e}");
            state.wake_active.store(false, Ordering::SeqCst);
            return;
        }
    };

    let silero = state.silero.clone();
    let active = state.wake_active.clone();
    let app = app.clone();
    tokio::spawn(async move {
        listen(app, audio_rx, model, threshold, phrase, silero).await;
        active.store(false, Ordering::SeqCst);
    });
}

/// The listening loop. Runs until the audio channel closes — which happens when
/// something else claims the microphone, or when [`disarm`] stops capture.
async fn listen(
    app: AppHandle,
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    model: Arc<crate::core::wake::WakeModel>,
    threshold: f32,
    phrase: String,
    silero: Option<Arc<crate::core::vad::SileroModel>>,
) {
    let mut spotter = WakeSpotter::new(model, threshold);

    // Gate the spotter behind the VAD that already runs for dictation: the
    // three-stage ONNX chain only executes on frames containing speech, so an
    // idle room costs a VAD pass rather than a full wake-word inference.
    let mut vad: Box<dyn Vad> = match silero {
        Some(m) => Box::new(SileroVad::new(m)),
        None => Box::new(EnergyVad::new(0.01)),
    };

    info!("Wake listener armed for '{phrase}'");

    while let Some(chunk) = audio_rx.recv().await {
        if chunk.is_empty() {
            break;
        }
        if !vad.is_speech(&chunk) {
            continue;
        }
        let Some(score) = spotter.detect(&chunk) else {
            continue;
        };

        info!("Wake phrase '{phrase}' detected (score {score:.2})");
        let _ = app.emit(
            AppEvent::WakeDetected {
                phrase: String::new(),
                score: 0.0,
            }
            .event_name(),
            serde_json::json!({ "phrase": phrase, "score": score }),
        );

        {
            let state = app.state::<AppState>();
            let conn = state.db.lock().unwrap();
            state.telemetry.record(
                &conn,
                "wake_detected",
                Some(serde_json::json!({ "phrase": phrase, "score": score })),
            );
        }

        start_wake_dictation(app.clone()).await;
        // The recording task has taken the microphone; this loop's channel is
        // now closed. Exit and let `end_recording` rearm us.
        return;
    }
}

/// Begin a one-shot dictation: record until the first final transcript lands
/// (or the watchdog fires), then stop, which rearms the listener.
async fn start_wake_dictation(app: AppHandle) {
    // Register the completion listener *before* starting capture so a fast
    // transcript can't land before we are listening for it.
    let (done_tx, done_rx) = oneshot::channel::<()>();
    let handler = app.once("echo://transcript-final", move |_| {
        let _ = done_tx.send(());
    });

    {
        let state = app.state::<AppState>();
        if let Err(e) =
            crate::commands::recording::begin_recording(app.clone(), state.inner(), None, None)
                .await
        {
            warn!("Wake word could not start recording: {e}");
            app.unlisten(handler);
            return;
        }
    }

    let watchdog = app.clone();
    tokio::spawn(async move {
        if tokio::time::timeout(Duration::from_secs(MAX_UTTERANCE_SECS), done_rx)
            .await
            .is_err()
        {
            warn!("Wake dictation timed out with no transcript; stopping");
            watchdog.unlisten(handler);
        }
        let state = watchdog.state::<AppState>();
        if let Err(e) = crate::commands::recording::end_recording(watchdog.clone(), state.inner())
            .await
        {
            warn!("Failed to stop wake dictation: {e}");
        }
    });
}

/// Stop the listener and release the microphone.
pub fn disarm(state: &AppState) {
    if state.wake_active.load(Ordering::SeqCst) {
        state.audio.stop_capture();
    }
}

/// Turn wake-word listening on or off, persisting the choice.
#[tauri::command]
pub async fn set_wake_word_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        repositories::set_setting(
            &conn,
            "wake_word_enabled",
            if enabled { "true" } else { "false" },
        )?;
    }
    if enabled {
        rearm(&app);
    } else {
        disarm(state.inner());
    }
    Ok(())
}

/// Whether wake-word listening is currently armed.
#[tauri::command]
pub fn wake_word_active(state: State<'_, AppState>) -> bool {
    state.wake_active.load(Ordering::SeqCst)
}

/// The wake phrase catalog with local download status.
#[tauri::command]
pub fn list_wake_words(state: State<'_, AppState>) -> Vec<WakePhraseInfo> {
    state.wake_models.list()
}

/// Whether the selected phrase can actually be loaded right now.
#[tauri::command]
pub fn wake_word_ready(state: State<'_, AppState>) -> bool {
    let (_, phrase, _) = wake_settings(state.inner());
    state.wake_models.is_ready(&phrase)
}

/// Download a wake phrase (and the shared feature models on first use),
/// emitting `echo://wake-model-progress` (bare f32, 0..1).
#[tauri::command]
pub async fn download_wake_model(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    let models = state.wake_models.clone();
    let (tx, mut rx) = mpsc::channel::<f32>(32);
    let progress_app = app.clone();
    tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            let _ = progress_app.emit("echo://wake-model-progress", p);
        }
    });
    models.download(&name, tx).await?;
    let _ = app.emit("echo://wake-model-progress", 1.0_f32);
    Ok(())
}

/// Select the wake phrase, restarting the listener so the change takes effect
/// without a relaunch.
#[tauri::command]
pub async fn set_wake_word_model(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        repositories::set_setting(&conn, "wake_word_model", &name)?;
    }
    restart(&app, state.inner());
    Ok(())
}

/// Set the detection threshold (0.05..0.99 — lower catches more and misfires
/// more) and restart the listener with it.
#[tauri::command]
pub async fn set_wake_word_sensitivity(
    app: AppHandle,
    state: State<'_, AppState>,
    threshold: f32,
) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        repositories::set_setting(
            &conn,
            "wake_word_sensitivity",
            &threshold.clamp(0.05, 0.99).to_string(),
        )?;
    }
    restart(&app, state.inner());
    Ok(())
}

/// Import a user-trained `.onnx` classifier and select it. The shared feature
/// models are fetched first if this is the user's first wake model.
#[tauri::command]
pub async fn import_wake_model(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<()> {
    let models = state.wake_models.clone();

    if !models.shared_ready() {
        let (tx, mut rx) = mpsc::channel::<f32>(32);
        let progress_app = app.clone();
        tokio::spawn(async move {
            while let Some(p) = rx.recv().await {
                let _ = progress_app.emit("echo://wake-model-progress", p);
            }
        });
        models.ensure_shared(tx).await?;
    }

    models.import_custom(std::path::Path::new(&path)).await?;

    {
        let conn = state.db.lock().unwrap();
        repositories::set_setting(&conn, "wake_word_model", crate::core::wake::CUSTOM_ID)?;
    }
    restart(&app, state.inner());
    Ok(())
}

/// Stop and start the listener so a settings change is picked up. A no-op when
/// wake word is disabled or a recording is in progress.
///
/// The old listener exits asynchronously (it notices its audio channel closed),
/// so wait for it to clear the active flag before rearming — otherwise the new
/// listener loses the compare-exchange and never starts.
fn restart(app: &AppHandle, state: &AppState) {
    disarm(state);
    let app = app.clone();
    tokio::spawn(async move {
        for _ in 0..50 {
            if !app.state::<AppState>().wake_active.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        rearm(&app);
    });
}

/// Report why the wake word cannot run, for the settings UI to display.
#[tauri::command]
pub fn wake_word_status(state: State<'_, AppState>) -> Result<String> {
    let (enabled, phrase, _) = wake_settings(state.inner());
    Ok(if !enabled {
        "disabled".into()
    } else if !state.wake_models.is_ready(&phrase) {
        "model-missing".into()
    } else if state.wake_active.load(Ordering::SeqCst) {
        "listening".into()
    } else {
        "idle".into()
    })
}
