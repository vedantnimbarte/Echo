use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    error::{EchoError, Result},
    state::AppState,
    storage::{
        models::{DictionaryEntry, Snippet},
        repositories,
    },
};

/// Portable representation of a dictionary entry for import/export (no ids or
/// timestamps so files move cleanly between machines).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryExportEntry {
    pub phrase: String,
    pub replacement: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Portable snippet, for the same reason: no id, no timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetExport {
    pub trigger: String,
    pub body: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// What an export file holds.
///
/// Files written before snippets existed are a bare array of entries, and they
/// still import. New files are an object, which an older Echo refuses to read —
/// deliberately the better failure: the alternative was writing snippets into
/// the array as entries, and an older version would have imported "sign off"
/// as a dictionary rule that pastes a signature into any sentence containing
/// those words.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DictionaryFile {
    Legacy(Vec<DictionaryExportEntry>),
    Current {
        entries: Vec<DictionaryExportEntry>,
        #[serde(default)]
        snippets: Vec<SnippetExport>,
    },
}

/// Rebuild the in-memory engine from the current DB rows. Called after any
/// mutation so transcription always uses the latest entries (architectural
/// rule 6).
pub(crate) async fn refresh_engine(state: &AppState, raw: Vec<DictionaryEntry>) {
    let entries = raw
        .into_iter()
        .map(|e| crate::core::dictionary::DictionaryEntry {
            id: e.id,
            phrase: e.phrase,
            replacement: e.replacement,
            enabled: e.enabled,
            profile_id: e.profile_id,
        })
        .collect();
    state.dictionary.write().await.update_entries(entries);
}

#[tauri::command]
pub fn list_dictionary(state: State<'_, AppState>) -> Result<Vec<DictionaryEntry>> {
    let conn = state.db.lock().unwrap();
    repositories::list_dictionary_entries(&conn)
}

#[tauri::command]
pub async fn add_dictionary_entry(
    state: State<'_, AppState>,
    phrase: String,
    replacement: String,
) -> Result<i64> {
    let entry = DictionaryEntry {
        id: None,
        phrase,
        replacement,
        enabled: true,
        profile_id: None,
        created_at: String::new(),
    };

    // Hold and release the MutexGuard before awaiting.
    let (id, raw) = {
        let conn = state.db.lock().unwrap();
        let id = repositories::insert_dictionary_entry(&conn, &entry)?;
        let raw = repositories::list_dictionary_entries(&conn)?;
        (id, raw)
    };

    refresh_engine(&state, raw).await;
    Ok(id)
}

#[tauri::command]
pub async fn delete_dictionary_entry(state: State<'_, AppState>, id: i64) -> Result<()> {
    let raw = {
        let conn = state.db.lock().unwrap();
        repositories::delete_dictionary_entry(&conn, id)?;
        repositories::list_dictionary_entries(&conn)?
    };

    refresh_engine(&state, raw).await;
    Ok(())
}

#[tauri::command]
pub async fn toggle_dictionary_entry(
    state: State<'_, AppState>,
    id: i64,
    enabled: bool,
) -> Result<()> {
    let raw = {
        let conn = state.db.lock().unwrap();
        repositories::set_dictionary_entry_enabled(&conn, id, enabled)?;
        repositories::list_dictionary_entries(&conn)?
    };

    refresh_engine(&state, raw).await;
    Ok(())
}

/// Serialize all entries and snippets to a JSON file at the user-chosen path.
#[tauri::command]
pub async fn export_dictionary(state: State<'_, AppState>, path: String) -> Result<()> {
    let (raw, snippets) = {
        let conn = state.db.lock().unwrap();
        (
            repositories::list_dictionary_entries(&conn)?,
            repositories::list_snippets(&conn)?,
        )
    };

    let export: Vec<DictionaryExportEntry> = raw
        .into_iter()
        .map(|e| DictionaryExportEntry {
            phrase: e.phrase,
            replacement: e.replacement,
            enabled: e.enabled,
        })
        .collect();
    let snippets: Vec<SnippetExport> = snippets
        .into_iter()
        .map(|s| SnippetExport {
            trigger: s.trigger,
            body: s.body,
            enabled: s.enabled,
        })
        .collect();

    let json = serde_json::to_string_pretty(
        &serde_json::json!({ "entries": export, "snippets": snippets }),
    )?;
    std::fs::write(&path, json).map_err(|e| EchoError::Config(e.to_string()))?;
    Ok(())
}

/// Read a JSON file and insert entries whose phrase isn't already present
/// (case-insensitive), and snippets whose trigger isn't. Returns the number of
/// entries and snippets added.
#[tauri::command]
pub async fn import_dictionary(state: State<'_, AppState>, path: String) -> Result<usize> {
    let contents = std::fs::read_to_string(&path).map_err(|e| EchoError::Config(e.to_string()))?;
    let (imported, snippets) = match serde_json::from_str(&contents)? {
        DictionaryFile::Legacy(entries) => (entries, Vec::new()),
        DictionaryFile::Current { entries, snippets } => (entries, snippets),
    };

    let (added, raw) = {
        let conn = state.db.lock().unwrap();
        let existing: std::collections::HashSet<String> =
            repositories::list_dictionary_entries(&conn)?
                .into_iter()
                .map(|e| e.phrase.to_lowercase())
                .collect();

        let mut added = 0usize;
        for entry in imported {
            if entry.phrase.trim().is_empty() || existing.contains(&entry.phrase.to_lowercase()) {
                continue;
            }
            let row = DictionaryEntry {
                id: None,
                phrase: entry.phrase,
                replacement: entry.replacement,
                enabled: entry.enabled,
                profile_id: None,
                created_at: String::new(),
            };
            repositories::insert_dictionary_entry(&conn, &row)?;
            added += 1;
        }

        let mut triggers: std::collections::HashSet<String> = repositories::list_snippets(&conn)?
            .into_iter()
            .map(|s| s.trigger.to_lowercase())
            .collect();
        for s in snippets {
            // The same two refusals `save_snippet` makes, for a hand-edited file.
            if !s.trigger.chars().any(char::is_alphanumeric)
                || s.body.trim().is_empty()
                || !triggers.insert(s.trigger.to_lowercase())
            {
                continue;
            }
            repositories::save_snippet(
                &conn,
                &Snippet {
                    id: None,
                    trigger: s.trigger,
                    body: s.body,
                    enabled: s.enabled,
                },
            )?;
            added += 1;
        }
        let raw = repositories::list_dictionary_entries(&conn)?;
        (added, raw)
    };

    refresh_engine(&state, raw).await;
    Ok(added)
}

