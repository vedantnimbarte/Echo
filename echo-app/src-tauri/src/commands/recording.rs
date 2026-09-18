use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::core::lock::LockLive;
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
    // Hold and toggle end on the hotkey; only voice-activated mode has
    // nothing but a pause to say an utterance is over.
    let end_on_pause = {
        let conn = state.db.lock_live();
        crate::storage::repositories::get_setting(&conn, "recording_mode")
            .unwrap_or(None)
            .as_deref()
            == Some("auto")
    };
    begin_recording(app, state.inner(), device_name, language, end_on_pause).await
}

/// Start a capture session.
///
/// Split out of the command so the wake-word listener can start recording
/// directly, without bouncing a request through the frontend.
///
/// `end_on_pause` decides whether a long pause closes the utterance (see
/// [`vad_gate`]) or only the end of capture does.
pub async fn begin_recording(
    app: AppHandle,
    state: &AppState,
    device_name: Option<String>,
    language: Option<String>,
    end_on_pause: bool,
) -> Result<()> {
    {
        let mut recording = state.recording.lock_live();
        if *recording {
            return Ok(());
        }
        *recording = true;
    }

    app.emit(
        AppEvent::RecordingStarted.event_name(),
        AppEvent::RecordingStarted,
    )
    .map_err(|e| EchoError::Plugin(e.to_string()))?;
    info!("Recording started");

    let provider = state.asr.active_provider_name().await;
    {
        let conn = state.db.lock_live();
        state.telemetry.record(
            &conn,
            "recording_started",
            Some(serde_json::json!({ "provider": provider.clone() })),
        );
    }

    // Fall back to the device configured in Settings when the caller doesn't
    // pin one (the floating pill triggers recording without knowing the device).
    let device_name = device_name.filter(|s| !s.is_empty()).or_else(|| {
        let conn = state.db.lock_live();
        crate::storage::repositories::get_setting(&conn, "audio_device")
            .unwrap_or(None)
            .filter(|s| !s.is_empty())
    });

    // Same fallback shape as the device above: the pill triggers recording
    // without knowing the configured language. "auto" means let the model
    // detect it, which is what the providers already do for None.
    let language = language.filter(|s| !s.is_empty()).or_else(|| {
        let conn = state.db.lock_live();
        crate::storage::repositories::get_setting(&conn, "language")
            .unwrap_or(None)
            .filter(|s| !s.is_empty() && s != "auto")
    });

    let audio_rx = state.audio.start_capture(device_name.as_deref())?;
    let (transcript_tx, mut transcript_rx) = mpsc::channel::<TranscriptSegment>(32);

    // Plugins, read once for the session: which capabilities anyone offers
    // decides which stages exist at all. A plugin enabled mid-recording joins
    // at the next one; one disabled mid-recording stops being called at once,
    // because both stages re-read the loaded set as they go.
    let (audio_plugins, output_plugins) = {
        let plugins = state.plugins.lock_live().plugins();
        (
            plugins.iter().any(|p| p.plugin().as_audio().is_some()),
            plugins.iter().any(|p| p.plugin().as_output().is_some()),
        )
    };
    let loaded_plugins = {
        let app = app.clone();
        move || app.state::<AppState>().plugins.lock_live().plugins()
    };

    // Audio plugins sit between capture and the VAD, so what they do to the
    // signal is what speech detection and the decoder hear. Without one, the
    // stage is not inserted and capture feeds the VAD directly, as it always has.
    let audio_rx = if audio_plugins {
        let (tx, rx) = mpsc::channel::<Vec<f32>>(256);
        tokio::spawn(crate::core::plugins::dispatch::audio_stage(
            audio_rx,
            tx,
            loaded_plugins.clone(),
        ));
        rx
    } else {
        audio_rx
    };
    // Output plugins get their own thread; see `dispatch::output_worker`.
    let output_tx =
        output_plugins.then(|| crate::core::plugins::dispatch::output_worker(loaded_plugins));

    // VAD gating stage: sits between raw audio capture and the ASR pipeline.
    // It forwards only speech chunks and emits an empty-vec sentinel at each
    // speech→silence transition so the ASR provider knows an utterance ended.
    // The VAD instance belongs entirely to this task (see architectural rule 8).
    // Pick the VAD engine: Silero (neural, ignores keyboard/fan noise) when its
    // model loaded, else the energy fallback. `vad_engine` setting can force it.
    let silero_model = state.silero.clone();
    let vad_engine = {
        let conn = state.db.lock_live();
        crate::storage::repositories::get_setting(&conn, "vad_engine")
            .unwrap_or(None)
            .unwrap_or_else(|| "silero".into())
    };

    let (vad_tx, vad_rx) = mpsc::channel::<Vec<f32>>(256);
    let level_app = app.clone();
    // How long the microphone actually heard speech, milliseconds, since the
    // last transcript was written. Words per minute needs a denominator, and
    // the only honest one is speech time — not how long the hotkey was held,
    // which includes every pause while you thought about the next sentence.
    //
    // ponytail: measured from the VAD's own rising and falling edges rather
    // than by counting samples, so it is wall-clock over live capture. Good to
    // a frame or two, which is well inside what a words-per-minute figure can
    // claim. Counting forwarded samples would be exact, but only the retry
    // path sees them, and that path is optional.
    let spoken_ms = Arc::new(AtomicU64::new(0));
    let spoken_for_vad = spoken_ms.clone();
    // Loudest chunk this session, as f32 bits. Only read when a session ends
    // with nothing to show for it, to tell "the microphone is not working" from
    // "you did not say anything".
    let peak_level = Arc::new(AtomicU32::new(0));
    let peak_for_vad = peak_level.clone();
    let since_for_vad: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

    tokio::spawn(async move {
        let vad: Box<dyn Vad> = match silero_model {
            Some(model) if vad_engine != "energy" => Box::new(SileroVad::new(model)),
            _ => Box::new(EnergyVad::new(0.01)),
        };
        vad_gate(
            audio_rx,
            vad,
            vad_tx,
            end_on_pause,
            move |event| match event {
                // Payload is the bare f32 (read as `event.payload` in JS).
                VadEvent::Level(rms) => {
                    peak_for_vad.fetch_max(rms.to_bits(), Ordering::Relaxed);
                    let _ = level_app.emit("echo://audio-level", rms);
                }
                // Rising edge: drives the pill's listening state in voice-activated mode.
                VadEvent::SpeechStarted => {
                    *since_for_vad.lock_live() = Some(Instant::now());
                    let _ = level_app.emit("echo://speech-started", ());
                }
                // Falling edge: the pill switches to "transcribing".
                VadEvent::SpeechEnded => {
                    if let Some(started) = since_for_vad.lock_live().take() {
                        spoken_for_vad
                            .fetch_add(started.elapsed().as_millis() as u64, Ordering::Relaxed);
                    }
                    let _ = level_app.emit("echo://speech-ended", ());
                }
            },
        )
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
        let conn = state.db.lock_live();
        resolve_delivery(&conn, focused_at_start.as_deref())
    };
    state
        .prompt_ctx
        .set_app(focused_at_start.clone(), start_delivery.dictionary_profile);

    // Keep each finished utterance so a retry can re-decode it on a stronger
    // model rather than asking the user to say it again.
    let retain = {
        let conn = state.db.lock_live();
        crate::storage::repositories::get_setting(&conn, "retry_enabled")
            .unwrap_or(None)
            .map(|v| v != "false")
            .unwrap_or(true)
    };
    // The crash spool runs whether or not retry does: retry is a convenience
    // the user may switch off, and this is the difference between losing a
    // sentence and losing nothing.
    let spool = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|dir| crate::core::spool::Spool::create(&dir));
    if !retain {
        *state.last_utterance.lock_live() = None;
    }
    let asr_rx = {
        let (keep_tx, keep_rx) = mpsc::channel::<Vec<f32>>(256);
        let slot = retain.then(|| state.last_utterance.clone());
        tokio::spawn(retain_utterances(vad_rx, keep_tx, slot, spool));
        keep_rx
    };

    // Never type a dictated password into the box that is masking it. Checked
    // again per transcript below, because focus can move while you talk; this
    // one exists to keep partials out of a field that is already secure.
    let secure_guard = {
        let conn = state.db.lock_live();
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
        // A styled app would watch its sentence typed as spoken and then
        // retyped in the style, which is the flicker live text exists to avoid.
        && !(start_delivery.style.is_some() && {
            let conn = state.db.lock_live();
            crate::storage::repositories::get_setting(&conn, "app_style_enabled")
                .unwrap_or(None)
                .is_some_and(|v| v == "true")
        })
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
        if let Err(e) = asr
            .transcribe_stream(asr_rx, transcript_tx, lang.as_deref())
            .await
        {
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
        let conn = state.db.lock_live();
        crate::storage::repositories::get_setting(&conn, "scratch_that_enabled")
            .unwrap_or(None)
            .map(|v| v == "true")
            .unwrap_or(false)
    };

    // Command mode: a transcript opening with the prefix word is an instruction
    // for the LLM rather than text to type. Read once per session so a settings
    // change applies to the next recording without a restart.
    let command_cfg = command_config(state);
    // The LLM cleanup pass is separate from command mode and off by default:
    // it changes the words you said, which every other stage is careful not to.
    let auto_edit_llm = {
        let conn = state.db.lock_live();
        crate::storage::repositories::get_setting(&conn, "auto_edit_llm")
            .unwrap_or(None)
            .map(|v| v == "true")
            .unwrap_or(false)
    };
    // Per-app styles: off unless switched on here *and* the focused app's
    // profile has a style. Same reasoning as the cleanup pass above — this
    // rewrites your words, and costs a model call on every utterance.
    let style_enabled = {
        let conn = state.db.lock_live();
        crate::storage::repositories::get_setting(&conn, "app_style_enabled")
            .unwrap_or(None)
            .is_some_and(|v| v == "true")
    };
    let command_key = if (command_cfg.enabled || auto_edit_llm || style_enabled)
        && command_cfg.provider == "openai"
    {
        crate::storage::keychain::get_api_key("openai").unwrap_or(None)
    } else {
        None
    };

    let app_clone = app.clone();
    let quiet_app = app.clone();
    let peak_for_report = peak_level.clone();
    tokio::spawn(async move {
        let session_started = Instant::now();
        // Whether anything was actually delivered. A session that ends with
        // this false and a silent microphone is the failure this whole pipeline
        // used to keep to itself: the audio was dropped, a debug line was
        // written, and the screen said nothing at all.
        let mut delivered = false;
        // Partial text this process has typed into the focused app and not yet
        // replaced. Empty whenever nothing is streamed, which is the only state
        // in which a backspace count would be a guess.
        let mut shown = String::new();
        // Streaming can be abandoned mid-utterance and is re-armed for the next
        // one — a single failure should not silence partials for the session.
        let mut stream_live = stream_partials;

        while let Some(segment) = transcript_rx.recv().await {
            if segment.is_final {
                delivered |= !segment.text.trim().is_empty();
                // Which app is focused decides how the text is delivered and
                // which dictionary entries apply, so resolve it now rather than
                // at recording start — focus can move while you talk.
                // The macOS/Linux lookups shell out, so keep them off the runtime.
                let focused = tokio::task::spawn_blocking(crate::core::appcontext::foreground_app)
                    .await
                    .ok()
                    .flatten();

                let (delivery, snippets) = {
                    let state = app_clone.state::<AppState>();
                    let conn = state.db.lock_live();
                    (
                        resolve_delivery(&conn, focused.as_deref()),
                        crate::storage::repositories::list_snippets(&conn).unwrap_or_default(),
                    )
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
                        AppEvent::ErrorOccurred {
                            message: String::new(),
                        }
                        .event_name(),
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
                let corrected = dictionary
                    .read()
                    .await
                    .process_for(&segment.text, delivery.dictionary_profile);
                // The decoder's own answer wins over the configured language:
                // with auto-detect on, it is the only one that knows what was
                // actually spoken.
                let spoken = segment.language.as_deref().or(lang_for_format.as_deref());
                let processed = crate::core::format::apply(&corrected, delivery.format, spoken);

                // "Scratch that" is a correction, not dictation: take back the
                // last delivery instead of typing the words. Checked before
                // history so the log stays a record of what was said and kept.
                if scratch_enabled && crate::core::undo::is_scratch_phrase(&processed, spoken) {
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

                // From here the order is: snippet → LLM cleanup → command mode
                // or per-app style → History → injection.
                //
                // A voice snippet replaces the whole utterance, and it is
                // decided first: after the dictionary and formatting, so a
                // trigger matches whichever form of it the user typed, and
                // before any model sees the text, so the body is never put
                // through anything. The number formatter must not touch the
                // digits in an address, and a style must not "improve" a
                // signature. A match consumes the whole utterance (see
                // `core::snippets` for why), so there is no partial span left
                // for a later stage to rewrite around — skipping them is the
                // protection. It also means a style never gets the chance to
                // reword a trigger into something that no longer matches.
                let snippet = crate::core::snippets::expand(&snippets, &[&corrected, &processed])
                    .map(str::to_owned);

                // Emit the transcript fields directly (not the tagged AppEvent
                // wrapper) so the frontend reads `event.payload.text` naturally.
                if let Err(e) = app_clone.emit(
                    "echo://transcript-final",
                    serde_json::json!({ "text": processed, "language": segment.language }),
                ) {
                    error!("Failed to emit transcript event: {e}");
                }

                // What History keeps. Normally the words as said: the LLM
                // cleanup below can change words and command mode replaces
                // them with a model's reply, and History is a log of what you
                // said, not of what a model made of it. A snippet and a style
                // are the exceptions, because each is a form the user chose
                // for their words: the row is what was delivered, so
                // re-inserting it from History puts back what landed. The
                // pre-style text is not kept beside it — a row holds one text,
                // and the style fallback already guarantees the words survive.
                let mut history_text = snippet.clone().unwrap_or_else(|| processed.clone());
                // Counted now, against the formatted text, before any model
                // or snippet replaces it: the figure is what the clean-up did.
                let cleanup_fixes = word_edits(&corrected, &processed);

                let processed = if let Some(body) = snippet.clone() {
                    body
                } else if auto_edit_llm {
                    crate::core::command::auto_edit(
                        &command_cfg,
                        command_key.as_deref(),
                        &processed,
                    )
                    .await
                } else {
                    processed
                };

                // Command mode intercepts before injection: the text to deliver
                // becomes the model's reply, not the transcript itself.
                let instruction = (command_cfg.enabled && snippet.is_none())
                    .then(|| crate::core::command::parse_command(&processed, &command_cfg.prefix))
                    .flatten();

                let to_inject = match instruction {
                    None if snippet.is_some() => Some(processed),
                    // Ordinary dictation: the per-app style, if there is one
                    // and styles are switched on. `restyle` makes no request
                    // without a style and returns the text unchanged on any
                    // failure, so this can only ever cost the styling.
                    None => {
                        let style = style_enabled.then_some(delivery.style.as_deref()).flatten();
                        let styled = crate::core::command::restyle(
                            &command_cfg,
                            command_key.as_deref(),
                            style,
                            &processed,
                            crate::core::command::STYLE_TIMEOUT,
                        )
                        .await;
                        if styled != processed {
                            history_text = styled.clone();
                        }
                        Some(styled)
                    }
                    Some(instruction) => {
                        match run_command(
                            &command_cfg,
                            command_key.as_deref(),
                            instruction,
                            &injector,
                        )
                        .await
                        {
                            Ok(reply) => Some(reply),
                            Err(e) => {
                                error!("Command mode failed: {e}");
                                let _ = app_clone.emit(
                                    AppEvent::ErrorOccurred {
                                        message: String::new(),
                                    }
                                    .event_name(),
                                    serde_json::json!({ "message": e.to_string() }),
                                );
                                // Still recorded below: the instruction was
                                // said, whatever the model did with it.
                                None
                            }
                        }
                    }
                };

                if delivery.record_history && !history_text.is_empty() {
                    let state = app_clone.state::<AppState>();
                    let conn = state.db.lock_live();
                    let record = crate::storage::models::TranscriptionRecord {
                        text: history_text,
                        language: segment.language.clone(),
                        provider: provider.clone(),
                        // Zero means the speech edges never fired — a provider
                        // that streams its own finals, say. Store nothing
                        // rather than a zero that would read as "instant".
                        duration_ms: match spoken_ms.swap(0, Ordering::Relaxed) {
                            0 => None,
                            ms => Some(ms as i64),
                        },
                        app: focused.clone(),
                        // Two passes, counted separately, because they answer
                        // different questions: the dictionary is your own
                        // correction working, the clean-up is Echo's.
                        dictionary_fixes: word_edits(segment.text.trim(), &corrected),
                        cleanup_fixes,
                        ..Default::default()
                    };
                    if let Err(e) = crate::storage::repositories::insert_history(&conn, &record) {
                        error!("Failed to record history: {e}");
                    }
                    // Trim here as well as at startup. The window is a promise
                    // about what Echo is still holding, and checking it only on
                    // launch quietly breaks that for anyone who leaves the app
                    // running — which is how it is meant to be used.
                    if let Err(e) = crate::storage::repositories::apply_retention(&conn) {
                        error!("History retention pass failed: {e}");
                    }
                }

                let Some(to_inject) = to_inject else {
                    continue;
                };

                // What output plugins will be told, prepared now because
                // injection consumes the text, and sent only after it: plugin
                // code must neither delay the words reaching the cursor nor run
                // alongside a paste that is borrowing the clipboard. An app
                // whose history is off is withheld — a plugin that writes
                // transcripts down is a history the user did not agree to.
                let for_plugins = (output_tx.is_some()
                    && delivery.record_history
                    && !to_inject.is_empty())
                .then(|| crate::core::plugins::Transcript {
                    text: to_inject.clone(),
                    app: focused.clone(),
                    language: spoken.map(str::to_owned),
                });

                // Inject into the focused application if enabled.
                if delivery.auto_inject && !to_inject.is_empty() {
                    // A multi-line snippet is always pasted, whatever the
                    // method. Typed, each line break is a Return key, and in
                    // a chat box or a form that sends the first line of the
                    // signature and leaves the rest behind; in a terminal it
                    // runs each line. Paste is the only way the body arrives
                    // intact. A single-line body follows the usual choice.
                    let force_paste = snippet.is_some() && to_inject.contains('\n');
                    let use_paste = force_paste || delivery.use_paste(&to_inject);
                    info!(
                        focused = focused.as_deref().unwrap_or("<unknown>"),
                        chars = to_inject.chars().count(),
                        method = if use_paste { "paste" } else { "keystrokes" },
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
                    let text = to_inject.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        if typed.is_empty() || force_paste {
                            // Streamed partials of the trigger are on screen
                            // as typed text; take them back before pasting
                            // rather than rewriting them key by key.
                            if !typed.is_empty() {
                                crate::core::injection::rewrite(inj.as_ref(), &typed, "")?;
                            }
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
                            *state.last_delivery.lock_live() =
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

                // Delivered — or rescued to the clipboard — so the text is
                // safe whatever a plugin does with its copy.
                if let (Some(tx), Some(transcript)) = (&output_tx, for_plugins) {
                    let _ = tx.send(transcript);
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

        report_if_nothing_was_heard(
            &quiet_app,
            delivered,
            f32::from_bits(peak_for_report.load(Ordering::Relaxed)),
            session_started.elapsed(),
        );
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    end_recording(app, state.inner()).await
}

/// Stop the capture session and, if wake-word listening is enabled, hand the
/// microphone back to the listener so the next phrase is heard.
pub async fn end_recording(app: AppHandle, state: &AppState) -> Result<()> {
    {
        let mut recording = state.recording.lock_live();
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
        let conn = state.db.lock_live();
        crate::storage::repositories::get_setting(&conn, "warm_mic")
            .unwrap_or(None)
            .map(|v| v != "false")
            .unwrap_or(true)
    };
    if warm {
        let device = {
            let conn = state.db.lock_live();
            crate::storage::repositories::get_setting(&conn, "audio_device")
                .unwrap_or(None)
                .filter(|s| !s.is_empty())
        };
        state.audio.stop_capture_warm(device.as_deref());
    } else {
        state.audio.stop_capture();
    }

    // People dictate in bursts, so the engine that just answered is the one
    // about to be asked again. Cheap when it is already resident.
    {
        let asr = state.asr.clone();
        tauri::async_runtime::spawn(async move { asr.preload_active().await });
    }

    app.emit(
        AppEvent::RecordingStopped.event_name(),
        AppEvent::RecordingStopped,
    )
    .map_err(|e| EchoError::Plugin(e.to_string()))?;
    info!("Recording stopped");

    crate::commands::wake::rearm(&app);

    Ok(())
}

#[tauri::command]
pub fn is_recording(state: State<'_, AppState>) -> bool {
    *state.recording.lock_live()
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
        let conn = state.db.lock_live();
        let get = |k: &str| crate::storage::repositories::get_setting(&conn, k).unwrap_or(None);
        let enabled = get("warm_mic").map(|v| v != "false").unwrap_or(true);
        (enabled, get("audio_device").filter(|s| !s.is_empty()))
    };
    if !enabled || *state.recording.lock_live() {
        return;
    }
    state.audio.warm(device.as_deref());

    // The microphone opening means a dictation is coming, which makes this the
    // moment to have the decoder's weights in memory too — the device costs
    // milliseconds to open, the model can cost twenty seconds to load.
    let asr = state.asr.clone();
    tauri::async_runtime::spawn(async move { asr.preload_active().await });
}

/// Below this peak level (about -46 dBFS) a whole session is, for practical
/// purposes, silence. The capture AGC has already lifted anything speech-shaped
/// well past it, so audio this quiet means the microphone is muted, unplugged,
/// or not the device Echo is listening to.
const SILENT_SESSION_PEAK: f32 = 0.005;

/// Shortest hold worth reporting on. Below this the hotkey was tapped, not
/// dictated into, and a warning would be nagging rather than news.
const MIN_REPORTABLE_SESSION: std::time::Duration = std::time::Duration::from_millis(1_500);

/// Say so when a dictation produced no text because nothing reached the
/// microphone.
///
/// Silence used to be indistinguishable from a broken pipeline from the user's
/// side: the utterance was gated out, a debug line went to the log, and the
/// screen showed the same nothing either way. This is the one case Echo can
/// explain rather than swallow.
fn report_if_nothing_was_heard(
    app: &AppHandle,
    delivered: bool,
    peak: f32,
    session: std::time::Duration,
) {
    if !should_report_silence(delivered, peak, session) {
        return;
    }
    info!(
        peak_dbfs = 20.0 * peak.max(1e-6).log10(),
        "Session produced no transcript and the input was silent"
    );
    let event = AppEvent::ErrorOccurred {
        message: "Echo heard nothing. Check the microphone in Settings — the selected input                   may be muted, unplugged, or the wrong device."
            .into(),
    };
    let _ = app.emit(event.event_name(), &event);
}

/// Whether a finished session is worth warning about.
///
/// Three ways to stay quiet, and each one is a complaint avoided: text did
/// arrive, the hotkey was only tapped, or the microphone was working and the
/// user simply did not speak.
pub(crate) fn should_report_silence(
    delivered: bool,
    peak: f32,
    session: std::time::Duration,
) -> bool {
    !delivered && session >= MIN_REPORTABLE_SESSION && peak < SILENT_SESSION_PEAK
}

/// Signals the VAD stage produces for the UI.
pub(crate) enum VadEvent {
    /// Per-chunk RMS of the captured audio, for the live waveform.
    Level(f32),
    SpeechStarted,
    SpeechEnded,
}

/// Audio kept from just before the VAD's rising edge and sent ahead of it.
///
/// The detector needs a frame or several of confident speech before it
/// triggers, and a soft onset ("f", "h", "s", a quiet first syllable) sits
/// under its threshold for most of that — dropping it is what turns "first"
/// into "irst".
const LEAD_IN_SAMPLES: usize = 16_000 * 300 / 1000;

/// Silence, beyond the VAD's own trailing debounce, that ends an utterance when
/// `end_on_pause` is set. With Silero's ~770 ms tail that is about two seconds
/// in all: long enough to stop and think mid-sentence without being cut off.
const END_OF_UTTERANCE_PAUSE_SAMPLES: usize = 16_000 * 1_200 / 1000;

/// The VAD gating stage: sits between raw audio capture and the ASR pipeline,
/// forwarding speech chunks (plus a short [`LEAD_IN_SAMPLES`] before each) and
/// emitting an empty-vec sentinel when an utterance ends.
///
/// With `end_on_pause` false — hold and toggle, where the hotkey brackets the
/// utterance — everything from the first speech to the end of capture is
/// forwarded as one utterance, pauses included. Splitting there only hands the
/// decoder fragments without the context around them, and throws away any
/// fragment spoken softly enough to fail the utterance gate on its own.
///
/// With it set — voice-activated mode and the wake word, where nothing else
/// says the speaker is done — an utterance ends after
/// [`END_OF_UTTERANCE_PAUSE_SAMPLES`] of silence. Speech resuming sooner
/// continues the same utterance, with the pause trimmed to the lead-in.
///
/// Split out of [`begin_recording`] so the pipeline can be driven in tests
/// without a Tauri app — `events` receives exactly what the app forwards to the
/// frontend. The VAD instance belongs entirely to this task (architectural
/// rule 8).
pub(crate) async fn vad_gate<F>(
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    mut vad: Box<dyn Vad>,
    vad_tx: mpsc::Sender<Vec<f32>>,
    end_on_pause: bool,
    events: F,
) where
    F: Fn(VadEvent),
{
    let mut agc = crate::core::audio::Agc::new();
    let mut was_speaking = false;
    // Speech has been forwarded and no sentinel has closed it yet.
    let mut open = false;
    let mut pause_len = 0usize;
    let mut lead_in: std::collections::VecDeque<Vec<f32>> = Default::default();
    let mut lead_in_len = 0usize;

    while let Some(mut chunk) = audio_rx.recv().await {
        if chunk.is_empty() {
            // Audio error/stop sentinel from the capture layer — flush and exit.
            let _ = vad_tx.send(Vec::new()).await;
            return;
        }

        // Before anything reads the audio, so speech detection, the meter and
        // the decoder all work from the same lifted signal.
        agc.apply(&mut chunk);

        // Computed before the VAD gate so the visualization stays responsive in
        // near-silence.
        let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
        events(VadEvent::Level(rms));

        let speech = vad.is_speech(&chunk);
        if speech && !was_speaking {
            was_speaking = true;
            open = true;
            pause_len = 0;
            events(VadEvent::SpeechStarted);
            for held in lead_in.drain(..) {
                if vad_tx.send(held).await.is_err() {
                    return;
                }
            }
            lead_in_len = 0;
        } else if !speech && was_speaking {
            was_speaking = false;
            events(VadEvent::SpeechEnded);
        }

        if speech || (open && !end_on_pause) {
            if vad_tx.send(chunk).await.is_err() {
                return;
            }
            continue;
        }

        lead_in_len += chunk.len();
        pause_len += chunk.len();
        lead_in.push_back(chunk);
        // Whole chunks only, keeping at least the lead-in's worth.
        while lead_in
            .front()
            .is_some_and(|c| lead_in_len - c.len() >= LEAD_IN_SAMPLES)
        {
            lead_in_len -= lead_in.pop_front().map_or(0, |c| c.len());
        }

        if open && pause_len >= END_OF_UTTERANCE_PAUSE_SAMPLES {
            open = false;
            if vad_tx.send(Vec::new()).await.is_err() {
                return;
            }
        }
    }

    // Capture closed (recording stopped): flush any trailing utterance.
    let _ = vad_tx.send(Vec::new()).await;
}

/// Roughly how many words changed between two versions of the same sentence.
///
/// Deliberately not a diff. A real alignment would tell you an inserted word
/// shifted everything after it, and then Insights would have to explain what
/// an edit distance is. Position-by-position plus the length difference
/// answers the only question being asked — "about how many words did Echo
/// change?" — and it never claims more edits than there are words.
fn word_edits(before: &str, after: &str) -> i64 {
    let a: Vec<&str> = before.split_whitespace().collect();
    let b: Vec<&str> = after.split_whitespace().collect();
    let changed = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
    (changed + a.len().abs_diff(b.len())) as i64
}

/// Cap on retained audio: three minutes at 16 kHz mono f32, about 11.5 MB.
///
/// Was thirty seconds, which is one breath — but an utterance ends at a pause,
/// not at a breath, and someone reading a prepared paragraph runs well past it.
/// Losing retry there is the case retry exists for. One buffer at a time, held
/// only until the next utterance replaces it, so the memory is worth the cover.
pub(crate) const MAX_RETAINED_SAMPLES: usize = 180 * 16_000;

/// Seconds of audio [`MAX_RETAINED_SAMPLES`] stands for, for saying so.
pub(crate) const MAX_RETAINED_SECONDS: usize = MAX_RETAINED_SAMPLES / 16_000;

/// What is being held for a re-decode of the last utterance.
///
/// Not an `Option<Vec<f32>>`: "nothing was recorded" and "what was recorded ran
/// past the cap" are different answers, and collapsing them told a user who had
/// just dictated for four minutes that there was no recent dictation.
#[derive(Debug, Clone, PartialEq)]
pub enum Retained {
    /// Audio, ready to decode again.
    Audio(Vec<f32>),
    /// The utterance ran past [`MAX_RETAINED_SAMPLES`], so none was kept.
    TooLong,
}

/// Forward the VAD's output to the ASR unchanged, keeping a copy of the most
/// recent complete utterance so it can be re-decoded without being re-spoken.
///
/// A passthrough rather than a change to the provider trait: the buffer is
/// already assembled here, and every provider — including one from a plugin —
/// gets the behaviour without implementing anything.
///
/// Also where the crash spool is written, for the same reason: this is the one
/// stage that already sees every speech chunk and every utterance boundary.
/// `slot` is `None` when the user has turned retry off; the spool is not,
/// because losing a sentence to a crash is not a preference.
///
/// An utterance longer than [`MAX_RETAINED_SAMPLES`] is dropped rather than
/// truncated. Retrying the first thirty seconds of a longer sentence would
/// silently return a shorter transcript than the one it replaced, which looks
/// exactly like the retry itself failing.
pub(crate) async fn retain_utterances(
    mut rx: mpsc::Receiver<Vec<f32>>,
    tx: mpsc::Sender<Vec<f32>>,
    slot: Option<Arc<std::sync::Mutex<Option<Retained>>>>,
    mut spool: Option<crate::core::spool::Spool>,
) {
    let mut current: Vec<f32> = Vec::new();
    let mut overflowed = false;

    while let Some(chunk) = rx.recv().await {
        if let (Some(spool), false) = (spool.as_mut(), chunk.is_empty()) {
            spool.write(&chunk);
        }
        if chunk.is_empty() {
            // Utterance boundary: publish what was collected, or clear the slot
            // if it was too long to keep honestly.
            let finished = std::mem::take(&mut current);
            if let Some(slot) = &slot {
                if overflowed {
                    *slot.lock_live() = Some(Retained::TooLong);
                } else if !finished.is_empty() {
                    *slot.lock_live() = Some(Retained::Audio(finished));
                }
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
            break;
        }
    }

    // Capture ended and every chunk reached the ASR stage, so nothing is left
    // that a crash could have taken. Dropping the spool instead would leave the
    // file behind and stage a recovery of audio that was already transcribed.
    if let Some(spool) = spool {
        spool.finish();
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
        AppEvent::ErrorOccurred {
            message: String::new(),
        }
        .event_name(),
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
    /// Raw `injection_method`: `"type"`, `"paste"`, `"auto"`, or `None` for
    /// unset, which [`use_paste_for`](crate::core::injection::use_paste_for)
    /// reads as typing. Kept unresolved because `"auto"` decides from the text,
    /// which does not exist yet when settings are read.
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
    /// The focused app's writing style, if its profile sets a non-blank one.
    /// Whether it is applied also depends on the global `app_style_enabled`.
    pub style: Option<String>,
}

/// Resolve delivery settings for the focused app.
///
/// Global settings are the baseline; a matching per-app profile overrides only
/// the fields it actually sets (a `NULL` column means "inherit"). With no
/// focused app or no profile, this is exactly the old global behaviour.
pub(crate) fn resolve_delivery(conn: &rusqlite::Connection, focused: Option<&str>) -> Delivery {
    use crate::storage::repositories as repo;

    let get = |key: &str| repo::get_setting(conn, key).unwrap_or(None);

    let mut delivery = Delivery {
        auto_inject: get("auto_inject").map(|v| v != "false").unwrap_or(true),
        // Unset stays "type" — `use_paste_for` owns that default now, so this
        // passes the setting through exactly as stored rather than stating it a
        // second time here.
        method: get("injection_method"),
        // Off unless asked for: this one types into another app's text field
        // while the user is still speaking into it.
        stream_partials: get("stream_partials").map(|v| v == "true").unwrap_or(false),
        // Spoken punctuation is off by default: it takes words out of the
        // language ("period" stops being usable as a noun), and that is the
        // user's trade to opt into. Tidy and numbers only reshape what is
        // already there, so they default on.
        format: crate::core::format::FormatOptions {
            // On by default: the words it removes ("um", a stuttered "the")
            // are ones nobody meant to write, so it costs no fidelity. The
            // LLM rewrite, which does change your words, is separate and off.
            cleanup: get("auto_edit").map(|v| v != "false").unwrap_or(true),
            spoken_punctuation: get("spoken_punctuation")
                .map(|v| v == "true")
                .unwrap_or(false),
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
        // No global style: what "formal" means depends on where you type.
        style: None,
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
            delivery.style = profile.style.filter(|s| !s.trim().is_empty());
        }
    }

    delivery
}

/// Read command-mode settings, falling back to the local-first defaults.
fn command_config(state: &AppState) -> CommandConfig {
    let defaults = CommandConfig::default();
    let conn = state.db.lock_live();
    let get = |key: &str| {
        crate::storage::repositories::get_setting(&conn, key)
            .unwrap_or(None)
            .filter(|s| !s.is_empty())
    };

    CommandConfig {
        enabled: get("command_mode_enabled")
            .map(|v| v == "true")
            .unwrap_or(false),
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
    let selection =
        tokio::task::spawn_blocking(move || crate::core::injection::copy_selection(inj.as_ref()))
            .await
            .map_err(|e| EchoError::Injection(format!("selection task panicked: {e}")))??;

    crate::core::command::run(cfg, api_key, instruction, selection.as_deref()).await
}

#[cfg(test)]
mod word_edit_tests {
    use super::word_edits;

    #[test]
    fn unchanged_text_counts_nothing() {
        assert_eq!(word_edits("ship it on friday", "ship it on friday"), 0);
        assert_eq!(word_edits("", ""), 0);
    }

    #[test]
    fn a_replaced_word_counts_once() {
        // What a dictionary entry does: one word in, one word out.
        assert_eq!(word_edits("send it to jeera", "send it to Jira"), 1);
    }

    #[test]
    fn dropped_and_added_words_count() {
        // What the clean-up pass does: fillers out, punctuation in.
        assert_eq!(word_edits("um so we ship", "so we ship"), 4);
        assert_eq!(word_edits("ship it", "Ship it."), 2);
    }
}
