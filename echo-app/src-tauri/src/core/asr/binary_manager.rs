use std::io::Read;
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::error::{EchoError, Result};

/// Pinned whisper.cpp release whose prebuilt CLI we download on first run.
/// NOTE: v1.7.4/v1.7.5 published no binary assets (the download 404s); v1.7.6 is
/// the nearest tag that ships `whisper-bin-x64.zip`. Keep in sync with
/// `scripts/stage-runtime-deps.mjs`.
const WHISPER_RELEASE_TAG: &str = "v1.7.6";

/// The executable name whisper.cpp ships (renamed from `main` in v1.7.x).
#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "whisper-cli.exe";
#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "whisper-cli";

/// The Windows release archive that contains `whisper-cli.exe` and its DLLs.
/// whisper.cpp only ships ready-to-run binaries for Windows; on macOS/Linux we
/// fall back to a `whisper-cli` found on PATH (e.g. `brew install whisper-cpp`).
#[cfg(target_os = "windows")]
const WINDOWS_ASSET: &str = "whisper-bin-x64.zip";

/// Resolves and installs the bundled whisper.cpp command-line binary.
///
/// Sibling to [`super::model_manager::ModelManager`]: the model manager fetches
/// the `.bin` weights, this fetches the executable that runs them.
pub struct BinaryManager {
    bin_dir: PathBuf,
    /// A read-only directory of a copy bundled with the app (see
    /// `core::runtime_deps::bundled_whisper_dir`). Preferred over downloading.
    bundled_dir: Option<PathBuf>,
}

impl BinaryManager {
    pub fn new(bin_dir: PathBuf) -> Self {
        Self {
            bin_dir,
            bundled_dir: None,
        }
    }

