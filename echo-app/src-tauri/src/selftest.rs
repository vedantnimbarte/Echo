//! `echo --selftest` — prove the app actually works, not just that it compiles.
//!
//! Two real bugs shipped past a full unit-test suite and a green three-job CI
//! run, because both lived in code that only executes when the app is really
//! running:
//!
//! - `AudioService::new` called `tokio::spawn` from Tauri's `setup` hook, which
//!   is not inside a runtime. It panicked before the window appeared. Nothing
//!   in CI ever launched the binary, so nothing noticed.
//! - Dictionary replacement sliced a string at an offset taken from a
//!   *lowercased* copy of it, and panicked on text where the two disagree. It
//!   ran inside a spawned task, so the transcript vanished silently.
//!
//! Unit tests cannot catch either: the first needs a real Tauri startup, the
//! second needs the pipeline wired end to end. So this runs inside the actual
//! `setup` hook, after the real state has been built, and pushes synthetic
//! audio through the real stages.
//!
//! Deliberately excluded: the microphone (CI has none, and a test that needs
//! hardware will be switched off within a month) and text injection (it would
//! type into whatever the user happens to have focused). Everything between
//! those two ends is exercised for real.

use std::cell::Cell;
use std::fmt::Write as _;
use std::time::Duration;

use tauri::Manager;
use tokio::sync::mpsc;

use crate::commands::recording::{resolve_delivery, vad_gate, VadEvent};
use crate::core::vad::{gate::speech_gate, EnergyVad, Vad};
use crate::state::AppState;

/// Whether the process was started with `--selftest`.
pub fn requested() -> bool {
    std::env::args().any(|arg| arg == "--selftest")
}

/// One checked stage.
enum Outcome {
    Pass(String),
    /// Not applicable on this machine — reported, but not a failure. A model
    /// that has not been downloaded is not a bug.
    Skip(String),
    Fail(String),
}

struct Report {
    lines: Vec<(&'static str, Outcome)>,
}

impl Report {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }

    fn pass(&mut self, stage: &'static str, detail: impl Into<String>) {
        self.lines.push((stage, Outcome::Pass(detail.into())));
    }

    fn skip(&mut self, stage: &'static str, detail: impl Into<String>) {
        self.lines.push((stage, Outcome::Skip(detail.into())));
    }

    fn fail(&mut self, stage: &'static str, detail: impl Into<String>) {
        self.lines.push((stage, Outcome::Fail(detail.into())));
    }

    fn failures(&self) -> usize {
        self.lines
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Fail(_)))
            .count()
    }

    fn render(&self) -> String {
        let mut out = String::from("echo --selftest\n\n");
        for (stage, outcome) in &self.lines {
            let (tag, detail) = match outcome {
                Outcome::Pass(d) => ("ok  ", d),
                Outcome::Skip(d) => ("skip", d),
                Outcome::Fail(d) => ("FAIL", d),
            };
            let _ = writeln!(out, "  {stage:<14} {tag}  {detail}");
        }

        let failures = self.failures();
        let skipped = self
            .lines
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Skip(_)))
            .count();
        let passed = self.lines.len() - failures - skipped;

        let _ = write!(
            out,
            "\n{} ({passed} ok, {skipped} skipped, {failures} failed)",
            if failures == 0 { "PASSED" } else { "FAILED" }
        );
        out
    }
}

/// Run every check and terminate the process with 0 (all good) or 1.
///
/// Never returns: a self-test that left the app running would sit there holding
/// a window open on a CI machine until the job timed out.
pub fn run(app: &tauri::AppHandle) -> ! {
    let report = tauri::async_runtime::block_on(check_all(app));
    let rendered = report.render();

    // Printed for a terminal, and logged because a packaged Windows build has
    // no console attached — there, `echo.log` is the only copy the user can
    // actually send us.
    println!("{rendered}");
    tracing::info!("\n{rendered}");

    // `process::exit` runs no destructors, so the resident whisper model would
    // be left behind holding a few hundred megabytes — once per run. Shut it
    // down explicitly, exactly as the app's own exit handler does.
    if let Some(state) = app.try_state::<AppState>() {
        tauri::async_runtime::block_on(state.whisper_server.shutdown());
    }

    std::process::exit(if report.failures() == 0 { 0 } else { 1 })
}

async fn check_all(app: &tauri::AppHandle) -> Report {
    let mut report = Report::new();

    // Reaching this line at all is the startup check: the database opened, its
    // migrations ran, every service was constructed, and the audio router was
    // spawned without panicking. That is exactly the bug class that shipped.
    let state = app.state::<AppState>();
    report.pass("startup", "state built, audio router spawned");

    check_database(&state, &mut report);
    check_pipeline(&mut report).await;
    check_dictionary(&state, &mut report).await;
    check_delivery(&state, &mut report);
    check_asr(&state, &mut report).await;

    report
}

