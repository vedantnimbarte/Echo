# Echo — Implementation Plan

**Project root:** `C:\Users\PRENEEL\Documents\Vedant Nimbarte\Echo\`
**App directory:** `echo-app\`
**Stack:** Rust · Tauri v2 · React 19 · TypeScript · TailwindCSS v4 · SQLite

---

## Current Status

| Phase | Name | Status |
|---|---|---|
| 0 | Foundation | ✅ Complete |
| 1 | Audio Pipeline | ✅ Complete (Silero VAD deferred) |
| 2 | Local ASR (Whisper) | ✅ Code complete (build needs libclang) |
| 3 | Text Injection | ✅ All platforms (macOS/Linux unverified on Win host) |
| 4 | Dictionaries | ✅ Complete |
| 5 | Cloud ASR Providers | ✅ Complete (Deepgram streams over WebSocket) |
| 6 | Telemetry | ✅ Complete |
| 7 | Plugin System | ✅ Complete (echo-sdk crate + export_plugin! macro) |
| 8 | Packaging | ✅ Config + CI (signing certs TBD) |
| 9 | v1 Launch | ✅ Hotkey + CSP + docs (perf/signing TBD) |
| 10 | Post-Dictation Loop | ✅ Undo, retry, prompting, streaming, injection rescue |
| 11 | Formatting & Field Awareness | ✅ Spoken punctuation, numbers, password guard, auto-method, CLI |
| 12 | Local streaming | ✅ Live text works offline (rolling re-decode) |
| 13 | Measurement & languages | ✅ 9.2 measured; punctuation in 7 languages |

---

## Phase 0 — Foundation ✅ Complete

Everything compiles cleanly (`cargo check`, `tsc --noEmit`).

### What was built

**Rust (`echo-app/src-tauri/src/`)**

```
error.rs                        EchoError enum, serde-serializable for Tauri
state.rs                        AppState: Mutex<Connection>, Arc<AudioService>,
                                Arc<AsrManager>, RwLock<DictionaryEngine>,
                                Mutex<EnergyVad>, Arc<dyn TextInjector>, Mutex<bool>
lib.rs                          Tauri builder, plugin registration, AppState init
core/mod.rs
core/events.rs                  AppEvent enum with echo:// event name strings
core/audio/mod.rs               AudioService: CPAL host, device list, f32 capture, 16kHz resample
core/asr/mod.rs                 AsrProvider trait (async_trait), TranscriptSegment
core/asr/manager.rs             AsrManager: RwLock<HashMap<String, Arc<dyn AsrProvider>>>
core/vad/mod.rs                 EnergyVad: RMS-based, configurable threshold
core/dictionary/mod.rs          DictionaryEngine: normalize → replace → output
core/injection/mod.rs           TextInjector trait + platform_injector() factory fn
platform/mod.rs
platform/windows.rs             SendInput UTF-16 keyboard injection
platform/macos.rs               Stub
platform/linux.rs               Stub
storage/mod.rs
storage/db.rs                   SQLite open, WAL mode, versioned migration runner
storage/models.rs               Setting, Profile, DictionaryEntry, TranscriptionRecord
storage/repositories.rs         Typed SQL: settings, dictionary CRUD, history CRUD
commands/mod.rs
commands/audio.rs               get_audio_devices
commands/recording.rs           start_recording, stop_recording, is_recording
commands/dictionary.rs          list_dictionary, add_dictionary_entry, delete_dictionary_entry
commands/history.rs             get_history, clear_history
commands/settings.rs            get_setting, set_setting
```

**React (`echo-app/src/`)**

```
main.tsx                        QueryClientProvider root
styles.css                      @import "tailwindcss"
App.tsx                         4-tab shell (Record / Dictionary / History / Settings)
ipc/commands.ts                 Typed invoke() wrappers for all Tauri commands
ipc/events.ts                   Typed listen() wrappers for all echo:// events
store/recordingStore.ts         Zustand: isRecording, mode, partial/final transcript, error
hooks/useEchoEvents.ts          Subscribes to all echo:// events on mount
components/recording/RecordingPanel.tsx
components/dictionary/DictionaryPanel.tsx
components/history/HistoryPanel.tsx
components/settings/SettingsPanel.tsx
```

**SQLite schema (migration v1)**
- `settings (key PK, value)`
- `profiles (id, name, created_at, updated_at)`
- `dictionary_entries (id, phrase, replacement, enabled, profile_id FK, created_at)`
- `transcription_history (id, text, language, provider, created_at)`
- `telemetry_events (id, event_type, payload, created_at)`
- `plugins (id, name, version, enabled, manifest, installed_at)`

---

## Phase 1 — Audio Pipeline ✅

**Goal:** Audio flows end-to-end from microphone through VAD into the ASR pipeline.

**Done:** 1.1 VAD gating wired into `start_recording` (VAD removed from `AppState`, lives in the audio task); 1.2 device selector UI; 1.4 audio resample tests (also fixed a mono-channel down-mix bug). **Deferred:** 1.3 Silero ONNX VAD — requires bundling `silero_vad.onnx` and the `ort` runtime; `EnergyVad` is sufficient for the pipeline for now.

### 1.1 Wire VAD into `start_recording`

**File:** `echo-app/src-tauri/src/commands/recording.rs`

Current `start_recording` feeds raw CPAL chunks directly to ASR. Add a VAD gating stage in between.

```rust
// After audio_rx is opened:
let vad_tx = ...;   // feeds ASR
let mut vad = state.vad.lock().unwrap(); // or clone config

tokio::spawn(async move {
    while let Some(chunk) = audio_rx.recv().await {
        if vad.is_speech(&chunk) {
            let _ = vad_tx.send(chunk).await;
        }
    }
});
// pass vad_tx receiver to ASR instead of audio_rx directly
```

Move `EnergyVad` out of `Mutex` and into the spawn — it does not need shared access; it belongs entirely to the audio task.

**Refactor `state.rs`:** Remove `vad: Mutex<EnergyVad>` from `AppState`. The VAD instance is created fresh per recording session inside the spawn.

### 1.2 Device selector UI

**File:** `echo-app/src/components/recording/RecordingPanel.tsx`

- Add a `useQuery` to fetch `get_audio_devices` on mount.
- Render a `<select>` dropdown showing device names; mark default with `(default)`.
- Store selected device in Zustand (`selectedDevice: string | null`).
- Pass `selectedDevice` to `start_recording` command.

**Store change — `recordingStore.ts`:** Add `selectedDevice: string | null` and `setSelectedDevice`.

### 1.3 Silero VAD (production quality)

**Crate:** `ort` (ONNX Runtime Rust bindings) version `~2.0`

**Cargo.toml addition:**
```toml
ort = { version = "2", features = ["download-binaries"] }
ndarray = "0.16"
```

**New file:** `echo-app/src-tauri/src/core/vad/silero.rs`

```rust
use ort::{Environment, Session, SessionBuilder, Value};
use ndarray::Array2;

pub struct SileroVad {
    session: Session,
    threshold: f32,
    h: Array2<f32>,   // hidden state (2, 1, 64)
    c: Array2<f32>,   // cell state  (2, 1, 64)
}

