//! Tests for the storage layer.
//!
//! This is the one part of Echo where a bug is not recoverable. Everywhere else
//! a failure costs you a transcript; here it costs you every transcript, every
//! dictionary entry, and every setting you ever configured — and an upgrade
//! that drops a table does it to people who did nothing but install a new
//! version.
//!
//! So the emphasis is on the destructive paths and the upgrade path, not on
//! proving that `INSERT` inserts: migrations running twice, an old database
//! meeting new code, foreign keys that delete rows the user did not ask to
//! delete, and the ordering the History window depends on.
//!
//! In their own file rather than at the bottom of `repositories.rs` because
//! they cover `db.rs` as well, and because that module is long enough already.

use std::path::Path;

use super::db;
use super::models::{AppProfile, DictionaryEntry, TranscriptionRecord};
use super::repositories as repo;
use rusqlite::Connection;

fn open() -> Connection {
    db::open(Path::new(":memory:")).expect("in-memory database")
}

fn schema_version(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |r| r.get(0),
        )
        .unwrap();
    count == 1
}

fn entry(phrase: &str, profile_id: Option<i64>) -> DictionaryEntry {
    DictionaryEntry {
        id: None,
        phrase: phrase.into(),
        replacement: "x".into(),
        enabled: true,
        profile_id,
        created_at: String::new(),
    }
}

fn transcript(text: &str) -> TranscriptionRecord {
    TranscriptionRecord {
        id: None,
        text: text.into(),
        language: None,
        provider: "test".into(),
        created_at: String::new(),
    }
}

fn app_profile(app_match: &str) -> AppProfile {
    AppProfile {
        id: None,
        app_match: app_match.into(),
        label: None,
        auto_inject: Some(true),
        injection_method: None,
        profile_id: None,
        enabled: true,
    }
}

// ── Migrations ───────────────────────────────────────────────────────────────

#[test]
fn a_fresh_database_has_every_table_the_app_uses() {
    let conn = open();
    for table in [
        "schema_migrations",
        "settings",
        "profiles",
        "dictionary_entries",
        "transcription_history",
        "telemetry_events",
        "plugins",
        "app_profiles",
        "egress_log",
    ] {
        assert!(table_exists(&conn, table), "{table} is missing");
    }
    assert_eq!(schema_version(&conn), 2);
}

/// Every launch runs `migrate`. Applying a migration twice must be harmless,
/// and must not record a second row for the same version.
#[test]
fn migrating_an_already_current_database_changes_nothing() {
    let conn = open();
    repo::set_setting(&conn, "keep", "me").unwrap();

    db::migrate_for_test(&conn).unwrap();
    db::migrate_for_test(&conn).unwrap();

    assert_eq!(schema_version(&conn), 2);
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 2, "one row per version, not one per launch");
    assert_eq!(repo::get_setting(&conn, "keep").unwrap().as_deref(), Some("me"));
}

/// The upgrade path, which is the one that reaches existing users.
///
/// A database created before migration 2 is simulated by dropping what
/// migration 2 added, then re-running the migrator over data that was already
/// there. Nothing from version 1 may be disturbed.
#[test]
fn an_old_database_upgrades_without_losing_data() {
    let conn = open();

    repo::set_setting(&conn, "hotkey", "Ctrl+Shift+Space").unwrap();
    let profile_id = repo::insert_profile(&conn, "work").unwrap();
    repo::insert_dictionary_entry(&conn, &entry("k8s", Some(profile_id))).unwrap();
    repo::insert_history(&conn, &transcript("something said long ago")).unwrap();

    // Rewind to a version-1 database.
    conn.execute_batch(
        "DROP TABLE app_profiles;
         DROP TABLE egress_log;
         DELETE FROM schema_migrations WHERE version = 2;",
    )
    .unwrap();
    assert_eq!(schema_version(&conn), 1);

    db::migrate_for_test(&conn).unwrap();

    assert_eq!(schema_version(&conn), 2);
    assert!(table_exists(&conn, "app_profiles"));
    assert!(table_exists(&conn, "egress_log"));

    assert_eq!(
        repo::get_setting(&conn, "hotkey").unwrap().as_deref(),
        Some("Ctrl+Shift+Space")
    );
    assert_eq!(repo::list_profiles(&conn).unwrap().len(), 1);
    assert_eq!(repo::list_dictionary_entries(&conn).unwrap().len(), 1);
    assert_eq!(repo::list_history(&conn, 10).unwrap().len(), 1);
}

/// `open` enables foreign keys, and the schema relies on them. Without the
/// pragma the `ON DELETE` clauses below are silently inert.
#[test]
fn foreign_keys_are_enforced() {
    let conn = open();
    let orphan = entry("nope", Some(9_999));
    assert!(
        repo::insert_dictionary_entry(&conn, &orphan).is_err(),
        "an entry pointing at a profile that does not exist was accepted"
    );
}

