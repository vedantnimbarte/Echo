//! Keeping one dictionary on several machines, with no server in between.
//!
//! Echo has no account and no Echo server, and that is not going to change for
//! a word list. What people with two computers already have is a folder that
//! follows them — Dropbox, OneDrive, iCloud Drive, Syncthing, a network share —
//! so sync is one JSON file in that folder, and the syncing service does the
//! moving. Echo's half is reading the file, merging it with the local
//! dictionary, and writing the result back.
//!
//! # Why merge, and what merges
//!
//! Overwriting in either direction loses work: two machines that were edited
//! offline and then come online would each throw away the other's changes. So
//! every entry is merged independently, **last writer wins per entry**, and a
//! deletion is a record in its own right (a tombstone) rather than an absence —
//! otherwise the other machine, which still has the entry, would put it back.
//!
//! LWW compares wall clocks. Two machines whose clocks disagree by more than
//! the gap between two edits of the *same* entry can pick the older edit. That
//! is the known ceiling, and a dictionary entry is small enough that it is the
//! right one: the alternative, vector clocks, buys correctness for concurrent
//! edits to one phrase — which is not a thing people do — at the cost of a file
//! no human can read.
//!
//! # Identity and timestamps without a migration
//!
//! `dictionary_entries` has a local autoincrement id (meaningless on the other
//! machine), a local `profile_id` (likewise), a `created_at`, and no
//! `updated_at`. Rather than add columns, an entry's identity is derived:
//! **the phrase, trimmed and lowercased, plus the name of its profile** (none
//! for a global entry). That is already what the user thinks of as "the same
//! entry" — matching is case-insensitive, and profile names are unique — and
//! it means an entry needs no id to be carried between machines at all.
//!
//! Timestamps live in a sidecar: the last merged state, stored as one JSON value
//! in the settings table (`dictionary_sync_state`). Each sync diffs the database
//! against it. An entry that matches its snapshot keeps the snapshot's time; one
//! that differs, or is new since the last sync, was edited "now"; one in the
//! snapshot but gone from the database was deleted "now" and becomes a
//! tombstone. The snapshot is also where tombstones live between syncs.
//!
//! Three consequences worth knowing:
//!
//! - "Now" is when the edit was *noticed*, not when it was made. Edits trigger a
//!   sync a few seconds later, so the two are close — except for edits made
//!   while sync was switched off, which are stamped when it is switched back on.
//! - An entry with no snapshot record at all (the very first sync on a machine)
//!   is stamped with its `created_at`, not "now". Otherwise a machine joining
//!   late would resurrect everything the others had deleted since it last saw
//!   the dictionary, just because its copy looks brand new.
//! - Moving an entry to another profile changes its identity, so it syncs as a
//!   deletion of the old entry and the addition of a new one. Same outcome.
//!
//! # What cannot go wrong
//!
//! A file that will not parse — half-downloaded, hand-edited, written by a newer
//! Echo — aborts the whole sync before anything local is touched. It is not
//! overwritten either: a half-downloaded file is someone's changes on their way
//! in, and replacing it would lose them. Sync tries again when the file next
//! changes (a download finishing changes it), on the next edit, or from the
//! button.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::{EchoError, Result};
use crate::storage::repositories;

/// The file Echo reads and writes inside the sync folder.
pub const FILE_NAME: &str = "echo-dictionary.json";

/// Written first, then renamed over [`FILE_NAME`]. A leading dot so it is
/// hidden, and a suffix that does not end in `.json` so it is never mistaken
/// for a conflicted copy if a crash leaves it behind.
const TEMP_NAME: &str = ".echo-dictionary.json.tmp";

/// Bumped when the file changes in a way an older Echo would misread. An older
/// Echo that meets a newer file refuses to sync rather than rewriting it in the
/// old shape and dropping whatever the new fields meant.
pub const FORMAT_VERSION: u32 = 1;

pub const KEY_FOLDER: &str = "dictionary_sync_folder";
pub const KEY_ENABLED: &str = "dictionary_sync_enabled";
/// The sidecar: last merged state, including tombstones. See the module docs.
const KEY_STATE: &str = "dictionary_sync_state";