impl SileroVad {
    pub fn load(model_path: &Path) -> Result<Self> { ... }

    // Process 512-sample window at 16kHz → returns speech probability 0..1
    pub fn process_chunk(&mut self, samples: &[f32]) -> f32 { ... }

    pub fn is_speech(&mut self, samples: &[f32]) -> bool {
        self.process_chunk(samples) > self.threshold
    }

    pub fn reset(&mut self) { /* zero h and c */ }
}
```

The Silero ONNX model file (`silero_vad.onnx`) should be bundled in `src-tauri/models/` and referenced via `tauri::path::resource_dir`.

**Update `core/vad/mod.rs`:** expose a `VadEngine` enum:
```rust
pub enum VadEngine {
    Energy(EnergyVad),
    Silero(SileroVad),
}
impl VadEngine {
    pub fn is_speech(&mut self, samples: &[f32]) -> bool { ... }
}
```

### 1.4 Audio tests

**File:** `echo-app/src-tauri/src/core/audio/mod.rs` — add unit tests:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn resample_passthrough_16k() { ... }
    #[test]
    fn resample_48k_to_16k_length() { ... }
    #[test]
    fn mono_downmix() { ... }
}
```

---

## Phase 2 — Local ASR (Whisper) ✅

**Goal:** Offline transcription using whisper.cpp via Rust bindings.

**Done:** `WhisperProvider` (2.2), `ModelManager` download manager (2.3), `commands/asr.rs` with `list_models`/`download_model`/`set_asr_provider` (2.3), `ModelSelector` UI with progress bar (2.4), startup registration in `lib.rs` (2.5).

**Build note:** `whisper-rs` is behind an off-by-default `whisper` Cargo feature because `whisper-rs-sys` runs `bindgen`, which needs **libclang** (LLVM) plus **cmake** at build time. The default build excludes it and stays green; `WhisperProvider` code is verified against the whisper-rs 0.13 API but compiles only with `cargo build --features whisper` on a machine with libclang installed. reqwest uses platform-default TLS (schannel on Windows) to avoid extra native build deps. See README for local setup.

### 2.1 Add whisper-rs

**Cargo.toml addition:**
```toml
whisper-rs = { version = "0.13", features = ["opencl"] }
reqwest = { version = "0.12", features = ["stream", "rustls-tls"], default-features = false }
futures-util = "0.3"
```

### 2.2 Implement `WhisperProvider`

**New file:** `echo-app/src-tauri/src/core/asr/whisper.rs`

```rust
use async_trait::async_trait;
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};

pub struct WhisperProvider {
    ctx: Arc<Mutex<WhisperContext>>,
    model_name: String,
}

impl WhisperProvider {
    pub fn load(model_path: &Path, model_name: &str) -> Result<Self> {
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().unwrap(),
            WhisperContextParameters::default(),
        ).map_err(|e| EchoError::AsrProvider(e.to_string()))?;
        Ok(Self { ctx: Arc::new(Mutex::new(ctx)), model_name: model_name.into() })
    }
}

#[async_trait]
impl AsrProvider for WhisperProvider {
    fn name(&self) -> &str { &self.model_name }

    async fn transcribe(&self, audio: Vec<f32>, language: Option<&str>) -> Result<TranscriptSegment> {
        let ctx = self.ctx.clone();
        let lang = language.map(str::to_string);
        tokio::task::spawn_blocking(move || {
            let ctx = ctx.lock().unwrap();
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            if let Some(l) = &lang { params.set_language(Some(l)); }
            params.set_print_progress(false);
            params.set_print_realtime(false);
            let mut state = ctx.create_state().map_err(|e| EchoError::AsrProvider(e.to_string()))?;
            state.full(params, &audio).map_err(|e| EchoError::AsrProvider(e.to_string()))?;
            let n = state.full_n_segments().map_err(|e| EchoError::AsrProvider(e.to_string()))?;
            let text: String = (0..n)
                .filter_map(|i| state.full_get_segment_text(i).ok())
                .collect::<Vec<_>>().join(" ");
            Ok(TranscriptSegment { text, is_final: true, language: None, confidence: None })
        }).await.map_err(|e| EchoError::AsrProvider(e.to_string()))?
    }

    async fn transcribe_stream(
        &self,
        mut audio_rx: mpsc::Receiver<Vec<f32>>,
        tx: mpsc::Sender<TranscriptSegment>,
        language: Option<&str>,
    ) -> Result<()> {
        // Accumulate audio until silence (tracked by VAD upstream), then transcribe.
        let mut buffer: Vec<f32> = Vec::new();
        while let Some(chunk) = audio_rx.recv().await {
            if chunk.is_empty() { break; } // sentinel from VAD: end of utterance
            buffer.extend_from_slice(&chunk);
        }
        if !buffer.is_empty() {
            let segment = self.transcribe(buffer, language).await?;
            let _ = tx.send(segment).await;
        }
        Ok(())
    }

    fn supports_streaming(&self) -> bool { false } // true streaming added in Phase 5
}
```

### 2.3 Model download manager

**New file:** `echo-app/src-tauri/src/core/asr/model_manager.rs`

```rust
pub struct ModelManager {
    models_dir: PathBuf,
}

// Whisper model URLs (Hugging Face)
const MODEL_URLS: &[(&str, &str)] = &[
    ("tiny",   "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin"),
    ("base",   "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"),
    ("small",  "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"),
    ("medium", "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin"),
];

impl ModelManager {
    pub fn new(models_dir: PathBuf) -> Self { Self { models_dir } }

    pub fn model_path(&self, name: &str) -> PathBuf {
        self.models_dir.join(format!("ggml-{name}.bin"))
    }

    pub fn is_downloaded(&self, name: &str) -> bool {
        self.model_path(name).exists()
    }

    pub async fn download(
        &self,
        name: &str,
        progress_tx: mpsc::Sender<f32>,  // 0.0 .. 1.0
    ) -> Result<PathBuf> {
        // Use reqwest stream, write to temp file, rename on complete.
        // Emit progress_tx updates so UI can show progress bar.
    }
}
```

**New IPC commands to add in `commands/asr.rs`:**
```rust
list_models()          -> Vec<ModelInfo>   // {name, downloaded, size_mb}
download_model(name)   -> ()               // long-running, use events for progress
set_asr_provider(name) -> ()               // switches active provider
```

**New Tauri events:**
- `echo://model-download-progress` `{ name: string, progress: f32 }`
- `echo://model-download-complete` `{ name: string }`

### 2.4 Model selector UI

**New component:** `echo-app/src/components/settings/ModelSelector.tsx`

- Query `list_models` on mount.
- Show each model with download status and size.
- "Download" button triggers `download_model`; progress bar listens to `echo://model-download-progress`.
- Selected model saved via `set_setting("asr_model", name)`.

### 2.5 Register WhisperProvider in `lib.rs`

