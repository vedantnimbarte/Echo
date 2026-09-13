use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    core::lock::LockLive,
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
    load_engine(state, raw).await;
    // Every user edit to the dictionary comes through here, which makes this
    // the one place to tell sync there is something new to write. Sync itself
    // calls `load_engine`, so applying another machine's changes does not
    // trigger a sync of its own.
    SYNC_WANTED.notify_one();
}

/// Also where enabled dictionary plugins contribute: their entries are
/// appended after the user's, so a rule the user wrote for the same phrase
/// runs first and wins. Asked here rather than per transcript, which keeps
/// plugin code off the transcript path entirely. They join only the engine,
/// never the database, so sync never writes a plugin's entries to the shared
/// file — another machine may not have the plugin.
async fn load_engine(state: &AppState, raw: Vec<DictionaryEntry>) {
    let from_plugins = {
        // Snapshot, then release the loader before any plugin code runs.
        let plugins = state.plugins.lock_live().plugins();
        crate::core::plugins::dispatch::dictionary_entries(&plugins)
    };
    let entries = raw
        .into_iter()
        .map(|e| crate::core::dictionary::DictionaryEntry {
            id: e.id,
            phrase: e.phrase,
            replacement: e.replacement,
            enabled: e.enabled,
            profile_id: e.profile_id,
        })
        .chain(from_plugins)
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

// ── Sync through a folder ────────────────────────────────────────────────────
//
// The merge lives in `core::dictionary::sync`. This is when it runs: once at
// startup, a few seconds after the dictionary changes, and whenever the file in
// the folder does.

use crate::core::dictionary::sync;

/// Poked by [`refresh_engine`]. A `Notify` keeps one pending wake-up when nobody
/// is waiting, so an edit made while a sync is running is not lost.
static SYNC_WANTED: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// One sync at a time, whichever of the three triggers asked for it.
static SYNC_RUNNING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Quiet period after an edit before syncing. Adding an entry can be several
/// quick commands in a row (add, move to a profile, toggle), and each would
/// otherwise write the file and send it to every other machine.
const SYNC_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(3);

/// How often the folder is checked for another machine's writes. A `stat` of
/// one or two files, so this can be frequent; the syncing service's own delay
/// is longer than this anyway.
const SYNC_POLL: std::time::Duration = std::time::Duration::from_secs(30);

const KEY_STATUS: &str = "dictionary_sync_status";

/// What the Sync section shows. Persisted, so "last synced" survives a restart.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DictionarySyncStatus {
    pub last_synced_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_error: Option<String>,
    /// Conflicted copies merged in on the last sync, by file name.
    #[serde(default)]
    pub conflict_copies: Vec<String>,
}

/// The configured folder, when sync is switched on and has one.
fn sync_folder(state: &AppState) -> Option<std::path::PathBuf> {
    let conn = state.db.lock().unwrap();
    let get = |k: &str| repositories::get_setting(&conn, k).ok().flatten();
    let enabled = get(sync::KEY_ENABLED).as_deref() == Some("true");
    let folder = get(sync::KEY_FOLDER).filter(|f| !f.trim().is_empty())?;
    enabled.then(|| folder.into())
}

fn load_status(state: &AppState) -> DictionarySyncStatus {
    let conn = state.db.lock().unwrap();
    repositories::get_setting(&conn, KEY_STATUS)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Run one sync if it is switched on, record how it went, and tell the window.
async fn run_sync(app: &tauri::AppHandle) -> DictionarySyncStatus {
    use tauri::{Emitter, Manager};

    let _running = SYNC_RUNNING.lock().await;
    let state = app.state::<AppState>();
    let mut status = load_status(&state);
    let Some(folder) = sync_folder(&state) else {
        return status;
    };

    let handle = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        sync::sync(&handle.state::<AppState>().db, &folder, chrono::Utc::now())
    })
    .await
    .unwrap_or_else(|e| Err(EchoError::Config(format!("Sync stopped unexpectedly: {e}"))));

    match result {
        Ok(outcome) => {
            if outcome.changed_local {
                let raw = {
                    let conn = state.db.lock().unwrap();
                    repositories::list_dictionary_entries(&conn).unwrap_or_default()
                };
                load_engine(&state, raw).await;
            }
            if !outcome.conflict_copies.is_empty() {
                tracing::info!(copies = ?outcome.conflict_copies, "Merged conflicted dictionary copies");
            }
            status = DictionarySyncStatus {
                last_synced_at: Some(chrono::Utc::now()),
                last_error: None,
                conflict_copies: outcome.conflict_copies,
            };
        }
        Err(e) => {
            // `last_synced_at` is kept: when it last *worked* is what the user
            // needs to see next to an error.
            tracing::error!("Dictionary sync failed: {e}");
            status.last_error = Some(e.to_string());
        }
    }

    if let Ok(json) = serde_json::to_string(&status) {
        let conn = state.db.lock().unwrap();
        let _ = repositories::set_setting(&conn, KEY_STATUS, &json);
    }
    let _ = app.emit("echo://dictionary-synced", &status);
    status
}

/// Start the sync loop: once now, then after edits and when the folder changes.
pub fn start_sync(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        use tauri::Manager;

        let mut poll = tokio::time::interval(SYNC_POLL);
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // What the folder looked like after the last sync. `None` until the
        // first one, so the first tick — which is immediate — syncs at startup.
        let mut seen = None;

        loop {
            tokio::select! {
                _ = poll.tick() => {
                    let Some(folder) = sync_folder(&app.state::<AppState>()) else {
                        seen = None;
                        continue;
                    };
                    if seen.as_ref() == Some(&sync::fingerprint(&folder)) {
                        continue;
                    }
                }
                _ = SYNC_WANTED.notified() => {
                    // Wait for the edits to stop before writing.
                    while tokio::time::timeout(SYNC_DEBOUNCE, SYNC_WANTED.notified())
                        .await
                        .is_ok()
                    {}
                }
            }
            run_sync(&app).await;
            seen = sync_folder(&app.state::<AppState>()).map(|f| sync::fingerprint(&f));
        }
    });
}

/// Sync now, from the button. The window also calls this after the folder or
/// the switch changes, so the result shows at once rather than on the next poll.
#[tauri::command]
pub async fn sync_dictionary_now(app: tauri::AppHandle) -> Result<DictionarySyncStatus> {
    Ok(run_sync(&app).await)
}

#[tauri::command]
pub fn get_dictionary_sync_status(state: State<'_, AppState>) -> DictionarySyncStatus {
    load_status(&state)
}
