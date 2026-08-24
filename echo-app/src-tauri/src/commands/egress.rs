//! The outbound-request log, surfaced to the user.
//!
//! See [`crate::core::egress`] for what this does and does not claim. The
//! summary that matters: it records requests **Echo** made, which is not the
//! same as proving nothing else left the machine. The UI copy is written to
//! match, and should stay that way.

use tauri::State;

use crate::{
    error::Result,
    state::AppState,
    storage::{models::EgressRecord, repositories},
};

/// Whether the current configuration is capable of reaching the network at all.
#[derive(serde::Serialize)]
pub struct EgressStatus {
    /// True when transcription, command mode and wake word are all local and no
    /// cloud key is stored — i.e. normal use makes no requests.
    pub offline_capable: bool,
    /// Reasons the app can currently reach out, for display.
    pub reasons: Vec<String>,
    /// Requests logged in the last 24 hours.
    pub recent_count: i64,
}

#[tauri::command]
pub fn get_egress_log(state: State<'_, AppState>, limit: Option<i64>) -> Result<Vec<EgressRecord>> {
    let conn = state.db.lock().unwrap();
    repositories::list_egress(&conn, limit.unwrap_or(200))
}

#[tauri::command]
pub fn clear_egress_log(state: State<'_, AppState>) -> Result<()> {
    let conn = state.db.lock().unwrap();
    repositories::clear_egress(&conn)
}

/// Summarise whether this configuration talks to anything.
///
/// This reports *configuration*, not observed behaviour — the log is the
/// behavioural half. Update checks are listed because the updater plugin makes
/// its own request that this module cannot instrument.
#[tauri::command]
pub fn get_egress_status(state: State<'_, AppState>) -> Result<EgressStatus> {
    let conn = state.db.lock().unwrap();
    let get = |key: &str| repositories::get_setting(&conn, key).unwrap_or(None);

    let mut reasons = Vec::new();

    let asr = get("asr_provider").unwrap_or_else(|| "local".into());
    if asr != "local" && asr != "none" {
        reasons.push(format!("Transcription uses the {asr} cloud API"));
    }

    let command_on = get("command_mode_enabled").map(|v| v == "true").unwrap_or(false);
    let command_provider = get("command_llm_provider").unwrap_or_else(|| "ollama".into());
    if command_on && command_provider == "openai" {
        reasons.push("Command mode sends selected text to OpenAI".into());
    }

    for provider in ["openai", "groq", "deepgram"] {
        if matches!(crate::storage::keychain::get_api_key(provider), Ok(Some(_))) {
            reasons.push(format!("An API key is stored for {provider}"));
        }
    }

    // Model and wake-word downloads are one-off, but they are real requests and
    // hiding them would make the log look dishonest the first time one appears.
    reasons.push("Model downloads and update checks contact GitHub and Hugging Face".into());

    let recent_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM egress_log WHERE created_at >= datetime('now', '-1 day')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // "Offline capable" ignores the downloads line: it describes steady-state
    // dictation, which is what the user is actually asking about.
    let offline_capable = reasons.len() == 1;

    Ok(EgressStatus {
        offline_capable,
        reasons,
        recent_count,
    })
}
