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
| 5 | Cloud ASR Providers | ✅ Complete (Deepgram via HTTP, not WS yet) |
| 6 | Telemetry | ✅ Complete |
| 7 | Plugin System | ✅ Core complete (SDK crate deferred) |
| 8 | Packaging | ✅ Config + CI (signing certs TBD) |
| 9 | v1 Launch | ✅ Hotkey + CSP + docs (perf/signing TBD) |

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

**Done:** plugin traits + `PluginContext` (7.1), `PluginManifest`/`plugin.json` schema (7.2), `PluginLoader` via libloading (7.3), commands `list_plugins`/`install_plugin`/`enable_plugin`/`disable_plugin`/`uninstall_plugin` with DB+disk registry and startup loading (7.4), and the `PluginsPanel` UI + tab (7.6). **Deferred:** the standalone `echo-sdk` proc-macro crate (7.5) — it requires extracting the plugin API into a shared crate; the loader's `echo_plugin_create` contract is documented in `loader.rs` so plugins can be written against the traits today.

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

| Metric | Target | How to measure |
|---|---|---|
| Startup time | < 2 seconds | Tauri `setup` → first window paint |
| Transcription latency | < 300ms perceived | `recording_started` → first `transcript_partial` |
| Memory at idle | < 100 MB | Task Manager / `heaptrack` |
| Memory during transcription | < 500 MB | Includes Whisper model loaded |

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
