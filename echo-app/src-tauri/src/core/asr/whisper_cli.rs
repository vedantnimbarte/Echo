//! Running whisper.cpp's one-shot CLI.
//!
//! Every call reloads the model from disk, so this is not how dictation is
//! served — [`super::local::LocalWhisperProvider`] prefers the resident server
//! and falls back to here only when there is no server to talk to. It stays
//! useful in its own right: no port, no child process to supervise, and it is
//! what transcribes imported files.

use std::path::Path;
use std::sync::Arc;

use tokio::process::Command;
use tokio::sync::RwLock;

use super::decode_opts::DecodeConfig;
use crate::core::dictionary::DictionaryEngine;
use crate::error::{EchoError, Result};

/// English-only models must run in `en` — letting them auto-detect wastes a
/// detection pass and occasionally mislabels short utterances.
pub(crate) fn is_english_only(model_name: &str) -> bool {
    model_name.ends_with(".en")
}

/// The language to hand whisper for this model and request.
pub(crate) fn resolve_language<'a>(model_name: &str, requested: Option<&'a str>) -> &'a str {
    if is_english_only(model_name) {
        "en"
    } else {
        requested.unwrap_or("auto")
    }
}

/// The dictionary's vocabulary hint, if there is one.
///
/// Global entries only: a local backend has no notion of which app is focused,
/// so a profile-scoped hint could bias toward vocabulary that will not even be
/// applied afterwards.
pub(super) async fn initial_prompt(
    dictionary: Option<&Arc<RwLock<DictionaryEngine>>>,
) -> Option<String> {
    dictionary?.read().await.prompt_terms(None)
}

/// Run `whisper-cli` over one WAV buffer and return the transcript.
///
/// Shared with [`super::local::LocalWhisperProvider`] so the CLI fallback and
/// the standalone provider cannot drift apart in flags or output handling.
pub(super) async fn run_cli(
    binary: &Path,
    model_path: &Path,
    wav: &[u8],
    language: &str,
    decode: DecodeConfig,
    prompt: Option<&str>,
) -> Result<String> {
    // whisper-cli reads audio from disk. Stage it under a unique name so
    // concurrent utterances never collide.
    let tmp = std::env::temp_dir().join(format!("echo-{}.wav", uuid::Uuid::new_v4()));
    tokio::fs::write(&tmp, wav)
        .await
        .map_err(|e| EchoError::Config(e.to_string()))?;

    let result = run_cli_on_file(binary, model_path, &tmp, language, decode, prompt).await;
    let _ = tokio::fs::remove_file(&tmp).await;
    result
}

/// Run `whisper-cli` over an audio file already on disk.
///
/// whisper.cpp decodes flac/mp3/ogg/wav itself, so an imported recording can go
/// straight to the binary — there is no reason to decode and re-encode it here.
pub(crate) async fn run_cli_on_file(
    binary: &Path,
    model_path: &Path,
    audio_path: &Path,
    language: &str,
    decode: DecodeConfig,
    prompt: Option<&str>,
) -> Result<String> {
    let tmp = audio_path;
    let mut cmd = Command::new(binary);
    cmd.arg("-m")
        .arg(model_path)
        .arg("-f")
        .arg(tmp)
        .args(["-l", language])
        .args(decode.args())
        .arg("-nt") // no timestamps — stdout is plain transcript text
        .arg("-np"); // no progress / system-info prints

    if let Some(prompt) = prompt {
        cmd.arg("--prompt").arg(prompt);
    }

    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output().await.map_err(|e| {
        EchoError::AsrProvider(format!(
            "failed to launch whisper-cli at {}: {e}",
            binary.display()
        ))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(EchoError::AsrProvider(format!(
            "whisper-cli exited with {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    Ok(clean_transcript(&String::from_utf8_lossy(&output.stdout)))
}

/// Flatten whisper.cpp's line-per-segment output into one transcript line.
///
/// `-sns` suppresses most non-speech tokens, but `[BLANK_AUDIO]` is emitted by
/// the front-end itself rather than the decoder, so it still has to go here.
pub(super) fn clean_transcript(stdout: &str) -> String {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != "[BLANK_AUDIO]")
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_audio_marker_is_not_transcript_text() {
        assert_eq!(clean_transcript("[BLANK_AUDIO]\n"), "");
        assert_eq!(
            clean_transcript(" hello there \n[BLANK_AUDIO]\n world \n"),
            "hello there world"
        );
    }

    #[test]
    fn english_only_models_ignore_the_requested_language() {
        assert_eq!(resolve_language("base.en", Some("fr")), "en");
        assert_eq!(resolve_language("base.en", None), "en");
    }

    #[test]
    fn multilingual_models_honour_the_request_and_default_to_auto() {
        assert_eq!(resolve_language("base", Some("fr")), "fr");
        assert_eq!(resolve_language("large-v3", None), "auto");
    }
}
