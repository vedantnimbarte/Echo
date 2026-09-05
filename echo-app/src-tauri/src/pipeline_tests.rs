//! End-to-end tests for the dictation pipeline.
//!
//! These drive the real stages — VAD gating, the ASR streaming contract, the
//! dictionary, delivery resolution, and text injection — wired together the
//! same way [`crate::commands::recording::begin_recording`] wires them, but
//! without a Tauri app, a microphone, or a Whisper model.
//!
//! **What is and isn't covered.** The audio *device* layer (CPAL) and the real
//! ASR providers are substituted: PCM is synthesised and the provider is a
//! fake. So these prove the pipeline's plumbing and contracts — that an
//! utterance boundary produces exactly one transcript, that dictionary scoping
//! and per-app overrides reach injection — not that Whisper transcribes well or
//! that a real microphone opens. The energy VAD is used rather than Silero:
//! synthetic tones aren't speech, so a neural VAD's verdict on them would be
//! meaningless. Silero has its own inference test in `core::vad::silero`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::commands::recording::{resolve_delivery, vad_gate, VadEvent};
use crate::core::{
    asr::{manager::AsrManager, AsrProvider, TranscriptSegment},
    dictionary::{DictionaryEngine, DictionaryEntry},
    injection::{deliver, TextInjector},
    vad::{EnergyVad, Vad},
};
use crate::error::Result;
use crate::storage::{db, models::AppProfile, repositories as repo};

// ── Test doubles ─────────────────────────────────────────────────────────────

/// An ASR provider that returns canned text, one phrase per utterance, and
/// records the audio it was handed.
struct FakeAsr {
    phrases: Mutex<std::collections::VecDeque<String>>,
    /// Sample counts of each buffer handed to `transcribe`, in order.
    received: Arc<Mutex<Vec<usize>>>,
    /// Language the pipeline passed down, from the last call.
    language: Arc<Mutex<Option<String>>>,
}

impl FakeAsr {
    fn new(phrases: &[&str]) -> Self {
        Self {
            phrases: Mutex::new(phrases.iter().map(|s| s.to_string()).collect()),
            received: Arc::new(Mutex::new(Vec::new())),
            language: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl AsrProvider for FakeAsr {
    fn name(&self) -> &str {
        "fake"
    }

    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        self.received.lock().unwrap().push(audio.len());
        *self.language.lock().unwrap() = language.map(str::to_string);

        let text = self
            .phrases
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| "(no more phrases)".into());

        Ok(TranscriptSegment {
            text,
            is_final: true,
            language: language.map(str::to_string),
            confidence: Some(1.0),
        })
    }
}

/// Captures whatever the pipeline decides to type.
#[derive(Default)]
struct SpyInjector {
    typed: Mutex<Vec<String>>,
}

impl TextInjector for SpyInjector {
    fn inject_text(&self, text: &str) -> Result<()> {
        self.typed.lock().unwrap().push(text.to_string());
        Ok(())
    }
    fn send_paste(&self) -> Result<()> {
        Ok(())
    }
    fn send_copy(&self) -> Result<()> {
        Ok(())
    }
}

// ── Audio fixtures ───────────────────────────────────────────────────────────

/// One 20ms chunk at 16kHz. `amplitude` of 0 is silence; the energy VAD's
/// threshold is 0.01, so 0.3 is unambiguously "speech".
fn chunk(amplitude: f32) -> Vec<f32> {
    (0..320)
        .map(|i| amplitude * if i % 2 == 0 { 1.0 } else { -1.0 })
        .collect()
}

fn silence() -> Vec<f32> {
    chunk(0.0)
}
fn loud() -> Vec<f32> {
    chunk(0.3)
}

/// Feed chunks through the real VAD gate and collect everything downstream.
/// Returns (chunks reaching ASR, events emitted to the UI).
async fn run_vad_gate(input: Vec<Vec<f32>>) -> (Vec<Vec<f32>>, Vec<&'static str>) {
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>(64);
    let (vad_tx, mut vad_rx) = mpsc::channel::<Vec<f32>>(64);

    for c in input {
        audio_tx.send(c).await.unwrap();
    }
    drop(audio_tx); // end of capture

    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();

    // EnergyVad debounces 15 frames of silence before declaring the end of
    // speech, so the fixtures below pad generously past that.
    vad_gate(
        audio_rx,
        Box::new(EnergyVad::new(0.01)) as Box<dyn Vad>,
        vad_tx,
        move |e| {
            let label = match e {
                VadEvent::Level(_) => "level",
                VadEvent::SpeechStarted => "started",
                VadEvent::SpeechEnded => "ended",
            };
            // Levels fire per chunk and would drown the assertions.
            if label != "level" {
                sink.lock().unwrap().push(label);
            }
        },
    )
    .await;

    let mut out = Vec::new();
    while let Ok(c) = vad_rx.try_recv() {
        out.push(c);
    }
    let events = events.lock().unwrap().clone();
    (out, events)
}

/// An in-memory database with migrations applied.
fn test_db() -> rusqlite::Connection {
    db::open(Path::new(":memory:")).expect("in-memory db")
}

// ── VAD stage ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn vad_gate_drops_silence_and_marks_the_utterance_boundary() {
    let mut input = vec![silence(), silence()];
    input.extend(std::iter::repeat_with(loud).take(5));
    // Past the 15-frame debounce so the falling edge actually fires.
    input.extend(std::iter::repeat_with(silence).take(25));