```rust
// In setup closure, after AppState is created:
let models_dir = data_dir.join("models");
std::fs::create_dir_all(&models_dir)?;

let model_name = repositories::get_setting(&conn, "asr_model")
    .unwrap_or(None)
    .unwrap_or_else(|| "base".into());

let model_path = models_dir.join(format!("ggml-{model_name}.bin"));
if model_path.exists() {
    let provider = Arc::new(WhisperProvider::load(&model_path, &model_name)?);
    app_state.asr.register(provider).await;
    app_state.asr.set_active(&model_name).await?;
}
```

---

## Phase 3 — Text Injection ✅

**Windows:** Complete. `platform/windows.rs` uses `SendInput` with `KEYEVENTF_UNICODE`.

**macOS (3.1):** `platform/macos.rs` posts per-character CGEvents via core-graphics, gated on `AXIsProcessTrusted`. **Linux (3.2):** `platform/linux.rs` shells out to `ydotool`/`xdotool` with args after `--` (no shell, no flag injection). **Settings UI (3.3):** auto-inject toggle, inject delay, accessibility check. Also added `inject_delay_ms` handling and a `check_accessibility_permission` command. macOS/Linux paths are cfg-gated and were not compiled on the Windows dev host — verify on those platforms.

### 3.1 macOS injection

**File:** `echo-app/src-tauri/src/platform/macos.rs`

**Dependencies (Cargo.toml):**
```toml
[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10"
core-graphics = { version = "0.24", features = ["highsierra"] }
```

```rust
use core_graphics::event::{CGEvent, CGEventTapLocation, CGEventType, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

impl TextInjector for MacosInjector {
    fn inject_text(&self, text: &str) -> Result<()> {
        // Check AXIsProcessTrusted() — request accessibility if not granted.
        // For each UTF-16 code unit: CGEventCreateKeyboardEvent(source, 0, true/false)
        // with CGEventKeyboardSetUnicodeString() to set the character.
        // Post via CGEventPost(kCGHIDEventTap, event).
    }
}
```

**macOS permission flow:**
- On first injection attempt, call `AXIsProcessTrustedWithOptions` with prompt option.
- Emit `echo://permission-required` event to frontend.
- Show permission dialog in UI linking to System Settings → Privacy → Accessibility.

### 3.2 Linux injection

**File:** `echo-app/src-tauri/src/platform/linux.rs`

```rust
// Detect display server at runtime:
// - WAYLAND_DISPLAY set → use ydotool
// - DISPLAY set → use xdotool
// Both via std::process::Command

impl TextInjector for LinuxInjector {
    fn inject_text(&self, text: &str) -> Result<()> {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            // ydotool type --key-delay 0 -- "<text>"
        } else {
            // xdotool type --clearmodifiers -- "<text>"
        }
    }
}
```

**Note:** `ydotool` requires `ydotoold` daemon running. Document this as a Linux prerequisite.

### 3.3 Injection settings UI

**File:** `echo-app/src/components/settings/SettingsPanel.tsx`

Add:
- "Inject after transcription" checkbox (setting key: `auto_inject`, default: `true`)
- "Inject delay (ms)" number input (setting key: `inject_delay_ms`, default: `0`)
- On macOS: "Check Accessibility Permission" button

---

## Phase 4 — Dictionaries ✅

**Done:** 4.1 JSON import/export (commands + dialog-plugin UI), 4.2 per-entry enable/disable toggle, 4.3 dictionary applied to final transcripts + auto-inject into the focused app (`auto_inject` setting, default on). `state.dictionary` is now `Arc<RwLock<..>>` so the transcript task can share it.

Original remaining items (now complete):

### 4.1 Import/export

**New IPC commands in `commands/dictionary.rs`:**

```rust
#[tauri::command]
pub async fn export_dictionary(state: State<'_,AppState>) -> Result<String> {
    // Serialize entries to JSON string, return to frontend
    // Frontend uses tauri-plugin-fs or showSaveDialog to write file
}

#[tauri::command]
pub async fn import_dictionary(state: State<'_,AppState>, json: String) -> Result<usize> {
    // Deserialize JSON, insert all entries (skip duplicates), return count added
}
```

**UI in `DictionaryPanel.tsx`:**
- "Export JSON" button: calls `export_dictionary`, then `save` dialog.
- "Import JSON" button: opens file dialog, reads JSON, calls `import_dictionary`.

Use `@tauri-apps/plugin-dialog` for file dialogs:
```toml
tauri-plugin-dialog = "2"   # Cargo.toml
```
```
npm install @tauri-apps/plugin-dialog
```

### 4.2 Enable/disable toggle per entry

**UI:** Add toggle switch per entry in the list. Call a new command:
```rust
#[tauri::command]
pub fn toggle_dictionary_entry(state: State<'_,AppState>, id: i64, enabled: bool) -> Result<()>
```

### 4.3 Dictionary applied to transcription output

**File:** `commands/recording.rs` — in the transcript emitter task:
```rust
// After receiving TranscriptFinal:
let processed = state.dictionary.read().await.process(&segment.text);
// Emit processed text instead of raw text
// If auto_inject == "true": state.injector.inject_text(&processed)?
```

---

## Phase 5 — Cloud ASR Providers ✅

**Done:** OpenAI + Groq via shared `WhisperApiProvider` (5.1, 5.2), Deepgram via the pre-recorded `/v1/listen` HTTP API (5.3 — WebSocket streaming deferred), `keychain.rs` for OS-keychain key storage (5.4), `commands/providers.rs` with `set_api_key`/`get_api_key_set`/`remove_api_key` + startup registration (5.5), and the `CloudProviders` settings UI (5.6). Shared WAV encoder (`hound`) and a default buffered `transcribe_stream` on the trait support all batch providers.

### 5.1 OpenAI Whisper API

**New file:** `echo-app/src-tauri/src/core/asr/openai.rs`

```rust
pub struct OpenAiProvider {
    api_key: String,
    client: reqwest::Client,
}

#[async_trait]
impl AsrProvider for OpenAiProvider {
    fn name(&self) -> &str { "openai" }

    async fn transcribe(&self, audio: Vec<f32>, language: Option<&str>) -> Result<TranscriptSegment> {
        // Convert f32 PCM → WAV bytes in memory (hound crate)
        // POST multipart to https://api.openai.com/v1/audio/transcriptions
        // model: "whisper-1", response_format: "verbose_json"
        // Parse response for text + language
    }
}
```

**Cargo.toml:**
```toml
hound = "3.5"   # WAV encoding
```

### 5.2 Groq provider

**New file:** `echo-app/src-tauri/src/core/asr/groq.rs`

Groq uses the same API shape as OpenAI (`/openai/v1/audio/transcriptions`), just different base URL and key. Can share most code with `OpenAiProvider` via a `WhisperApiProvider` base struct:

```rust
pub struct WhisperApiProvider {
    name: String,
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}
// OpenAiProvider and GroqProvider both wrap WhisperApiProvider
```

### 5.3 Deepgram provider

**New file:** `echo-app/src-tauri/src/core/asr/deepgram.rs`

Deepgram supports true streaming via WebSocket. Implement streaming path:
```rust
// Use tokio-tungstenite for WebSocket
// Send raw PCM chunks as binary frames
// Receive JSON transcript events (interim + final)
fn supports_streaming(&self) -> bool { true }
```

