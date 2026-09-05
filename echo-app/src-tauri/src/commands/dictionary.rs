use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    error::{EchoError, Result},
    state::AppState,
    storage::{models::DictionaryEntry, repositories},
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
pub async fn delete_dictionary_entry(
    state: State<'_, AppState>,
    id: i64,
) -> Result<()> {
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

/// Serialize all entries to a JSON file at the user-chosen path.
#[tauri::command]
pub async fn export_dictionary(state: State<'_, AppState>, path: String) -> Result<()> {
    let raw = {
        let conn = state.db.lock().unwrap();
        repositories::list_dictionary_entries(&conn)?
    };

    let export: Vec<DictionaryExportEntry> = raw
        .into_iter()
        .map(|e| DictionaryExportEntry {
            phrase: e.phrase,
            replacement: e.replacement,
            enabled: e.enabled,
        })
        .collect();

    let json = serde_json::to_string_pretty(&export)?;
    std::fs::write(&path, json).map_err(|e| EchoError::Config(e.to_string()))?;
    Ok(())
}

/// Read a JSON file and insert entries whose phrase isn't already present
/// (case-insensitive). Returns the number of entries added.
#[tauri::command]
pub async fn import_dictionary(state: State<'_, AppState>, path: String) -> Result<usize> {
    let contents = std::fs::read_to_string(&path).map_err(|e| EchoError::Config(e.to_string()))?;
    let imported: Vec<DictionaryExportEntry> = serde_json::from_str(&contents)?;

    let (added, raw) = {
        let conn = state.db.lock().unwrap();
        let existing: std::collections::HashSet<String> =
            repositories::list_dictionary_entries(&conn)?
                .into_iter()
                .map(|e| e.phrase.to_lowercase())
                .collect();

        let mut added = 0usize;
        for entry in imported {
            if entry.phrase.trim().is_empty()
                || existing.contains(&entry.phrase.to_lowercase())
            {
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