/// Learn dictionary entries from a transcript the user corrected by hand.
///
/// Returns the corrections that were actually stored, so the UI can show what
/// it learned. Learning nothing is a normal outcome — most edits are rewording,
/// not corrections — and is reported as an empty list rather than an error.
///
/// Entries land in the ordinary dictionary, visible and deletable like any
/// other. That is the safety net for the heuristic in
/// [`crate::core::dictionary::learn`]: anything it gets wrong is one click away
/// from being removed, rather than an invisible rule the user cannot find.
#[tauri::command]
pub async fn learn_from_correction(
    state: State<'_, AppState>,
    original: String,
    edited: String,
) -> Result<Vec<DictionaryExportEntry>> {
    let enabled = {
        let conn = state.db.lock().unwrap();
        repositories::get_setting(&conn, "auto_learn")
            .unwrap_or(None)
            .map(|v| v != "false")
            .unwrap_or(true)
    };
    if !enabled {
        return Ok(Vec::new());
    }

    let learned = crate::core::dictionary::learn::extract_corrections(&original, &edited);
    if learned.is_empty() {
        return Ok(Vec::new());
    }

    let (stored, raw) = {
        let conn = state.db.lock().unwrap();
        let existing = repositories::list_dictionary_entries(&conn)?;
        let mut stored = Vec::new();

        for correction in learned {
            // Never shadow a rule the user wrote themselves.
            if existing
                .iter()
                .any(|e| e.phrase.eq_ignore_ascii_case(&correction.from))
            {
                continue;
            }
            let entry = DictionaryEntry {
                id: None,
                phrase: correction.from.clone(),
                replacement: correction.to.clone(),
                enabled: true,
                profile_id: None,
                created_at: String::new(),
            };
            repositories::insert_dictionary_entry(&conn, &entry)?;
            tracing::info!(
                from = %correction.from,
                to = %correction.to,
                "Learned a correction from a manual edit"
            );
            stored.push(DictionaryExportEntry {
                phrase: correction.from,
                replacement: correction.to,
                enabled: true,
            });
        }
        (stored, repositories::list_dictionary_entries(&conn)?)
    };

    refresh_engine(&state, raw).await;
    Ok(stored)
}

// ── Snippets ─────────────────────────────────────────────────────────────────
//
// No engine to refresh: the recording pipeline reads the table per utterance.
// It is a handful of rows, read under a lock that is already being taken for
// the per-app lookup, and it means an edit applies to the very next sentence.

#[tauri::command]
pub fn list_snippets(state: State<'_, AppState>) -> Result<Vec<Snippet>> {
    let conn = state.db.lock().unwrap();
    repositories::list_snippets(&conn)
}

/// Create a snippet (`id: None`) or update one. Returns its id.
#[tauri::command]
pub fn save_snippet(state: State<'_, AppState>, snippet: Snippet) -> Result<i64> {
    // A trigger with no words in it can never match, and a snippet that
    // expands to nothing would silently eat the utterance that triggered it.
    if !snippet.trigger.chars().any(char::is_alphanumeric) {
        return Err(EchoError::Config(
            "A snippet needs a trigger phrase with at least one word".into(),
        ));
    }
    if snippet.body.trim().is_empty() {
        return Err(EchoError::Config(
            "A snippet needs some text to insert".into(),
        ));
    }
    let conn = state.db.lock().unwrap();
    repositories::save_snippet(&conn, &snippet)
}

#[tauri::command]
pub fn delete_snippet(state: State<'_, AppState>, id: i64) -> Result<()> {
    let conn = state.db.lock().unwrap();
    repositories::delete_snippet(&conn, id)
}

#[cfg(test)]
mod tests {
    use super::DictionaryFile;

    /// Files exported before snippets existed are a bare array, and people
    /// keep those around as backups. They must go on importing.
    #[test]
    fn both_export_shapes_import() {
        let legacy = r#"[{"phrase":"k8s","replacement":"Kubernetes"}]"#;
        assert!(matches!(
            serde_json::from_str(legacy).unwrap(),
            DictionaryFile::Legacy(e) if e.len() == 1
        ));

        let current = r#"{"entries":[],"snippets":[{"trigger":"sign off","body":"Best,\nV"}]}"#;
        match serde_json::from_str(current).unwrap() {
            DictionaryFile::Current { snippets, .. } => {
                assert_eq!(snippets[0].body, "Best,\nV");
                assert!(snippets[0].enabled, "a missing flag means enabled");
            }
            DictionaryFile::Legacy(_) => panic!("read as the legacy shape"),
        }
    }
}