/// One entry in the sync file.
///
/// The export format's fields (`phrase`, `replacement`, `enabled`) under the
/// same names, plus what merging needs: which profile the entry belongs to,
/// when it last changed, and whether this is a tombstone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncEntry {
    pub phrase: String,
    /// Empty on a tombstone: a deleted replacement is nobody's business.
    #[serde(default)]
    pub replacement: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Profile *name*: a profile's id is local to each machine's database.
    #[serde(default)]
    pub profile: Option<String>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
struct SyncFile {
    version: u32,
    entries: Vec<SyncEntry>,
}

/// What a sync did, for the status line.
#[derive(Debug, Default)]
pub struct Outcome {
    /// The local dictionary was changed, so the engine needs rebuilding.
    pub changed_local: bool,
    /// Conflicted copies found beside the file and merged in. Left in place:
    /// they are the user's files, and deleting them is the user's call.
    pub conflict_copies: Vec<String>,
}

/// Identity of an entry across machines: profile name, normalized phrase.
type Key = (Option<String>, String);

fn key(profile: &Option<String>, phrase: &str) -> Key {
    (profile.clone(), phrase.trim().to_lowercase())
}

/// Sync the dictionary in `conn` with the file in `folder`.
///
/// The database lock is held only for the merge itself, not for the file IO on
/// either side of it: the folder may be a network share, and dictation should
/// not wait on a slow one to reach its own database.
pub fn sync(db: &Mutex<Connection>, folder: &Path, now: DateTime<Utc>) -> Result<Outcome> {
    if !folder.is_dir() {
        return Err(EchoError::Config(format!(
            "The sync folder {} isn't available. Your local dictionary was left as it is.",
            folder.display()
        )));
    }

    // Every file is read and parsed before the database is touched, so one bad
    // file leaves the local dictionary exactly as it was.
    let remote = read_folder(folder)?;

    let (merged, changed_local) = {
        let mut conn = db.lock().unwrap();
        let tx = conn.transaction()?;

        let snapshot = load_snapshot(&tx);
        let local = local_entries(&tx, &snapshot, now)?;
        let merged = merge(local.into_iter().chain(remote.entries));
        let changed_local = apply(&tx, &merged)?;

        let state: Vec<&SyncEntry> = merged.values().collect();
        repositories::set_setting(&tx, KEY_STATE, &serde_json::to_string(&state)?)?;
        tx.commit()?;
        (merged, changed_local)
    };

    let file = SyncFile {
        version: FORMAT_VERSION,
        entries: merged.into_values().collect(),
    };
    let bytes = serde_json::to_vec_pretty(&file)?;
    // Only write when something changed. Rewriting identical bytes would still
    // bump the mtime, every other machine's poll would see a new file, sync, and
    // the folder would upload the same file back and forth forever.
    if remote.main.as_deref() != Some(bytes.as_slice()) {
        write_atomic(folder, &bytes)?;
    }

    Ok(Outcome {
        changed_local,
        conflict_copies: remote.conflict_copies,
    })
}

/// Last writer wins per entry.
///
/// Ties are broken on the content so every machine picks the same winner from
/// the same inputs — otherwise two machines could each keep their own version
/// and never converge. On an exact tie of time, a deletion wins.
pub fn merge(entries: impl IntoIterator<Item = SyncEntry>) -> BTreeMap<Key, SyncEntry> {
    let rank = |e: &SyncEntry| {
        (
            e.updated_at,
            e.deleted,
            e.replacement.clone(),
            e.enabled,
            e.phrase.clone(),
        )
    };
    let mut out: BTreeMap<Key, SyncEntry> = BTreeMap::new();
    for entry in entries {
        if entry.phrase.trim().is_empty() {
            continue;
        }
        let k = key(&entry.profile, &entry.phrase);
        match out.get(&k) {
            Some(current) if rank(current) >= rank(&entry) => {}
            _ => {
                out.insert(k, entry);
            }
        }
    }
    out
}

struct Remote {
    entries: Vec<SyncEntry>,
    /// Bytes of the main file as read, to tell whether a write would change it.
    main: Option<Vec<u8>>,
    conflict_copies: Vec<String>,
}

