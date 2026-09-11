//! Audio on disk, so a crash does not take the words with it.
//!
//! Everything else Echo keeps about a dictation is written *after* the decode:
//! History gets the transcript, the retry slot gets the samples, and both live
//! in memory until then. So the one window where a crash — or a power cut, or
//! an OOM kill — destroys something unrecoverable is the window where the user
//! has already spoken and Echo has not finished listening or thinking. They
//! said it, and there is no trace anywhere that they did.
//!
//! This closes that. Speech chunks are appended to a spool file as they are
//! captured; a clean stop deletes it. A file still sitting there at the next
//! launch therefore means exactly one thing — the last session ended badly —
//! and [`recover`] turns it into a WAV the user can transcribe from Settings.
//!
//! **Raw `f32` samples, not a WAV.** A WAV header carries the length, so a
//! file that stops mid-write is a broken WAV; raw samples have no length to be
//! wrong, and the header is added at recovery when the length is known.
//!
//! ponytail: unbuffered writes, one per chunk, ~4 KB each. That survives a
//! process crash, which is the case this exists for. It does not survive a
//! power cut mid-write beyond what the OS has flushed, and buying that would
//! mean an fsync per chunk — real latency in the capture path for a rarer
//! failure. Raise it if anyone ever reports losing audio to a hard power loss.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::core::asr::wav;
use crate::error::{EchoError, Result};

/// Name of the in-progress spool. One per install: two Echo processes sharing a
/// data directory is not a supported arrangement, and a second one would be
/// racing over the database long before it raced over this.
const SPOOL: &str = "unfinished.pcm";

/// Prefix for a spool that outlived its session and became a real file.
const RECOVERED: &str = "recovered-";

/// Sample rate everything downstream of capture runs at.
const SAMPLE_RATE: u32 = 16_000;

/// Below this, a recovered spool is not worth showing anyone: a quarter second
/// is a click of the hotkey, not a sentence someone wants back.
const MIN_RECOVERABLE_SAMPLES: usize = SAMPLE_RATE as usize / 4;

/// The open spool for the session being captured.
pub struct Spool {
    file: fs::File,
    path: PathBuf,
}

impl Spool {
    /// Open a fresh spool in `dir`, discarding any previous one.
    ///
    /// Best effort, like the log: a data directory that cannot be written is
    /// worth a warning, never a dictation that refuses to start.
    pub fn create(dir: &Path) -> Option<Self> {
        let path = dir.join(SPOOL);
        match fs::File::create(&path) {
            Ok(file) => Some(Self { file, path }),
            Err(e) => {
                tracing::warn!("No crash spool this session ({}): {e}", path.display());
                None
            }
        }
    }

    /// Append one chunk of captured speech.
    pub fn write(&mut self, samples: &[f32]) {
        let mut bytes = Vec::with_capacity(samples.len() * 4);
        for s in samples {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        if let Err(e) = self.file.write_all(&bytes) {
            // Logged once per failure rather than swallowed, but never fatal:
            // losing the safety net must not stop the dictation it protects.
            tracing::warn!("Could not extend the crash spool: {e}");
        }
    }

    /// Delete the spool. Called when the session ends the way it should, which
    /// is what makes a surviving file mean "something went wrong".
    pub fn finish(self) {
        drop(self.file);
        if let Err(e) = fs::remove_file(&self.path) {
            tracing::warn!("Could not clear the crash spool: {e}");
        }
    }
}

/// Turn a spool left behind by a crashed session into a WAV beside it.
///
/// Returns the recovered file, or `None` when there was nothing to recover —
/// no spool, or one too short to be a sentence. Either way the spool is gone
/// afterwards, so a startup never recovers the same audio twice.
pub fn recover(dir: &Path) -> Result<Option<PathBuf>> {
    let spool = dir.join(SPOOL);
    let raw = match fs::read(&spool) {
        Ok(raw) => raw,
        Err(_) => return Ok(None), // The ordinary case: last session ended well.
    };

    let samples = decode(&raw);
    if samples.len() < MIN_RECOVERABLE_SAMPLES {
        let _ = fs::remove_file(&spool);
        return Ok(None);
    }

    // Seconds since the epoch: unique enough for a file nobody sorts by name,
    // and it does not need a clock that agrees with anything.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let out = dir.join(format!("{RECOVERED}{stamp}.wav"));
    let encoded = wav::pcm_f32_to_wav(&samples, SAMPLE_RATE)?;
    fs::write(&out, encoded)
        .map_err(|e| EchoError::Config(format!("could not write {}: {e}", out.display())))?;

    // Only after the WAV is on disk: a failure above must leave the spool for
    // the next launch to try again, not consume it.
    let _ = fs::remove_file(&spool);
    tracing::info!(
        "Recovered {:.1}s of audio from an interrupted session: {}",
        samples.len() as f32 / SAMPLE_RATE as f32,
        out.display()
    );
    Ok(Some(out))
}

/// Every recovered recording still waiting to be dealt with, newest first.
pub fn recovered(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_recovered(p))
        .collect();
    // By name, which is by timestamp — the one thing the name is for.
    found.sort();
    found.reverse();
    found
}

