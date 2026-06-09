use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Aggregated count of telemetry events of one type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySummaryItem {
    pub event_type: String,
    pub count: i64,
}

/// Privacy-preserving local usage telemetry.
///
/// Events are stored only in the local SQLite database — nothing is sent
/// anywhere. Recording is gated by a runtime flag mirrored from the
/// `telemetry_enabled` setting. Never record raw audio, transcript text, file
/// paths, or window titles.
pub struct TelemetryService {
    enabled: AtomicBool,
}

impl TelemetryService {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Record an event if telemetry is enabled. Failures are swallowed —
    /// telemetry must never break the app.
    pub fn record(&self, conn: &Connection, event_type: &str, payload: Option<Value>) {
        if !self.is_enabled() {
            return;
        }
        let payload_str = payload.map(|p| p.to_string());
        let _ = conn.execute(
            "INSERT INTO telemetry_events (event_type, payload) VALUES (?1, ?2)",
            params![event_type, payload_str],
        );
    }

    pub fn summary(&self, conn: &Connection) -> rusqlite::Result<Vec<TelemetrySummaryItem>> {
        let mut stmt = conn.prepare(
            "SELECT event_type, COUNT(*) FROM telemetry_events GROUP BY event_type ORDER BY event_type",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(TelemetrySummaryItem {
                    event_type: r.get(0)?,
                    count: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn clear(&self, conn: &Connection) -> rusqlite::Result<()> {
        conn.execute("DELETE FROM telemetry_events", [])?;
        Ok(())
    }
}
