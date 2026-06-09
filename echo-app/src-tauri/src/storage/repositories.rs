use rusqlite::{params, Connection, OptionalExtension};

use super::models::{DictionaryEntry, Setting, TranscriptionRecord};
use crate::error::{EchoError, Result};

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
    let mut stmt = conn.prepare(
        "SELECT id, text, language, provider, created_at
         FROM transcription_history ORDER BY created_at DESC LIMIT ?1",
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