fn read_folder(folder: &Path) -> Result<Remote> {
    let io = |what: &str, e: std::io::Error| {
        EchoError::Config(format!(
            "Couldn't read {what} in the sync folder: {e}. Your local dictionary was left as it is."
        ))
    };

    let main = match std::fs::read(folder.join(FILE_NAME)) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io(FILE_NAME, e)),
    };
    let mut entries = match &main {
        Some(bytes) => parse(bytes, FILE_NAME)?,
        None => Vec::new(),
    };

    let mut conflict_copies = Vec::new();
    for path in conflict_copy_paths(folder).map_err(|e| io("the folder", e))? {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let bytes = std::fs::read(&path).map_err(|e| io(&name, e))?;
        entries.extend(parse(&bytes, &name)?);
        conflict_copies.push(name);
    }
    conflict_copies.sort();

    Ok(Remote {
        entries,
        main,
        conflict_copies,
    })
}

fn parse(bytes: &[u8], name: &str) -> Result<Vec<SyncEntry>> {
    let file: SyncFile = serde_json::from_slice(bytes).map_err(|e| {
        EchoError::Config(format!(
            "{name} isn't a readable Echo dictionary ({e}). Your local dictionary was left as it is."
        ))
    })?;
    if file.version > FORMAT_VERSION {
        return Err(EchoError::Config(format!(
            "{name} was written by a newer version of Echo. Update Echo on this computer to keep syncing."
        )));
    }
    Ok(file.entries)
}

/// Copies a syncing service made when two machines wrote the file at once.
///
/// Every service names them differently — Dropbox "echo-dictionary (… conflicted
/// copy …).json", OneDrive "echo-dictionary-MACHINE.json", Syncthing
/// "echo-dictionary.sync-conflict-….json", iCloud "echo-dictionary 2.json" — but
/// all of them keep the stem at the front and the extension at the end.
fn conflict_copy_paths(folder: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for dirent in std::fs::read_dir(folder)? {
        let dirent = dirent?;
        let name = dirent.file_name().to_string_lossy().to_lowercase();
        if name != FILE_NAME
            && name.starts_with("echo-dictionary")
            && name.ends_with(".json")
            && dirent.file_type()?.is_file()
        {
            out.push(dirent.path());
        }
    }
    Ok(out)
}

/// Name, modification time and size of every file sync reads. Cheap to take
/// every thirty seconds, and different whenever another machine has written.
pub fn fingerprint(folder: &Path) -> Vec<(PathBuf, Option<SystemTime>, u64)> {
    let mut paths = conflict_copy_paths(folder).unwrap_or_default();
    paths.push(folder.join(FILE_NAME));
    let mut out: Vec<_> = paths
        .into_iter()
        .filter_map(|p| {
            let meta = std::fs::metadata(&p).ok()?;
            Some((p, meta.modified().ok(), meta.len()))
        })
        .collect();
    out.sort();
    out
}

/// Write to a temporary file in the same folder, flush it to disk, then rename
/// it over the real one.
///
/// Syncing services upload whatever they see, and a file written in place is
/// visible half-written. A rename within one folder is atomic on every
/// filesystem these services run on, so the other machine only ever receives
/// the old file or the new one. (On Windows `std::fs::rename` replaces an
/// existing target.)
pub fn write_atomic(folder: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = folder.join(TEMP_NAME);
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, folder.join(FILE_NAME))
    })();
    result.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        EchoError::Config(format!(
            "Couldn't write {FILE_NAME} to the sync folder: {e}"
        ))
    })
}

fn load_snapshot(conn: &Connection) -> HashMap<Key, SyncEntry> {
    let raw = repositories::get_setting(conn, KEY_STATE).ok().flatten();
    let entries: Vec<SyncEntry> = match raw.as_deref().map(serde_json::from_str) {
        Some(Ok(entries)) => entries,
        Some(Err(e)) => {
            // Echo wrote this itself, so this is not expected. Carrying on with
            // no snapshot falls back to `created_at` stamps, which is the
            // first-sync behaviour — safe, if less precise about deletions.
            tracing::warn!("Dictionary sync state was unreadable, starting afresh: {e}");
            Vec::new()
        }
        None => Vec::new(),
    };
    entries
        .into_iter()
        .map(|e| (key(&e.profile, &e.phrase), e))
        .collect()
}

