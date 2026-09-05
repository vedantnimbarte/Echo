use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::core::gpu::GpuBackend;
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

/// The HTTP server front-end. Same decoder as the CLI, but it keeps the model
/// resident between requests — see [`super::whisper_server`].
#[cfg(target_os = "windows")]
const SERVER_NAME: &str = "whisper-server.exe";
#[cfg(not(target_os = "windows"))]
const SERVER_NAME: &str = "whisper-server";

/// An interchangeable set of whisper.cpp binaries built against one compute
/// backend. Each lives in its own directory because they ship conflicting
/// copies of the same `ggml*` DLLs — unpacking two into one folder gives you
/// whichever was extracted last, which is exactly the kind of failure that only
/// shows up on someone else's machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pack {
    /// Stock CPU build. Always the fallback; never fails to be applicable.
    Cpu,
    /// cuBLAS build for CUDA 11.x drivers.
    Cuda11,
    /// cuBLAS build for CUDA 12.x drivers.
    Cuda12,
}

impl Pack {
    /// Subdirectory under the binaries directory. `Cpu` is the root so the
    /// pre-existing install location keeps working untouched.
    pub fn subdir(self) -> Option<&'static str> {
        match self {
            Pack::Cpu => None,
            Pack::Cuda11 => Some("cuda11"),
            Pack::Cuda12 => Some("cuda12"),
        }
    }

    /// Release asset providing this pack on Windows.
    pub fn asset(self) -> &'static str {
        match self {
            Pack::Cpu => "whisper-bin-x64.zip",
            Pack::Cuda11 => "whisper-cublas-11.8.0-bin-x64.zip",
            Pack::Cuda12 => "whisper-cublas-12.4.0-bin-x64.zip",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Pack::Cpu => "cpu",
            Pack::Cuda11 => "cuda11",
            Pack::Cuda12 => "cuda12",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Pack::Cpu => "CPU",
            Pack::Cuda11 => "NVIDIA CUDA 11",
            Pack::Cuda12 => "NVIDIA CUDA 12",
        }
    }

    /// The pack that matches a detected GPU, if one exists for this platform.
    ///
    /// Metal is absent on purpose: it is compiled into the stock macOS build,
    /// so the "GPU pack" for a Mac *is* [`Pack::Cpu`].
    pub fn for_gpu(gpu: GpuBackend) -> Option<Self> {
        match gpu {
            // A 12.x driver runs the 12.4 build; anything older that still
            // reports CUDA gets the 11.8 build. Newer majors keep the newest
            // pack we know about rather than falling back to CPU.
            GpuBackend::Cuda { major } if major >= 12 => Some(Pack::Cuda12),
            GpuBackend::Cuda { major: 11 } => Some(Pack::Cuda11),
            _ => None,
        }
    }
}

/// Resolves and installs the bundled whisper.cpp binaries.
///
/// Sibling to [`super::model_manager::ModelManager`]: the model manager fetches
/// the `.bin` weights, this fetches the executables that run them.
pub struct BinaryManager {
    bin_dir: PathBuf,
    /// A read-only directory of a copy bundled with the app (see
    /// `core::runtime_deps::bundled_whisper_dir`). Preferred over downloading.
    bundled_dir: Option<PathBuf>,
    /// What this machine can accelerate with, detected once at startup.
    gpu: GpuBackend,
    /// Set when an accelerated binary failed to run. Latches for the session so
    /// a machine with a broken CUDA install degrades to CPU once instead of
    /// retrying — and paying the failed startup — on every utterance.
    gpu_failed: AtomicBool,
}

impl BinaryManager {
    pub fn new(bin_dir: PathBuf) -> Self {
        Self {
            bin_dir,
            bundled_dir: None,
            gpu: GpuBackend::None,
            gpu_failed: AtomicBool::new(false),
        }
    }

