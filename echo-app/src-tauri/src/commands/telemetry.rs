use tauri::State;

use crate::{
    core::telemetry::TelemetrySummaryItem,
    error::Result,
    state::AppState,
    storage::repositories,
};

/// Counts of locally stored telemetry events, grouped by type.
#[tauri::command]
pub fn get_telemetry_summary(state: State<'_, AppState>) -> Result<Vec<TelemetrySummaryItem>> {
    let conn = state.db.lock().unwrap();
    Ok(state.telemetry.summary(&conn)?)
}

/// Delete all locally stored telemetry events.
#[tauri::command]
pub fn clear_telemetry(state: State<'_, AppState>) -> Result<()> {
    let conn = state.db.lock().unwrap();
    Ok(state.telemetry.clear(&conn)?)
}

/// Enable or disable telemetry collection (persists the choice).
#[tauri::command]
pub fn set_telemetry_enabled(state: State<'_, AppState>, enabled: bool) -> Result<()> {
    {
        let conn = state.db.lock().unwrap();
        repositories::set_setting(&conn, "telemetry_enabled", if enabled { "true" } else { "false" })?;
    }
    state.telemetry.set_enabled(enabled);
    Ok(())
}

/// Record a usage event from the frontend (e.g. transcription_complete). Only
/// non-sensitive metadata should be passed — never transcript text or audio.
#[tauri::command]
pub fn record_telemetry_event(
    state: State<'_, AppState>,
    event_type: String,
    payload: Option<serde_json::Value>,
) -> Result<()> {
    let conn = state.db.lock().unwrap();
    state.telemetry.record(&conn, &event_type, payload);
    Ok(())
}
