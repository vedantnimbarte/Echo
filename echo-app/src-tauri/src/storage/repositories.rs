use rusqlite::{params, Connection, OptionalExtension};

use super::models::{
    AppProfile, DictionaryEntry, EgressRecord, Profile, Snippet, TranscriptionRecord,
};
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

// ── Snippets ─────────────────────────────────────────────────────────────────

/// Every snippet in the order it was created, which is also the order a
/// duplicate trigger is resolved in: the older one wins.
pub fn list_snippets(conn: &Connection) -> Result<Vec<Snippet>> {
    let mut stmt = conn.prepare("SELECT id, trigger, body, enabled FROM snippets ORDER BY id")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Snippet {
                id: r.get(0)?,
                trigger: r.get(1)?,
                body: r.get(2)?,
                enabled: r.get::<_, i64>(3)? != 0,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Insert a snippet, or rewrite the one `s.id` names. Returns its id.
pub fn save_snippet(conn: &Connection, s: &Snippet) -> Result<i64> {
    match s.id {
        Some(id) => {
            conn.execute(
                "UPDATE snippets SET trigger = ?2, body = ?3, enabled = ?4 WHERE id = ?1",
                params![id, s.trigger, s.body, s.enabled as i64],
            )?;
            Ok(id)
        }
        None => {
            conn.execute(
                "INSERT INTO snippets (trigger, body, enabled) VALUES (?1, ?2, ?3)",
                params![s.trigger, s.body, s.enabled as i64],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }
}

pub fn delete_snippet(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM snippets WHERE id = ?1", params![id])?;
    Ok(())
}

// ── History ───────────────────────────────────────────────────────────────────

pub fn insert_history(conn: &Connection, record: &TranscriptionRecord) -> Result<i64> {
    conn.execute(
        "INSERT INTO transcription_history
            (text, language, provider, duration_ms, app, dictionary_fixes, cleanup_fixes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            record.text,
            record.language,
            record.provider,
            record.duration_ms,
            record.app,
            record.dictionary_fixes,
            record.cleanup_fixes,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_history(conn: &Connection, limit: i64) -> Result<Vec<TranscriptionRecord>> {
    // `created_at` only resolves to the second, so several dictations in the
    // same second tie and SQLite may return them in any order — in practice
    // oldest-first, the exact opposite of what History shows. `id DESC` breaks
    // the tie by insertion order, matching `list_egress`.
    let mut stmt = conn.prepare(
        "SELECT id, text, language, provider, created_at,
                duration_ms, app, dictionary_fixes, cleanup_fixes
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
                duration_ms: r.get(5)?,
                app: r.get(6)?,
                dictionary_fixes: r.get(7)?,
                cleanup_fixes: r.get(8)?,
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

/// Apply the configured retention window, reading it from settings.
///
/// One function rather than the policy being assembled at each call site, so the
/// two callers cannot drift apart: startup — which also catches a window the
/// user just shortened — and every history write, which is what makes the
/// setting true at all for an app designed to stay open for weeks.
///
/// An unset or unparseable value means keep everything, matching
/// [`trim_history_older_than`]: retention is opt-in, and a typo in a setting
/// must not delete anybody's transcripts.
///
/// ponytail: this runs a dated `DELETE` after every utterance rather than on a
/// timer. The table holds one row per dictation and the statement is a scan of
/// it, which is nothing at the sizes this reaches; a timer would be more code
/// and one more thing to get wrong at shutdown. Index `created_at` if a very
/// long-lived history ever makes it show up.
pub fn apply_retention(conn: &Connection) -> Result<usize> {
    let days = get_setting(conn, "history_retention_days")?
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    trim_history_older_than(conn, days)
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
        stream_partials: r.get::<_, Option<i64>>(5)?.map(|v| v != 0),
        formatting: r.get::<_, Option<i64>>(6)?.map(|v| v != 0),
        profile_id: r.get(7)?,
        enabled: r.get::<_, i64>(8)? != 0,
    })
}

pub fn list_app_profiles(conn: &Connection) -> Result<Vec<AppProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, app_match, label, auto_inject, injection_method, stream_partials,
                formatting, profile_id, enabled
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
        "SELECT id, app_match, label, auto_inject, injection_method, stream_partials,
                formatting, profile_id, enabled
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
            (app_match, label, auto_inject, injection_method, stream_partials,
             formatting, profile_id, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(app_match) DO UPDATE SET
            label = excluded.label,
            auto_inject = excluded.auto_inject,
            injection_method = excluded.injection_method,
            stream_partials = excluded.stream_partials,
            formatting = excluded.formatting,
            profile_id = excluded.profile_id,
            enabled = excluded.enabled
         RETURNING id",
        params![
            p.app_match.to_lowercase(),
            p.label,
            p.auto_inject.map(|v| v as i64),
            p.injection_method,
            p.stream_partials.map(|v| v as i64),
            p.formatting.map(|v| v as i64),
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

/// What dictation has added up to, derived from History.
///
/// Derived rather than counted separately: History already is the record, and a
/// second tally would be one more thing to keep in step with it. The cost is
/// that turning History off turns these off too, which the UI says.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DictationStats {
    pub transcripts: i64,
    pub words: i64,
    /// Distinct days with at least one transcript.
    pub days: i64,
    pub words_last_7_days: i64,
    /// Earliest transcript still stored, ISO-8601. History retention trims old
    /// rows, so this is "since when the record goes back", not "since install".
    pub since: Option<String>,
}

/// Word counts are computed in SQL rather than by reading every transcript into
/// memory: the point of a summary is not to load the thing it summarises.
///
/// The count is spaces-plus-one, which is an approximation — it over-counts
/// double spaces and under-counts hyphenates. Good enough for "you have
/// dictated about 40,000 words", which is the only claim being made.
const WORDS: &str =
    "CASE WHEN trim(text) = '' THEN 0      ELSE length(trim(text)) - length(replace(trim(text), ' ', '')) + 1 END";

pub fn dictation_stats(conn: &Connection) -> Result<DictationStats> {
    let totals = format!(
        "SELECT count(*), COALESCE(sum({WORDS}), 0), count(DISTINCT date(created_at)), min(created_at)
         FROM transcription_history"
    );
    let (transcripts, words, days, since) = conn.query_row(&totals, [], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })?;

    let recent = format!(
        "SELECT COALESCE(sum({WORDS}), 0) FROM transcription_history
         WHERE created_at >= datetime('now', '-7 days')"
    );
    let words_last_7_days = conn.query_row(&recent, [], |r| r.get(0))?;

    Ok(DictationStats {
        transcripts,
        words,
        days,
        words_last_7_days,
        since,
    })
}

// ── Insights ─────────────────────────────────────────────────────────────────

/// One row of a "how much of it was X" breakdown — an app, a provider, a
/// language. Three questions with the same shape, so they share one answer.
#[derive(Debug, serde::Serialize)]
pub struct Tally {
    pub key: String,
    pub transcripts: i64,
    pub words: i64,
}

/// A single day's dictation, for the calendar.
#[derive(Debug, serde::Serialize)]
pub struct DayWords {
    /// ISO `YYYY-MM-DD`, local to whatever SQLite considers "now".
    pub date: String,
    pub words: i64,
    pub transcripts: i64,
}

/// Everything the Insights page shows, in one round trip.
///
/// Same source as [`dictation_stats`] — History — and the same consequence:
/// with History off there is nothing to count, and the page says so rather
/// than inventing numbers from a second tally nobody can inspect or delete.
#[derive(Debug, serde::Serialize)]
pub struct Insights {
    pub transcripts: i64,
    pub words: i64,
    pub days: i64,
    pub words_last_7_days: i64,
    pub since: Option<String>,

    /// Speech time and the words spoken in it, counted only over rows that
    /// actually carry a duration. Words per minute is their ratio, and doing
    /// it this way keeps rows recorded before durations existed from dragging
    /// the rate down to nothing.
    pub spoken_ms: i64,
    pub timed_words: i64,
    pub timed_transcripts: i64,

    pub dictionary_fixes: i64,
    pub cleanup_fixes: i64,

    /// Consecutive days up to today (or yesterday, if today is still empty).
    pub streak: i64,
    pub longest_streak: i64,

    /// Busiest first, and only what is known: apps go unrecorded on a machine
    /// where Echo cannot see the focused window.
    pub apps: Vec<Tally>,
    pub providers: Vec<Tally>,
    pub languages: Vec<Tally>,

    /// Transcripts per hour of the day, 24 buckets starting at midnight.
    pub hours: Vec<i64>,

    /// The last 365 days that had any dictation, oldest first.
    pub daily: Vec<DayWords>,
}

/// Group by one column, busiest first. `NULL` keys are left out — "unknown" is
/// not a category the user can act on, and counting it as one would put a
/// fictional app at the top of the list on Linux.
fn tally(conn: &Connection, column: &str) -> Result<Vec<Tally>> {
    let sql = format!(
        "SELECT {column}, count(*), COALESCE(sum({WORDS}), 0)
         FROM transcription_history
         WHERE {column} IS NOT NULL AND trim({column}) <> ''
         GROUP BY {column} ORDER BY count(*) DESC, {column} LIMIT 12"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Tally {
                key: r.get(0)?,
                transcripts: r.get(1)?,
                words: r.get(2)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Current and longest run of consecutive days, from day numbers ascending.
///
/// `today` is passed in rather than read here so the calculation is a pure
/// function of its inputs — which is the only way to test the "you dictated
/// yesterday but not yet today" case, the one a naive version gets wrong by
/// resetting a long streak at midnight.
fn streaks(days: &[i64], today: i64) -> (i64, i64) {
    let mut longest = 0i64;
    let mut run = 0i64;
    let mut current = 0i64;

    for (i, day) in days.iter().enumerate() {
        run = if i > 0 && day - days[i - 1] == 1 {
            run + 1
        } else {
            1
        };
        longest = longest.max(run);
        // A run counts as live if it reaches today or stopped at yesterday;
        // anything older has been broken by a day with nothing in it.
        if today - day <= 1 {
            current = run;
        }
    }
    (current, longest)
}

pub fn insights(conn: &Connection) -> Result<Insights> {
    let base = dictation_stats(conn)?;

    let totals = format!(
        "SELECT COALESCE(sum(duration_ms), 0),
                COALESCE(sum(CASE WHEN duration_ms IS NULL THEN 0 ELSE {WORDS} END), 0),
                COALESCE(sum(CASE WHEN duration_ms IS NULL THEN 0 ELSE 1 END), 0),
                COALESCE(sum(dictionary_fixes), 0),
                COALESCE(sum(cleanup_fixes), 0)
         FROM transcription_history"
    );
    let (spoken_ms, timed_words, timed_transcripts, dictionary_fixes, cleanup_fixes) = conn
        .query_row(&totals, [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?;

    // Day numbers come from SQLite rather than being parsed back out of the
    // date string: julianday already knows how many days apart two dates are,
    // including across months and leap years.
    let daily_sql = format!(
        "SELECT date(created_at),
                CAST(julianday(date(created_at)) AS INTEGER),
                COALESCE(sum({WORDS}), 0),
                count(*)
         FROM transcription_history
         WHERE created_at >= datetime('now', '-365 days')
         GROUP BY date(created_at) ORDER BY date(created_at)"
    );
    let mut stmt = conn.prepare(&daily_sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get(2)?,
                r.get(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let day_numbers: Vec<i64> = rows.iter().map(|(_, n, _, _)| *n).collect();
    let today: i64 = conn.query_row("SELECT CAST(julianday(date('now')) AS INTEGER)", [], |r| {
        r.get(0)
    })?;
    let (streak, longest_streak) = streaks(&day_numbers, today);

    let daily = rows
        .into_iter()
        .map(|(date, _, words, transcripts)| DayWords {
            date,
            words,
            transcripts,
        })
        .collect();

    let mut hours = vec![0i64; 24];
    let mut stmt = conn.prepare(
        "SELECT CAST(strftime('%H', created_at) AS INTEGER), count(*)
         FROM transcription_history GROUP BY 1",
    )?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
        let (hour, n) = row?;
        if let Some(slot) = hours.get_mut(hour as usize) {
            *slot = n;
        }
    }

    Ok(Insights {
        transcripts: base.transcripts,
        words: base.words,
        days: base.days,
        words_last_7_days: base.words_last_7_days,
        since: base.since,
        spoken_ms,
        timed_words,
        timed_transcripts,
        dictionary_fixes,
        cleanup_fixes,
        streak,
        longest_streak,
        apps: tally(conn, "app")?,
        providers: tally(conn, "provider")?,
        languages: tally(conn, "language")?,
        hours,
        daily,
    })
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
    lib_sha256: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO plugins (name, version, enabled, manifest, lib_sha256)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(name) DO UPDATE SET version = excluded.version,
            enabled = excluded.enabled, manifest = excluded.manifest,
            lib_sha256 = excluded.lib_sha256",
        params![name, version, enabled as i64, manifest, lib_sha256],
    )?;
    Ok(())
}

/// Record the fingerprint of an already-installed plugin without touching
/// anything else about it. Used to adopt one that predates the column.
pub fn set_plugin_fingerprint(conn: &Connection, name: &str, sha256: &str) -> Result<()> {
    conn.execute(
        "UPDATE plugins SET lib_sha256 = ?2 WHERE name = ?1",
        params![name, sha256],
    )?;
    Ok(())
}

/// One installed plugin, as the loader needs it.
pub struct InstalledPlugin {
    pub name: String,
    pub enabled: bool,
    pub manifest: String,
    /// `None` for a plugin installed before fingerprints were recorded.
    pub lib_sha256: Option<String>,
}

/// Every installed plugin, in name order.
pub fn list_plugins(conn: &Connection) -> Result<Vec<InstalledPlugin>> {
    let mut stmt =
        conn.prepare("SELECT name, enabled, manifest, lib_sha256 FROM plugins ORDER BY name")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(InstalledPlugin {
                name: r.get(0)?,
                enabled: r.get::<_, i64>(1)? != 0,
                manifest: r.get(2)?,
                lib_sha256: r.get(3)?,
            })
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

#[cfg(test)]
mod streak_tests {
    use super::streaks;

    #[test]
    fn a_run_that_ends_yesterday_is_still_live() {
        // Days 8, 9, 10 with "today" = 11: three days, unbroken, not yet
        // dictated into today.
        assert_eq!(streaks(&[8, 9, 10], 11), (3, 3));
    }

    #[test]
    fn a_gap_breaks_the_current_streak_but_keeps_the_record() {
        // A four-day run in the past, then a single day today.
        assert_eq!(streaks(&[1, 2, 3, 4, 10], 10), (1, 4));
    }

    #[test]
    fn nothing_recorded_is_no_streak() {
        assert_eq!(streaks(&[], 10), (0, 0));
        // Last dictation was three days ago.
        assert_eq!(streaks(&[5, 6, 7], 10), (0, 3));
    }
}