    let (downstream, events) = run_vad_gate(input).await;

    assert_eq!(
        events,
        vec!["started", "ended"],
        "one utterance should produce exactly one rising and one falling edge"
    );

    // The sentinel is how the ASR stage learns an utterance ended.
    let first_sentinel = downstream
        .iter()
        .position(|c| c.is_empty())
        .expect("a speech→silence transition must emit the empty-vec sentinel");

    // Everything before it is speech: leading silence never reaches ASR.
    assert!(
        first_sentinel > 0,
        "speech chunks should reach the ASR stage before the sentinel"
    );
    assert!(
        downstream[..first_sentinel].iter().all(|c| !c.is_empty()),
        "silence must be dropped, not forwarded"
    );

    // Closing capture flushes a second sentinel after the falling edge already
    // sent one. That redundancy is deliberate and safe: `transcribe_stream`
    // guards on an empty buffer, so the extra sentinel transcribes nothing —
    // see `each_utterance_produces_exactly_one_transcript`, which pins that.
    assert!(
        downstream[first_sentinel..].iter().all(|c| c.is_empty()),
        "no speech may follow the utterance boundary in this fixture"
    );
}

#[tokio::test]
async fn vad_gate_separates_two_utterances() {
    let mut input = vec![silence()];
    input.extend(std::iter::repeat_with(loud).take(4));
    input.extend(std::iter::repeat_with(silence).take(25));
    input.extend(std::iter::repeat_with(loud).take(4));
    input.extend(std::iter::repeat_with(silence).take(25));

    let (_, events) = run_vad_gate(input).await;

    assert_eq!(
        events,
        vec!["started", "ended", "started", "ended"],
        "two separated utterances should produce two edge pairs"
    );
}

#[tokio::test]
async fn vad_gate_flushes_a_trailing_utterance_when_capture_stops() {
    // Speech that never falls silent: stopping the recording mid-sentence must
    // still flush what was captured, or the last utterance is lost.
    let mut input = vec![silence()];
    input.extend(std::iter::repeat_with(loud).take(5));

    let (downstream, events) = run_vad_gate(input).await;

    assert_eq!(events, vec!["started"], "no falling edge is expected here");
    assert!(
        downstream.last().is_some_and(|c| c.is_empty()),
        "closing capture must flush a sentinel so the buffered audio is transcribed"
    );
}

// ── VAD → ASR ────────────────────────────────────────────────────────────────

/// The sentinel contract, end to end: each utterance boundary must produce
/// exactly one transcript, not one per chunk and not one for the whole session.
#[tokio::test]
async fn each_utterance_produces_exactly_one_transcript() {
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>(64);
    let (vad_tx, vad_rx) = mpsc::channel::<Vec<f32>>(64);
    let (text_tx, mut text_rx) = mpsc::channel::<TranscriptSegment>(16);

    let mut input = vec![silence()];
    input.extend(std::iter::repeat_with(loud).take(4));
    input.extend(std::iter::repeat_with(silence).take(25));
    input.extend(std::iter::repeat_with(loud).take(4));
    input.extend(std::iter::repeat_with(silence).take(25));
    for c in input {
        audio_tx.send(c).await.unwrap();
    }
    drop(audio_tx);

    let asr = Arc::new(FakeAsr::new(&["hello world", "second utterance"]));
    let received = asr.received.clone();
    let language = asr.language.clone();

    let manager = AsrManager::new("fake".into());
    manager.register(asr).await;

    tokio::spawn(async move {
        vad_gate(
            audio_rx,
            Box::new(EnergyVad::new(0.01)) as Box<dyn Vad>,
            vad_tx,
            |_| {},
        )
        .await;
    });

    manager
        .transcribe_stream(vad_rx, text_tx, Some("en"))
        .await
        .expect("stream should complete");

    let mut texts = Vec::new();
    while let Some(seg) = text_rx.recv().await {
        assert!(seg.is_final);
        texts.push(seg.text);
    }

    assert_eq!(texts, vec!["hello world", "second utterance"]);
    assert_eq!(
        received.lock().unwrap().len(),
        2,
        "the provider should be called once per utterance, not per chunk"
    );
    assert_eq!(
        language.lock().unwrap().as_deref(),
        Some("en"),
        "the configured language must reach the provider"
    );
}