**Cargo.toml:**
```toml
tokio-tungstenite = { version = "0.26", features = ["native-tls"] }
```

### 5.4 Secure API key storage

Use OS keychain, not plain SQLite.

**Cargo.toml:**
```toml
keyring = "3"
```

**New file:** `echo-app/src-tauri/src/storage/keychain.rs`
```rust
pub fn store_api_key(service: &str, key: &str) -> Result<()>
pub fn get_api_key(service: &str) -> Result<Option<String>>
pub fn delete_api_key(service: &str) -> Result<()>
```

**New IPC commands in `commands/providers.rs`:**
```rust
set_api_key(provider: String, key: String) -> ()
get_api_key_set(provider: String) -> bool   // only confirms presence, never returns the key
```

### 5.5 Provider registration in `lib.rs`

On startup, check which providers have API keys stored and register them:
```rust
for provider_name in ["openai", "groq", "deepgram"] {
    if let Ok(Some(key)) = keychain::get_api_key(provider_name) {
        let provider: Arc<dyn AsrProvider> = match provider_name {
            "openai" => Arc::new(OpenAiProvider::new(key)),
            "groq"   => Arc::new(GroqProvider::new(key)),
            "deepgram" => Arc::new(DeepgramProvider::new(key)),
            _ => continue,
        };
        asr_manager.register(provider).await;
    }
}
```

### 5.6 Settings UI additions

**`SettingsPanel.tsx`** — for each cloud provider:
- Password input for API key (never pre-filled, only shows "••••••••" if set)
- "Save" and "Remove" buttons
- Provider description and link to their docs

---

## Phase 6 — Telemetry ✅

**Done:** `TelemetryService` (6.1, local-only, gated by opt-in flag), commands `get_telemetry_summary`/`clear_telemetry`/`set_telemetry_enabled`/`record_telemetry_event` (6.2), and `TelemetrySettings` UI (6.3). Records `app_started`, `recording_started`, and `transcription_complete` (word count only). Nothing is sent off-device.

### 6.1 Telemetry service

**New file:** `echo-app/src-tauri/src/core/telemetry/mod.rs`

```rust
pub struct TelemetryService {
    db: Arc<Mutex<Connection>>,
    enabled: AtomicBool,
}

impl TelemetryService {
    pub fn record(&self, event_type: &str, payload: Option<serde_json::Value>) {
        if !self.enabled.load(Ordering::Relaxed) { return; }
        // INSERT INTO telemetry_events
    }
}
```

**Events to record:**
- `app_started` `{ version, os, arch }`
- `recording_started` `{ provider }`
- `transcription_complete` `{ duration_ms, word_count, provider }`
- `error` `{ kind }` — never includes text/audio

**Never record:** raw audio, transcript text, file paths, window titles.

### 6.2 IPC commands

```rust
get_telemetry_summary() -> TelemetrySummary   // counts by event type
clear_telemetry() -> ()
set_telemetry_enabled(enabled: bool) -> ()
```

### 6.3 Settings UI

Add to `SettingsPanel.tsx`:
- "Share anonymous usage data" toggle (default: on)
- "View collected data" expandable section showing event counts
- "Delete all telemetry data" button

---

## Phase 7 — Plugin System ✅

**Done:** plugin traits + `PluginContext` (7.1), `PluginManifest`/`plugin.json` schema (7.2), `PluginLoader` via libloading (7.3), commands `list_plugins`/`install_plugin`/`enable_plugin`/`disable_plugin`/`uninstall_plugin` with DB+disk registry and startup loading (7.4), and the `PluginsPanel` UI + tab (7.6). **Done (7.5):** the standalone `echo-sdk` crate now holds the shared plugin API (`Plugin`, `PluginContext`, manifest types, self-contained `OutputPlugin`/`AudioPlugin`, a dependency-free `PluginError`, and an `export_plugin!` macro that emits `echo_plugin_create`). `src-tauri` is the workspace root with `echo-sdk` as a member; `core/plugins` re-exports the SDK types so nothing else changed. `AsrPlugin`/`DictionaryPlugin` stay host-side (they reference host types). Uses a declarative macro rather than the originally-sketched proc-macro — no extra crate needed.

**Safety:** plugins run in-process with full trust; the manifest permission list is advisory. True sandboxing (WASM) is a future goal.

### 7.1 Plugin trait

**File:** `echo-app/src-tauri/src/core/plugins/mod.rs`

```rust
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn on_load(&self, ctx: &PluginContext) -> Result<()>;
    fn on_unload(&self) -> Result<()>;
}

pub trait AsrPlugin: Plugin {
    fn as_asr_provider(&self) -> Arc<dyn AsrProvider>;
}

pub trait OutputPlugin: Plugin {
    fn inject_text(&self, text: &str) -> Result<()>;
}

pub trait AudioPlugin: Plugin {
    fn process(&self, samples: &mut Vec<f32>);
}

pub trait DictionaryPlugin: Plugin {
    fn entries(&self) -> Vec<DictionaryEntry>;
}

pub struct PluginContext {
    pub data_dir: PathBuf,
    pub settings: Arc<dyn Fn(&str) -> Option<String> + Send + Sync>,
}
```

### 7.2 Plugin manifest (JSON)

Each plugin ships with `plugin.json`:
```json
{
  "name": "my-plugin",
  "version": "1.0.0",
  "description": "...",
  "author": "...",
  "permissions": ["asr", "output"],
  "entry": "my_plugin.dll"
}
```

### 7.3 Plugin loader

**File:** `echo-app/src-tauri/src/core/plugins/loader.rs`

```rust
use libloading::{Library, Symbol};

pub struct PluginLoader {
    plugins_dir: PathBuf,
    loaded: Vec<(Library, Arc<dyn Plugin>)>,
}

impl PluginLoader {
    // Load shared library, call exported `echo_plugin_create()` fn
    // Validate manifest permissions
    // Register with AsrManager / OutputEngine as appropriate
    pub fn load(&mut self, path: &Path) -> Result<()> { ... }
    pub fn unload(&mut self, name: &str) -> Result<()> { ... }
}
```

**Safety note:** Plugin code runs in-process. The permission model is advisory in v0.7; true sandboxing (WASM runtime) is a v1.x goal.

### 7.4 Plugin IPC commands

```rust
list_plugins() -> Vec<PluginInfo>
install_plugin(path: String) -> ()   // copies to plugins_dir, loads
enable_plugin(name: String) -> ()
disable_plugin(name: String) -> ()
uninstall_plugin(name: String) -> ()
```

### 7.5 Plugin SDK crate (`echo-sdk`)

**New crate at:** `echo-app/src-tauri/echo-sdk/`

```toml
[package]
name = "echo-sdk"
version = "0.1.0"

[lib]
crate-type = ["rlib"]

[dependencies]
echo-plugin-api = { path = "../" }  # re-exports Plugin traits
```

Provides:
- All plugin traits
- `#[echo_plugin]` proc-macro for boilerplate
- `PluginContext` helpers

