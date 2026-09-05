use rusqlite::{params, Connection, OptionalExtension};

use super::models::{AppProfile, DictionaryEntry, EgressRecord, Profile, TranscriptionRecord};
use crate::error::Result;

// ── Settings ─────────────────────────────────────────────────────────────────

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let result = stmt
        .query_row(params![key], |r| r.get::<_, String>(0))
        .optional()?;
    Ok(result)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// ── Dictionary ────────────────────────────────────────────────────────────────

pub fn list_dictionary_entries(conn: &Connection) -> Result<Vec<DictionaryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, phrase, replacement, enabled, profile_id, created_at
         FROM dictionary_entries ORDER BY id",
    )?;
    let entries = stmt
        .query_map([], |r| {
            Ok(DictionaryEntry {
                id: r.get(0)?,
                phrase: r.get(1)?,
                replacement: r.get(2)?,
                enabled: r.get::<_, i64>(3)? != 0,
                profile_id: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(entries)
}

pub fn insert_dictionary_entry(conn: &Connection, entry: &DictionaryEntry) -> Result<i64> {
    conn.execute(
        "INSERT INTO dictionary_entries (phrase, replacement, enabled, profile_id)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            entry.phrase,
            entry.replacement,
            entry.enabled as i64,
            entry.profile_id,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_dictionary_entry(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM dictionary_entries WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn set_dictionary_entry_enabled(conn: &Connection, id: i64, enabled: bool) -> Result<()> {
    conn.execute(
        "UPDATE dictionary_entries SET enabled = ?2 WHERE id = ?1",
        params![id, enabled as i64],
    )?;
    Ok(())
}

// ── History ───────────────────────────────────────────────────────────────────

pub fn insert_history(conn: &Connection, record: &TranscriptionRecord) -> Result<i64> {
    conn.execute(
        "INSERT INTO transcription_history (text, language, provider)
         VALUES (?1, ?2, ?3)",
        params![record.text, record.language, record.provider],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_history(conn: &Connection, limit: i64) -> Result<Vec<TranscriptionRecord>> {
    // `created_at` only resolves to the second, so several dictations in the
    // same second tie and SQLite may return them in any order — in practice
    // oldest-first, the exact opposite of what History shows. `id DESC` breaks
    // the tie by insertion order, matching `list_egress`.
    let mut stmt = conn.prepare(
        "SELECT id, text, language, provider, created_at
         FROM transcription_history ORDER BY created_at DESC, id DESC LIMIT ?1",
    )?;
    let records = stmt
        .query_map(params![limit], |r| {
            Ok(TranscriptionRecord {
                id: r.get(0)?,
                text: r.get(1)?,
                language: r.get(2)?,
                provider: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(records)
}

pub fn clear_history(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM transcription_history", [])?;
    Ok(())
}

/// Delete history older than `days`, returning how many rows went.
///
/// Retention is a privacy feature before it is a housekeeping one: transcripts
/// are a verbatim record of everything the user has said to their computer, and
/// keeping them forever by default is a decision nobody consciously made. Zero
/// or a negative value means "keep everything" — the caller decides whether to
/// call at all, but a nonsense value must not silently wipe the history.
pub fn trim_history_older_than(conn: &Connection, days: i64) -> Result<usize> {
    if days <= 0 {
        return Ok(0);
    }
    let removed = conn.execute(
        "DELETE FROM transcription_history
         WHERE created_at < datetime('now', ?1)",
        params![format!("-{days} days")],
    )?;
    Ok(removed)
}

// ── Dictionary profiles ──────────────────────────────────────────────────────
//
// `profiles` has existed since migration 1 but had no queries; per-app profiles
// are the first feature that needs them.

pub fn list_profiles(conn: &Connection) -> Result<Vec<Profile>> {
    let mut stmt =
        conn.prepare("SELECT id, name, created_at, updated_at FROM profiles ORDER BY name")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Profile {
                id: r.get(0)?,
                name: r.get(1)?,
                created_at: r.get(2)?,
                updated_at: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn insert_profile(conn: &Connection, name: &str) -> Result<i64> {
    conn.execute("INSERT INTO profiles (name) VALUES (?1)", params![name])?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_profile(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM profiles WHERE id = ?1", params![id])?;
    Ok(())
}

/// Move a dictionary entry into a profile, or back to global with `None`.
pub fn set_dictionary_entry_profile(
    conn: &Connection,
    id: i64,
    profile_id: Option<i64>,
) -> Result<()> {
    conn.execute(
        "UPDATE dictionary_entries SET profile_id = ?2 WHERE id = ?1",
        params![id, profile_id],
    )?;
    Ok(())
}

// ── Per-app profiles ─────────────────────────────────────────────────────────

fn row_to_app_profile(r: &rusqlite::Row) -> rusqlite::Result<AppProfile> {
    Ok(AppProfile {
        id: r.get(0)?,
        app_match: r.get(1)?,
        label: r.get(2)?,
        auto_inject: r.get::<_, Option<i64>>(3)?.map(|v| v != 0),
        injection_method: r.get(4)?,
        profile_id: r.get(5)?,
        enabled: r.get::<_, i64>(6)? != 0,
    })
}

pub fn list_app_profiles(conn: &Connection) -> Result<Vec<AppProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, app_match, label, auto_inject, injection_method, profile_id, enabled
         FROM app_profiles ORDER BY app_match",
    )?;
    let rows = stmt
        .query_map([], |r| row_to_app_profile(r))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The enabled profile matching `app_match`, if any. Matching is exact on the
/// lowercased identifier the platform layer reports.
pub fn find_app_profile(conn: &Connection, app_match: &str) -> Result<Option<AppProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, app_match, label, auto_inject, injection_method, profile_id, enabled
         FROM app_profiles WHERE app_match = ?1 AND enabled = 1",
    )?;
    let row = stmt
        .query_row(params![app_match.to_lowercase()], |r| row_to_app_profile(r))
        .optional()?;
    Ok(row)
}

/// Insert or update the profile for an application, returning its id.
///
/// `RETURNING id` rather than `last_insert_rowid()`: on the conflict path no
/// insert happens, so the rowid counter still holds whatever was inserted last
/// on this connection — a different profile entirely. That id is handed to the
/// frontend, which uses it to address the profile afterwards, so returning the
/// wrong one points later edits and deletes at somebody else's row.
pub fn upsert_app_profile(conn: &Connection, p: &AppProfile) -> Result<i64> {
    let id = conn.query_row(
        "INSERT INTO app_profiles
            (app_match, label, auto_inject, injection_method, profile_id, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(app_match) DO UPDATE SET
            label = excluded.label,
            auto_inject = excluded.auto_inject,
            injection_method = excluded.injection_method,
            profile_id = excluded.profile_id,
            enabled = excluded.enabled
         RETURNING id",
        params![
            p.app_match.to_lowercase(),
            p.label,
            p.auto_inject.map(|v| v as i64),
            p.injection_method,
            p.profile_id,
            p.enabled as i64,
        ],
        |r| r.get(0),
    )?;
    Ok(id)
}

pub fn delete_app_profile(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM app_profiles WHERE id = ?1", params![id])?;
    Ok(())
}

// ── Egress log ───────────────────────────────────────────────────────────────

pub fn insert_egress(conn: &Connection, host: &str, purpose: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO egress_log (host, purpose) VALUES (?1, ?2)",
        params![host, purpose],
    )?;
    Ok(())
}

pub fn list_egress(conn: &Connection, limit: i64) -> Result<Vec<EgressRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, host, purpose, created_at
         FROM egress_log ORDER BY created_at DESC, id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit], |r| {
            Ok(EgressRecord {
                id: r.get(0)?,
                host: r.get(1)?,
                purpose: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn clear_egress(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM egress_log", [])?;
    Ok(())
}

/// Keep the log bounded — it is a rolling record, not an audit trail.
pub fn trim_egress(conn: &Connection, keep: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM egress_log WHERE id NOT IN
            (SELECT id FROM egress_log ORDER BY id DESC LIMIT ?1)",
        params![keep],
    )?;
    Ok(())
}

// ── Plugins ─────────────────────────────────────────────────────────────────

/// Insert or replace a plugin registry row. `manifest` is the raw plugin.json.
pub fn upsert_plugin(
    conn: &Connection,
    name: &str,
    version: &str,
    enabled: bool,
    manifest: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO plugins (name, version, enabled, manifest) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(name) DO UPDATE SET version = excluded.version,
            enabled = excluded.enabled, manifest = excluded.manifest",
        params![name, version, enabled as i64, manifest],
    )?;
    Ok(())
}

/// Returns (name, version, enabled, manifest) rows for all installed plugins.
pub fn list_plugins(conn: &Connection) -> Result<Vec<(String, String, bool, String)>> {
    let mut stmt =
        conn.prepare("SELECT name, version, enabled, manifest FROM plugins ORDER BY name")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? != 0,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set_plugin_enabled(conn: &Connection, name: &str, enabled: bool) -> Result<()> {
    conn.execute(
        "UPDATE plugins SET enabled = ?2 WHERE name = ?1",
        params![name, enabled as i64],
    )?;
    Ok(())
}

pub fn delete_plugin(conn: &Connection, name: &str) -> Result<()> {
    conn.execute("DELETE FROM plugins WHERE name = ?1", params![name])?;
    Ok(())
}
