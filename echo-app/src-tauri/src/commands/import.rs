//! Transcribing audio files that were recorded somewhere else.
//!
//! Everything else in Echo transcribes the microphone. This transcribes a file
//! the user already has — a voice memo, a call recording, an interview — using
//! the same offline engine, so it costs nothing and sends nothing anywhere.
//!
//! It deliberately uses the one-shot CLI rather than the resident server: an
//! import is a single long decode where the model-load cost is irrelevant, and
//! keeping it off the server means a twenty-minute recording cannot block the
//! dictation path behind it.

use std::path::{Path, PathBuf};

use tauri::State;

use crate::{
    core::asr::{decode_opts::DecodeConfig, whisper_cli},
    error::{EchoError, Result},
    state::AppState,
};

/// What whisper.cpp can decode without external help.
const SUPPORTED_EXTENSIONS: [&str; 4] = ["wav", "mp3", "ogg", "flac"];

/// Transcribe an audio file with the local Whisper engine.
///
/// The dictionary is applied to the result exactly as it is for dictation, so
/// an imported transcript spells names the same way a dictated one does.
#[tauri::command]
pub async fn transcribe_file(
    state: State<'_, AppState>,
    path: String,
    language: Option<String>,
) -> Result<String> {
    let path = PathBuf::from(path);
    validate(&path)?;

    let binary = state.binaries.resolve().ok_or_else(|| {
        EchoError::NotFound(
            "The offline Whisper engine is not installed yet. Set it up in Settings first.".into(),
        )
    })?;

    let (model_name, threads, gpu_allowed) = {
        let conn = state.db.lock().unwrap();
        let model = crate::storage::repositories::get_setting(&conn, "whisper_model")
            .unwrap_or(None)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| crate::core::asr::model_manager::DEFAULT_MODEL.to_string());
        let (threads, gpu) = super::asr::local_decode_settings(&conn);
        (model, threads, gpu)
    };

    if !state.models.is_downloaded(&model_name) {
        return Err(EchoError::NotFound(format!(
            "The '{model_name}' model is not downloaded yet."
        )));
    }

    let decode = DecodeConfig {
        threads,
        use_gpu: gpu_allowed
            && state
                .binaries
                .active_dir()
                .map(|(_, accel)| accel)
                .unwrap_or(false),
    };

    let prompt = state.dictionary.read().await.prompt_terms(None);
    let lang = whisper_cli::resolve_language(&model_name, language.as_deref());

    let text = whisper_cli::run_cli_on_file(
        &binary,
        &state.models.model_path(&model_name),
        &path,
        lang,
        decode,
        prompt.as_deref(),
    )
    .await?;

    Ok(state.dictionary.read().await.process_for(&text, None))
}

/// The formats this can accept, for a file-picker filter.
#[tauri::command]
pub fn supported_import_formats() -> Vec<String> {
    SUPPORTED_EXTENSIONS.iter().map(|s| s.to_string()).collect()
}

/// Reject a file we cannot decode before spending a process spawn on it, and
/// give a reason the user can act on.
fn validate(path: &Path) -> Result<()> {
    if !path.is_file() {
        return Err(EchoError::NotFound(format!(
            "No file at {}",
            path.display()
        )));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();

    if !SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(EchoError::Config(format!(
            "Echo can transcribe {} files; '{}' is not one of them.",
            SUPPORTED_EXTENSIONS.join(", "),
            if ext.is_empty() { "(no extension)" } else { &ext }
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_reported_before_anything_else() {
        let err = validate(Path::new("definitely-not-here.wav")).unwrap_err();
        assert!(err.to_string().contains("No file at"), "{err}");
    }

    #[test]
    fn unsupported_formats_are_rejected_with_the_list() {
        let dir = std::env::temp_dir().join(format!("echo-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.m4a");
        std::fs::write(&path, b"stub").unwrap();

        let err = validate(&path).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("wav"), "{msg}");
        assert!(msg.contains("m4a"), "{msg}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn supported_formats_pass_regardless_of_case() {
        let dir = std::env::temp_dir().join(format!("echo-import-ok-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["clip.wav", "clip.MP3", "clip.Flac", "clip.ogg"] {
            let path = dir.join(name);
            std::fs::write(&path, b"stub").unwrap();
            assert!(validate(&path).is_ok(), "{name} should be accepted");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