### 7.6 Plugin UI

**New component:** `echo-app/src/components/settings/PluginsPanel.tsx`

- List installed plugins with name, version, enabled toggle, uninstall button
- "Install from file" button (opens file dialog for `.dll`/`.dylib`/`.so`)
- Add "Plugins" tab to `App.tsx`

---

## Phase 8 — Packaging ✅

**Done:** per-platform `bundle` config in `tauri.conf.json` (8.1–8.3), `entitlements.plist`, template manifests under `packaging/` (winget, homebrew cask, flatpak, snap), and a `.github/workflows/release.yml` CI matrix building Win/macOS(universal)/Linux on `v*` tags (8.4). **TBD:** code-signing certificates and filling per-release URLs/hashes in the manifests.

### 8.1 Windows

**`tauri.conf.json` additions:**
```json
"bundle": {
  "windows": {
    "wix": { "language": "en-US" },
    "nsis": { "displayLanguageSelector": false },
    "signCommand": null
  }
}
```

**Winget manifest** at `packaging/winget/`:
```yaml
PackageIdentifier: Echo.Echo
PackageVersion: 1.0.0
```

**GitHub Actions workflow** (`ci/windows.yml`): build MSI + EXE on push to `release/*`.

### 8.2 macOS

```json
"bundle": {
  "macOS": {
    "minimumSystemVersion": "12.0",
    "entitlements": "entitlements.plist",
    "signingIdentity": null
  }
}
```

**`entitlements.plist`** must include:
```xml
<key>com.apple.security.automation.apple-events</key><true/>
<key>com.apple.security.cs.allow-unsigned-executable-memory</key><true/>
```
(whisper.cpp JIT needs the last one)

**Homebrew formula** at `packaging/homebrew/echo.rb`.

### 8.3 Linux

**AppImage:** Tauri builds this automatically.
**Deb/RPM:** `tauri.conf.json` `bundle.linux` settings.
**Flatpak manifest** at `packaging/flatpak/com.echo.app.yml`.
**Snap** at `packaging/snap/snapcraft.yaml`.

### 8.4 GitHub Actions CI matrix

```yaml
strategy:
  matrix:
    os: [windows-latest, macos-latest, ubuntu-latest]
```

---

## Phase 9 — v1 Launch ✅ (core)

**Done:** 9.4 global hotkey (configurable, default `CommandOrControl+Shift+Space`, toggles recording); CSP configured (9.1); `npm audit` clean; `cargo audit` clean (only upstream-pinned transitive advisories remain — accepted in `src-tauri/.cargo/audit.toml`; `anyhow` bumped to 1.0.103 to clear RUSTSEC-2026-0190); a `.github/workflows/ci.yml` now runs audit + clippy + frontend build on every push/PR; `CONTRIBUTING.md` + `PLUGINS.md` (9.3); API keys never returned to the UI / kept in keychain; Linux injector args are shell-safe. **TBD:** measure perf targets (9.2) and code-signing certs (8.x/9.1) — both need a release/hardware environment. README is the final deliverable.

### 9.1 Security review checklist

- [x] API keys never logged or emitted in events (verified: no key values in any log/event; kept in OS keychain)
- [ ] Plugin permissions enforced at load time (intentionally advisory in v1 — plugins run in-process; enforcement awaits a WASM/sandbox runtime)
- [ ] SQLite data at rest: consider SQLCipher if user requests encryption (not requested)
- [x] No shell injection in Linux injector (verified: `Command::new(prog).args(&args)` with `--`, never a shell)
- [x] CSP headers configured in `tauri.conf.json`
- [ ] Signed releases (code signing certs for Windows + macOS) — blocked on certificates
- [x] Dependency audit: `cargo audit` (clean, policy documented), `npm audit` (clean); both wired into CI

### 9.2 Performance targets (from PRD)

**Measured 2026-09-06** by `echo --benchmark` on a CPU-only Windows machine,
`base.en`, 3-second clip. Re-run it rather than trusting these numbers on other
hardware — that is what the command is for.

| Metric | Target | Measured | |
|---|---|---|---|
| Startup time | < 2s | **0.74s** | ✅ process start → `setup` complete (Rust cannot see first paint) |
| Transcription latency | < 300ms | **~1850ms** | ❌ on CPU. Warm resident server; cold CLI ~2190ms. See below |
| Memory at idle | < 100 MB | **~57 MB** | ✅ this process only |
| Memory during transcription | < 500 MB | **~57 MB** | ✅ but the model lives in a `whisper-server` child, so machine-wide is higher |

**The latency target is not met on CPU, and Echo cannot fix that.** The time is
inside whisper's encoder, which pads every clip under 30 seconds to a full
window — so a one-second utterance costs what a twenty-second one does. What
closes the gap is a GPU pack (Phase 8) or a smaller model, not tuning here.
Recorded as *met only with acceleration* rather than left as an aspiration.

This is also the number that sets expectations for live text (Phase 12): each
partial is a full re-decode, so on CPU-only hardware they arrive about every two
seconds rather than the ~0.8s the scheduler asks for.

### 9.3 Documentation

- `README.md` — install, quickstart, hotkey reference
- `CONTRIBUTING.md` — dev setup, architecture overview
- `PLUGINS.md` — SDK guide and plugin manifest spec
- Inline Rust doc comments on all public traits (`///`)

### 9.4 Global hotkey configuration

Currently the global shortcut plugin is registered but no hotkeys are wired.

**Required work:**
- Add `register_hotkey(shortcut: String) -> ()` IPC command
- Use `tauri_plugin_global_shortcut::Builder::with_handler` in `lib.rs`
- Default hotkey: `CommandOrControl+Shift+Space`
- Make it configurable in `SettingsPanel.tsx`
- Save to `settings` table with key `hotkey`

---

## Phase 10 — Post-Dictation Loop ✅

Everything before this phase gets words *out* of your mouth and into a text
field. This phase is about the seconds after that: the transcript is wrong, or
it is slow to appear, or the audio never came from a microphone at all.

**Constraints agreed for this phase:**
- **Cross-platform or not at all.** Nothing ships that only works on one OS.
  Where a mechanism cannot be made identical on Windows/macOS/Linux, the plan
  says so and picks the one that can.
- **Local default, cloud opt-in** — the same shape as the ASR providers today.
- No new runtime dependency unless the alternative is more than a few lines.

**Order matters:** 10.1 adds the "remove the text I just typed" primitive that
10.4 needs, and the retained-utterance state that 10.2 needs. Build it first
even if the other items look more interesting.

| # | Item | Status |
|---|---|---|
| 10.1 | Undo last insert | ✅ `core/undo.rs`, `commands/fixup.rs`, `TextInjector::send_undo` |
| 10.2 | Retry last utterance on a stronger model | ✅ `retain_utterances`, `AsrManager::transcribe_with` |
| 10.3 | Context-aware decoder prompting | ✅ `core/asr/prompt.rs` |
| 10.4 | Live streaming injection | ✅ opt-in per app, migration 3 |
| 10.5 | New input sources | ↩︎ redirected — see below |

