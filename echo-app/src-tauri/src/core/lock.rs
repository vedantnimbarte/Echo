//! Locks that survive a panic somewhere else.
//!
//! Echo shares its database connection, dictionary and decoder context across
//! the recording pipeline with `Mutex`, and every site took the guard with
//! `.lock().unwrap()`. That is idiomatic Rust and it has one consequence worth
//! naming: if any thread panics while holding one of those locks, the `Mutex` is
//! *poisoned*, and every later `.lock().unwrap()` on it panics too.
//!
//! So a single recoverable failure — one bad utterance, one unexpected `None` —
//! does not cost one transcript. It costs every transcript for the rest of the
//! session: dictation silently stops working and restarting is the only cure.
//! With no crash reporter, the user cannot even say what happened.
//!
//! Recovering is the right call *for these locks specifically*, because none of
//! them guards a multi-step invariant that a panic could leave half-applied:
//!
//! - the SQLite connection — rusqlite rolls an open transaction back when the
//!   statement is dropped, so what the next caller sees is a committed state
//! - the dictionary and the decoder prompt — read-mostly caches, where a panic
//!   mid-read leaves the contents exactly as they were
//! - the recording flags and last-delivery slots — single values, replaced whole
//!
//! Poisoning is a useful signal where a panic really can leave broken data. The
//! point here is that it is a decision, taken per lock, rather than a default
//! inherited from `unwrap`.

use std::sync::{Mutex, MutexGuard};

pub trait LockLive<T> {
    /// Take the guard, ignoring poison left by a panic in another thread.
    ///
    /// Still panics if the current thread already holds the lock, because that
    /// is a deadlock in the code rather than a state to recover from.
    fn lock_live(&self) -> MutexGuard<'_, T>;
}

impl<T> LockLive<T> for Mutex<T> {
    fn lock_live(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// The defect: one panic while holding a shared lock used to break every
    /// later use of it for the rest of the process.
    #[test]
    fn a_poisoned_lock_still_hands_over_its_contents() {
        let shared = Arc::new(Mutex::new(vec!["a transcript".to_string()]));

        let poisoner = Arc::clone(&shared);
        let died = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("something went wrong mid-utterance");
        })
        .join();
        assert!(died.is_err(), "the test needs that thread to have panicked");

        assert!(shared.lock().is_err(), "the lock should now be poisoned");
        assert_eq!(
            shared.lock_live().as_slice(),
            ["a transcript".to_string()],
            "the data was intact and should still be reachable"
        );
    }
}
