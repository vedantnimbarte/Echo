//! Transcribing audio files that were recorded somewhere else.
//!
//! Everything else in Echo transcribes the microphone. This transcribes a file
//! the user already has — a voice memo, a call recording, an interview — using
//! the same offline engine, so by default it costs nothing and sends nothing
//! anywhere.
//!
//! It deliberately uses the one-shot CLI rather than the resident server: an
//! import is a single long decode where the model-load cost is irrelevant, and
//! keeping it off the server means a twenty-minute recording cannot block the
//! dictation path behind it.
//!
//! **Speaker labels are the exception, and they leave the machine.** Whisper
//! does not diarize, so "Speaker 1: … / Speaker 2: …" is only on offer when
//! the active engine is a cloud provider that does, and then the *whole file*
//! is uploaded to that provider. That is never a silent upgrade: it happens
//! only when the user ticks "Label speakers" (or passes `--speakers`), the
//! panel names the provider the file goes to, and the upload is recorded in
//! the egress log like any other cloud request.
//!
//! ponytail: speaker labels are cloud-only. The offline upgrade is sherpa-onnx's
//! pipeline run through the `ort` crate already in the tree: pyannote
//! segmentation (`sherpa-onnx-pyannote-segmentation-3-0`, i.e.
//! pyannote/segmentation-3.0) to find speaker turns, a speaker-embedding model
//! (`3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx`, or
//! `nemo_en_titanet_small.onnx` for English) to embed each turn, clustering the
//! embeddings, then aligning the clusters with Whisper's segment timestamps.

use std::path::{Path, PathBuf};

use tauri::State;

use crate::{
    core::asr::{catalog, decode_opts::DecodeConfig, whisper_cli},
    error::{EchoError, Result},
    state::AppState,
};

/// What whisper.cpp can decode without external help.
const SUPPORTED_EXTENSIONS: [&str; 4] = ["wav", "mp3", "ogg", "flac"];

/// Transcribe an audio file with the local Whisper engine, or — with
/// `speakers` — with the active cloud engine, labelled by speaker.
///
/// The dictionary is applied to the result exactly as it is for dictation, so
/// an imported transcript spells names the same way a dictated one does.
#[tauri::command]
pub async fn transcribe_file(
    state: State<'_, AppState>,
    path: String,
    language: Option<String>,
    speakers: Option<bool>,
) -> Result<String> {
    transcribe_path(
        &state,
        &path,
        language.as_deref(),
        speakers.unwrap_or(false),
    )
    .await
}

/// The transcription itself, without the Tauri command wrapper.
///
/// Split out so [`crate::cli`] can reach it: the command form takes owned
/// `String`s because that is what the IPC layer deserializes into, and a
/// command-line caller has no reason to allocate them.
pub async fn transcribe_path(
    state: &AppState,
    path: &str,
    language: Option<&str>,
    speakers: bool,
) -> Result<String> {
    let path = PathBuf::from(path);
    validate(&path)?;

    if speakers {
        return transcribe_speakers(state, &path, language).await;
    }

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
    let lang = whisper_cli::resolve_language(&model_name, language);

    let text = whisper_cli::run_cli_on_file(
        &binary,
        &state.models.model_path(&model_name),
        &path,
        lang,
        decode,
        prompt.as_deref(),
        None,
    )
    .await?;

    let text = state.dictionary.read().await.process_for(&text, None);
    // The same formatting dictation gets. An import has no focused app, so the
    // global settings apply with no per-app override.
    let format = {
        let conn = state.db.lock().unwrap();
        crate::commands::recording::resolve_delivery(&conn, None).format
    };
    Ok(crate::core::format::apply(&text, format, language))
}

/// Upload the file to the active cloud engine and return a transcript in
/// paragraphs, one per turn: "Speaker 1: …", blank line, "Speaker 2: …".
///
/// No fallback to the offline engine, unlike dictation. Whisper would hand
/// back a transcript with no labels, which is not what was asked for, and an
/// error the user can read beats an answer that quietly isn't one.
async fn transcribe_speakers(
    state: &AppState,
    path: &Path,
    language: Option<&str>,
) -> Result<String> {
    let active = {
        let conn = state.db.lock().unwrap();
        crate::storage::repositories::get_setting(&conn, "asr_provider")
            .unwrap_or(None)
            .unwrap_or_else(|| "local".into())
    };
    let spec = catalog::find(&active)
        .filter(|spec| spec.speaker_labels)
        .ok_or_else(|| EchoError::Config(no_speakers_message(&active)))?;

    let key = crate::storage::keychain::get_api_key(spec.id)?.ok_or_else(|| {
        EchoError::Config(format!(
            "{} has no API key saved. Add one in Settings → Cloud providers.",
            spec.label
        ))
    })?;
    let provider = super::providers::build_provider(state, spec.id, key)?;

    let audio = tokio::fs::read(path)
        .await
        .map_err(|e| EchoError::NotFound(format!("Could not read {}: {e}", path.display())))?;
    let turns = provider
        .transcribe_speakers(audio, mime_for(path), language)
        .await?;

    let format = {
        let conn = state.db.lock().unwrap();
        crate::commands::recording::resolve_delivery(&conn, None).format
    };
    let dictionary = state.dictionary.read().await;
    Ok(paragraphs(turns)
        .into_iter()
        .map(|(n, text)| {
            // Per paragraph rather than over the joined result, so the
            // formatter never sees "Speaker 2:" and the dictionary never
            // matches a term across two people's words.
            let text = dictionary.process_for(&text, None);
            let text = crate::core::format::apply(&text, format, language);
            format!("Speaker {n}: {text}")
        })
        .collect::<Vec<_>>()
        .join("\n\n"))
}