**What shipped differently from the plan, and why.** Two corrections came out of
reading the code rather than the docs:

- **10.5(a) was already built.** `commands/import.rs` transcribes wav/mp3/ogg/
  flac through the one-shot CLI, and whisper.cpp decodes all four itself — so
  the "WAV only, add a decoder crate later" caveat in the original plan was
  wrong, and there was nothing to write. It is missing from the README's
  feature list, which is why it read as a gap.
- **10.5(b) was not a real hole.** Injection with no identifiable focused app
  still works: the OS sends synthetic keystrokes to whatever holds keyboard
  focus, named or not. The actual way words get lost is injection *failing* —
  macOS Accessibility denied, `xdotool` missing — which logged an error and
  left the user watching a cursor that never moved. That is what
  `rescue_to_clipboard` now covers: the transcript goes to the clipboard and
  the error says so.

---

### 10.1 Undo last insert

**The problem:** Echo has no undo anywhere in the tree. When it types the wrong
thing the only recovery is manual — and manual cleanup is worse than usual
here, because the transcript arrived as one silent burst the user did not watch
land.

**Mechanism.** Two candidates, and the cross-platform rule decides between them:

| | Send the app's undo chord (`Ctrl/Cmd+Z`) | Send N backspaces |
|---|---|---|
| Cross-platform | Yes — one new trait method, like `send_paste` | Yes |
| Correct after the caret moved | Yes (the app's own undo stack) | **No** — eats the wrong text |
| Granularity | The app's, not ours | Exact |
| Paste-injected text | Usually one undo step ✅ | Exact |
| Keystroke-injected text | May be many undo steps ⚠️ | Exact |

Take the undo chord. Backspacing a character count is only correct while
nothing else has touched the field, and a process cannot observe that — the
failure mode is silently deleting the user's own typing, which is worse than
the bug being fixed.

**Required work:**
- Add `fn send_undo(&self) -> Result<()>` to `TextInjector`
  (`core/injection/mod.rs`), implemented in all three `platform/*.rs` injectors
  next to the existing `send_paste`/`send_copy`. Linux reuses
  `linux_chord_command` (keycode `44` = `z`, xdotool key `ctrl+z`).
- Store the last delivery in `AppState`: `Mutex<Option<LastDelivery>>` holding
  the delivered text, whether paste or keystrokes were used, and the target app
  id. Set it in the injection block of `commands/recording.rs`.
- `undo_last_insert` IPC command plus its own configurable hotkey (default
  `CommandOrControl+Shift+Z`), registered the way 9.4 registers the main one.
- Clear the stored delivery after an undo, so a second press does not walk back
  into the user's own edits.
- Spoken form: `"scratch that"` as a built-in phrase checked before injection,
  behind a command-mode-style opt-in so it cannot fire on dictated prose that
  happens to contain the words.

**Known ceiling** (leave it as a `ponytail:` comment): keystroke-injected text
in an app with per-character undo needs more than one press. The mitigation is a
settings note recommending paste injection when undo matters, not more code.

**Check:** unit test that `LastDelivery` is recorded on the inject path and
cleared by undo. The chord itself is platform code and gets the same treatment
as `send_paste` — argument-shape test for Linux, manual verification elsewhere.

---

### 10.2 Retry last utterance on a stronger model

**The problem:** the fast local model mishears one word. Today the recovery is
to say the whole sentence again — and the second attempt is transcribed by the
same model that just got it wrong.

**Mechanism:** keep the last utterance's PCM in memory and re-run it through a
different provider on request. No re-speaking, no new audio path.

**Required work:**
- Retain the finished utterance. `default_transcribe_stream`
  (`core/asr/mod.rs`) already owns the complete buffer at the moment it calls
  `transcribe_utterance` — hand a clone to `AppState.last_utterance`
  (`Mutex<Option<Vec<f32>>>`). 16 kHz f32 mono is ~64 KB/s, so a 30-second cap
  is ~2 MB. Cap it; do not retain unbounded.
- Setting `retry_provider` — which provider the retry uses, defaulting to the
  largest *local* model installed. Cloud is selectable but never the default.
- `retry_last` IPC command plus hotkey: undo (10.1) → re-transcribe → deliver
  through the same `injection::deliver` path, so the dictionary and
  smart-spacing still apply.
- The retained buffer is audio of the user speaking. It lives in memory only,
  never touches disk, and is dropped on `stop_recording` when retry is
  disabled. Say exactly that in the settings UI next to the toggle.

**Check:** a test that retry re-runs the retained buffer through the configured
retry provider and not the primary one.

---

### 10.3 Context-aware decoder prompting

Whisper's `initial_prompt` biases the decoder toward the vocabulary it should
expect. Echo already uses it — `whisper_cli.rs:39` and `whisper_server.rs:111`
both take a prompt built from `DictionaryEngine::prompt_terms`. Two gaps, both
small:

**(a) Per-app dictionary terms never reach the decoder.**
`whisper_cli::initial_prompt` calls `prompt_terms(None)`, so profile-scoped
entries — the entire point of per-app dictionaries — are filtered out at
`core/dictionary/mod.rs:70` before the prompt is built.

The tension is real and worth stating: the profile is deliberately resolved at
*injection* time (see the `appcontext/mod.rs` header) because focus can move
while you talk, but the prompt is needed at *decode* time, before a transcript
exists. Resolve it by sampling `foreground_app()` once at recording start and
using that for the prompt only. A prompt is a hint — being wrong costs a weaker
prompt, never a wrong transcript — while delivery keeps its injection-time
resolution. Both whisper front-ends must get the same change or they drift, per
the `decode_opts.rs` header.

**(b) Use the previous transcript as context.**
`initial_prompt` is designed to take the *preceding text*, which is exactly what
`transcription_history` already stores. Feeding the last transcript from the
same app into the prompt makes continued dictation carry its own context: names
and terminology from sentence one bias sentence two. Cost is one indexed query
per utterance — no clipboard, no platform code, nothing new on disk. Bound it to
the existing `MAX_PROMPT_CHARS` budget and to recent history; a transcript from
yesterday is not context.

**Deliberately not doing:** reading the focused app's selection via
`copy_selection` as prompt context. It works, but it fires a `Ctrl+C` at another
app and round-trips the user's clipboard on *every* utterance, and that cost is
paid whether or not the selection is relevant. Revisit as a per-profile opt-in
if (a) and (b) prove insufficient.

**Check:** `prompt_terms` already has coverage — add a case asserting a
profile-scoped entry appears in the prompt when that profile is active, and does
not when it is not.

---

### 10.4 Live streaming injection

**The problem:** text appears only after you stop talking.
`echo://transcript-partial` is already emitted (`commands/recording.rs:268`) but
goes only to Echo's own UI; the focused app sees nothing until the final
segment.

**Why this is the hard one.** Injecting partials means editing text inside
someone else's app: each new partial has to remove the previous one and write a
longer version. That is a delete-and-rewrite loop in a text field Echo does not
own, competing with the user's own typing, with autocorrect, and with editor
autocomplete that rewrites what was just inserted. The failure mode is not
"looks wrong", it is deleting text the user typed.

**Constraints that make it shippable:**
- **Opt-in per app profile**, never global. Default off.
- **Keystroke injection only.** Paste-mode partials would thrash the clipboard
  dozens of times per sentence and lose the user's clipboard contents on any
  interruption.
- **Only for providers that genuinely stream.** Under the buffered
  `default_transcribe_stream`, partials arrive one utterance at a time anyway —
  there is nothing to stream, so the feature would carry all of the risk and
  none of the benefit. Gate on `supports_streaming()`.
- **Abandon on interference.** If anything about the field changes between
  partials that Echo did not cause, stop injecting partials for the rest of the
  utterance and fall back to delivering the final transcript. Detecting that
  cheaply is the open design question — settle it before writing code, not
  during.
- Needs the backspace primitive that 10.1 rejected for undo. Here the rewrite
  knows exactly how many characters it wrote and nothing else has legitimately
  intervened, which is precisely the condition that makes counting safe in this
  case and unsafe in that one.

**Check:** the partial-diff logic (previous partial → the backspaces and
keystrokes that reach the next one) is pure string work. Test it directly,
including the case where a partial gets *shorter*.

---

### 10.5 Nothing is lost when injection fails

**(a) File import already existed** — see the note at the top of this phase.
No work; the README needs the feature listed, not the code.

**(b) A failed injection no longer loses the words.** `rescue_to_clipboard`
(`commands/recording.rs`) puts the transcript on the clipboard and emits the
reason as an error the UI shows. History always had it, but nobody watching
their cursor not move thinks to go and look there.

Deliberately *not* changed: dictating into Echo's own window. It looks like a
case worth intercepting and is not — typing a phrase into the dictionary field
by voice is a real workflow, and routing it elsewhere would break it.

---

### Not planned, and why

- **Backspace-count undo as the primary mechanism** — see the table in 10.1.
- **Selection-as-prompt-context on every utterance** — see 10.3.
- **Non-WAV file import** — see 10.5(a).
- **Cloud LLM as anything's default** — every LLM-shaped item here (spoken
  commands, retry, formatting) follows command mode's existing shape: local
  first, cloud opt-in, key from the keychain.

