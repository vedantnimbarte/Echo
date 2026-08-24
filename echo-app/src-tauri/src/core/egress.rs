//! A record of every outbound request Echo itself makes.
//!
//! This is deliberately scoped and deliberately honest. It logs **what this
//! process asked the network for** — cloud transcription, model downloads,
//! update checks, cloud command mode. It is *not* proof that nothing else left
//! your machine: a process cannot observe its own OS's traffic, and anything
//! outside this binary (the OS, other apps, a plugin calling out on its own)
//! is invisible to it. The UI wording has to match that, or the feature is
//! worse than nothing.
//!
//! Recording is fire-and-forget through a channel so a request never blocks on
//! the database, and a no-op before [`init`] runs (unit tests, early startup).

use std::sync::OnceLock;

use tokio::sync::mpsc::UnboundedSender;

/// One logged request: the host contacted and why.
pub struct Egress {
    pub host: String,
    pub purpose: String,
}

static SINK: OnceLock<UnboundedSender<Egress>> = OnceLock::new();

/// Install the sink that drains to the database. Called once during setup;
/// later calls are ignored.
pub fn init(tx: UnboundedSender<Egress>) {
    let _ = SINK.set(tx);
}

/// Log an outbound request to `url`. Only the host is kept — never the path or
/// query, which can carry identifiers we have no business storing.
pub fn record(url: &str, purpose: &str) {
    let Some(tx) = SINK.get() else {
        return;
    };
    let host = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".into());

    let _ = tx.send(Egress {
        host,
        purpose: purpose.to_string(),
    });
}

/// Log an outbound request to an already-resolved host (WebSocket upgrades,
/// where we hold the host rather than a full URL).
pub fn record_host(host: &str, purpose: &str) {
    let Some(tx) = SINK.get() else {
        return;
    };
    let _ = tx.send(Egress {
        host: host.to_string(),
        purpose: purpose.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recording before `init` must be a silent no-op rather than a panic —
    /// providers call this from paths that run in unit tests with no app.
    #[test]
    fn record_without_a_sink_is_a_noop() {
        record("https://api.openai.com/v1/audio/transcriptions", "test");
        record_host("api.deepgram.com", "test");
    }

    /// Only the host is retained; the path must never reach the log.
    #[test]
    fn only_the_host_is_extracted() {
        let host = reqwest::Url::parse("https://api.openai.com/v1/audio/transcriptions?key=secret")
            .ok()
            .and_then(|u| u.host_str().map(str::to_string));
        assert_eq!(host.as_deref(), Some("api.openai.com"));
    }
}