/// The error for an engine that cannot label speakers, naming the ones that
/// can so the user knows what to switch to.
fn no_speakers_message(active: &str) -> String {
    let able = catalog::PROVIDERS
        .iter()
        .filter(|p| p.speaker_labels)
        .map(|p| p.label)
        .collect::<Vec<_>>()
        .join(", ");
    let engine = match active {
        "local" => "The offline Whisper engine does not label speakers.".to_string(),
        "none" => "Transcription is turned off.".to_string(),
        other => format!(
            "{} does not label speakers.",
            catalog::find(other).map_or(other, |p| p.label)
        ),
    };
    format!("{engine} Speaker labels need one of these cloud engines: {able}.")
}

/// Merge provider turns into numbered paragraphs.
///
/// Consecutive turns from one speaker become one paragraph — providers split
/// on pauses, and a new "Speaker 1:" line every time someone draws breath is
/// noise. Speakers are numbered from 1 in order of first appearance, so the
/// providers' own ids (`0`, `"A"`, `"S1"`, `"speaker_0"`) never reach the user
/// and whoever talks first is always Speaker 1.
fn paragraphs(turns: Vec<(String, String)>) -> Vec<(usize, String)> {
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<(usize, String)> = Vec::new();
    for (speaker, text) in turns {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let n = match seen.iter().position(|s| *s == speaker) {
            Some(i) => i + 1,
            None => {
                seen.push(speaker);
                seen.len()
            }
        };
        match out.last_mut() {
            Some((last, para)) if *last == n => {
                para.push(' ');
                para.push_str(text);
            }
            _ => out.push((n, text.to_string())),
        }
    }
    out
}

/// The MIME type to upload a supported file as. Providers that sniff the bytes
/// ignore it; the ones that take it at its word need it to be true.
fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("mp3") => "audio/mpeg",
        Some("ogg") => "audio/ogg",
        Some("flac") => "audio/flac",
        _ => "audio/wav",
    }
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
            if ext.is_empty() {
                "(no extension)"
            } else {
                &ext
            }
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(speaker: &str, text: &str) -> (String, String) {
        (speaker.to_string(), text.to_string())
    }

    #[test]
    fn consecutive_turns_merge_and_speakers_number_by_first_appearance() {
        // Azure numbers its speakers without particular order, so "1" talking
        // first must still come out as Speaker 1.
        let got = paragraphs(vec![
            turn("1", "Good afternoon."),
            turn("1", "Thanks for coming."),
            turn("0", "Hi there."),
            turn("1", "Shall we start?"),
            turn("0", "  "),
        ]);
        assert_eq!(
            got,
            vec![
                (1, "Good afternoon. Thanks for coming.".to_string()),
                (2, "Hi there.".to_string()),
                (1, "Shall we start?".to_string()),
            ]
        );
    }

    #[test]
    fn a_blank_turn_does_not_split_one_speakers_paragraph() {
        let got = paragraphs(vec![turn("A", "one"), turn("B", ""), turn("A", "two")]);
        assert_eq!(got, vec![(1, "one two".to_string())]);
    }

    #[test]
    fn the_refusal_names_the_engines_that_would_work() {
        let msg = no_speakers_message("local");
        assert!(
            msg.contains("Whisper engine does not label speakers"),
            "{msg}"
        );
        for label in ["Deepgram", "AssemblyAI", "Speechmatics", "Azure AI Speech"] {
            assert!(msg.contains(label), "{msg}");
        }
        assert!(!msg.contains("Groq"), "{msg}");
        assert!(no_speakers_message("groq").starts_with("Groq does not"));
    }

    #[test]
    fn uploads_are_labelled_with_the_files_real_type() {
        assert_eq!(mime_for(Path::new("a.MP3")), "audio/mpeg");
        assert_eq!(mime_for(Path::new("a.flac")), "audio/flac");
        assert_eq!(mime_for(Path::new("a.ogg")), "audio/ogg");
        assert_eq!(mime_for(Path::new("a.wav")), "audio/wav");
    }

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
