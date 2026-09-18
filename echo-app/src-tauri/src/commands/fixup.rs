//! Fixing a transcript after it has already been typed somewhere.
//!
//! Both commands here answer the same complaint — "that's not what I said" —
//! and both are bound to their own global hotkeys, because by the time you
//! notice, the focus is in the app that received the text and not in Echo.
//!
//! - [`undo_last_insert`] takes the words back.
//! - [`retry_last`] decodes the *same audio* again on a stronger model, so a
//!   misheard word does not cost you the sentence a second time.

use tauri::{AppHandle, Emitter, Manager, State};
use tracing::info;

use crate::{
    core::asr::{decode_opts::DecodeConfig, model_manager, whisper_cli},
    error::{EchoError, Result},
    state::AppState,
};

/// Take back the last thing Echo typed into another application.
///
/// Returns whether there was anything to undo, so the caller can stay quiet
/// rather than reporting a failure when the answer is simply "nothing yet".
#[tauri::command]
pub async fn undo_last_insert(app: AppHandle) -> Result<bool> {
    undo_delivery(&app).await
}

/// The undo itself, callable from inside the recording pipeline (where the
/// spoken "scratch that" lands) as well as from the command above.
pub(crate) async fn undo_delivery(app: &AppHandle) -> Result<bool> {
    let (injector, last) = {
        let state = app.state::<AppState>();
        let last = state.last_delivery.lock().unwrap().take();
        (state.injector.clone(), last)
    };

    let Some(last) = last else {
        return Ok(false);
    };

    // Synthesizing a keystroke must not run on the async runtime.
    tokio::task::spawn_blocking(move || injector.send_undo())
        .await
        .map_err(|e| EchoError::Injection(format!("undo task panicked: {e}")))??;

    info!(
        chars = last.text.chars().count(),
        used_paste = last.used_paste,
        "Undid the last insert"
    );
    Ok(true)
}

/// Re-decode the last utterance and replace what was typed with the result.
///
/// The audio is the one already in memory, so nothing has to be said again.
/// What changes is the decoder: [`retry_target`] picks a stronger local model
/// by default, and a cloud provider only if the user chose one.
#[tauri::command]
pub async fn retry_last(app: AppHandle) -> Result<Option<String>> {
    let (audio, injector, language) = {
        let state = app.state::<AppState>();
        let audio = state.last_utterance.lock().unwrap().clone();
        let language = {
            let conn = state.db.lock().unwrap();
            crate::storage::repositories::get_setting(&conn, "language")
                .unwrap_or(None)
                .filter(|s| !s.is_empty() && s != "auto")
        };
        (audio, state.injector.clone(), language)
    };

    use crate::commands::recording::{Retained, MAX_RETAINED_SECONDS};
    let audio = match audio {
        Some(Retained::Audio(a)) if !a.is_empty() => a,
        // Worth saying plainly: the user just spoke for minutes, and "there's
        // no recent dictation" would read as Echo having missed all of it.
        Some(Retained::TooLong) => {
            return Err(EchoError::NotFound(format!(
                "That dictation ran past {} minutes without a pause, so Echo didn't \
                 keep the audio to retry with. The transcript is still in History.",
                MAX_RETAINED_SECONDS / 60
            )))
        }
        _ => {
            return Err(EchoError::NotFound(
                "There's no recent dictation to retry.".into(),
            ))
        }
    };

    let text = transcribe_again(&app, audio, language.as_deref()).await?;
    if text.trim().is_empty() {
        return Err(EchoError::AsrProvider("The retry produced no text.".into()));
    }

    // Take back the first attempt before typing the second, so the two do not
    // end up concatenated in the target app.
    undo_delivery(&app).await?;

    // The rejected transcript must not bias the replacement's decode, and it
    // is not context for whatever gets dictated next either.
    let (dictionary, profile) = {
        let state = app.state::<AppState>();
        state.prompt_ctx.clear_previous();
        (state.dictionary.clone(), state.prompt_ctx.profile())
    };
    let text = dictionary.read().await.process_for(&text, profile);
    // Same pass dictation runs, so a retry cannot come out formatted
    // differently from the transcript it replaces.
    let text = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().unwrap();
        let format = super::recording::resolve_delivery(&conn, None).format;
        let language = crate::storage::repositories::get_setting(&conn, "language")
            .unwrap_or(None)
            .filter(|s| !s.is_empty() && s != "auto");
        crate::core::format::apply(&text, format, language.as_deref())
    };

    // Settings are read after the dictionary pass: the guard must not be held
    // across an await (architectural rule 2).
    let (method, settle_ms) = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().unwrap();
        let get = |k: &str| crate::storage::repositories::get_setting(&conn, k).unwrap_or(None);
        (
            get("injection_method"),
            get("clipboard_settle_ms")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(crate::core::injection::DEFAULT_SETTLE_MS),
        )
    };

    let use_paste = crate::core::injection::use_paste_for(method.as_deref(), &text);
    let deliver_text = text.clone();
    tokio::task::spawn_blocking(move || {
        crate::core::injection::deliver(injector.as_ref(), &deliver_text, use_paste, settle_ms)
    })
    .await
    .map_err(|e| EchoError::Injection(format!("retry injection task panicked: {e}")))??;

    {
        let state = app.state::<AppState>();
        *state.last_delivery.lock().unwrap() = Some(crate::core::undo::LastDelivery {
            text: text.clone(),
            used_paste: use_paste,
        });
    }

    let _ = app.emit(
        "echo://transcript-final",
        serde_json::json!({ "text": text, "language": null }),
    );
    info!(chars = text.chars().count(), "Retry delivered");
    Ok(Some(text))
}

