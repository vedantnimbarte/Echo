//! Where plugin capabilities meet the dictation pipeline.
//!
//! The host used to load plugins, run `on_load`, and ask nothing else of them.
//! This module is the other half: each capability is called at one
//! point in the pipeline, chosen for what the hook can safely be allowed to
//! touch there. In the order an utterance meets them:
//!
//! 1. **Audio** — [`audio_stage`], between capture and voice detection. Every
//!    chunk, while the microphone is open. The only hook on the hot path, so
//!    it is skipped entirely when no loaded plugin offers it, its cost is
//!    logged per recording, and a plugin that fails there is benched.
//! 2. **ASR** — [`PluginAsrProvider`], registered with the ASR manager as
//!    `plugin:<name>` and used only if the user selects it. Wrapped in the same
//!    offline fallback as a cloud engine, so a failure costs a retry, not words.
//! 3. **Dictionary** — [`dictionary_entries`], merged into the engine after the
//!    user's own entries whenever the engine is rebuilt. Nothing runs per
//!    transcript; the merged engine does the work it always did.
//! 4. **Output** — [`output_worker`], after the text has been delivered, on a
//!    thread of its own, in order. It observes and cannot alter or block
//!    delivery; see `echo_sdk::OutputPlugin` for why.
//!
//! Every dispatcher takes a snapshot of the loaded plugins rather than the
//! loader's lock, so plugin code never runs while that lock is held, and
//! loaded is enabled — see [`PluginLoader`](super::loader::PluginLoader).
//!
//! Panics are contained inside the plugin library, not here. A plugin carries
//! its own copy of std, and a panic from one copy cannot be caught by another,
//! so `export_plugin!` wraps every plugin in `echo_sdk::Guarded`, which turns a
//! panic into an ordinary `PluginError` before it reaches this side. What this
//! module owns is what happens next: log it, discard that plugin's
//! contribution, and carry on with the user's text untouched.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::loader::LoadedPlugin;
use super::Transcript;
use crate::core::asr::{AsrProvider, TranscriptSegment};
use crate::error::{EchoError, Result};

/// Prefix every plugin-provided engine is registered under, so a plugin named
/// `local` or `openai` cannot shadow a built-in one — and so the host can find
/// its own registrations again when a plugin is disabled.
pub const ASR_PREFIX: &str = "plugin:";

/// Hand a delivered transcript to every output plugin, in load order.
///
/// A failure is logged and the next plugin still gets the transcript: one
/// broken plugin must not silence the others, and nothing here can reach the
/// text the user already has.
pub fn deliver_transcript(plugins: &[Arc<LoadedPlugin>], transcript: &Transcript) {
    for loaded in plugins {
        let plugin = loaded.plugin();
        if let Some(output) = plugin.as_output() {
            if let Err(e) = output.on_transcript(transcript) {
                tracing::error!(plugin = plugin.name(), "Output plugin failed: {e}");
            }
        }
    }
}

/// Start the thread output plugins are called from, and return its inbox.
///
/// One thread per recording session, and only when some plugin offers output.
/// A thread of its own because an output plugin may be slow — a webhook, a
/// file on a network drive — and the transcript loop that types the user's
/// next sentence must not wait on it. One thread rather than a task per
/// transcript because order matters: a plugin appending to a notes file should
/// not get the second sentence first because the first one's write stalled.
///
/// `snapshot` is read per transcript, so a plugin disabled mid-session stops
/// receiving from the next one. The thread ends when the sender is dropped.
pub fn output_worker(
    snapshot: impl Fn() -> Vec<Arc<LoadedPlugin>> + Send + 'static,
) -> std::sync::mpsc::Sender<Transcript> {
    let (tx, rx) = std::sync::mpsc::channel::<Transcript>();
    let spawned = std::thread::Builder::new()
        .name("echo-output-plugins".into())
        .spawn(move || {
            for transcript in rx {
                deliver_transcript(&snapshot(), &transcript);
            }
        });
    if let Err(e) = spawned {
        // The sender below then goes nowhere, which is the right failure: the
        // user's text is delivered regardless, and only the plugins miss out.
        tracing::error!("Couldn't start the output plugin thread: {e}");
    }
    tx
}