    /// Builder: prefer a `whisper-cli` bundled in `dir` (from
    /// `core::runtime_deps::bundled_whisper_dir`) over downloading one.
    pub fn with_bundled_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.bundled_dir = dir;
        self
    }

    /// Builder: record the detected GPU so accelerated packs can be selected.
    pub fn with_gpu(mut self, gpu: GpuBackend) -> Self {
        self.gpu = gpu;
        self
    }

    pub fn gpu(&self) -> GpuBackend {
        self.gpu
    }

    /// Directory holding a pack's binaries.
    pub fn pack_dir(&self, pack: Pack) -> PathBuf {
        match pack.subdir() {
            Some(sub) => self.bin_dir.join(sub),
            None => self.bin_dir.clone(),
        }
    }

    /// Whether a pack's CLI is present on disk.
    pub fn pack_installed(&self, pack: Pack) -> bool {
        self.pack_dir(pack).join(BINARY_NAME).is_file()
    }

    /// The accelerated pack this machine should use, if the hardware supports
    /// one *and* it has been downloaded *and* it has not already failed.
    pub fn active_gpu_pack(&self) -> Option<Pack> {
        if self.gpu_failed.load(Ordering::Relaxed) {
            return None;
        }
        let pack = Pack::for_gpu(self.gpu)?;
        self.pack_installed(pack).then_some(pack)
    }

    /// The accelerated pack this machine *could* use once downloaded. Drives the
    /// settings UI's offer to install it.
    pub fn available_gpu_pack(&self) -> Option<Pack> {
        Pack::for_gpu(self.gpu)
    }

    /// Record that an accelerated binary failed to start or crashed.
    ///
    /// Latching this rather than reacting per-call is deliberate: a broken CUDA
    /// install fails the same way every time, and retrying it once per
    /// utterance would turn a working CPU fallback into a permanent stutter.
    pub fn mark_gpu_failed(&self) {
        if !self.gpu_failed.swap(true, Ordering::Relaxed) {
            tracing::warn!("GPU whisper backend failed; falling back to CPU for this session");
        }
    }

    pub fn gpu_failed(&self) -> bool {
        self.gpu_failed.load(Ordering::Relaxed)
    }

    /// Path where we install/keep the downloaded CPU binary.
    pub fn binary_path(&self) -> PathBuf {
        self.bin_dir.join(BINARY_NAME)
    }

    /// The directory whose binaries should be run, and whether it is accelerated.
    ///
    /// Order: an installed GPU pack, then a copy bundled in the installer, then
    /// our downloaded copy. `None` means nothing is installed and the caller
    /// should fall back to `PATH`.
    pub fn active_dir(&self) -> Option<(PathBuf, bool)> {
        if let Some(pack) = self.active_gpu_pack() {
            return Some((self.pack_dir(pack), true));
        }
        // Metal is not a pack — the stock macOS build is already accelerated.
        let metal = matches!(self.gpu, GpuBackend::Metal) && !self.gpu_failed();
        if let Some(dir) = &self.bundled_dir {
            if dir.join(BINARY_NAME).is_file() {
                return Some((dir.clone(), metal));
            }
        }
        if self.binary_path().is_file() {
            return Some((self.bin_dir.clone(), metal));
        }
        None
    }

    /// Resolve a runnable whisper-cli: the active pack first, then anything
    /// named `whisper-cli` on the system PATH. `None` if none exist.
    pub fn resolve(&self) -> Option<PathBuf> {
        self.resolve_named(BINARY_NAME)
    }

    /// Resolve the HTTP server front-end, which not every install has: older
    /// bundled copies and most PATH installs ship only the CLI.
    pub fn resolve_server(&self) -> Option<PathBuf> {
        self.resolve_named(SERVER_NAME)
    }

    fn resolve_named(&self, name: &str) -> Option<PathBuf> {
        if let Some((dir, _)) = self.active_dir() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        // The active dir is chosen by CLI presence, so a pack without a server
        // still has to fall through to the other locations here.
        for dir in [self.bundled_dir.clone(), Some(self.bin_dir.clone())]
            .into_iter()
            .flatten()
        {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        find_on_path(name)
    }

    pub fn is_installed(&self) -> bool {
        self.resolve().is_some()
    }

    /// Whether we can auto-download binaries for this platform (Windows only).
    pub fn can_auto_install() -> bool {
        cfg!(target_os = "windows")
    }

    /// Download and extract the stock CPU whisper.cpp binaries.
    pub async fn download(&self, progress_tx: mpsc::Sender<f32>) -> Result<PathBuf> {
        self.download_pack(Pack::Cpu, progress_tx).await
    }

    /// Download and extract one binary pack, streaming fractional progress
    /// (0.0..1.0) on `progress_tx`. Returns the installed CLI path.
    ///
    /// Errors on non-Windows platforms: whisper.cpp publishes ready-to-run
    /// binaries for Windows only, so elsewhere we rely on a `whisper-cli` the
    /// user installed themselves (e.g. `brew install whisper-cpp`).
    pub async fn download_pack(
        &self,
        pack: Pack,
        progress_tx: mpsc::Sender<f32>,
    ) -> Result<PathBuf> {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (pack, progress_tx);
            return Err(EchoError::Config(format!(
                "No prebuilt whisper binaries for this platform. Install one named \
                 '{BINARY_NAME}' on your PATH (e.g. `brew install whisper-cpp`)."
            )));
        }

        #[cfg(target_os = "windows")]
        {
            let dest = self.pack_dir(pack);
            tokio::fs::create_dir_all(&dest)
                .await
                .map_err(|e| EchoError::Config(e.to_string()))?;

            let url = format!(
                "https://github.com/ggml-org/whisper.cpp/releases/download/{WHISPER_RELEASE_TAG}/{}",
                pack.asset()
            );
            let tmp_zip = dest.join("whisper-pack.zip.part");

            crate::core::egress::record(&url, "whisper binary download");

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
            let extract_dir = dest.clone();
            let zip_path = tmp_zip.clone();
            tokio::task::spawn_blocking(move || extract_zip_flat(&zip_path, &extract_dir))
                .await
                .map_err(|e| EchoError::Config(e.to_string()))??;

            let _ = tokio::fs::remove_file(&tmp_zip).await;

            // Older whisper.cpp archives ship the CLI as `main.exe`; normalise to
            // `whisper-cli.exe` so the provider always finds it.
            let installed = dest.join(BINARY_NAME);
            if !installed.exists() {
                for legacy in ["main.exe", "whisper.exe"] {
                    let candidate = dest.join(legacy);
                    if candidate.exists() {
                        let _ = std::fs::copy(&candidate, &installed);
                        break;
                    }
                }
            }

            let _ = progress_tx.send(1.0).await;
            if !installed.exists() {
                return Err(EchoError::Config(format!(
                    "The {} archive did not contain a recognisable CLI ('{BINARY_NAME}')",
                    pack.label()
                )));
            }
            Ok(installed)
        }
    }
}