/// Decode `audio` with whatever the retry is configured to use.
async fn transcribe_again(
    app: &AppHandle,
    audio: Vec<f32>,
    language: Option<&str>,
) -> Result<String> {
    let (target, asr) = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().unwrap();
        let setting = crate::storage::repositories::get_setting(&conn, "retry_target")
            .unwrap_or(None)
            .filter(|s| !s.is_empty());
        (setting, state.asr.clone())
    };

    // A registered provider name routes to that provider; anything else is a
    // whisper model name, including the default of "the biggest one installed".
    if let Some(name) = &target {
        if !model_manager::is_whisper_model(name) {
            info!(provider = name, "Retrying on a different ASR provider");
            return Ok(asr.transcribe_with(name, audio, language).await?.text);
        }
    }

    let model = match target {
        Some(model) => model,
        None => largest_installed_model(app).ok_or_else(|| {
            EchoError::NotFound(
                "No local Whisper model is installed to retry with. Pick one in Settings.".into(),
            )
        })?,
    };
    retry_locally(app, &model, audio, language).await
}

/// The biggest downloaded model, which is the best local answer available
/// without asking the user to choose one.
fn largest_installed_model(app: &AppHandle) -> Option<String> {
    let state = app.state::<AppState>();
    state
        .models
        .list()
        .into_iter()
        .filter(|m| m.downloaded)
        .max_by_key(|m| m.size_mb)
        .map(|m| m.name)
}

/// Decode with a specific local model through the one-shot CLI.
///
/// The CLI rather than the resident server on purpose: the server holds the
/// *dictation* model, and swapping it for a heavier one to serve a single retry
/// would evict it and make the next ordinary utterance pay a reload.
async fn retry_locally(
    app: &AppHandle,
    model: &str,
    audio: Vec<f32>,
    language: Option<&str>,
) -> Result<String> {
    let (binary, model_path, decode, prompt) = {
        let state = app.state::<AppState>();
        if !state.models.is_downloaded(model) {
            return Err(EchoError::NotFound(format!(
                "The '{model}' model isn't downloaded, so there's nothing stronger to retry with."
            )));
        }
        let binary = state.binaries.resolve().ok_or_else(|| {
            EchoError::NotFound("The offline Whisper engine is not installed yet.".into())
        })?;
        let (threads, gpu_allowed) = {
            let conn = state.db.lock().unwrap();
            super::asr::local_decode_settings(&conn)
        };
        let decode = DecodeConfig {
            threads,
            use_gpu: gpu_allowed
                && state
                    .binaries
                    .active_dir()
                    .map(|(_, accel)| accel)
                    .unwrap_or(false),
        };
        // The vocabulary hint still applies; the rejected transcript does not,
        // which is why `clear_previous` ran before this.
        let profile = state.prompt_ctx.profile();
        let prompt = state.dictionary.read().await.prompt_terms(profile);
        (binary, state.models.model_path(model), decode, prompt)
    };

    let wav = crate::core::asr::wav::pcm_f32_to_wav(&audio, 16_000)?;
    let lang = whisper_cli::resolve_language(model, language);
    info!(model, "Retrying the last utterance locally");
    let audio_seconds = audio.len().div_ceil(16_000) as u32;
    whisper_cli::run_cli(
        &binary,
        &model_path,
        &wav,
        lang,
        decode,
        prompt.as_deref(),
        audio_seconds,
    )
    .await
}

/// What the retry can be pointed at: every registered provider, plus every
/// local model that is actually downloaded.
#[tauri::command]
pub async fn retry_targets(state: State<'_, AppState>) -> Result<Vec<String>> {
    let mut targets: Vec<String> = state
        .models
        .list()
        .into_iter()
        .filter(|m| m.downloaded)
        .map(|m| m.name)
        .collect();
    targets.extend(
        state
            .asr
            .registered()
            .await
            .into_iter()
            .filter(|n| n != "local"),
    );
    Ok(targets)
}

#[cfg(test)]
mod tests {
    /// The retry target is disambiguated by asking the model catalog, so a
    /// provider name and a model name can share one setting without a prefix.
    #[test]
    fn model_names_are_distinguishable_from_provider_names() {
        use crate::core::asr::model_manager::is_whisper_model;
        assert!(is_whisper_model("medium"));
        assert!(is_whisper_model("small.en"));
        for provider in ["openai", "groq", "deepgram"] {
            assert!(
                !is_whisper_model(provider),
                "{provider} looked like a model"
            );
        }
    }
}