/// Run one capture chunk through every audio plugin, in load order.
///
/// Each plugin works on a copy. A failure has to leave the chunk exactly as it
/// was, and there is no telling how far through the buffer a plugin got before
/// it returned an error; copying a few hundred samples is nothing beside the
/// voice detector that runs on the same chunk next. A plugin that fails — or
/// empties the chunk, which the VAD stage would take as the microphone
/// stopping — is benched for the rest of its load, so a broken plugin costs one
/// log line instead of thirty a second.
pub fn process_audio(plugins: &[Arc<LoadedPlugin>], samples: &mut Vec<f32>) {
    for loaded in plugins {
        if loaded.audio_benched.load(Ordering::Relaxed) {
            continue;
        }
        let plugin = loaded.plugin();
        let Some(audio) = plugin.as_audio() else {
            continue;
        };
        let mut work = samples.clone();
        let failure = match audio.process(&mut work) {
            Ok(()) if !work.is_empty() => {
                *samples = work;
                continue;
            }
            Ok(()) => "returned an empty chunk".to_string(),
            Err(e) => e.to_string(),
        };
        loaded.audio_benched.store(true, Ordering::Relaxed);
        tracing::error!(
            plugin = plugin.name(),
            "Audio plugin failed ({failure}); bypassing it until it is enabled again"
        );
    }
}

/// The audio-plugin stage: forwards capture chunks to the VAD after the
/// plugins have had them.
///
/// Inserted by `begin_recording` only when a loaded plugin offers audio, so
/// nobody without one pays even the forwarding hop. The empty-chunk sentinel
/// the capture layer uses for "stopped" is passed through untouched.
pub async fn audio_stage(
    mut rx: mpsc::Receiver<Vec<f32>>,
    tx: mpsc::Sender<Vec<f32>>,
    snapshot: impl Fn() -> Vec<Arc<LoadedPlugin>>,
) {
    let mut spent = Duration::ZERO;
    let mut chunks: u32 = 0;
    let mut samples: usize = 0;

    while let Some(mut chunk) = rx.recv().await {
        if !chunk.is_empty() {
            // ponytail: plugin code runs inline on the runtime, as the VAD on
            // the next stage does. Fine at a few milliseconds a chunk; a
            // plugin doing real DSP would want a dedicated thread here.
            let started = Instant::now();
            samples += chunk.len();
            process_audio(&snapshot(), &mut chunk);
            spent += started.elapsed();
            chunks += 1;
        }
        if tx.send(chunk).await.is_err() {
            break;
        }
    }

    // The visible cost: time spent against the audio it was spent on. Over
    // 100% would mean the plugins are slower than real time and the capture
    // buffer is filling faster than it drains.
    if chunks > 0 {
        let audio = Duration::from_secs_f64(samples as f64 / 16_000.0);
        tracing::info!(
            chunks,
            plugin_ms = spent.as_millis() as u64,
            per_chunk_us = (spent / chunks).as_micros() as u64,
            share_of_audio = format!("{:.1}%", 100.0 * spent.as_secs_f64() / audio.as_secs_f64()),
            "Audio plugins this recording"
        );
    }
}

/// Entries every dictionary plugin contributes, in the engine's own type.
///
/// Plugin entries are global and always enabled: a plugin cannot know the
/// user's per-app profile ids, and one it could not see the user switch off
/// would be a rule nobody can explain. A blank phrase is dropped here rather
/// than trusted to the engine, since this is where the data crosses in.
pub fn dictionary_entries(
    plugins: &[Arc<LoadedPlugin>],
) -> Vec<crate::core::dictionary::DictionaryEntry> {
    let mut merged = Vec::new();
    for loaded in plugins {
        let plugin = loaded.plugin();
        let Some(dictionary) = plugin.as_dictionary() else {
            continue;
        };
        match dictionary.entries() {
            Ok(entries) => merged.extend(
                entries
                    .into_iter()
                    .filter(|e| !e.phrase.trim().is_empty())
                    .map(|e| crate::core::dictionary::DictionaryEntry {
                        id: None,
                        phrase: e.phrase,
                        replacement: e.replacement,
                        enabled: true,
                        profile_id: None,
                    }),
            ),
            Err(e) => {
                tracing::error!(plugin = plugin.name(), "Dictionary plugin failed: {e}")
            }
        }
    }
    merged
}

/// A plugin's transcription function, dressed as an [`AsrProvider`].
///
/// The SDK trait is a plain blocking function on purpose: an async trait
/// object cannot cross the library boundary without both sides agreeing on a
/// runtime, and a plugin author should not have to know which one Echo uses.
/// The host puts the call on a blocking thread and the manager's buffered
/// default does the rest — each utterance arrives here whole, which is also
/// why a plugin engine never streams partials.
pub struct PluginAsrProvider {
    name: String,
    loaded: Arc<LoadedPlugin>,
}