/// The database as sync entries, stamped against the snapshot. Includes a
/// tombstone for everything the snapshot had that the database no longer does.
fn local_entries(
    conn: &Connection,
    snapshot: &HashMap<Key, SyncEntry>,
    now: DateTime<Utc>,
) -> Result<Vec<SyncEntry>> {
    let profiles: HashMap<i64, String> = repositories::list_profiles(conn)?
        .into_iter()
        .filter_map(|p| Some((p.id?, p.name)))
        .collect();

    let mut out: HashMap<Key, SyncEntry> = HashMap::new();
    for row in repositories::list_dictionary_entries(conn)? {
        if row.phrase.trim().is_empty() {
            continue;
        }
        let profile = row.profile_id.and_then(|id| profiles.get(&id).cloned());
        let k = key(&profile, &row.phrase);
        // Two rows with the same identity: the first speaks for both. `apply`
        // deletes every row of a deleted identity, so the second cannot
        // outlive the first and come back as a "new" entry.
        if out.contains_key(&k) {
            continue;
        }
        let updated_at = match snapshot.get(&k) {
            Some(prev)
                if !prev.deleted
                    && prev.phrase == row.phrase
                    && prev.replacement == row.replacement
                    && prev.enabled == row.enabled =>
            {
                prev.updated_at
            }
            Some(_) => now,
            None => NaiveDateTime::parse_from_str(&row.created_at, "%Y-%m-%d %H:%M:%S")
                .map(|t| t.and_utc())
                .unwrap_or(now),
        };
        out.insert(
            k,
            SyncEntry {
                phrase: row.phrase,
                replacement: row.replacement,
                enabled: row.enabled,
                profile,
                updated_at,
                deleted: false,
            },
        );
    }

    for (k, prev) in snapshot {
        if out.contains_key(k) {
            continue;
        }
        let tombstone = if prev.deleted {
            prev.clone()
        } else {
            SyncEntry {
                replacement: String::new(),
                updated_at: now,
                deleted: true,
                ..prev.clone()
            }
        };
        out.insert(k.clone(), tombstone);
    }
    // ponytail: tombstones are kept forever. They are a phrase and a date each;
    // prune ones older than a few months if a file ever grows big enough to
    // notice, accepting that a machine offline that long could resurrect them.

    Ok(out.into_values().collect())
}

