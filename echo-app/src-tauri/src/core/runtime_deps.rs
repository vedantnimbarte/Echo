//! Locating an optionally-bundled copy of the **whisper.cpp CLI** inside a
//! packaged build's resource directory.
//!
//! whisper-cli is an external subprocess binary (plus its sidecar DLLs) that
//! the default local ASR engine shells out to. By default
//! [`BinaryManager`](super::asr::binary_manager::BinaryManager) downloads it on
//! first run; when a copy is staged in the bundle (`<resources>/bin`) we prefer
//! that, so a signed installer can work fully offline.
//!
//! (The ONNX Runtime that backs Silero VAD needs no such handling: `ort`
//! statically links it into the executable, so there is no shared library to
//! ship — see the `ort` dependency note in `Cargo.toml`.)
//!
//! See `docs/BUNDLING.md` for how the resource dir gets populated at package
//! time.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// Resource subdir holding the whisper.cpp CLI (and its sidecar DLLs).
const BIN_SUBDIR: &str = "bin";

/// Directory of a bundled whisper-cli (with its DLLs), if staged. Passed to
/// [`BinaryManager`](super::asr::binary_manager::BinaryManager) so it prefers
/// the shipped copy over downloading one.
pub fn bundled_whisper_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().resource_dir().ok()?.join(BIN_SUBDIR);
    dir.is_dir().then_some(dir)
}
