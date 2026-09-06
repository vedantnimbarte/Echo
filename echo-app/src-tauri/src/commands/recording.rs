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

    // Which app is focused *now*, used only to prime the decoder and to decide
    // whether partials may be typed. Delivery resolves focus again when the
    // transcript is ready — see `core::asr::prompt` for why the two differ.
    let focused_at_start = tokio::task::spawn_blocking(crate::core::appcontext::foreground_app)
        .await
        .ok()
        .flatten();
    let start_delivery = {
        let conn = state.db.lock().unwrap();
        resolve_delivery(&conn, focused_at_start.as_deref())
    };
    state
        .prompt_ctx
        .set_app(focused_at_start.clone(), start_delivery.dictionary_profile);

    // Keep each finished utterance so a retry can re-decode it on a stronger
    // model rather than asking the user to say it again.
    let retain = {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::get_setting(&conn, "retry_enabled")
            .unwrap_or(None)
            .map(|v| v != "false")
            .unwrap_or(true)
    };
    let asr_rx = if retain {
        let (keep_tx, keep_rx) = mpsc::channel::<Vec<f32>>(256);
        let slot = state.last_utterance.clone();
        tokio::spawn(retain_utterances(vad_rx, keep_tx, slot));
        keep_rx
    } else {
        *state.last_utterance.lock().unwrap() = None;
        vad_rx
    };

    // Never type a dictated password into the box that is masking it. Checked
    // again per transcript below, because focus can move while you talk; this
    // one exists to keep partials out of a field that is already secure.
    let secure_guard = {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::get_setting(&conn, "block_secure_fields")
            .unwrap_or(None)
            .map(|v| v != "false")
            .unwrap_or(true)
    };

    let asr = state.asr.clone();
    let lang = language.clone();

    // Partial injection needs a provider that actually streams: under the
    // buffered default there is one segment per utterance, so it would carry
    // the risk of rewriting another app's text with none of the benefit. It is
    // also keystrokes-only — paste-mode partials would thrash the clipboard
    // dozens of times a sentence and lose whatever the user had on it.
    let stream_partials = start_delivery.stream_partials
        && start_delivery.auto_inject
        // Paste-mode partials would thrash the clipboard; "auto" is allowed
        // because a partial is short, which is exactly what it types.
        && !start_delivery.use_paste("")
        && asr.supports_streaming().await
        // Command mode turns an utterance into an instruction for a model, and
        // an instruction is not text the user wants in their document even
        // briefly. Streaming would type "make this more formal" into it and
        // then take it back — and if the model call fails, not take it back.
        && !command_config(state).enabled
        // A masked field must not receive partials either — those are the same
        // characters, just delivered in more pieces.
        && !(secure_guard
            && tokio::task::spawn_blocking(crate::core::field::focused_field)
                .await
                .map(|kind| kind.is_secure())
                .unwrap_or(false));

    // Producing partials from a local model means re-decoding the utterance as
    // it grows, so the provider is told whether anybody is actually reading
    // them before the session starts. With live text off, nothing changes.
    asr.set_partials_wanted(stream_partials).await;

    tokio::spawn(async move {
        if let Err(e) = asr.transcribe_stream(asr_rx, transcript_tx, lang.as_deref()).await {
            error!("ASR stream error: {e}");
        }
    });

    // Capture shared handles before the spawn — `state` is not 'static.
    let dictionary = state.dictionary.clone();
    let injector = state.injector.clone();
    let prompt_ctx = state.prompt_ctx.clone();
    // The configured language, as a fallback for formatting when the decoder
    // does not report one.
    let lang_for_format = language.clone();
    let scratch_enabled = {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::get_setting(&conn, "scratch_that_enabled")
            .unwrap_or(None)
            .map(|v| v == "true")
            .unwrap_or(false)
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
        // Partial text this process has typed into the focused app and not yet
        // replaced. Empty whenever nothing is streamed, which is the only state
        // in which a backspace count would be a guess.
        let mut shown = String::new();
        // Streaming can be abandoned mid-utterance and is re-armed for the next
        // one — a single failure should not silence partials for the session.
        let mut stream_live = stream_partials;

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

                // The secure-field check comes before *everything* — before
                // the dictionary, before History, before injection. A password
                // spoken into a masked box must not be typed there and must
                // not be written to disk on the way past.
                if secure_guard
                    && tokio::task::spawn_blocking(crate::core::field::focused_field)
                        .await
                        .map(|kind| kind.is_secure())
                        .unwrap_or(false)
                {
                    // Anything already streamed into the field goes back out.
                    if !shown.is_empty() {
                        let inj = injector.clone();
                        let typed = std::mem::take(&mut shown);
                        let _ = tokio::task::spawn_blocking(move || {
                            crate::core::injection::rewrite(inj.as_ref(), &typed, "")
                        })
                        .await;
                    }
                    tracing::warn!("Focused field is a password box; transcript discarded");
                    let _ = app_clone.emit(
                        AppEvent::ErrorOccurred { message: String::new() }.event_name(),
                        serde_json::json!({
                            "message": "That looked like a password field, so Echo didn't type \
                                        the transcript or keep it. Turn the guard off in \
                                        Settings → Output if you meant to dictate here."
                        }),
                    );
                    continue;
                }

                // Dictionary first, then formatting: a dictionary rule is the
                // user's own correction and should be able to produce text the
                // formatter then spaces and capitalises properly.
                let processed = dictionary
                    .read()
                    .await
                    .process_for(&segment.text, delivery.dictionary_profile);
                // The decoder's own answer wins over the configured language:
                // with auto-detect on, it is the only one that knows what was
                // actually spoken.
                let spoken = segment
                    .language
                    .as_deref()
                    .or(lang_for_format.as_deref());
                let processed =
                    crate::core::format::apply(&processed, delivery.format, spoken);

                // "Scratch that" is a correction, not dictation: take back the
                // last delivery instead of typing the words. Checked before
                // history so the log stays a record of what was said and kept.
                if scratch_enabled && crate::core::undo::is_scratch_phrase(&processed) {
                    // The phrase itself may already be on screen from streaming;
                    // remove it before undoing what came before it.
                    if !shown.is_empty() {
                        let inj = injector.clone();
                        let typed = std::mem::take(&mut shown);
                        let _ = tokio::task::spawn_blocking(move || {
                            crate::core::injection::rewrite(inj.as_ref(), &typed, "")
                        })
                        .await;
                    }
                    prompt_ctx.clear_previous();
                    match crate::commands::fixup::undo_delivery(&app_clone).await {
                        Ok(true) => info!("Scratch that: last insert undone"),
                        Ok(false) => info!("Scratch that: nothing to undo"),
                        Err(e) => error!("Scratch that failed: {e}"),
                    }
                    continue;
                }

                // Carry this sentence into the next utterance's decoder prompt.
                prompt_ctx.set_previous(&processed);

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
                    info!(
                        focused = focused.as_deref().unwrap_or("<unknown>"),
                        chars = to_inject.chars().count(),
                        method = if delivery.use_paste(&to_inject) { "paste" } else { "keystrokes" },
                        settle_ms = delivery.settle_ms,
                        "Injecting transcript"
                    );
                    if delivery.delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(delivery.delay_ms))
                            .await;
                    }
                    let inj = injector.clone();
                    // When partials were streamed, the target already holds an
                    // approximation of this sentence; only its differing tail
                    // is rewritten rather than the whole line being retyped.
                    let typed = std::mem::take(&mut shown);
                    stream_live = stream_partials;
                    let use_paste = delivery.use_paste(&to_inject);
                    let text = to_inject.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        if typed.is_empty() {
                            crate::core::injection::deliver(
                                inj.as_ref(),
                                &text,
                                use_paste,
                                delivery.settle_ms,
                            )
                        } else {
                            crate::core::injection::finish_streamed(inj.as_ref(), &typed, &text)
                                .map(|_| ())
                        }
                    })
                    .await;
                    match result {
                        Ok(Err(e)) => {
                            error!("Text injection failed: {e}");
                            rescue_to_clipboard(&app_clone, &to_inject, &e.to_string());
                        }
                        Err(e) => error!("Injection task panicked: {e}"),
                        Ok(Ok(())) => {
                            info!("Transcript injected");
                            let state = app_clone.state::<AppState>();
                            *state.last_delivery.lock().unwrap() =
                                Some(crate::core::undo::LastDelivery {
                                    text: to_inject,
                                    used_paste: use_paste,
                                });
                        }
                    }
                } else {
                    // Silently dropping the transcript here is the one outcome
                    // that looks identical to a broken pipeline from outside.
                    info!(
                        auto_inject = delivery.auto_inject,
                        empty = to_inject.is_empty(),
                        "Transcript not injected"
                    );
                }
            } else {
                if let Err(e) = app_clone.emit(
                    "echo://transcript-partial",
                    serde_json::json!({ "text": segment.text }),
                ) {
                    error!("Failed to emit transcript event: {e}");
                }

                // Type the partial into the focused app, so words appear while
                // they are being spoken instead of in one burst at the end.
                //
                // ponytail: `shown` is only ever text this process typed during
                // this utterance, and only its own characters are ever deleted.
                // What it cannot see is the user typing into the same field
                // mid-utterance, which no API exposes — so the backspaces would
                // land on their text. That is why this is opt-in per app and
                // off by default; the ceiling is real, not a rough edge.
                if stream_live {
                    let inj = injector.clone();
                    let typed = shown.clone();
                    let next = segment.text.clone();
                    match tokio::task::spawn_blocking(move || {
                        crate::core::injection::rewrite(inj.as_ref(), &typed, &next)
                    })
                    .await
                    {
                        Ok(Ok(())) => shown = segment.text,
                        // A failed rewrite leaves the field in a state we can
                        // no longer describe, so stop streaming this utterance.
                        // `shown` keeps its last known-good value: the final
                        // transcript then repairs the tail from there instead
                        // of appending a second copy of the sentence.
                        Ok(Err(e)) => {
                            error!("Partial injection failed; delivering the final only: {e}");
                            stream_live = false;
                        }
                        Err(e) => {
                            error!("Partial injection task panicked: {e}");
                            stream_live = false;
                        }
                    }
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

    // People dictate in bursts. Keeping the device open for a few seconds
    // means the next sentence starts instantly instead of paying the
    // device-open cost again — and the pre-roll it collects means a word begun
    // before the hotkey registers is still captured.
    //
    // It is opt-out because an open microphone lights the OS "in use"
    // indicator, and that is the user's call to make, not ours.
    let warm = {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::get_setting(&conn, "warm_mic")
            .unwrap_or(None)
            .map(|v| v != "false")
            .unwrap_or(true)
    };
    if warm {
        let device = {
            let conn = state.db.lock().unwrap();
            crate::storage::repositories::get_setting(&conn, "audio_device")
                .unwrap_or(None)
                .filter(|s| !s.is_empty())
        };
        state.audio.stop_capture_warm(device.as_deref());
    } else {
        state.audio.stop_capture();
    }

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

/// Open the microphone before it is needed, so the recording that follows
/// starts instantly and keeps the half-second before the key press.
///
/// Called on the interactions that reliably precede a dictation — pressing the
/// pill, arming the hotkey. Best-effort by design: it returns nothing and fails
/// silently, because a failed warm-up must never stop a recording from starting
/// the ordinary way.
#[tauri::command]
pub fn warm_microphone(state: State<'_, AppState>) {
    let (enabled, device) = {
        let conn = state.db.lock().unwrap();
        let get = |k: &str| crate::storage::repositories::get_setting(&conn, k).unwrap_or(None);
        let enabled = get("warm_mic").map(|v| v != "false").unwrap_or(true);
        (enabled, get("audio_device").filter(|s| !s.is_empty()))
    };
    if !enabled || *state.recording.lock().unwrap() {
        return;
    }
    state.audio.warm(device.as_deref());
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

/// Cap on retained audio: 30 seconds at 16 kHz mono f32, about 1.9 MB.
///
/// Long enough for anything said in one breath, which is what a retry is for.
const MAX_RETAINED_SAMPLES: usize = 30 * 16_000;

/// Forward the VAD's output to the ASR unchanged, keeping a copy of the most
/// recent complete utterance so it can be re-decoded without being re-spoken.
///
/// A passthrough rather than a change to the provider trait: the buffer is
/// already assembled here, and every provider — including one from a plugin —
/// gets the behaviour without implementing anything.
///
/// An utterance longer than [`MAX_RETAINED_SAMPLES`] is dropped rather than
/// truncated. Retrying the first thirty seconds of a longer sentence would
/// silently return a shorter transcript than the one it replaced, which looks
/// exactly like the retry itself failing.
pub(crate) async fn retain_utterances(
    mut rx: mpsc::Receiver<Vec<f32>>,
    tx: mpsc::Sender<Vec<f32>>,
    slot: Arc<std::sync::Mutex<Option<Vec<f32>>>>,
) {
    let mut current: Vec<f32> = Vec::new();
    let mut overflowed = false;

    while let Some(chunk) = rx.recv().await {
        if chunk.is_empty() {
            // Utterance boundary: publish what was collected, or clear the slot
            // if it was too long to keep honestly.
            let finished = std::mem::take(&mut current);
            if !finished.is_empty() || overflowed {
                *slot.lock().unwrap() = (!overflowed).then_some(finished);
            }
            overflowed = false;
        } else if !overflowed {
            if current.len() + chunk.len() > MAX_RETAINED_SAMPLES {
                overflowed = true;
                current = Vec::new();
            } else {
                current.extend_from_slice(&chunk);
            }
        }

        if tx.send(chunk).await.is_err() {
            return;
        }
    }
}

/// Injection failed, so put the words somewhere the user can still reach them.
///
/// The transcript is already in History, but nobody watching their cursor not
/// move thinks to go and look there. The clipboard is the one place every app
/// can paste from, and the error carries the reason the typing failed.
fn rescue_to_clipboard(app: &AppHandle, text: &str, reason: &str) {
    let saved = arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_owned()));
    let message = match saved {
        Ok(()) => format!("Couldn't type that ({reason}). It's on your clipboard — paste it."),
        Err(e) => format!("Couldn't type that ({reason}), and the clipboard refused it too: {e}"),
    };
    let _ = app.emit(
        AppEvent::ErrorOccurred { message: String::new() }.event_name(),
        serde_json::json!({ "message": message }),
    );
}

impl Delivery {
    /// Whether this text goes in by paste, honouring an explicit choice and
    /// letting `"auto"` decide from the text itself.
    pub(crate) fn use_paste(&self, text: &str) -> bool {
        crate::core::injection::use_paste_for(self.method.as_deref(), text)
    }
}

/// How one finished transcript should be delivered. Resolved per utterance so
/// a per-app profile — and any settings change — takes effect immediately.
pub(crate) struct Delivery {
    pub auto_inject: bool,
    /// Raw `injection_method`: `"type"`, `"paste"` or `"auto"`. Kept unresolved
    /// because `"auto"` decides from the text, which does not exist yet when
    /// settings are read.
    pub method: Option<String>,
    /// Type partial transcripts as they arrive rather than waiting for the
    /// finished sentence. Off unless the app's profile asks for it.
    pub stream_partials: bool,
    /// Which formatting stages run on the finished transcript.
    pub format: crate::core::format::FormatOptions,
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
        // Unset stays "type", which is what it has always meant. "auto" is an
        // explicit choice, not a silent change of behaviour for everyone.
        method: Some(get("injection_method").unwrap_or_else(|| "type".into())),
        // Off unless asked for: this one types into another app's text field
        // while the user is still speaking into it.
        stream_partials: get("stream_partials").map(|v| v == "true").unwrap_or(false),
        // Spoken punctuation is off by default: it takes words out of the
        // language ("period" stops being usable as a noun), and that is the
        // user's trade to opt into. Tidy and numbers only reshape what is
        // already there, so they default on.
        format: crate::core::format::FormatOptions {
            spoken_punctuation: get("spoken_punctuation").map(|v| v == "true").unwrap_or(false),
            numbers: get("format_numbers").map(|v| v != "false").unwrap_or(true),
            tidy: get("format_tidy").map(|v| v != "false").unwrap_or(true),
        },
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
                delivery.method = Some(method);
            }
            if let Some(stream) = profile.stream_partials {
                delivery.stream_partials = stream;
            }
            // A per-app profile switches the whole pass, not individual
            // stages: "leave my terminal alone" is one decision.
            if profile.formatting == Some(false) {
                delivery.format = Default::default();
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