// ── Delivery resolution ──────────────────────────────────────────────────────

#[test]
fn delivery_falls_back_to_global_settings() {
    let conn = test_db();
    // Nothing configured at all: the documented defaults.
    let d = resolve_delivery(&conn, None);
    assert!(d.auto_inject, "auto-insert defaults on");
    assert!(!d.use_paste, "typing is the default insert method");
    assert!(d.record_history, "history defaults on");
    assert_eq!(d.dictionary_profile, None);

    repo::set_setting(&conn, "auto_inject", "false").unwrap();
    repo::set_setting(&conn, "injection_method", "paste").unwrap();
    repo::set_setting(&conn, "history_enabled", "false").unwrap();
    let d = resolve_delivery(&conn, None);
    assert!(!d.auto_inject);
    assert!(d.use_paste);
    assert!(!d.record_history);
}

#[test]
fn an_app_profile_overrides_only_the_fields_it_sets() {
    let conn = test_db();
    repo::set_setting(&conn, "auto_inject", "true").unwrap();
    repo::set_setting(&conn, "injection_method", "type").unwrap();

    // A password-manager-shaped profile: never insert, inherit everything else.
    repo::upsert_app_profile(
        &conn,
        &AppProfile {
            id: None,
            app_match: "1password.exe".into(),
            label: None,
            auto_inject: Some(false),
            injection_method: None,
            profile_id: None,
            enabled: true,
        },
    )
    .unwrap();

    let d = resolve_delivery(&conn, Some("1password.exe"));
    assert!(!d.auto_inject, "the profile's override applies");
    assert!(
        !d.use_paste,
        "a NULL override must inherit the global setting, not reset it"
    );

    // A different app is unaffected.
    let d = resolve_delivery(&conn, Some("code.exe"));
    assert!(d.auto_inject);
}

#[test]
fn app_matching_is_case_insensitive_and_disabled_profiles_are_ignored() {
    let conn = test_db();
    repo::upsert_app_profile(
        &conn,
        &AppProfile {
            id: None,
            app_match: "Code.exe".into(),
            label: None,
            auto_inject: Some(false),
            injection_method: None,
            profile_id: None,
            enabled: true,
        },
    )
    .unwrap();

    // Stored lowercased; the platform layer also reports lowercase.
    assert!(!resolve_delivery(&conn, Some("code.exe")).auto_inject);

    repo::upsert_app_profile(
        &conn,
        &AppProfile {
            id: None,
            app_match: "code.exe".into(),
            label: None,
            auto_inject: Some(false),
            injection_method: None,
            profile_id: None,
            enabled: false,
        },
    )
    .unwrap();

    assert!(
        resolve_delivery(&conn, Some("code.exe")).auto_inject,
        "a disabled profile must not apply"
    );
}

// ── Full chain ───────────────────────────────────────────────────────────────

/// PCM in, typed text out: the whole pipeline with only the device and the
/// speech model substituted.
#[tokio::test]
async fn pipeline_delivers_dictionary_corrected_text_to_the_focused_app() {
    let conn = test_db();
    let injector = SpyInjector::default();

    // "echo app" is what the model hears; the dictionary fixes the casing.
    let dictionary = DictionaryEngine::new(vec![DictionaryEntry {
        id: Some(1),
        phrase: "echo app".into(),
        replacement: "Echo".into(),
        enabled: true,
        profile_id: None,
    }]);

    let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>(64);
    let (vad_tx, vad_rx) = mpsc::channel::<Vec<f32>>(64);
    let (text_tx, mut text_rx) = mpsc::channel::<TranscriptSegment>(16);

    let mut input = vec![silence()];
    input.extend(std::iter::repeat_with(loud).take(5));
    input.extend(std::iter::repeat_with(silence).take(25));
    for c in input {
        audio_tx.send(c).await.unwrap();
    }
    drop(audio_tx);

    let manager = AsrManager::new("fake".into());
    manager.register(Arc::new(FakeAsr::new(&["echo app is listening"]))).await;

    tokio::spawn(async move {
        vad_gate(
            audio_rx,
            Box::new(EnergyVad::new(0.01)) as Box<dyn Vad>,
            vad_tx,
            |_| {},
        )
        .await;
    });
    manager.transcribe_stream(vad_rx, text_tx, None).await.unwrap();

    // The delivery half of `begin_recording`'s transcript loop.
    while let Some(segment) = text_rx.recv().await {
        let delivery = resolve_delivery(&conn, None);
        let processed = dictionary.process_for(&segment.text, delivery.dictionary_profile);

        if delivery.record_history {
            repo::insert_history(
                &conn,
                &crate::storage::models::TranscriptionRecord {
                    id: None,
                    text: processed.clone(),
                    language: segment.language.clone(),
                    provider: "fake".into(),
                    created_at: String::new(),
                },
            )
            .unwrap();
        }
        if delivery.auto_inject {
            deliver(&injector, &processed, delivery.use_paste, delivery.settle_ms).unwrap();
        }
    }

    // Trailing space is `deliver`'s smart spacing, so back-to-back dictations
    // don't run together ("listeningEcho is"). See `core::injection`.
    assert_eq!(
        injector.typed.lock().unwrap().as_slice(),
        &["Echo is listening ".to_string()],
        "the focused app should receive the dictionary-corrected transcript"
    );

    // History is written — the regression that made it silently empty.
    let history = repo::list_history(&conn, 10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].text, "Echo is listening");
}