    /// Builder: prefer a `whisper-cli` bundled in `dir` (from
    /// `core::runtime_deps::bundled_whisper_dir`) over downloading one.
    pub fn with_bundled_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.bundled_dir = dir;
        self
    }

    /// Path where we install/keep the downloaded binary.
    pub fn binary_path(&self) -> PathBuf {
        self.bin_dir.join(BINARY_NAME)
    }

    /// Resolve a runnable whisper-cli: a bundled copy first, then our installed
    /// (downloaded) copy, then anything named `whisper-cli` on the system PATH.
    /// `None` if none exist.
    pub fn resolve(&self) -> Option<PathBuf> {
        if let Some(dir) = &self.bundled_dir {
            let bundled = dir.join(BINARY_NAME);
            if bundled.exists() {
                return Some(bundled);
            }
        }
        let local = self.binary_path();
        if local.exists() {
            return Some(local);
        }
        find_on_path(BINARY_NAME)
    }

    pub fn is_installed(&self) -> bool {
        self.resolve().is_some()
    }

    /// Whether we can auto-download the binary for this platform (Windows only).
    pub fn can_auto_install() -> bool {
        cfg!(target_os = "windows")
    }

    /// Download and extract the whisper.cpp CLI for this platform, streaming
    /// fractional progress (0.0..1.0) on `progress_tx`. Returns the installed
    /// binary path. Errors on non-Windows platforms (no prebuilt asset).
    pub async fn download(&self, progress_tx: mpsc::Sender<f32>) -> Result<PathBuf> {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = progress_tx;
            return Err(EchoError::Config(format!(
                "No prebuilt whisper-cli for this platform. Install one named \
                 '{BINARY_NAME}' on your PATH (e.g. `brew install whisper-cpp`)."
            )));
        }

        #[cfg(target_os = "windows")]
        {
            tokio::fs::create_dir_all(&self.bin_dir)
                .await
                .map_err(|e| EchoError::Config(e.to_string()))?;

            let url = format!(
                "https://github.com/ggml-org/whisper.cpp/releases/download/{WHISPER_RELEASE_TAG}/{WINDOWS_ASSET}"
            );
            let tmp_zip = self.bin_dir.join("whisper-cli.zip.part");

            let resp = reqwest::get(&url)
                .await
                .map_err(|e| EchoError::AsrProvider(e.to_string()))?
                .error_for_status()
                .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

            let total = resp.content_length();
            let mut downloaded: u64 = 0;
            let mut last_emitted = -1.0_f32;

            let mut file = tokio::fs::File::create(&tmp_zip)
                .await
                .map_err(|e| EchoError::Config(e.to_string()))?;
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| EchoError::AsrProvider(e.to_string()))?;
                file.write_all(&chunk)
                    .await
                    .map_err(|e| EchoError::Config(e.to_string()))?;
                downloaded += chunk.len() as u64;
                if let Some(total) = total {
                    // Reserve the last 2% for extraction.
                    let p = (downloaded as f32 / total as f32 * 0.98).clamp(0.0, 0.98);
                    if p - last_emitted >= 0.01 {
                        last_emitted = p;
                        let _ = progress_tx.send(p).await;
                    }
                }
            }
            file.flush()
                .await
                .map_err(|e| EchoError::Config(e.to_string()))?;
            drop(file);

            // Extract on the blocking pool — zip reads are synchronous.
            let bin_dir = self.bin_dir.clone();
            let zip_path = tmp_zip.clone();
            tokio::task::spawn_blocking(move || extract_zip_flat(&zip_path, &bin_dir))
                .await
                .map_err(|e| EchoError::Config(e.to_string()))??;

            let _ = tokio::fs::remove_file(&tmp_zip).await;

            // Older whisper.cpp archives ship the CLI as `main.exe`; normalise to
            // `whisper-cli.exe` so the provider always finds it.
            let installed = self.binary_path();
            if !installed.exists() {
                for legacy in ["whisper-cli.exe", "main.exe", "whisper.exe"] {
                    let candidate = self.bin_dir.join(legacy);
                    if candidate.exists() {
                        let _ = std::fs::copy(&candidate, &installed);
                        break;
                    }
                }
            }

            let _ = progress_tx.send(1.0).await;
            if !installed.exists() {
                return Err(EchoError::Config(format!(
                    "whisper.cpp archive did not contain a recognisable CLI ('{BINARY_NAME}')"
                )));
            }
            Ok(installed)
        }
    }
}

/// Extract every file in `zip_path` into `dest`, flattening directory structure
/// so the binary and its DLLs land side by side.
fn extract_zip_flat(zip_path: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path).map_err(|e| EchoError::Config(e.to_string()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| EchoError::Config(e.to_string()))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| EchoError::Config(e.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = match entry.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_owned())) {
            Some(n) => n,
            None => continue,
        };
        let out_path = dest.join(&name);
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| EchoError::Config(e.to_string()))?;
        std::fs::write(&out_path, &buf).map_err(|e| EchoError::Config(e.to_string()))?;
    }
    Ok(())
}

/// Search the `PATH` environment variable for an executable named `name`.
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A staged/bundled copy must win over the download dir and PATH.
    #[test]
    fn resolve_prefers_bundled_copy() {
        let base = std::env::temp_dir().join(format!("echo-bm-{}", std::process::id()));
        let bin_dir = base.join("data-bin");
        let bundled = base.join("res-bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::create_dir_all(&bundled).unwrap();
        std::fs::write(bundled.join(BINARY_NAME), b"stub").unwrap();

        let mgr = BinaryManager::new(bin_dir.clone()).with_bundled_dir(Some(bundled.clone()));
        assert_eq!(mgr.resolve(), Some(bundled.join(BINARY_NAME)));

        // Without a bundled dir and an empty download dir, it must NOT resolve to
        // the bundled path (it falls through to PATH, which won't have our stub).
        let plain = BinaryManager::new(bin_dir.clone());
        assert_ne!(plain.resolve(), Some(bundled.join(BINARY_NAME)));

        let _ = std::fs::remove_dir_all(&base);
    }
}