/// Whether `path` is one of ours, checked before any deletion.
///
/// A `discard` command takes a path from the frontend, and "delete the file at
/// this path" is not something to expose on the strength of the caller meaning
/// well.
pub fn is_recovered(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "wav")
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(RECOVERED))
}

/// Read raw little-endian `f32` samples back, ignoring a trailing partial one.
///
/// A crash can land mid-sample, so the tail is only kept when all four bytes
/// of it arrived.
fn decode(raw: &[u8]) -> Vec<f32> {
    raw.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory of our own, named the way every other test module
    /// here names one. Removed first, so a previous run cannot seed this one.
    struct Dir(PathBuf);
    impl Dir {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir()
                .join(format!("echo-spool-{}-{tag}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_finished_session_leaves_nothing_to_recover() {
        let d = Dir::new("finished");
        let mut s = Spool::create(d.path()).unwrap();
        s.write(&[0.5; 16_000]);
        s.finish();

        assert!(!d.path().join(SPOOL).exists());
        assert_eq!(recover(d.path()).unwrap(), None);
    }

    #[test]
    fn a_crashed_session_leaves_a_wav() {
        let d = Dir::new("crashed");
        let mut s = Spool::create(d.path()).unwrap();
        s.write(&[0.25; 8_000]);
        s.write(&[-0.25; 8_000]);
        drop(s); // The crash: no `finish`, so the file stays.

        let out = recover(d.path()).unwrap().expect("a second of audio is worth keeping");
        assert!(is_recovered(&out));
        assert!(out.exists());
        // Consumed, so the next launch does not recover it a second time.
        assert!(!d.path().join(SPOOL).exists());
        assert_eq!(recover(d.path()).unwrap(), None);
        assert_eq!(recovered(d.path()), vec![out]);
    }

    #[test]
    fn a_stray_hotkey_press_is_not_offered_back() {
        let d = Dir::new("stray");
        let mut s = Spool::create(d.path()).unwrap();
        s.write(&[0.1; 100]); // Far under a quarter second.
        drop(s);

        assert_eq!(recover(d.path()).unwrap(), None);
        // Still cleared, or it would be retried at every launch forever.
        assert!(!d.path().join(SPOOL).exists());
        assert!(recovered(d.path()).is_empty());
    }

    #[test]
    fn a_spool_cut_mid_sample_keeps_the_whole_ones() {
        // Two complete samples and a byte of a third, which is what a crash
        // between `write_all`s can leave behind.
        let raw = [0u8, 0, 0, 0, 0, 0, 0, 0, 7];
        assert_eq!(decode(&raw).len(), 2);
    }

    #[test]
    fn only_our_own_files_are_recognised() {
        assert!(is_recovered(Path::new("/x/recovered-17.wav")));
        // The live spool is not a recovered file.
        assert!(!is_recovered(Path::new("/x/unfinished.pcm")));
        // Nor is anything else that happens to sit in the data directory.
        assert!(!is_recovered(Path::new("/x/echo.db")));
        assert!(!is_recovered(Path::new("/x/models/base.en.bin")));
        assert!(!is_recovered(Path::new("/x/my-notes.wav")));
    }
}
