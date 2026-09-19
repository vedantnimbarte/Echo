use tauri::State;

use crate::{
    error::Result,
    state::AppState,
    storage::{models::TranscriptionRecord, repositories},
};

#[tauri::command]
#[specta::specta]
pub fn get_history(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<TranscriptionRecord>> {
    let conn = state.db.lock().unwrap();
    repositories::list_history(&conn, limit.unwrap_or(100))
}

/// What dictation has added up to, for the stats card.
///
/// Derived from History, so it is empty when History is off — which is the
/// honest outcome: there is nothing to count, rather than a number invented
/// from somewhere else.
#[tauri::command]
#[specta::specta]
pub fn get_dictation_stats(
    state: State<'_, AppState>,
) -> Result<crate::storage::repositories::DictationStats> {
    let conn = state.db.lock().unwrap();
    crate::storage::repositories::dictation_stats(&conn)
}

/// The Insights page, in one call.
///
/// Same source and the same caveat as [`get_dictation_stats`]: it is History,
/// so it is empty when History is off.
#[tauri::command]
#[specta::specta]
pub fn get_insights(state: State<'_, AppState>) -> Result<crate::storage::repositories::Insights> {
    let conn = state.db.lock().unwrap();
    crate::storage::repositories::insights(&conn)
}

#[tauri::command]
#[specta::specta]
pub fn clear_history(state: State<'_, AppState>) -> Result<()> {
    let conn = state.db.lock().unwrap();
    repositories::clear_history(&conn)
}

/// Serialize history to a JSON file at the user-chosen path.
///
/// Exports everything, not the 100-row window the UI shows — an export the user
/// has to paginate is not an export.
#[tauri::command]
#[specta::specta]
pub async fn export_history(state: State<'_, AppState>, path: String) -> Result<()> {
    let records = {
        let conn = state.db.lock().unwrap();
        repositories::list_history(&conn, i64::MAX)?
    };
    let json = serde_json::to_string_pretty(&records)?;
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| crate::error::EchoError::Config(e.to_string()))?;
    Ok(())
}

/**
 * SOURCE OF TRUTH KEYWORDS: latency_summary, p50, p95
 * WHAT:  What Echo's own timings say about how fast it is on this machine.
 * WHY:   Measured, not claimed. The p50 and p95 come from the last 200
 *        dictations rather than from a benchmark, so the number is the one this
 *        user actually gets on this hardware with the model they have chosen —
 *        which is the only version of the number worth showing them.
 * WHERE: Rendered by the insights panel.
 */
#[tauri::command]
#[specta::specta]
pub fn latency_summary(
    state: State<'_, AppState>,
) -> Result<Vec<crate::core::telemetry::latency::StageLatency>> {
    use crate::core::telemetry::latency::{percentile, StageLatency};

    let conn = state
        .db
        .lock()
        .map_err(|_| crate::error::EchoError::Storage(rusqlite::Error::InvalidQuery))?;

    // Assembled here rather than in storage: deciding which stages are
    // user-facing is the registry's job, and storage sits below it.
    let mut out = Vec::new();
    for capability in crate::registry::capabilities() {
        for metric in &capability.metrics {
            if !metric.user_facing {
                continue;
            }
            let values =
                crate::storage::repositories::latency_values(&conn, metric.stage.as_str())?;
            if values.is_empty() {
                continue;
            }
            out.push(StageLatency {
                stage: metric.stage.as_str().to_string(),
                label: metric.label.clone(),
                p50_ms: percentile(&values, 0.5),
                p95_ms: percentile(&values, 0.95),
                samples: values.len() as u64,
            });
        }
    }
    Ok(out)
}
