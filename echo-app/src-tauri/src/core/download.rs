//! Streaming file download with fractional progress.
//!
//! Shared by the Whisper model manager and the wake-word model manager — both
//! fetch a large file from a fixed URL and want a progress bar while it lands.

use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::error::{EchoError, Result};

/// How long to wait for the server to answer at all.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a download may deliver nothing before it is declared dead.
const STALL_TIMEOUT: Duration = Duration::from_secs(60);

/// A client for large downloads.
///
/// Only the connect phase is bounded here. A total timeout would be wrong: the
/// CUDA 12 pack is 443 MB, and taking ten minutes on a slow line is legitimate
/// rather than a failure. The stall is handled per chunk instead — see
/// [`next_chunk`] for why `reqwest`'s own `read_timeout` is not enough.
pub fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|e| EchoError::AsrProvider(format!("could not build a download client: {e}")))
}

/// Read the next chunk, giving up if the connection goes quiet.
///
/// This exists because the obvious fix did not work. `reqwest::get` has no
/// timeout at all, so a stalled download hangs forever — observed for real: a
/// pack download stopped at 29 MB and sat there, progress bar frozen, with no
/// error and nothing to retry. Configuring `read_timeout` on the client looked
/// like the answer and changed nothing; a later stall sat for eleven minutes
/// with a sixty-second `read_timeout` configured, because it does not cover
/// reads driven through `bytes_stream()`.
///
/// Timing the stream read explicitly does work, and is obvious enough to stay
/// working.
pub(crate) async fn next_chunk<S, B>(stream: &mut S) -> Result<Option<B>>
where
    S: futures_util::Stream<Item = reqwest::Result<B>> + Unpin,
{
    next_chunk_within(stream, STALL_TIMEOUT).await
}

/// [`next_chunk`] with the deadline supplied, so a test need not wait a minute
/// to prove the timeout works.
async fn next_chunk_within<S, B>(stream: &mut S, limit: Duration) -> Result<Option<B>>
where
    S: futures_util::Stream<Item = reqwest::Result<B>> + Unpin,
{
    match tokio::time::timeout(limit, stream.next()).await {
        Err(_) => Err(EchoError::AsrProvider(format!(
            "the download stalled: nothing received for {}s",
            limit.as_secs().max(1)
        ))),
        Ok(None) => Ok(None),
        Ok(Some(chunk)) => chunk
            .map(Some)
            .map_err(|e| EchoError::AsrProvider(e.to_string())),
    }
}

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

    let resp = client()?
        .get(url)
        .send()
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

    while let Some(chunk) = next_chunk(&mut stream).await? {
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    /// The bug this guards: a connection that goes quiet must error, not hang.
    /// Verified against the real thing first — an 11-minute stall with a
    /// 60-second `read_timeout` configured, which never fired.
    #[tokio::test]
    async fn a_stalled_stream_errors_instead_of_hanging_forever() {
        // Yields one chunk, then never resolves again.
        let mut stalled = Box::pin(stream::once(async { Ok(vec![1u8, 2, 3]) }).chain(
            stream::once(async {
                std::future::pending::<()>().await;
                unreachable!()
            }),
        ));

        let brief = Duration::from_millis(50);
        let first = next_chunk_within(&mut stalled, brief).await.expect("first chunk");
        assert_eq!(first, Some(vec![1, 2, 3]));

        let err = next_chunk_within(&mut stalled, brief)
            .await
            .expect_err("a stalled stream must not hang");
        assert!(
            err.to_string().contains("stalled"),
            "the message should say what happened: {err}"
        );
    }

    /// A stream that ends normally is not a stall.
    #[tokio::test]
    async fn a_finished_stream_ends_cleanly() {
        let mut done = Box::pin(stream::iter(Vec::<reqwest::Result<Vec<u8>>>::new()));
        assert!(next_chunk_within(&mut done, Duration::from_millis(50))
            .await
            .unwrap()
            .is_none());
    }
}
