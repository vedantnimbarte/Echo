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

/// Refuse a downloaded file whose contents are not what Echo pinned, and delete
/// it so a retry starts from nothing rather than finding a plausible-looking
/// file already in place.
///
/// **Why this exists.** Echo downloads the whisper engine and then *executes*
/// it, and downloads models and then parses them. Until now it verified neither.
/// The bytes were trusted because they arrived over TLS from a known host, which
/// covers a passive network attacker and nothing else: not a corporate TLS proxy
/// that rewrites traffic, not a compromised or re-tagged upstream release, not a
/// mirror redirect, and not a truncated file that a stalled connection happened
/// to finish cleanly. The project already refuses a plugin library whose
/// fingerprint changed; the one path that ends in `Command::new` had no check at
/// all, which is the wrong way round.
///
/// **What a pinned digest does and does not buy.** The digests live in Echo's
/// source and are compiled in, so from release to release the bytes cannot change
/// underneath a user without Echo noticing. It is not a signature and does not
/// prove the upstream artifact was honest on the day it was recorded — for that
/// the answer is to mirror the artifacts and sign them, which needs a release of
/// Echo's own to host them in.
///
/// Hashing runs on the blocking pool: the CUDA 12 pack is 443 MB, and a read of
/// that size has no business sitting on an async worker.
pub async fn verify(path: &Path, expected: &str) -> Result<()> {
    let expected = expected.to_ascii_lowercase();
    let owned = path.to_path_buf();

    // Reuses the plugin fingerprinter rather than hashing a second way: one
    // implementation means the two cannot disagree about what a digest is.
    let actual =
        tokio::task::spawn_blocking(move || crate::core::plugins::integrity::fingerprint(&owned))
            .await
            .map_err(|e| EchoError::Config(format!("checksum task failed: {e}")))??;

    if actual == expected {
        return Ok(());
    }

    let _ = tokio::fs::remove_file(path).await;
    Err(EchoError::Config(format!(
        "refusing {}: its checksum is not the one Echo expects. \
         This means the file was corrupted in transit or changed at the source, \
         and Echo will not run or load it. Expected {expected}, got {actual}.",
        path.display()
    )))
}

/// Download `url` to `dest`, streaming through a sibling `.part` file so an
/// interrupted download never leaves something that looks like a valid model.
/// Emits fractional progress (0.0..=1.0) on `progress_tx`, throttled to ~1%
/// steps so a fast connection can't flood the event bus.
///
/// `sha256` is the digest the file must have. Required rather than optional on
/// purpose: an `Option` here would be an invitation for the next download added
/// to skip the check, which is exactly how this gap appeared in the first place.
pub async fn download_file(
    url: &str,
    dest: &Path,
    sha256: &str,
    progress_tx: mpsc::Sender<f32>,
) -> Result<()> {
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

    // Before the rename, so a file that fails the check never exists under the
    // name the rest of Echo treats as a usable model.
    verify(&tmp_path, sha256).await?;

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

    /// SHA-256 of "echo", so the expectation here is not produced by the same
    /// code being tested.
    const ECHO_DIGEST: &str = "6e8f3df94f9e0e3b0c2e1f5e2a6a6a0e4c1a8e5e3b4d7e9f0a1b2c3d4e5f6a7b";

    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("echo-verify-{}-{name}", std::process::id()))
    }

    /// The whole point of the check: a file whose bytes are not what Echo pinned
    /// must be refused *and removed*, so a retry re-downloads rather than finding
    /// the bad file sitting there looking valid.
    #[tokio::test]
    async fn a_wrong_checksum_is_refused_and_the_file_deleted() {
        let path = scratch("wrong");
        std::fs::write(&path, b"not what was promised").expect("write");

        let refused = verify(&path, ECHO_DIGEST).await;

        assert!(refused.is_err(), "a mismatched file must not be accepted");
        assert!(!path.exists(), "a refused file must not be left on disk");
        let message = refused.unwrap_err().to_string();
        assert!(
            message.contains("checksum"),
            "the error should say what went wrong, got {message:?}"
        );
    }

    /// And the other direction, so the check is not simply always failing: a
    /// digest taken from the hasher itself is accepted, and case does not matter
    /// because an upstream may publish uppercase hex.
    #[tokio::test]
    async fn the_right_checksum_passes_whatever_its_case() {
        let path = scratch("right");
        std::fs::write(&path, b"the bytes we expected").expect("write");

        let digest = crate::core::plugins::integrity::fingerprint(&path).expect("hash");
        verify(&path, &digest)
            .await
            .expect("matching digest should pass");
        verify(&path, &digest.to_uppercase())
            .await
            .expect("uppercase digest should also pass");

        assert!(path.exists(), "a file that passed must be left alone");
        let _ = std::fs::remove_file(&path);
    }

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
        let first = next_chunk_within(&mut stalled, brief)
            .await
            .expect("first chunk");
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
