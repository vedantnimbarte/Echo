//! Proving a plugin library is the one the user agreed to run.
//!
//! **This is not a sandbox, and nothing here pretends to be one.** A native
//! plugin is `dlopen`'d into Echo's own process; once its code runs it has
//! every privilege Echo has, and no check performed in this process can take
//! that away. Enforcing the manifest's permission list in-process would be
//! theatre — a reassuring switch with nothing behind it — which is worse than
//! the current honest warning, because it would be believed.
//!
//! What *can* be closed, and is closed here, is a different hole: today a
//! plugin the user vetted and installed could be replaced on disk afterwards
//! and would be loaded on the next launch without a word. Anything able to
//! write into the plugins directory — another program, a sync client, a
//! malicious installer — inherits Echo's privileges silently.
//!
//! So the library is fingerprinted at install and checked at every load. The
//! trust decision stays the user's, made once, on a specific file; a *different*
//! file has to be agreed to again. That is a real property, and a small one.
//!
//! A genuine sandbox needs an OS boundary — a child process with a restricted
//! token, or a WASM runtime — and a wire protocol replacing the FFI. That is a
//! project of its own; it is not a change that can be smuggled in behind a hash.

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::{EchoError, Result};

/// SHA-256 of a file, lowercase hex.
///
/// Read in chunks rather than slurped: a plugin library is a few megabytes
/// today, and nothing about this function should be the reason that stops
/// being true.
pub fn fingerprint(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)
        .map_err(|e| EchoError::Plugin(format!("Can't read {}: {e}", path.display())))?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    std::io::copy(&mut reader, &mut hasher)
        .map_err(|e| EchoError::Plugin(format!("Can't hash {}: {e}", path.display())))?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Check a library against the fingerprint recorded when it was installed.
///
/// `expected` is `None` for a plugin installed before fingerprints existed.
/// Those load with a warning rather than being refused: the file on disk is the
/// one the user has been running all along, and locking them out of it would
/// punish them for an upgrade they did not ask for. The fingerprint is recorded
/// on that first load, so it is checked from then on.
pub fn verify(path: &Path, expected: Option<&str>) -> Result<Verdict> {
    let actual = fingerprint(path)?;
    Ok(match expected {
        None => Verdict::FirstSeen(actual),
        Some(known) if known == actual => Verdict::Unchanged,
        Some(known) => Verdict::Changed {
            expected: known.to_string(),
            actual,
        },
    })
}

/// What checking a library against its record found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Matches what was installed.
    Unchanged,
    /// No fingerprint on record; this one should be stored.
    FirstSeen(String),
    /// The file is not the one that was installed.
    Changed { expected: String, actual: String },
}

impl Verdict {
    /// Whether loading may proceed.
    pub fn is_trusted(&self) -> bool {
        !matches!(self, Verdict::Changed { .. })
    }

    /// What to tell the user when it may not.
    pub fn refusal(&self, name: &str) -> String {
        match self {
            Verdict::Changed { expected, actual } => format!(
                "The plugin '{name}' is not the file you installed — its contents changed on \
                 disk. Echo has disabled it rather than run it. Reinstall it from a source you \
                 trust if the change was yours.\n  installed: {}\n  now:       {}",
                short(expected),
                short(actual)
            ),
            _ => String::new(),
        }
    }
}

/// First eight hex characters, which is enough for a person to compare two
/// fingerprints on screen and quite enough to notice they differ.
fn short(hash: &str) -> &str {
    &hash[..hash.len().min(8)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("echo-integrity-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_same_bytes_hash_the_same_way() {
        let dir = scratch("same");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        std::fs::write(&a, b"plugin code").unwrap();
        std::fs::write(&b, b"plugin code").unwrap();
        assert_eq!(fingerprint(&a).unwrap(), fingerprint(&b).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole point: a library swapped after install must not load.
    #[test]
    fn a_changed_library_is_refused() {
        let dir = scratch("changed");
        let lib = dir.join("plugin.dll");
        std::fs::write(&lib, b"the code you agreed to").unwrap();
        let installed = fingerprint(&lib).unwrap();

        std::fs::write(&lib, b"something else entirely").unwrap();
        let verdict = verify(&lib, Some(&installed)).unwrap();

        assert!(!verdict.is_trusted());
        assert!(matches!(verdict, Verdict::Changed { .. }));
        let message = verdict.refusal("thing");
        assert!(message.contains("thing"), "{message}");
        assert!(message.contains("disabled it"), "{message}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_untouched_library_loads() {
        let dir = scratch("unchanged");
        let lib = dir.join("plugin.dll");
        std::fs::write(&lib, b"the code you agreed to").unwrap();
        let installed = fingerprint(&lib).unwrap();

        let verdict = verify(&lib, Some(&installed)).unwrap();
        assert_eq!(verdict, Verdict::Unchanged);
        assert!(verdict.is_trusted());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A plugin installed before fingerprints existed is adopted rather than
    /// blocked — the file is the one they have been running all along.
    #[test]
    fn a_plugin_with_no_record_is_adopted_not_refused() {
        let dir = scratch("first");
        let lib = dir.join("plugin.dll");
        std::fs::write(&lib, b"installed last year").unwrap();

        let verdict = verify(&lib, None).unwrap();
        assert!(verdict.is_trusted());
        match verdict {
            Verdict::FirstSeen(hash) => assert_eq!(hash, fingerprint(&lib).unwrap()),
            other => panic!("expected FirstSeen, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_pass() {
        assert!(fingerprint(Path::new("definitely-not-here.dll")).is_err());
    }
}
