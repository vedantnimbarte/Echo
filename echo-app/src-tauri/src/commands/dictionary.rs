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
async fn refresh_engine(state: &AppState, raw: Vec<DictionaryEntry>) {
    let entries = raw
        .into_iter()
        .map(|e| crate::core::dictionary::DictionaryEntry {
            id: e.id,
            phrase: e.phrase,
            replacement: e.replacement,
            enabled: e.enabled,
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
