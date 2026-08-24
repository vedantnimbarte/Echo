//! Per-app profiles, and the dictionary profiles they can point at.
//!
//! A per-app profile overrides delivery behaviour while a given application is
//! focused. Every override is optional: a `None` field inherits the global
//! setting, so a profile can pin one behaviour (say, never auto-inject into a
//! password manager) without freezing everything else.

use tauri::State;

use crate::{
    core::appcontext,
    error::Result,
    state::AppState,
    storage::{
        models::{AppProfile, Profile},
        repositories,
    },
};

/// The identifier of the currently focused app, for the "add current app"
/// affordance in settings. `None` when the platform can't tell us — Wayland,
/// or macOS without Automation permission.
#[tauri::command]
pub async fn get_foreground_app() -> Result<Option<String>> {
    // The macOS and Linux lookups shell out, so keep them off the async runtime.
    Ok(tokio::task::spawn_blocking(appcontext::foreground_app)
        .await
        .ok()
        .flatten())
}

// ── Per-app profiles ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_app_profiles(state: State<'_, AppState>) -> Result<Vec<AppProfile>> {
    let conn = state.db.lock().unwrap();
    repositories::list_app_profiles(&conn)
}

/// Create or update the profile for an application. `app_match` is the
/// identifier [`get_foreground_app`] reports, and is stored lowercased.
#[tauri::command]
pub fn save_app_profile(state: State<'_, AppState>, profile: AppProfile) -> Result<i64> {
    if profile.app_match.trim().is_empty() {
        return Err(crate::error::EchoError::Config(
            "An app profile needs an application to match".into(),
        ));
    }
    let conn = state.db.lock().unwrap();
    repositories::upsert_app_profile(&conn, &profile)
}

#[tauri::command]
pub fn delete_app_profile(state: State<'_, AppState>, id: i64) -> Result<()> {
    let conn = state.db.lock().unwrap();
    repositories::delete_app_profile(&conn, id)
}

// ── Dictionary profiles ──────────────────────────────────────────────────────

#[tauri::command]
pub fn list_profiles(state: State<'_, AppState>) -> Result<Vec<Profile>> {
    let conn = state.db.lock().unwrap();
    repositories::list_profiles(&conn)
}

#[tauri::command]
pub fn add_profile(state: State<'_, AppState>, name: String) -> Result<i64> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(crate::error::EchoError::Config(
            "A profile needs a name".into(),
        ));
    }
    let conn = state.db.lock().unwrap();
    repositories::insert_profile(&conn, &name)
}

/// Delete a dictionary profile. Entries scoped to it become global again
/// (`ON DELETE CASCADE` is not used here — losing a user's phrases because they
/// deleted a grouping would be the wrong trade).
#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, id: i64) -> Result<()> {
    let raw = {
        let conn = state.db.lock().unwrap();
        conn.execute(
            "UPDATE dictionary_entries SET profile_id = NULL WHERE profile_id = ?1",
            rusqlite::params![id],
        )?;
        repositories::delete_profile(&conn, id)?;
        repositories::list_dictionary_entries(&conn)?
    };
    crate::commands::dictionary::refresh_engine(state.inner(), raw).await;
    Ok(())
}

/// Move a dictionary entry into a profile, or back to global with `None`.
#[tauri::command]
pub async fn set_dictionary_entry_profile(
    state: State<'_, AppState>,
    id: i64,
    profile_id: Option<i64>,
) -> Result<()> {
    let raw = {
        let conn = state.db.lock().unwrap();
        repositories::set_dictionary_entry_profile(&conn, id, profile_id)?;
        repositories::list_dictionary_entries(&conn)?
    };
    crate::commands::dictionary::refresh_engine(state.inner(), raw).await;
    Ok(())
}