/// Make the database agree with `merged`. Returns whether anything changed.
fn apply(conn: &Connection, merged: &BTreeMap<Key, SyncEntry>) -> Result<bool> {
    let mut profiles: HashMap<String, i64> = repositories::list_profiles(conn)?
        .into_iter()
        .filter_map(|p| Some((p.name, p.id?)))
        .collect();
    let names: HashMap<i64, String> = profiles.iter().map(|(n, id)| (*id, n.clone())).collect();

    let mut rows: HashMap<Key, Vec<_>> = HashMap::new();
    for row in repositories::list_dictionary_entries(conn)? {
        let profile = row.profile_id.and_then(|id| names.get(&id).cloned());
        rows.entry(key(&profile, &row.phrase))
            .or_default()
            .push(row);
    }

    let mut changed = false;
    for (k, entry) in merged {
        let existing = rows.get(k).map(Vec::as_slice).unwrap_or_default();
        if entry.deleted {
            for row in existing {
                repositories::delete_dictionary_entry(conn, row.id.unwrap_or_default())?;
                changed = true;
            }
            continue;
        }
        match existing.first() {
            Some(row) => {
                if row.phrase != entry.phrase
                    || row.replacement != entry.replacement
                    || row.enabled != entry.enabled
                {
                    conn.execute(
                        "UPDATE dictionary_entries SET phrase = ?2, replacement = ?3, enabled = ?4
                         WHERE id = ?1",
                        params![row.id, entry.phrase, entry.replacement, entry.enabled],
                    )?;
                    changed = true;
                }
            }
            None => {
                // The other machine may have a profile this one has never
                // heard of; it is created by name so the entry keeps its scope.
                let profile_id = match &entry.profile {
                    None => None,
                    Some(name) => Some(match profiles.get(name) {
                        Some(id) => *id,
                        None => {
                            let id = repositories::insert_profile(conn, name)?;
                            profiles.insert(name.clone(), id);
                            id
                        }
                    }),
                };
                repositories::insert_dictionary_entry(
                    conn,
                    &crate::storage::models::DictionaryEntry {
                        id: None,
                        phrase: entry.phrase.clone(),
                        replacement: entry.replacement.clone(),
                        enabled: entry.enabled,
                        profile_id,
                        created_at: String::new(),
                    },
                )?;
                changed = true;
            }
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{db, models::DictionaryEntry};
    use chrono::TimeZone;

    /// A fresh temp directory. Not cleaned up on failure, which is when you
    /// want to look inside it.
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("echo-sync-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A machine: its own database file in its own directory.
    fn machine(tag: &str) -> Mutex<Connection> {
        Mutex::new(db::open(&temp_dir(tag).join("echo.db")).unwrap())
    }

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2030, 1, 1, 12, minute, 0).unwrap()
    }

    fn add(m: &Mutex<Connection>, phrase: &str, replacement: &str) {
        let conn = m.lock().unwrap();
        repositories::insert_dictionary_entry(
            &conn,
            &DictionaryEntry {
                id: None,
                phrase: phrase.into(),
                replacement: replacement.into(),
                enabled: true,
                profile_id: None,
                created_at: String::new(),
            },
        )
        .unwrap();
    }

    fn id_of(m: &Mutex<Connection>, phrase: &str) -> i64 {
        let conn = m.lock().unwrap();
        repositories::list_dictionary_entries(&conn)
            .unwrap()
            .into_iter()
            .find(|e| e.phrase == phrase)
            .and_then(|e| e.id)
            .unwrap()
    }

    fn edit(m: &Mutex<Connection>, phrase: &str, replacement: &str) {
        let id = id_of(m, phrase);
        m.lock()
            .unwrap()
            .execute(
                "UPDATE dictionary_entries SET replacement = ?2 WHERE id = ?1",
                params![id, replacement],
            )
            .unwrap();
    }

    fn remove(m: &Mutex<Connection>, phrase: &str) {
        let id = id_of(m, phrase);
        repositories::delete_dictionary_entry(&m.lock().unwrap(), id).unwrap();
    }

    /// The dictionary as sorted (phrase, replacement) pairs.
    fn dict(m: &Mutex<Connection>) -> Vec<(String, String)> {
        let mut out: Vec<_> = repositories::list_dictionary_entries(&m.lock().unwrap())
            .unwrap()
            .into_iter()
            .map(|e| (e.phrase, e.replacement))
            .collect();
        out.sort();
        out
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn concurrent_edits_to_different_entries_both_survive_either_order() {
        for a_first in [true, false] {
            let folder = temp_dir("both");
            let (a, b) = (machine("a"), machine("b"));
            add(&a, "k8s", "Kubernetes");
            sync(&a, &folder, at(0)).unwrap();
            sync(&b, &folder, at(1)).unwrap();
            assert_eq!(dict(&b), pairs(&[("k8s", "Kubernetes")]));

            // Both offline: A edits the shared entry, B adds a new one.
            edit(&a, "k8s", "K8s");
            add(&b, "gh", "GitHub");

            let (first, second) = if a_first { (&a, &b) } else { (&b, &a) };
            sync(first, &folder, at(2)).unwrap();
            sync(second, &folder, at(3)).unwrap();
            sync(first, &folder, at(4)).unwrap();

            let want = pairs(&[("gh", "GitHub"), ("k8s", "K8s")]);
            assert_eq!(dict(&a), want, "a_first = {a_first}");
            assert_eq!(dict(&b), want, "a_first = {a_first}");
        }
    }

    #[test]
    fn the_later_edit_of_the_same_entry_wins_on_both_machines() {
        let folder = temp_dir("lww");
        let (a, b) = (machine("a"), machine("b"));
        add(&a, "teh", "the");
        sync(&a, &folder, at(0)).unwrap();
        sync(&b, &folder, at(1)).unwrap();

        edit(&a, "teh", "THE-from-a");
        edit(&b, "teh", "THE-from-b");
        // B's edit is noticed later, so B wins — whichever machine syncs first.
        sync(&b, &folder, at(5)).unwrap();
        sync(&a, &folder, at(3)).unwrap();
        sync(&b, &folder, at(6)).unwrap();

        assert_eq!(dict(&a), pairs(&[("teh", "THE-from-b")]));
        assert_eq!(dict(&b), pairs(&[("teh", "THE-from-b")]));
    }

    #[test]
    fn a_deletion_propagates_and_the_tombstone_stops_resurrection() {
        let folder = temp_dir("delete");
        let (a, b) = (machine("a"), machine("b"));
        add(&a, "teh", "the");
        add(&a, "gh", "GitHub");
        sync(&a, &folder, at(0)).unwrap();
        sync(&b, &folder, at(1)).unwrap();

        remove(&a, "teh");
        sync(&a, &folder, at(2)).unwrap();
        sync(&b, &folder, at(3)).unwrap();
        assert_eq!(dict(&b), pairs(&[("gh", "GitHub")]));

        // B still had "teh" when A deleted it. Without a tombstone, the next
        // round of syncs would read B's copy as an addition and bring it back.
        for minute in 4..8 {
            sync(&a, &folder, at(minute)).unwrap();
            sync(&b, &folder, at(minute)).unwrap();
        }
        assert_eq!(dict(&a), pairs(&[("gh", "GitHub")]));
        assert_eq!(dict(&b), pairs(&[("gh", "GitHub")]));

        // A machine joining late with an old copy of the entry does not
        // resurrect it either: its row's `created_at` predates the deletion.
        let c = machine("c");
        add(&c, "teh", "the");
        c.lock()
            .unwrap()
            .execute(
                "UPDATE dictionary_entries SET created_at = '2029-01-01 00:00:00'",
                [],
            )
            .unwrap();
        sync(&c, &folder, at(9)).unwrap();
        assert_eq!(dict(&c), pairs(&[("gh", "GitHub")]));

        // But re-adding it after the deletion is a real edit, and wins.
        add(&a, "teh", "the");
        sync(&a, &folder, at(10)).unwrap();
        sync(&b, &folder, at(11)).unwrap();
        assert_eq!(dict(&b), pairs(&[("gh", "GitHub"), ("teh", "the")]));
    }

    #[test]
    fn profile_scope_travels_by_name() {
        let folder = temp_dir("profile");
        let (a, b) = (machine("a"), machine("b"));
        {
            let conn = a.lock().unwrap();
            let id = repositories::insert_profile(&conn, "Code").unwrap();
            repositories::insert_dictionary_entry(
                &conn,
                &DictionaryEntry {
                    id: None,
                    phrase: "pr".into(),
                    replacement: "pull request".into(),
                    enabled: true,
                    profile_id: Some(id),
                    created_at: String::new(),
                },
            )
            .unwrap();
        }
        sync(&a, &folder, at(0)).unwrap();
        sync(&b, &folder, at(1)).unwrap();

        let conn = b.lock().unwrap();
        let profile = repositories::list_profiles(&conn).unwrap();
        let entries = repositories::list_dictionary_entries(&conn).unwrap();
        assert_eq!(profile[0].name, "Code");
        assert_eq!(entries[0].profile_id, profile[0].id);
    }

    #[test]
    fn a_corrupt_file_leaves_local_intact_and_is_not_overwritten() {
        let folder = temp_dir("corrupt");
        let a = machine("a");
        add(&a, "teh", "the");
        sync(&a, &folder, at(0)).unwrap();

        let garbage = b"{\"version\": 1, \"entries\": [ {\"phr";
        std::fs::write(folder.join(FILE_NAME), garbage).unwrap();
        let err = sync(&a, &folder, at(1)).unwrap_err().to_string();
        assert!(err.contains(FILE_NAME), "{err}");

        assert_eq!(dict(&a), pairs(&[("teh", "the")]));
        // Possibly another machine's changes, mid-download: left for next time.
        assert_eq!(std::fs::read(folder.join(FILE_NAME)).unwrap(), garbage);

        // A file from a newer Echo is refused the same way.
        std::fs::write(folder.join(FILE_NAME), br#"{"version": 99, "entries": []}"#).unwrap();
        assert!(sync(&a, &folder, at(2)).is_err());
        assert_eq!(dict(&a), pairs(&[("teh", "the")]));
    }

    #[test]
    fn the_write_is_atomic_and_leaves_no_temp_file() {
        let folder = temp_dir("atomic");
        std::fs::write(folder.join(FILE_NAME), b"old").unwrap();
        // A temp file left by a crash mid-write is overwritten, not tripped on.
        std::fs::write(folder.join(TEMP_NAME), b"half a fi").unwrap();

        write_atomic(&folder, b"new contents").unwrap();

        assert_eq!(
            std::fs::read(folder.join(FILE_NAME)).unwrap(),
            b"new contents"
        );
        assert!(!folder.join(TEMP_NAME).exists());
        // And a leftover temp file is never read as a conflicted copy.
        assert!(conflict_copy_paths(&folder).unwrap().is_empty());
    }

    #[test]
    fn an_unchanged_dictionary_does_not_rewrite_the_file() {
        let folder = temp_dir("stable");
        let a = machine("a");
        add(&a, "teh", "the");
        sync(&a, &folder, at(0)).unwrap();
        let before = fingerprint(&folder);
        std::thread::sleep(std::time::Duration::from_millis(20));
        sync(&a, &folder, at(1)).unwrap();
        assert_eq!(fingerprint(&folder), before);
    }

    #[test]
    fn a_conflicted_copy_is_merged_in_and_kept() {
        let folder = temp_dir("conflict");
        let (a, b) = (machine("a"), machine("b"));
        add(&a, "teh", "the");
        sync(&a, &folder, at(0)).unwrap();

        // B wrote at the same moment; the service kept both files.
        add(&b, "gh", "GitHub");
        sync(&b, &folder, at(1)).unwrap();
        let copy = "echo-dictionary (Vedant's conflicted copy 2030-01-01).json";
        std::fs::rename(folder.join(FILE_NAME), folder.join(copy)).unwrap();
        let a_only = SyncFile {
            version: FORMAT_VERSION,
            entries: vec![SyncEntry {
                phrase: "teh".into(),
                replacement: "the".into(),
                enabled: true,
                profile: None,
                updated_at: at(0),
                deleted: false,
            }],
        };
        std::fs::write(folder.join(FILE_NAME), serde_json::to_vec(&a_only).unwrap()).unwrap();

        let outcome = sync(&a, &folder, at(2)).unwrap();
        assert_eq!(outcome.conflict_copies, vec![copy.to_string()]);
        assert!(outcome.changed_local);
        assert_eq!(dict(&a), pairs(&[("gh", "GitHub"), ("teh", "the")]));
        assert!(
            folder.join(copy).exists(),
            "the user's file is theirs to delete"
        );

        // The main file now carries both, so B is whole without the copy.
        std::fs::remove_file(folder.join(copy)).unwrap();
        sync(&b, &folder, at(3)).unwrap();
        assert_eq!(dict(&b), pairs(&[("gh", "GitHub"), ("teh", "the")]));
    }

    #[test]
    fn the_file_is_versioned_and_uses_the_export_field_names() {
        let folder = temp_dir("format");
        let a = machine("a");
        add(&a, "teh", "the");
        sync(&a, &folder, at(0)).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(folder.join(FILE_NAME)).unwrap()).unwrap();
        assert_eq!(json["version"], FORMAT_VERSION);
        let entry = &json["entries"][0];
        assert_eq!(entry["phrase"], "teh");
        assert_eq!(entry["replacement"], "the");
        assert_eq!(entry["enabled"], true);
        assert!(entry.get("deleted").is_none());
    }
}