---

## Phase 11 — Formatting and Field Awareness ✅

Phase 10 fixed a transcript after it landed. This one improves the transcript
itself, and stops it landing where it must not.

| # | Item | Where |
|---|---|---|
| 1 | Spoken punctuation | `core/format/punctuation.rs` |
| 2 | Never type into a password field | `core/field.rs` |
| 3 | Auto-pick the injection method | `injection::use_paste_for` |
| 4 | Field-aware behaviour | `core/field.rs` + per-app `formatting` column |
| 5 | CLI / stdin mode | `cli.rs` |
| 6 | Numbers, times, dates, units | `core/format/numbers.rs` |
| 7 | Snippets | multi-line injection + a textarea |

### 11.1 The formatting pass (items 1, 6, and the tidy-up)

`core::format` runs after the dictionary and before injection, in three
switchable stages. Order is not arbitrary: punctuation introduces marks,
numbers works on the resulting word stream, tidy exists to clean up after both.

**The hard problem is ambiguity, not mapping.** "Period" is a span of time,
"colon" is an organ. A naive replacement turns "a period of time" into "a . of
time" on every utterance, forever. Two rules carry it:

- a determiner in front means it is a noun — "a period", "the colon"
- "of" behind means the same — "period of time"

Multi-word phrases ("new paragraph", "question mark") skip the check, because
nobody says them meaning anything else. This is a heuristic and is documented as
one; the upgrade path is the standard prefix word ("press period"), which costs
a word on every mark, which is why it is not the default. **Spoken punctuation
ships off by default** — it takes words out of the language, and that is the
user's trade to accept.

Numbers follow one rule that keeps them safe: **never convert a lone small
word.** "One of the best" must survive. So digits appear when the speaker gave
more than one word of the number ("twenty five"), or when a unit settles it
("five percent"). Times, spoken years and currency get their own shapes —
"twenty twenty six" is 2026, not 2026 added up wrongly.

Per-app profiles switch the whole pass, not individual stages: "leave my
terminal alone" is one decision, and migration 4 adds `app_profiles.formatting`
for it.

### 11.2 Password fields (items 2 and 4)

`core::field` asks the accessibility layer one question: is the focused control
masked? Windows UI Automation exposes `IsPassword`, macOS exposes the
`AXSecureTextField` subrole. Both cover browsers and Electron, which is where
password fields actually are — Win32 window styles would not.

The check runs **before the dictionary, before History and before injection**,
so a password spoken into a masked box is neither typed there nor written to
disk on the way past. It also gates partial injection at recording start.

**Linux cannot answer, and the UI says so rather than showing a switch that does
nothing.** AT-SPI needs a D-Bus dependency and a toolkit that publishes its
tree; under Wayland, often neither holds. `detection_available()` reports the
truth per platform. A protection you wrongly believe in is worse than none.

`Unknown` is deliberately treated as "not secure": refusing to type whenever the
OS stays quiet would break dictation on all of Linux and in every app with no
accessibility tree — a far larger blast radius than the case being guarded.

**What item 4 turned out to be.** The accessibility APIs answer "is this
masked", not "is this code". Everything else app-shaped — no auto-punctuation in
an editor, prose capitals in an email — comes from the per-app profile, which
already knows the app. So field awareness is the password guard, and the rest of
item 4 is the `formatting` override in 11.1.

### 11.3 Injection method (item 3) and snippets (item 7)

`injection_method` gains `"auto"`: paste anything with a line break or longer
than ~160 characters, type the rest. **Unset still means "type"** — auto is an
explicit choice, not a silent change of behaviour for existing users.

Line breaks are why this matters, and they are also item 7. A newline is not a
character you can type: on Windows a Unicode scan code for it is silently
dropped, and on macOS it is accepted by some text views and not others. Both
injectors now send Return as a real key press between runs of text, so a
multi-line snippet arrives as a snippet. The dictionary's replacement field
became a textarea to match — the replacement engine always allowed a block, the
input never did.

### 11.4 CLI (item 5)

`echo --transcribe <file> [--language xx]` prints the transcript to stdout and
exits; errors go to stderr with a non-zero status, so `$(...)` is the transcript
and never a log line. It runs inside `setup` for the same reason `--selftest`
does: the database, model and engine only exist once setup has run.

---

## Phase 12 — Streaming the Local Engine ✅

Live text shipped in Phase 10 gated on `supports_streaming()`, and only Deepgram
answered true. So the feature could not reach the offline engine — the default,
and the reason people install Echo. This closes that.

**whisper has no incremental mode.** It decodes a buffer and returns a
transcript, so a "partial" is the whole utterance-so-far decoded again. That is
what whisper.cpp's own streaming example does, and the waste is real. Three
rules bound it, each guarding a specific failure (`should_decode_partial`):

- **Enough new speech.** ~0.8s, roughly a spoken phrase. Below it the decode
  costs more than the words are worth; far above it the text lags the voice.
