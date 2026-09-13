//! The one HTTP client every cloud ASR provider uses.
//!
//! This exists because of a specific failure. A `reqwest::Client` built with
//! `Client::new()` has **no timeout at all**: if a provider accepts the
//! connection and then stops responding, the future never resolves. That is not
//! a slow transcription — it is a transcription that never returns, so
//! [`super::fallback::FallbackProvider`] never sees an `Err` and never hands the
//! audio to the offline engine. From the user's side the hotkey simply stops
//! working, with nothing on screen to explain it.
//!
//! A timeout is what turns that hang into an error the fallback can act on, so
//! every provider must share this client rather than building its own.

use std::sync::OnceLock;
use std::time::Duration;

/// How long a single transcription request may take before it is abandoned.
///
/// Generous on purpose: it is a ceiling on a hang, not a latency target. A
/// cloud decode of a long utterance can legitimately take a while, and cutting
/// off a request that would have succeeded is a worse failure than waiting.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait for the TCP/TLS handshake alone.
///
/// Separate from the overall timeout because it fails for a different reason:
/// no connection in five seconds means unreachable — wrong endpoint, no
/// network, dead proxy — and there is no point spending the other twenty-five
/// discovering that.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// The shared client. Cheap to clone; holds the connection pool, so reusing it
/// across utterances also skips a fresh TLS handshake every time you speak.
pub fn client() -> reqwest::Client {
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .connect_timeout(CONNECT_TIMEOUT)
                .build()
                // A client with timeouts is worth having even if the builder
                // somehow fails; falling back to the default one keeps
                // transcription working rather than taking the app down.
                .unwrap_or_else(|_| reqwest::Client::new())
        })
        .clone()
}

/// Whether a failed response is worth sending again.
///
/// 429 and 5xx are the provider saying "not now" rather than "not ever" — a
/// rate limit or a bad minute upstream. A 4xx is a bad key or a malformed
/// request, and repeating it just wastes the user's time before the fallback
/// runs.
pub fn is_retryable(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// How long to wait before the single retry.
///
/// One fixed pause, not a backoff schedule: this is in the path of someone
/// waiting for their words to appear, so the choice is between one quick
/// second chance and giving up to the offline engine. A ladder of escalating
/// sleeps would just make a failing provider feel broken for longer.
pub const RETRY_DELAY: Duration = Duration::from_millis(500);

/// How long to keep asking a job-based provider whether it is done.
///
/// AssemblyAI and Speechmatics queue work rather than answering inline, so
/// "finished" only arrives by polling. The deadline exists because a queue can
/// stall: without it a stuck job holds the dictation open until the app closes,
/// where an error at least lets the offline engine take over.
pub const POLL_DEADLINE: Duration = Duration::from_secs(120);

/// Gap between polls. Short enough that a fast job is not held back by the
/// clock, long enough not to spend a request per frame.
pub const POLL_INTERVAL: Duration = Duration::from_millis(400);

/// The ceiling on one request, or one polled job, when importing a recording.
///
/// [`REQUEST_TIMEOUT`] and [`POLL_DEADLINE`] are sized for an utterance, and a
/// meeting recording is not one: uploading an hour of audio alone can outlast
/// thirty seconds, and a queued job for it legitimately takes minutes. Nobody
/// is waiting mid-sentence on an import, so the only job of this number is to
/// turn a request that will never answer into an error eventually.
pub const IMPORT_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Poll `check` until it yields a value, or give up after `limit`.
///
/// `check` returns `Ok(None)` for "still working" and `Ok(Some(_))` once there
/// is an answer; an `Err` aborts immediately, because a job that failed will
/// not un-fail by being asked again.
pub async fn poll_until<T, F, Fut>(
    what: &str,
    limit: Duration,
    mut check: F,
) -> crate::error::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = crate::error::Result<Option<T>>>,
{
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        if let Some(value) = check().await? {
            return Ok(value);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(crate::error::EchoError::AsrProvider(format!(
                "{what} did not finish within {}s",
                limit.as_secs()
            )));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn retries_only_what_could_succeed_next_time() {
        assert!(is_retryable(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable(StatusCode::SERVICE_UNAVAILABLE));
        assert!(is_retryable(StatusCode::BAD_GATEWAY));

        // A bad key stays a bad key. Retrying delays the fallback for nothing.
        assert!(!is_retryable(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable(StatusCode::FORBIDDEN));
        assert!(!is_retryable(StatusCode::BAD_REQUEST));
    }

    #[tokio::test]
    async fn polling_returns_the_first_real_answer() {
        let mut calls = 0;
        let got: i32 = poll_until("job", POLL_DEADLINE, || {
            calls += 1;
            let n = calls;
            async move { Ok(if n >= 3 { Some(n) } else { None }) }
        })
        .await
        .unwrap();
        assert_eq!(got, 3);
    }

    #[tokio::test]
    async fn a_failed_job_stops_immediately_instead_of_being_re_asked() {
        let mut calls = 0;
        let err = poll_until::<i32, _, _>("job", POLL_DEADLINE, || {
            calls += 1;
            async move { Err(crate::error::EchoError::AsrProvider("bad audio".into())) }
        })
        .await;
        assert!(err.is_err());
        assert_eq!(calls, 1, "a failed job must not be polled again");
    }

    #[test]
    fn the_client_is_shared_not_rebuilt() {
        // Cloning the same pool is the point: a fresh client per utterance
        // would pay a new TLS handshake every time you speak.
        let a = client();
        let b = client();
        assert!(std::ptr::eq(CLIENT.get().unwrap(), CLIENT.get().unwrap()));
        drop((a, b));
    }
}