/// The same chain, but the focused app selects a dictionary profile — so a
/// scoped entry applies that would otherwise be skipped.
#[tokio::test]
async fn a_per_app_profile_switches_which_dictionary_entries_apply() {
    let conn = test_db();
    let injector = SpyInjector::default();

    let profile_id = repo::insert_profile(&conn, "work").unwrap();
    repo::upsert_app_profile(
        &conn,
        &AppProfile {
            id: None,
            app_match: "slack.exe".into(),
            label: None,
            auto_inject: None,
            injection_method: None,
            profile_id: Some(profile_id),
            enabled: true,
        },
    )
    .unwrap();

    let dictionary = DictionaryEngine::new(vec![DictionaryEntry {
        id: Some(1),
        phrase: "ship it".into(),
        replacement: ":shipit:".into(),
        enabled: true,
        profile_id: Some(profile_id),
    }]);

    // Expectations carry `deliver`'s trailing smart space.
    for (app, expected) in [
        ("slack.exe", ":shipit: today "),
        ("notepad.exe", "ship it today "),
    ] {
        let delivery = resolve_delivery(&conn, Some(app));
        let processed = dictionary.process_for("ship it today", delivery.dictionary_profile);
        deliver(&injector, &processed, false, 1).unwrap();
        assert_eq!(
            injector.typed.lock().unwrap().last().unwrap(),
            expected,
            "entry scoped to '{app}' should only apply under its own profile"
        );
    }
}

/// Retention deletes old transcripts and *only* old ones.
///
/// This is the one path in the app that destroys user data on its own, without
/// anybody pressing anything, so the boundary matters more than the happy path:
/// an off-by-one here silently eats transcripts the user meant to keep.
#[test]
fn retention_removes_only_transcripts_past_the_window() {
    let conn = test_db();

    let insert = |text: &str, age_days: i64| {
        conn.execute(
            "INSERT INTO transcription_history (text, language, provider, created_at)
             VALUES (?1, NULL, 'test', datetime('now', ?2))",
            rusqlite::params![text, format!("-{age_days} days")],
        )
        .unwrap();
    };

    insert("today", 0);
    insert("last week", 7);
    insert("last year", 365);

    let removed = crate::storage::repositories::trim_history_older_than(&conn, 30).unwrap();
    assert_eq!(removed, 1, "only the year-old transcript is past a 30-day window");

    let kept: Vec<String> = crate::storage::repositories::list_history(&conn, 100)
        .unwrap()
        .into_iter()
        .map(|r| r.text)
        .collect();
    assert!(kept.contains(&"today".to_string()));
    assert!(kept.contains(&"last week".to_string()));
    assert!(!kept.contains(&"last year".to_string()));
}

/// "Keep everything" must never be mistaken for "delete everything".
#[test]
fn retention_of_zero_or_less_keeps_all_history() {
    let conn = test_db();
    conn.execute(
        "INSERT INTO transcription_history (text, language, provider, created_at)
         VALUES ('ancient', NULL, 'test', datetime('now', '-4000 days'))",
        [],
    )
    .unwrap();

    for days in [0, -1] {
        assert_eq!(
            crate::storage::repositories::trim_history_older_than(&conn, days).unwrap(),
            0,
            "days={days} must be treated as unlimited retention"
        );
    }
    assert_eq!(
        crate::storage::repositories::list_history(&conn, 100).unwrap().len(),
        1
    );
}