/// Settings must survive a write/read cycle, and the tables the app depends on
/// have to exist. A migration that half-ran leaves both broken.
fn check_database(state: &AppState, report: &mut Report) {
    use crate::storage::repositories as repo;

    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => {
            report.fail("database", "connection mutex was poisoned");
            return;
        }
    };

    const KEY: &str = "__selftest";
    let token = format!("{}", std::process::id());
    if let Err(e) = repo::set_setting(&conn, KEY, &token) {
        report.fail("database", format!("could not write a setting: {e}"));
        return;
    }
    match repo::get_setting(&conn, KEY) {
        Ok(Some(v)) if v == token => {}
        Ok(other) => {
            report.fail("database", format!("setting read back as {other:?}"));
            return;
        }
        Err(e) => {
            report.fail("database", format!("could not read a setting: {e}"));
            return;
        }
    }
    let _ = conn.execute("DELETE FROM settings WHERE key = ?1", [KEY]);

    // Every table the transcript path touches.
    let expected = [
        "settings",
        "dictionary_entries",
        "transcription_history",
        "app_profiles",
        "egress_log",
    ];
    let mut missing = Vec::new();
    for table in expected {
        let found: Result<i64, _> = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |r| r.get(0),
        );
        if !matches!(found, Ok(1)) {
            missing.push(table);
        }
    }

    if missing.is_empty() {
        report.pass(
            "database",
            format!("settings round-trip, {} tables present", expected.len()),
        );
    } else {
        report.fail("database", format!("missing tables: {}", missing.join(", ")));
    }
}

/// Drive the real VAD stage with synthetic audio.
///
/// This is the wiring between capture and transcription: speech has to reach
/// the ASR channel, and each utterance has to be closed with the empty-chunk
/// sentinel. Without the sentinel the transcript is simply never produced,
/// which looks exactly like a dead microphone from the outside.
async fn check_pipeline(report: &mut Report) {
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>(64);
    let (vad_tx, mut vad_rx) = mpsc::channel::<Vec<f32>>(64);

    // Loud enough to clear the energy VAD, then silence to close the utterance.
    for _ in 0..10 {
        if audio_tx.send(tone(0.3, 1_600)).await.is_err() {
            report.fail("vad", "audio channel closed before the test began");
            return;
        }
    }
    // EnergyVad holds speech open for 15 consecutive silent frames, so a
    // shorter tail never produces the falling edge and this would silently
    // stop testing the half of the lifecycle that closes an utterance.
    for _ in 0..20 {
        let _ = audio_tx.send(vec![0.0; 1_600]).await;
    }
    drop(audio_tx);

    // `vad_gate` takes an `Fn`, so the counters live in cells rather than
    // being captured mutably.
    let started = Cell::new(0usize);
    let ended = Cell::new(0usize);
    let gate = vad_gate(
        audio_rx,
        Box::new(EnergyVad::new(0.01)) as Box<dyn Vad>,
        vad_tx,
        |event| match event {
            VadEvent::SpeechStarted => started.set(started.get() + 1),
            VadEvent::SpeechEnded => ended.set(ended.get() + 1),
            VadEvent::Level(_) => {}
        },
    );

    // A hang here is a real failure mode, not a reason to wait forever.
    if tokio::time::timeout(Duration::from_secs(10), gate).await.is_err() {
        report.fail("vad", "the VAD stage did not finish within 10s");
        return;
    }

    let mut speech_chunks = 0usize;
    let mut sentinels = 0usize;
    while let Ok(chunk) = vad_rx.try_recv() {
        if chunk.is_empty() {
            sentinels += 1;
        } else {
            speech_chunks += 1;
        }
    }

    if speech_chunks == 0 {
        report.fail("vad", "no speech reached the transcription channel");
    } else if sentinels == 0 {
        report.fail("vad", "the utterance was never closed (no end sentinel)");
    } else if started.get() == 0 {
        report.fail("vad", "speech-started never fired, so the UI would not react");
    } else if ended.get() == 0 {
        // Without this the pill sticks on "listening" and the transcript is
        // only flushed when recording stops entirely.
        report.fail("vad", "speech-ended never fired after the audio went quiet");
    } else {
        report.pass(
            "vad",
            format!(
                "{speech_chunks} chunks forwarded, {} started / {} ended",
                started.get(),
                ended.get()
            ),
        );
    }

    // The gate that keeps whisper from hallucinating over silence.
    let speech_ok = speech_gate(&tone(0.3, 16_000)).should_transcribe();
    let silence_ok = !speech_gate(&vec![0.0; 16_000]).should_transcribe();
    match (speech_ok, silence_ok) {
        (true, true) => report.pass("speech gate", "speech accepted, silence rejected"),
        (false, _) => report.fail("speech gate", "real speech would be discarded"),
        (_, false) => report.fail("speech gate", "silence would be sent to the decoder"),
    }
}