impl PluginAsrProvider {
    /// `None` when the plugin offers no engine.
    pub fn new(loaded: Arc<LoadedPlugin>) -> Option<Self> {
        loaded.plugin().as_asr()?;
        Some(Self {
            name: format!("{ASR_PREFIX}{}", loaded.plugin().name()),
            loaded,
        })
    }
}

#[async_trait]
impl AsrProvider for PluginAsrProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<&str>,
    ) -> Result<TranscriptSegment> {
        // The clone keeps the library mapped for as long as the call runs,
        // even if the plugin is disabled halfway through an utterance.
        let loaded = self.loaded.clone();
        let language = language.map(str::to_owned);
        let heard = tokio::task::spawn_blocking(move || {
            let asr = loaded
                .plugin()
                .as_asr()
                .ok_or_else(|| EchoError::Plugin("plugin no longer offers ASR".into()))?;
            asr.transcribe(&audio, language.as_deref())
                .map_err(|e| EchoError::Plugin(e.to_string()))
        })
        .await
        .map_err(|e| EchoError::Plugin(format!("plugin ASR task failed: {e}")))??;

        Ok(TranscriptSegment {
            text: heard.text,
            is_final: true,
            language: heard.language,
            confidence: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use echo_sdk::{
        AsrPlugin, AudioPlugin, DictionaryEntry, DictionaryPlugin, Guarded, OutputPlugin, Plugin,
        PluginContext, PluginError, PluginResult, Transcription,
    };

    use super::*;
    use crate::core::dictionary::DictionaryEngine;
    use crate::core::plugins::loader::PluginLoader;

    #[derive(Clone, Copy, PartialEq)]
    enum Mood {
        Fine,
        Fails,
        Panics,
    }

    /// Offers every capability and writes down each call it gets.
    struct Probe {
        name: &'static str,
        mood: Mood,
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl Probe {
        fn misbehave(&self, hook: &str) -> PluginResult<()> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{}:{hook}", self.name));
            match self.mood {
                Mood::Fine => Ok(()),
                Mood::Fails => Err(PluginError::new("refused")),
                Mood::Panics => panic!("{} blew up in {hook}", self.name),
            }
        }
    }

    impl Plugin for Probe {
        fn name(&self) -> &str {
            self.name
        }
        fn version(&self) -> &str {
            "0"
        }
        fn on_load(&self, _: &PluginContext) -> PluginResult<()> {
            Ok(())
        }
        fn on_unload(&self) -> PluginResult<()> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{}:unload", self.name));
            Ok(())
        }
        fn as_output(&self) -> Option<&dyn OutputPlugin> {
            Some(self)
        }
        fn as_audio(&self) -> Option<&dyn AudioPlugin> {
            Some(self)
        }
        fn as_dictionary(&self) -> Option<&dyn DictionaryPlugin> {
            Some(self)
        }
        fn as_asr(&self) -> Option<&dyn AsrPlugin> {
            Some(self)
        }
    }

    impl OutputPlugin for Probe {
        fn on_transcript(&self, t: &Transcript) -> PluginResult<()> {
            self.misbehave(&format!("output={}", t.text))
        }
    }

    impl AudioPlugin for Probe {
        fn process(&self, samples: &mut Vec<f32>) -> PluginResult<()> {
            // Do damage first, so a failure proves the host threw it away.
            samples.iter_mut().for_each(|s| *s = 9.0);
            self.misbehave("audio")?;
            samples.iter_mut().for_each(|s| *s = 0.5);
            Ok(())
        }
    }

    impl DictionaryPlugin for Probe {
        fn entries(&self) -> PluginResult<Vec<DictionaryEntry>> {
            self.misbehave("dictionary")?;
            Ok(vec![
                DictionaryEntry {
                    phrase: "jeera".into(),
                    replacement: "Jira".into(),
                },
                DictionaryEntry {
                    phrase: "  ".into(),
                    replacement: "never".into(),
                },
            ])
        }
    }

    impl AsrPlugin for Probe {
        fn transcribe(&self, audio: &[f32], language: Option<&str>) -> PluginResult<Transcription> {
            self.misbehave("asr")?;
            Ok(Transcription {
                text: format!("{} samples", audio.len()),
                language: language.map(str::to_owned),
            })
        }
    }

    fn ctx() -> PluginContext {
        PluginContext {
            data_dir: PathBuf::new(),
            settings: Arc::new(|_| None),
        }
    }

    /// A loader holding probes, each wrapped exactly as `export_plugin!` wraps
    /// a real library — so the panic cases exercise the containment a shipped
    /// plugin actually gets, not a host-side net that could not catch a panic
    /// from another copy of std.
    fn loader(probes: &[(&'static str, Mood)]) -> (PluginLoader, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut loader = PluginLoader::new();
        for &(name, mood) in probes {
            let probe = Probe {
                name,
                mood,
                calls: calls.clone(),
            };
            loader
                .load_in_process(Box::new(Guarded(probe)), &ctx())
                .unwrap();
        }
        (loader, calls)
    }

    fn transcript(text: &str) -> Transcript {
        Transcript {
            text: text.into(),
            app: Some("notepad.exe".into()),
            language: None,
        }
    }

    fn taken(calls: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        std::mem::take(&mut *calls.lock().unwrap())
    }

    #[test]
    fn output_plugins_are_told_about_a_transcript() {
        let (loader, calls) = loader(&[("a", Mood::Fine)]);
        deliver_transcript(&loader.plugins(), &transcript("ship it"));
        assert_eq!(taken(&calls), ["a:output=ship it"]);
    }

    /// Through the real worker thread, in order.
    #[test]
    fn the_output_worker_delivers_in_order_off_the_calling_thread() {
        let (loader, calls) = loader(&[("a", Mood::Fine)]);
        let plugins = loader.plugins();
        let tx = output_worker(move || plugins.clone());
        for text in ["one", "two", "three"] {
            tx.send(transcript(text)).unwrap();
        }
        drop(tx);

        let deadline = Instant::now() + Duration::from_secs(5);
        while calls.lock().unwrap().len() < 3 && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(
            taken(&calls),
            ["a:output=one", "a:output=two", "a:output=three"]
        );
    }

    /// The failure that matters most: one broken plugin must not stop the next
    /// one hearing about the transcript, nor unwind into the pipeline.
    #[test]
    fn a_failing_or_panicking_output_plugin_does_not_stop_the_rest() {
        let (loader, calls) = loader(&[
            ("fails", Mood::Fails),
            ("panics", Mood::Panics),
            ("fine", Mood::Fine),
        ]);
        deliver_transcript(&loader.plugins(), &transcript("hello"));
        assert_eq!(
            taken(&calls),
            [
                "fails:output=hello",
                "panics:output=hello",
                "fine:output=hello"
            ]
        );
    }

    #[test]
    fn audio_plugins_process_the_chunk() {
        let (loader, calls) = loader(&[("a", Mood::Fine)]);
        let mut chunk = vec![0.1_f32; 4];
        process_audio(&loader.plugins(), &mut chunk);
        assert_eq!(chunk, vec![0.5; 4]);
        assert_eq!(taken(&calls), ["a:audio"]);
    }

    /// A panicking audio plugin leaves the chunk as captured, and is not asked
    /// again — while a healthy one beside it keeps working.
    #[test]
    fn a_panicking_audio_plugin_changes_nothing_and_is_benched() {
        let (loader, calls) = loader(&[("panics", Mood::Panics)]);
        let plugins = loader.plugins();

        let mut chunk = vec![0.1_f32; 4];
        process_audio(&plugins, &mut chunk);
        assert_eq!(chunk, vec![0.1; 4], "the half-done damage was kept");

        process_audio(&plugins, &mut chunk);
        assert_eq!(
            taken(&calls),
            ["panics:audio"],
            "benched plugin was called again"
        );
    }

    #[tokio::test]
    async fn the_audio_stage_forwards_every_chunk_and_the_stop_sentinel() {
        let (loader, _calls) = loader(&[("a", Mood::Fine)]);
        let plugins = loader.plugins();
        let (in_tx, in_rx) = mpsc::channel(8);
        let (out_tx, mut out_rx) = mpsc::channel(8);
        let stage = tokio::spawn(audio_stage(in_rx, out_tx, move || plugins.clone()));

        in_tx.send(vec![0.2; 3]).await.unwrap();
        in_tx.send(Vec::new()).await.unwrap();
        drop(in_tx);
        stage.await.unwrap();

        assert_eq!(out_rx.recv().await.unwrap(), vec![0.5; 3]);
        assert!(out_rx.recv().await.unwrap().is_empty());
    }

    /// Entries reach the engine and apply after the user's own, so the user's
    /// rule for the same phrase wins.
    #[test]
    fn dictionary_plugin_entries_merge_after_the_users() {
        let (loader, calls) = loader(&[("a", Mood::Fine), ("broken", Mood::Panics)]);
        let from_plugins = dictionary_entries(&loader.plugins());
        assert_eq!(from_plugins.len(), 1, "the blank phrase was not dropped");
        assert_eq!(taken(&calls), ["a:dictionary", "broken:dictionary"]);

        let engine = DictionaryEngine::new(from_plugins.clone());
        assert_eq!(engine.process_for("open jeera", None), "open Jira");

        let mut merged = vec![crate::core::dictionary::DictionaryEntry {
            id: Some(1),
            phrase: "jeera".into(),
            replacement: "cumin".into(),
            enabled: true,
            profile_id: None,
        }];
        merged.extend(from_plugins);
        let engine = DictionaryEngine::new(merged);
        assert_eq!(engine.process_for("open jeera", None), "open cumin");
    }

    #[tokio::test]
    async fn an_asr_plugin_transcribes_as_a_provider() {
        let (loader, calls) = loader(&[("engine", Mood::Fine), ("quiet", Mood::Fine)]);
        let provider = PluginAsrProvider::new(loader.plugins()[0].clone()).unwrap();
        assert_eq!(provider.name(), "plugin:engine");

        let seg = provider
            .transcribe(vec![0.0; 160], Some("en"))
            .await
            .unwrap();
        assert_eq!(seg.text, "160 samples");
        assert_eq!(seg.language.as_deref(), Some("en"));
        assert!(seg.is_final);
        assert_eq!(
            taken(&calls),
            ["engine:asr"],
            "only the selected engine runs"
        );
    }

    /// A panic in a plugin engine is an error the fallback can act on, not a
    /// dead transcript task.
    #[tokio::test]
    async fn a_panicking_asr_plugin_is_an_error() {
        let (loader, _calls) = loader(&[("engine", Mood::Panics)]);
        let provider = PluginAsrProvider::new(loader.plugins()[0].clone()).unwrap();
        let err = provider.transcribe(vec![0.0; 16], None).await.unwrap_err();
        assert!(err.to_string().contains("panicked"), "{err}");
    }

    /// Loaded is enabled: once unloaded, a plugin gets no capability call of
    /// any kind, while the one still enabled carries on.
    #[tokio::test]
    async fn a_disabled_plugin_is_not_called() {
        let (mut loader, calls) = loader(&[("off", Mood::Fine), ("on", Mood::Fine)]);
        loader.unload("off").unwrap();
        assert_eq!(taken(&calls), ["off:unload"]);

        let plugins = loader.plugins();
        deliver_transcript(&plugins, &transcript("x"));
        process_audio(&plugins, &mut vec![0.1; 2]);
        dictionary_entries(&plugins);
        for p in &plugins {
            PluginAsrProvider::new(p.clone())
                .unwrap()
                .transcribe(vec![0.0; 1], None)
                .await
                .unwrap();
        }

        let seen = taken(&calls);
        assert!(seen.iter().all(|c| c.starts_with("on:")), "{seen:?}");
        assert_eq!(seen.len(), 4);
    }

    /// A lifecycle-only plugin offers nothing and is skipped by every stage.
    #[test]
    fn a_plugin_without_capabilities_is_skipped() {
        #[derive(Default)]
        struct Bare;
        impl Plugin for Bare {
            fn name(&self) -> &str {
                "bare"
            }
            fn version(&self) -> &str {
                "0"
            }
            fn on_load(&self, _: &PluginContext) -> PluginResult<()> {
                Ok(())
            }
            fn on_unload(&self) -> PluginResult<()> {
                Ok(())
            }
        }
        let mut loader = PluginLoader::new();
        loader
            .load_in_process(Box::new(Guarded(Bare)), &ctx())
            .unwrap();
        let plugins = loader.plugins();

        let mut chunk = vec![0.3; 2];
        process_audio(&plugins, &mut chunk);
        assert_eq!(chunk, vec![0.3; 2]);
        assert!(dictionary_entries(&plugins).is_empty());
        assert!(PluginAsrProvider::new(plugins[0].clone()).is_none());
        deliver_transcript(&plugins, &transcript("nothing listens"));
    }
}
