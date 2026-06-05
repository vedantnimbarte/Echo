use tauri::State;

use crate::{
    error::Result,
    state::AppState,
    storage::{models::DictionaryEntry, repositories},
};

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
    let (id, core_entries) = {
        let conn = state.db.lock().unwrap();
        let id = repositories::insert_dictionary_entry(&conn, &entry)?;
        let raw = repositories::list_dictionary_entries(&conn)?;
        (id, raw)
    };

    let entries = core_entries
        .into_iter()
        .map(|e| crate::core::dictionary::DictionaryEntry {
            id: e.id,
            phrase: e.phrase,
            replacement: e.replacement,
            enabled: e.enabled,
        })
        .collect();
    state.dictionary.write().await.update_entries(entries);

    Ok(id)
}

#[tauri::command]
pub async fn delete_dictionary_entry(
    state: State<'_, AppState>,
    id: i64,
) -> Result<()> {
    let core_entries = {
        let conn = state.db.lock().unwrap();
        repositories::delete_dictionary_entry(&conn, id)?;
        repositories::list_dictionary_entries(&conn)?
    };

    let entries = core_entries
        .into_iter()
        .map(|e| crate::core::dictionary::DictionaryEntry {
            id: e.id,
            phrase: e.phrase,
            replacement: e.replacement,
            enabled: e.enabled,
        })
        .collect();
    state.dictionary.write().await.update_entries(entries);

    Ok(())
}
