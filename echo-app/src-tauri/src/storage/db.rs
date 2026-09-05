use rusqlite::{Connection, Result};
use std::path::Path;

/// Opens (or creates) the SQLite database and runs all migrations.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Re-run the migrator against an existing connection.
///
/// `open` always starts from whatever is on disk, so the upgrade path — an
/// old database meeting new code, which is the one that reaches existing
/// users — can only be exercised by calling this directly.
#[cfg(test)]
pub fn migrate_for_test(conn: &Connection) -> Result<()> {
    migrate(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
    ")?;

    let version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if version < 1 {
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS profiles (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL UNIQUE,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS dictionary_entries (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                phrase      TEXT NOT NULL,
                replacement TEXT NOT NULL,
                enabled     INTEGER NOT NULL DEFAULT 1,
                profile_id  INTEGER REFERENCES profiles(id) ON DELETE CASCADE,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS transcription_history (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                text        TEXT NOT NULL,
                language    TEXT,
                provider    TEXT NOT NULL,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS telemetry_events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type  TEXT NOT NULL,
                payload     TEXT,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS plugins (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL UNIQUE,
                version     TEXT NOT NULL,
                enabled     INTEGER NOT NULL DEFAULT 1,
                manifest    TEXT NOT NULL,
                installed_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            INSERT INTO schema_migrations (version) VALUES (1);
        ")?;
    }

    if version < 2 {
        conn.execute_batch("
            -- Per-app overrides. A NULL column means \"inherit the global
            -- setting\", so a profile can pin one behaviour without freezing
            -- the rest. `app_match` is a lowercased executable / bundle id /
            -- window class, matched exactly.
            CREATE TABLE IF NOT EXISTS app_profiles (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                app_match        TEXT NOT NULL UNIQUE,
                label            TEXT,
                auto_inject      INTEGER,
                injection_method TEXT,
                profile_id       INTEGER REFERENCES profiles(id) ON DELETE SET NULL,
                enabled          INTEGER NOT NULL DEFAULT 1,
                created_at       TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Every outbound request Echo itself makes. This is an honest record
            -- of what the app did, not proof about what the machine did.
            CREATE TABLE IF NOT EXISTS egress_log (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                host       TEXT NOT NULL,
                purpose    TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_egress_created
                ON egress_log (created_at DESC);

            INSERT INTO schema_migrations (version) VALUES (2);
        ")?;
    }

    Ok(())
}
