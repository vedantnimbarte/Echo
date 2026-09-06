//! Using Echo from a script instead of from the keyboard.
//!
//! Every other entry point in the app assumes a person: a hotkey, a window, an
//! app to type into. This one assumes a pipe. `echo --transcribe recording.mp3`
//! prints the transcript to stdout and exits, which is all a shell needs to put
//! Echo in the middle of anything else.
//!
//! It reuses the import path rather than the dictation path, for the reason
//! given in [`crate::commands::import`]: one long decode where a model reload
//! costs nothing, and keeping it off the resident server means a long file
//! cannot block dictation behind it.
//!
//! **Output discipline.** The transcript goes to stdout and *nothing else does*
//! — errors go to stderr and set a non-zero exit code, so `$(echo --transcribe
//! x.wav)` is the transcript and never a log line. Like [`crate::selftest`],
//! this runs inside `setup` (the database, models and engine only exist once
//! setup has run) and never returns.

use tauri::{AppHandle, Manager};

/// The flag that selects this mode.
const FLAG: &str = "--transcribe";

/// Whether the process was started with `--transcribe <path>`.
pub fn requested() -> bool {
    path().is_some()
}

/// The file argument, if the flag was given one.
fn path() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == FLAG)?;
    args.get(i + 1).filter(|a| !a.starts_with("--")).cloned()
}

/// The `--language` argument, if given. Absent means auto-detect, exactly as
/// it does everywhere else.
fn language() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == "--language")?;
    args.get(i + 1).filter(|a| !a.starts_with("--")).cloned()
}

/// Transcribe the file named on the command line, print it, and exit.
///
/// Never returns: the process exists to answer one question.
pub fn run(app: &AppHandle) -> ! {
    let Some(path) = path() else {
        eprintln!("{FLAG} needs a file path.");
        app.exit(2);
        std::process::exit(2);
    };

    let handle = app.clone();
    let language = language();
    let result = tauri::async_runtime::block_on(async move {
        let state = handle.state::<crate::state::AppState>();
        crate::commands::import::transcribe_path(&state, &path, language.as_deref()).await
    });

    match result {
        Ok(text) => {
            println!("{text}");
            app.exit(0);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("{e}");
            app.exit(1);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    /// A flag with no path is a usage error, not a transcription of the next
    /// flag. Pinned as a pure check on the parsing rule, since the real
    /// argument list belongs to the process.
    #[test]
    fn a_following_flag_is_not_mistaken_for_a_path() {
        let args = ["echo", "--transcribe", "--language", "en"];
        let i = args.iter().position(|a| *a == "--transcribe").unwrap();
        let path = args.get(i + 1).filter(|a| !a.starts_with("--"));
        assert!(path.is_none(), "a flag must not be read as a file path");
    }
}