/// Deleting a dictionary profile deletes the entries inside it — the schema
/// says `ON DELETE CASCADE`, and this pins that down so it cannot change by
/// accident. It is destructive and worth knowing about: the entries do not
/// fall back to global.
#[test]
fn deleting_a_profile_takes_its_dictionary_entries_with_it() {
    let conn = open();
    let profile_id = repo::insert_profile(&conn, "work").unwrap();
    repo::insert_dictionary_entry(&conn, &entry("scoped", Some(profile_id))).unwrap();
    repo::insert_dictionary_entry(&conn, &entry("global", None)).unwrap();

    repo::delete_profile(&conn, profile_id).unwrap();

    let remaining: Vec<String> = repo::list_dictionary_entries(&conn)
        .unwrap()
        .into_iter()
        .map(|e| e.phrase)
        .collect();
    assert_eq!(remaining, vec!["global"], "global entries must survive");
}

/// An app profile pointing at a deleted dictionary profile is kept, with the
/// reference cleared — `ON DELETE SET NULL`, deliberately different from the
/// cascade above. Losing the whole per-app rule because a dictionary profile
/// was tidied up would be a nasty surprise.
#[test]
fn deleting_a_profile_only_clears_the_reference_from_app_profiles() {
    let conn = open();
    let profile_id = repo::insert_profile(&conn, "work").unwrap();
    let mut app = app_profile("slack.exe");
    app.profile_id = Some(profile_id);
    repo::upsert_app_profile(&conn, &app).unwrap();

    repo::delete_profile(&conn, profile_id).unwrap();

    let found = repo::find_app_profile(&conn, "slack.exe").unwrap();
    let found = found.expect("the app profile itself must survive");
    assert_eq!(found.profile_id, None);
}

// ── Settings ─────────────────────────────────────────────────────────────────

