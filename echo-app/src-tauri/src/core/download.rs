//! Streaming file download with fractional progress.
//!
//! Shared by the Whisper model manager and the wake-word model manager — both
//! fetch a large file from a fixed URL and want a progress bar while it lands.

use std::path::Path;

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::error::{EchoError, Result};

/// Download `url` to `dest`, streaming through a sibling `.part` file so an
/// interrupted download never leaves something that looks like a valid model.
/// Emits fractional progress (0.0..=1.0) on `progress_tx`, throttled to ~1%
/// steps so a fast connection can't flood the event bus.
pub async fn download_file(url: &str, dest: &Path, progress_tx: mpsc::Sender<f32>) -> Result<()> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| EchoError::Config(e.to_string()))?;
    }

    let tmp_path = dest.with_extension("part");

    crate::core::egress::record(url, "download");

    let resp = reqwest::get(url)
        .await
        .map_err(|e| EchoError::AsrProvider(e.to_string()))?
        .error_for_status()
        .map_err(|e| EchoError::AsrProvider(e.to_string()))?;

    let total = resp.content_length();
    let mut downloaded: u64 = 0;
    let mut last_emitted = -1.0_f32;

    let mut file = tokio::fs::File::create(&tmp_path)
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
            let progress = (downloaded as f32 / total as f32).clamp(0.0, 1.0);
            if progress - last_emitted >= 0.01 {
                last_emitted = progress;
                let _ = progress_tx.send(progress).await;
            }
        }
    }

    file.flush()
        .await
        .map_err(|e| EchoError::Config(e.to_string()))?;
    drop(file);

    tokio::fs::rename(&tmp_path, dest)
        .await
        .map_err(|e| EchoError::Config(e.to_string()))?;

    let _ = progress_tx.send(1.0).await;
    Ok(())
}