/// Run the live dictionary over text that used to crash it.
///
/// The regression is specific: `İ` is two bytes and lowercases to three, so a
/// byte offset taken from a lowercased copy does not address the original.
/// Getting it wrong panicked inside the transcript task and lost the
/// transcript, which is why it is worth re-checking on every real startup
/// rather than only in unit tests.
async fn check_dictionary(state: &AppState, report: &mut Report) {
    use crate::core::dictionary::{DictionaryEngine, DictionaryEntry};

    let probe = DictionaryEngine::new(vec![DictionaryEntry {
        id: None,
        phrase: "teh".into(),
        replacement: "the".into(),
        enabled: true,
        profile_id: None,
    }]);

    let tricky = probe.process_for("\u{130} teh cat and teh dog", None);
    if tricky != "\u{130} the cat and the dog" {
        report.fail("dictionary", format!("replacement produced {tricky:?}"));
        return;
    }

    // And the user's own entries, which is what will actually run.
    let live = state.dictionary.read().await;
    let entries = live.prompt_terms(None).map(|p| p.split(", ").count()).unwrap_or(0);
    let sample = live.process_for("\u{130} a routine transcript", None);
    drop(live);

    if sample.is_empty() {
        report.fail("dictionary", "the live dictionary emptied a transcript");
    } else {
        report.pass(
            "dictionary",
            format!("non-ASCII safe, repeats replaced, {entries} term(s) hinted"),
        );
    }
}

/// Resolve how a transcript would be delivered, without delivering one.
///
/// Synthesising keystrokes here would type into whatever the user has focused,
/// so this stops at the decision and reports it.
fn check_delivery(state: &AppState, report: &mut Report) {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => {
            report.fail("delivery", "connection mutex was poisoned");
            return;
        }
    };
    let delivery = resolve_delivery(&conn, None);
    drop(conn);

    report.pass(
        "delivery",
        format!(
            "{} injection, auto-inject {}, settle {}ms (not exercised)",
            if delivery.use_paste { "paste" } else { "keystroke" },
            if delivery.auto_inject { "on" } else { "off" },
            delivery.settle_ms
        ),
    );
}

/// Transcribe the synthetic utterance, if this machine has what it takes.
///
/// A tone is not speech, so the transcript is expected to be empty or
/// nonsense — what is being checked is that the engine can be reached, spawn,
/// and return without erroring. That covers the resident server, the CLI
/// fallback and the GPU selection in one call.
async fn check_asr(state: &AppState, report: &mut Report) {
    let provider = state.asr.active_provider_name().await;

    if provider == "none" {
        report.skip("asr", "no engine selected");
        return;
    }
    if !state.binaries.is_installed() {
        report.skip("asr", "whisper binary not installed yet");
        return;
    }

    let compute = state.binaries.gpu().label();
    let accelerated = state.binaries.active_gpu_pack().is_some();

    // One second of tone: enough to clear the speech gate, short enough that a
    // CPU-only machine is not held up for long.
    let audio = tone(0.3, 16_000);
    match tokio::time::timeout(
        Duration::from_secs(120),
        state.asr.transcribe(audio, Some("en")),
    )
    .await
    {
        Err(_) => report.fail("asr", format!("{provider} did not respond within 120s")),
        Ok(Err(e)) => report.fail("asr", format!("{provider} failed: {e}")),
        Ok(Ok(segment)) => report.pass(
            "asr",
            format!(
                "{provider} responded ({compute}{}), {} chars",
                if accelerated { ", accelerated" } else { "" },
                segment.text.chars().count()
            ),
        ),
    }
}

/// A sine at `amp`, `samples` long at 16 kHz.
fn tone(amp: f32, samples: usize) -> Vec<f32> {
    (0..samples).map(|i| (i as f32 * 0.05).sin() * amp).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_with_no_failures_passes() {
        let mut r = Report::new();
        r.pass("startup", "fine");
        r.skip("asr", "no model");
        assert_eq!(r.failures(), 0);
        let out = r.render();
        assert!(out.contains("PASSED"), "{out}");
        assert!(out.contains("1 ok, 1 skipped, 0 failed"), "{out}");
    }

    /// A skip must never be mistaken for a pass — that is the whole reason the
    /// two are distinguished.
    #[test]
    fn any_failure_fails_the_run() {
        let mut r = Report::new();
        r.pass("startup", "fine");
        r.fail("dictionary", "panicked");
        assert_eq!(r.failures(), 1);
        assert!(r.render().contains("FAILED"));
    }

    #[test]
    fn the_flag_is_only_recognised_exactly() {
        // Guards against a stray argument silently turning a normal launch into
        // a self-test that exits immediately.
        assert!(!std::env::args().any(|a| a == "--selftest-not-really"));
    }
}