/// Extract every file in `zip_path` into `dest`, flattening directory structure
/// so the binaries and their DLLs land side by side.
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

    /// Unique scratch dir per test, so they can run in parallel.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("echo-bm-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A staged/bundled copy must win over the download dir and PATH.
    #[test]
    fn resolve_prefers_bundled_copy() {
        let base = scratch("bundled");
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

    /// The whole point of detection: an installed CUDA pack outranks the plain
    /// CPU binary sitting in the same tree.
    #[test]
    fn an_installed_gpu_pack_wins_over_cpu() {
        let base = scratch("gpu");
        std::fs::write(base.join(BINARY_NAME), b"cpu").unwrap();
        let cuda = base.join("cuda12");
        std::fs::create_dir_all(&cuda).unwrap();
        std::fs::write(cuda.join(BINARY_NAME), b"gpu").unwrap();

        let mgr = BinaryManager::new(base.clone()).with_gpu(GpuBackend::Cuda { major: 12 });
        assert_eq!(mgr.active_gpu_pack(), Some(Pack::Cuda12));
        assert_eq!(mgr.resolve(), Some(cuda.join(BINARY_NAME)));
        assert_eq!(mgr.active_dir().map(|(_, accel)| accel), Some(true));

        let _ = std::fs::remove_dir_all(&base);
    }

    /// Hardware alone is not enough — without the download we must stay on CPU
    /// rather than pointing at a directory that does not exist.
    #[test]
    fn a_detected_gpu_without_its_pack_stays_on_cpu() {
        let base = scratch("nopack");
        std::fs::write(base.join(BINARY_NAME), b"cpu").unwrap();

        let mgr = BinaryManager::new(base.clone()).with_gpu(GpuBackend::Cuda { major: 12 });
        assert_eq!(mgr.active_gpu_pack(), None);
        assert_eq!(mgr.available_gpu_pack(), Some(Pack::Cuda12));
        assert_eq!(mgr.resolve(), Some(base.join(BINARY_NAME)));

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A failed GPU start must drop us to CPU for the rest of the session.
    #[test]
    fn gpu_failure_latches_and_falls_back_to_cpu() {
        let base = scratch("failed");
        std::fs::write(base.join(BINARY_NAME), b"cpu").unwrap();
        let cuda = base.join("cuda12");
        std::fs::create_dir_all(&cuda).unwrap();
        std::fs::write(cuda.join(BINARY_NAME), b"gpu").unwrap();

        let mgr = BinaryManager::new(base.clone()).with_gpu(GpuBackend::Cuda { major: 12 });
        assert_eq!(mgr.resolve(), Some(cuda.join(BINARY_NAME)));

        mgr.mark_gpu_failed();
        assert_eq!(mgr.active_gpu_pack(), None);
        assert_eq!(mgr.resolve(), Some(base.join(BINARY_NAME)));

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A pack that ships no server must not stop us finding one elsewhere,
    /// otherwise installing a GPU pack would silently disable the fast path.
    #[test]
    fn server_lookup_falls_through_a_pack_without_one() {
        let base = scratch("server");
        std::fs::write(base.join(BINARY_NAME), b"cpu").unwrap();
        std::fs::write(base.join(SERVER_NAME), b"cpu-server").unwrap();
        let cuda = base.join("cuda12");
        std::fs::create_dir_all(&cuda).unwrap();
        std::fs::write(cuda.join(BINARY_NAME), b"gpu").unwrap(); // no server here

        let mgr = BinaryManager::new(base.clone()).with_gpu(GpuBackend::Cuda { major: 12 });
        assert_eq!(mgr.resolve(), Some(cuda.join(BINARY_NAME)));
        assert_eq!(mgr.resolve_server(), Some(base.join(SERVER_NAME)));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn gpu_backends_map_to_the_right_pack() {
        assert_eq!(Pack::for_gpu(GpuBackend::Cuda { major: 12 }), Some(Pack::Cuda12));
        assert_eq!(Pack::for_gpu(GpuBackend::Cuda { major: 13 }), Some(Pack::Cuda12));
        assert_eq!(Pack::for_gpu(GpuBackend::Cuda { major: 11 }), Some(Pack::Cuda11));
        // Metal needs no pack, and a CUDA 10 driver has no build we can offer.
        assert_eq!(Pack::for_gpu(GpuBackend::Metal), None);
        assert_eq!(Pack::for_gpu(GpuBackend::Cuda { major: 10 }), None);
        assert_eq!(Pack::for_gpu(GpuBackend::None), None);
    }

    #[test]
    fn every_pack_has_a_distinct_id() {
        let ids: Vec<_> = [Pack::Cpu, Pack::Cuda11, Pack::Cuda12]
            .iter()
            .map(|p| p.id())
            .collect();
        assert_eq!(ids, vec!["cpu", "cuda11", "cuda12"]);
    }
}
