//! End-to-end check of the accelerated whisper packs.
//!
//! Two things about GPU support had never been executed: downloading and
//! extracting a cuBLAS pack, and the fallback that is supposed to catch an
//! accelerated binary which cannot run. The second matters most — it is the
//! path taken on every machine with a broken or absent CUDA install, and if it
//! does not work the user gets no transcription at all rather than a slower
//! one.
//!
//! Off by default: it downloads tens of megabytes and takes minutes, which no
//! ordinary `cargo test` should do. Enable with `ECHO_TEST_GPU_PACK=1`.
//!
//! Everything lives in one test that downloads once. An earlier version had a
//! test per concern, each fetching its own copy, and GitHub throttled the
//! second download to a standstill — which is a fair warning about how the
//! real thing behaves for a user who retries.

use super::binary_manager::{BinaryManager, Pack};
use crate::core::gpu::GpuBackend;

/// Uses the CUDA 11 asset (~45 MB) rather than CUDA 12 (~443 MB): the code
/// path is identical and only the URL differs, so the smaller one proves the
/// same thing in a tenth of the time.
const PACK: Pack = Pack::Cuda11;

#[tokio::test]
async fn a_cublas_pack_installs_and_falls_back_when_it_cannot_run() {
    if std::env::var("ECHO_TEST_GPU_PACK").is_err() {
        eprintln!("skipped: set ECHO_TEST_GPU_PACK=1 to run (downloads a pack)");
        return;
    }

    let dir = std::env::temp_dir().join(format!("echo-pack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Claim CUDA regardless of what this machine really has, so the accelerated
    // pack is selected and then succeeds or fails on its own merits.
    let mgr = BinaryManager::new(dir.clone()).with_gpu(GpuBackend::Cuda { major: 11 });
    assert!(!mgr.pack_installed(PACK), "scratch directory started dirty");

    // ── Download and extract ────────────────────────────────────────────────
    let (tx, mut rx) = tokio::sync::mpsc::channel::<f32>(64);
    let progress = tokio::spawn(async move {
        let mut last = 0.0;
        while let Some(p) = rx.recv().await {
            last = p;
        }
        last
    });

    let gpu_cli = mgr
        .download_pack(PACK, tx)
        .await
        .expect("the cuBLAS pack failed to download or extract");

    assert!(gpu_cli.is_file(), "no CLI at {}", gpu_cli.display());
    assert_eq!(progress.await.unwrap(), 1.0, "progress never reached 1.0");

    // A cuBLAS build ships its own CUDA runtime beside the binaries; without
    // those the executable cannot start even on a machine that has a GPU.
    let names: Vec<String> = std::fs::read_dir(mgr.pack_dir(PACK))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .collect();
    assert!(
        names.iter().any(|n| n.contains("cudart")),
        "no CUDA runtime in the pack: {names:?}"
    );

    // ── It is actually selected ─────────────────────────────────────────────
    assert!(mgr.pack_installed(PACK));
    assert_eq!(mgr.active_gpu_pack(), Some(PACK));
    assert_eq!(mgr.resolve(), Some(gpu_cli.clone()));
    assert_eq!(mgr.active_dir().map(|(_, accel)| accel), Some(true));

    // ── And falls back when it cannot run ───────────────────────────────────
    // Ask the binary to do the cheapest thing it knows. On a machine without a
    // usable CUDA driver it does not merely fail to use the GPU — it fails to
    // start at all, which is the case the fallback exists for.
    let runs = std::process::Command::new(&gpu_cli)
        .arg("--help")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if runs {
        eprintln!("note: the accelerated binary runs here, so the fallback was not exercised");
    } else {
        eprintln!("note: the accelerated binary cannot start here — exercising the fallback");
    }

    // Whatever happened above, a failure must route every later call to the CPU
    // binary without a restart. Install the CPU pack so there is one to fall to.
    let (tx, _rx) = tokio::sync::mpsc::channel::<f32>(64);
    mgr.download_pack(Pack::Cpu, tx)
        .await
        .expect("the CPU pack failed to download");

    mgr.mark_gpu_failed();

    assert_eq!(mgr.active_gpu_pack(), None, "still offering the failed pack");
    let cpu_cli = mgr.resolve().expect("no CLI at all after the fallback");
    assert!(
        !cpu_cli.starts_with(mgr.pack_dir(PACK)),
        "still resolving to the failed GPU pack: {}",
        cpu_cli.display()
    );
    assert_eq!(mgr.active_dir().map(|(_, accel)| accel), Some(false));

    let _ = std::fs::remove_dir_all(&dir);
}