#[test]
fn settings_round_trip_and_overwrite_in_place() {
    let conn = open();
    assert_eq!(repo::get_setting(&conn, "missing").unwrap(), None);

    repo::set_setting(&conn, "mode", "toggle").unwrap();
    assert_eq!(repo::get_setting(&conn, "mode").unwrap().as_deref(), Some("toggle"));

    repo::set_setting(&conn, "mode", "hold").unwrap();
    assert_eq!(repo::get_setting(&conn, "mode").unwrap().as_deref(), Some("hold"));

    let rows: i64 = conn
        .query_row("SELECT count(*) FROM settings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1, "an overwrite must update, not accumulate");
}

// ── Dictionary ───────────────────────────────────────────────────────────────

#[test]
fn dictionary_entries_can_be_added_disabled_and_removed() {
    let conn = open();
    let id = repo::insert_dictionary_entry(&conn, &entry("teh", None)).unwrap();

    let stored = repo::list_dictionary_entries(&conn).unwrap();
    assert_eq!(stored.len(), 1);
    assert!(stored[0].enabled);

    repo::set_dictionary_entry_enabled(&conn, id, false).unwrap();
    assert!(!repo::list_dictionary_entries(&conn).unwrap()[0].enabled);

    repo::delete_dictionary_entry(&conn, id).unwrap();
    assert!(repo::list_dictionary_entries(&conn).unwrap().is_empty());
}

#[test]
fn an_entry_can_be_moved_between_a_profile_and_global() {
    let conn = open();
    let profile_id = repo::insert_profile(&conn, "work").unwrap();
    let id = repo::insert_dictionary_entry(&conn, &entry("k8s", None)).unwrap();

    repo::set_dictionary_entry_profile(&conn, id, Some(profile_id)).unwrap();
    assert_eq!(repo::list_dictionary_entries(&conn).unwrap()[0].profile_id, Some(profile_id));

    repo::set_dictionary_entry_profile(&conn, id, None).unwrap();
    assert_eq!(repo::list_dictionary_entries(&conn).unwrap()[0].profile_id, None);
}

// ── History ──────────────────────────────────────────────────────────────────

/// History is shown newest-first. `created_at` only has second resolution, so
/// several dictations in the same second tie — and without a tiebreaker SQLite
/// is free to return them in any order, which in practice meant oldest-first:
/// exactly backwards, and easy to miss because it only shows up when you
/// dictate quickly.
#[test]
fn history_is_returned_newest_first_even_within_one_second() {
    let conn = open();
    for text in ["oldest", "middle", "newest"] {
        repo::insert_history(&conn, &transcript(text)).unwrap();
    }

    let order: Vec<String> = repo::list_history(&conn, 10)
        .unwrap()
        .into_iter()
        .map(|r| r.text)
        .collect();
    assert_eq!(order, vec!["newest", "middle", "oldest"]);
}

#[test]
fn history_respects_its_limit_and_can_be_cleared() {
    let conn = open();
    for i in 0..5 {
        repo::insert_history(&conn, &transcript(&format!("line {i}"))).unwrap();
    }
    assert_eq!(repo::list_history(&conn, 2).unwrap().len(), 2);

    repo::clear_history(&conn).unwrap();
    assert!(repo::list_history(&conn, 10).unwrap().is_empty());
}

// ── Per-app profiles ─────────────────────────────────────────────────────────

/// The identifier is stored and matched lowercased, because what the platform
/// layer reports varies in case between Windows, macOS and Linux.
#[test]
fn app_profiles_match_regardless_of_case() {
    let conn = open();
    repo::upsert_app_profile(&conn, &app_profile("Slack.EXE")).unwrap();

    assert!(repo::find_app_profile(&conn, "slack.exe").unwrap().is_some());
    assert!(repo::find_app_profile(&conn, "SLACK.EXE").unwrap().is_some());
    assert!(repo::find_app_profile(&conn, "code.exe").unwrap().is_none());
}

#[test]
fn a_disabled_app_profile_is_not_matched() {
    let conn = open();
    let mut app = app_profile("slack.exe");
    app.enabled = false;
    repo::upsert_app_profile(&conn, &app).unwrap();

    assert!(repo::find_app_profile(&conn, "slack.exe").unwrap().is_none());
    assert_eq!(repo::list_app_profiles(&conn).unwrap().len(), 1, "still listed for editing");
}

/// Upserting the same application twice updates the existing row.
///
/// The returned id goes straight to the frontend, which uses it to address the
/// profile afterwards — so returning the wrong one would target somebody
/// else's row. The intervening insert is the point: it moves the connection's
/// last-inserted rowid, which a naive implementation would return instead.
#[test]
fn upserting_an_app_profile_updates_it_and_returns_its_own_id() {
    let conn = open();
    let slack_id = repo::upsert_app_profile(&conn, &app_profile("slack.exe")).unwrap();
    let code_id = repo::upsert_app_profile(&conn, &app_profile("code.exe")).unwrap();
    assert_ne!(slack_id, code_id);

    let mut updated = app_profile("slack.exe");
    updated.label = Some("Slack".into());
    updated.auto_inject = Some(false);
    let returned = repo::upsert_app_profile(&conn, &updated).unwrap();

    assert_eq!(returned, slack_id, "returned the wrong row's id");
    assert_eq!(repo::list_app_profiles(&conn).unwrap().len(), 2, "must update, not insert");

    let stored = repo::find_app_profile(&conn, "slack.exe").unwrap().unwrap();
    assert_eq!(stored.label.as_deref(), Some("Slack"));
    assert_eq!(stored.auto_inject, Some(false));
}

#[test]
fn deleting_an_app_profile_leaves_the_others_alone() {
    let conn = open();
    let slack_id = repo::upsert_app_profile(&conn, &app_profile("slack.exe")).unwrap();
    repo::upsert_app_profile(&conn, &app_profile("code.exe")).unwrap();

    repo::delete_app_profile(&conn, slack_id).unwrap();

    let left: Vec<String> = repo::list_app_profiles(&conn)
        .unwrap()
        .into_iter()
        .map(|p| p.app_match)
        .collect();
    assert_eq!(left, vec!["code.exe"]);
}

// ── Egress log ───────────────────────────────────────────────────────────────

#[test]
fn egress_is_listed_newest_first_and_can_be_cleared() {
    let conn = open();
    for host in ["a.example", "b.example", "c.example"] {
        repo::insert_egress(&conn, host, "test").unwrap();
    }

    let hosts: Vec<String> = repo::list_egress(&conn, 10)
        .unwrap()
        .into_iter()
        .map(|e| e.host)
        .collect();
    assert_eq!(hosts, vec!["c.example", "b.example", "a.example"]);

    repo::clear_egress(&conn).unwrap();
    assert!(repo::list_egress(&conn, 10).unwrap().is_empty());
}

/// The egress log is a rolling record, not an audit trail — it is trimmed to a
/// cap, and the *newest* entries are the ones kept.
#[test]
fn trimming_egress_keeps_the_most_recent_entries() {
    let conn = open();
    for i in 0..10 {
        repo::insert_egress(&conn, &format!("host{i}.example"), "test").unwrap();
    }

    repo::trim_egress(&conn, 3).unwrap();

    let hosts: Vec<String> = repo::list_egress(&conn, 10)
        .unwrap()
        .into_iter()
        .map(|e| e.host)
        .collect();
    assert_eq!(hosts, vec!["host9.example", "host8.example", "host7.example"]);
}