- **One decode in flight.** A queue of them would fall further behind the
  speaker with every one it started.
- **A ceiling of 20 seconds.** Each partial re-decodes everything said so far,
  so cost grows with the square of the utterance. Someone still going at twenty
  seconds is monologuing, not dictating — partials stop, the final still
  arrives whole.

**The decode is polled, never awaited, inside the receive loop.** Awaiting it
would stop reading audio for the length of a decode and the capture layer would
start dropping chunks — losing the user's words to make the display prettier,
which is the wrong trade in every case. The job is spawned with owned copies of
what it needs (all `Arc` or small) rather than borrowing the provider inside a
`select!`, which works and is much harder to read.

**Nobody pays for this unless they use it.** `AsrProvider::set_partials_wanted`
is new, additive, and defaulted, so an existing plugin keeps compiling; the
pipeline calls it before each session. With live text off, the local provider
runs exactly the buffered path it always did.

Also corrected here: the status table said Deepgram was HTTP-only. It has
streamed over WebSocket since Phase 5.

---

## Phase 13 — Measurement, and Languages ✅

Two of the four remaining gaps: the Phase 9.2 targets had never been measured,
and the formatting pass only existed in English.

### 13.1 The 9.2 numbers, finally taken

`core::procinfo` reports resident memory and time-since-process-start;
`--benchmark` prints both against the targets. See 9.2 for the measurements.

**Resident, not virtual.** Virtual size counts address space the process
reserved and may never touch — a model mapped but unread shows up there and
misleads. Resident is what the machine is actually handing over, which is what
"uses 400 MB" means to a person.

**Two honest caveats, both stated in the output itself:**

- *Startup* is process start to the end of Tauri's `setup`, not time to first
  pixel — Rust cannot see the paint. It is everything Echo controls, which is
  the part a change to Echo could make faster.
- *Memory* is **this process only**. The resident model lives in a
  `whisper-server` child, so the machine-wide figure is higher. Reporting one
  number as if it were the whole footprint would be the more flattering lie.

The first run of this reported a 26-second startup, because `since_start()` was
read at the *end* of `measure()` — inside `setup`, so it timed the benchmark's
own decodes. It now reads before any measurement runs. Worth recording: the
instrumentation was wrong in the direction that makes the product look bad,
which is the direction you notice. The opposite bug ships.

### 13.2 The latency target is not met on CPU

The engine numbers are the finding. On this machine (CPU only, `base.en`, 3s
clip), a warm resident server takes **~1.85s** per utterance and the cold CLI
~2.19s. The 9.2 target was "< 300ms perceived". That is not close, and no
amount of tuning in Echo closes it — the time is inside whisper's encoder,
which pads every clip under 30 seconds to a full window.

What actually closes it is a GPU pack (already built, Phase 8) or a smaller
model. So 9.2's latency row is now recorded as **met only with acceleration**,
with the CPU figure beside it, rather than left as an unmeasured aspiration.

**This bears directly on Phase 12.** Live text asks for a partial every ~0.8s
of speech, and each one costs a full re-decode. On CPU-only hardware that is
~1.85s, so the "one decode in flight" rule does the work it was written for:
partials arrive whenever the last one finished — roughly every two seconds
instead of every 0.8. It degrades rather than breaking, which is the behaviour
that rule exists to produce, but the experience on CPU-only is materially worse
than on a GPU and Settings now says so.

### 13.3 Spoken punctuation in seven languages

`punctuation.rs` is now a set of per-language tables — English, Spanish, French,
German, Italian, Portuguese, Dutch — selected by the language the *decoder*
reports, falling back to the configured one. Auto-detect therefore works.

**A language with no table does nothing**, rather than falling back to English.
Applying English rules to a French sentence is how "point" fires in the middle
of ordinary prose. `supported_languages()` reports which languages have tables,
and Settings prints the list — a speaker of one that is missing would otherwise
dictate "coma", get nothing, and reasonably conclude the feature is broken.

The missing languages — Russian, Ukrainian, Turkish, Arabic, Hindi, Chinese,
Japanese, Korean — are missing because writing their tables without a speaker to
check them would be guessing. Note the asymmetry that makes this safe to ship
incrementally: **an unidiomatic phrase never matches, so it costs nothing; a
missing determiner is a false positive on every utterance.** That is why the
determiner lists are the longest part of each table.

French gets one structural exception: it sets `? ! ; :` off with a space, so
`tidy` must not strip it. Everything else in that stage is language-neutral.

### 13.4 Numbers stay English

Number words are grammar, not a word list — "quatre-vingt-dix-sept",
"einundzwanzig". Another language is a parser of its own, not another table, so
`numbers::covers()` claims English only and the stage is skipped elsewhere. A
half-right conversion is worse than none, because the reader cannot tell it was
Echo that changed the figure.

---

## Key Architectural Rules (do not violate)

1. `AppState.db` is `Mutex<Connection>` — never share `Connection` across threads.
2. Always drop `MutexGuard` before `.await` in async Tauri commands (prevents Send bound violations).
3. All Tauri event names use the `echo://` prefix.
4. `AsrProvider::transcribe` and `transcribe_stream` must be `spawn_blocking`-safe for CPU-bound work.
5. Never return raw API keys to the frontend — only return whether a key is set.
6. Dictionary engine in `state.dictionary` must be updated (via `write().await`) any time the DB entries change.
7. Windows injection uses UTF-16 encoding — always iterate `text.encode_utf16()` not byte chars.
8. VAD runs in the audio capture task, not in the ASR task — keeps latency stages separate.

---

## Dependency Reference

```toml
# Already in Cargo.toml
tauri = "2"
tauri-plugin-opener = "2"
tauri-plugin-global-shortcut = "2"
tauri-plugin-notification = "2"
tauri-plugin-store = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
rusqlite = { version = "0.31", features = ["bundled"] }
cpal = "0.15"
ringbuf = "0.3"
thiserror = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
async-trait = "0.1"
windows = { version = "0.58", features = ["Win32_UI_Input_KeyboardAndMouse"] }  # Windows only

# To add in Phase 2
whisper-rs = "0.13"
reqwest = { version = "0.12", features = ["stream", "rustls-tls"] }
hound = "3.5"

# To add in Phase 1 (Silero)
ort = { version = "2", features = ["download-binaries"] }
ndarray = "0.16"

# To add in Phase 3 (macOS)
core-graphics = "0.24"
core-foundation = "0.10"

# To add in Phase 4 (file dialogs)
tauri-plugin-dialog = "2"

# To add in Phase 5
keyring = "3"
tokio-tungstenite = "0.26"

# To add in Phase 7 (plugins)
libloading = "0.8"
```

---

## Quick Start for a New Session

1. Open `C:\Users\PRENEEL\Documents\Vedant Nimbarte\Echo\echo-app\`
2. Run `cargo check` in `src-tauri\` to confirm Rust compiles.
3. Run `npx tsc --noEmit` in `echo-app\` to confirm TypeScript compiles.
4. Check this file for current phase status and pick up where the last session left off.
5. Run `npm run tauri dev` to launch the app in dev mode.
