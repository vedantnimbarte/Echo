# Echo — Universal Voice Keyboard

Echo is a privacy-first, cross-platform **voice keyboard**: press a hotkey, speak,
and Echo transcribes your speech and types it into whatever app is focused.
Transcription runs **locally** (Whisper) or via **cloud providers** — OpenAI,
Groq, Deepgram, Mistral, ElevenLabs, AssemblyAI, Speechmatics, Azure, Google, or
any OpenAI-compatible endpoint you host yourself. Your choice, your keys.

Built with **Rust · Tauri v2 · React 19 · TypeScript · TailwindCSS v4 · SQLite**.

---

## Features

- 🎙️ **Live capture** with device selection and voice-activity detection (VAD)
- 🧠 **Local transcription** via Whisper (whisper.cpp) — fully offline
- ☁️ **Cloud transcription (BYOK)** — ten providers: OpenAI, Groq, Deepgram, Mistral, ElevenLabs, AssemblyAI, Speechmatics, Azure and Google
- 🔌 **Any OpenAI-compatible endpoint** — LiteLLM, OpenRouter, vLLM, or a self-hosted Whisper server on your own machine
- 🎚️ **Pick the model per provider** — free text, so a model released after your copy of Echo still works
- 🧪 **Test a key before you trust it** — one button, caught at entry instead of mid-sentence
- 🛟 **Falls back to offline** if a cloud request fails — and only ever toward more privacy; local never falls back to cloud
- ⌨️ **Text injection** into the focused app — type keystrokes *or* clipboard-paste
- ↩️ **Undo the last insert** with a global hotkey, or by saying "scratch that"
- 🔁 **Retry the last utterance** on a stronger model without saying it again
- ⚡ **Live text** (opt-in, per app) — words appear as you speak them, offline or in the cloud
- 🧹 **Drops “um” and stuttered words** — and, optionally, fixes self-corrections with a local model
- 📊 **Insights** — speaking speed, the fixes Echo made, which apps you dictate into, a streak calendar and an on-device-vs-cloud split, all counted from your own History and never sent anywhere
- ✍️ **Spoken punctuation** (opt-in) — "comma", "new paragraph", "question mark"; English, Spanish, French, German, Italian, Portuguese, Dutch
- 🔢 **Numbers, times and units** written properly (English) — "twenty five" → 25, "five percent" → 5%
- 🔒 **Never types into a password field** (Windows/macOS; Linux can't detect it — see below)
- 🖥️ **Scriptable** — `echo --transcribe recording.mp3` prints to stdout
- 📈 **`echo --benchmark`** — measures your machine rather than promising numbers
- 📁 **Transcribe a file** you already have — wav, mp3, ogg or flac, offline
- 📖 **Custom dictionary** with replacements, enable/disable, JSON import/export — biases the decoder offline *and* in the cloud
- 🗂️ **Per-app profiles** — override insert behaviour and dictionary scope per application
- 🌍 **Language selection** — pin a dictation language or let Whisper auto-detect
- ⚡ **Global hotkey** to toggle recording from anywhere
- 📌 **Lives in the tray** — notification area on Windows, menu bar on macOS, status area on Linux; click it for Settings or to quit
- 🚀 **Starts at login** (opt-in) — a hotkey can only answer if Echo is already running
- 🗣️ **Wake word** (opt-in) — say a phrase to start dictating hands-free, matched on-device
- 🤖 **Command mode** (opt-in) — say a trigger word to rewrite the selection via a local LLM
- 🔄 **Auto-update** from GitHub Releases (signed)
- 🧩 **Plugin system** (experimental) for custom ASR / output / audio / dictionary
- 📊 **Local-only telemetry**, opt-in, viewable and deletable — nothing leaves your device
- 🕘 **History** of past transcriptions, searchable and exportable to JSON
- 🔎 **Request log** — see every outbound request Echo made, and whether your setup is offline-capable
- 🔐 **API keys stored in the OS keychain**, never in plain files

---

## Installing

Download from [**Releases**](https://github.com/vedantnimbarte/Echo/releases), or:

**macOS / Linux**

```sh
curl -fsSL https://raw.githubusercontent.com/vedantnimbarte/Echo/main/scripts/install.sh | sh
```

**Windows** (PowerShell)

```powershell
irm https://raw.githubusercontent.com/vedantnimbarte/Echo/main/scripts/install.ps1 | iex
```

Both scripts fetch the latest release, verify it against the published
`SHA256SUMS.txt`, and install it. Pin a version with `ECHO_VERSION=v0.1.0` (sh)
or `-Version v0.1.0` (PowerShell). Read them first if you'd rather not pipe a
script into a shell — that's a reasonable instinct, and they're short.

### Echo is not code-signed yet

Signing certificates cost money Echo hasn't spent yet, so your OS will warn you.
This is expected, and here is exactly what you'll see:

| OS | What happens | What to do |
|---|---|---|
| **Windows** | SmartScreen: *"Windows protected your PC"* | **More info** → **Run anyway** |
| **macOS** | Gatekeeper refuses to open it, offering only *Move to Trash* | The install script clears the quarantine flag for you. Installing by hand: right-click Echo in Applications → **Open** → **Open** |
| **Linux** | Nothing — no signing gate | — |

If that trade isn't one you want to make, [build from source](#running-locally)
instead: the result is identical and you compiled it yourself.

### The password-field guard is unverified on real hardware

Echo asks Windows UI Automation, or the macOS Accessibility API, whether the
focused control is masked, and refuses to type into it. That code compiles on
both platforms in CI but **has never been exercised against a real password
box**, so treat it as a seatbelt of unknown strength rather than a guarantee.
On Linux it does nothing at all: the question needs AT-SPI over D-Bus, and
under Wayland usually not even that answers. Settings says so plainly.

### Support tiers — what has actually been run

Echo's CI compiles and tests every platform, and the Rust suite plus a real
startup self-test run on Linux and macOS runners. That is not the same as
somebody dictating into twenty applications, so here is the honest state:

| Platform | Tier | What that means |
|---|---|---|
| **Windows x64** | Tested | Developed and used here. Text injection, the password-field guard, the tray, offline Whisper and the GPU pack have all been exercised by hand. |
| **macOS arm64** | Community | Compiles, unit-tests and self-tests in CI on a macOS runner, but has not been driven by hand. Accessibility and Automation permissions, and the password-field guard, are unverified against real applications. Bug reports welcome and expected. |
| **macOS x86_64** | Unsupported | No build exists. `ort` ships no prebuilt ONNX Runtime for Intel macOS, so Silero VAD and the wake word cannot link. See [docs/RELEASING.md](docs/RELEASING.md). |
| **Linux X11** | Community | Needs `xdotool`. No password-field detection on any Linux — the question needs AT-SPI over D-Bus. |
| **Linux Wayland** | Degraded | Needs `ydotool` plus the `ydotoold` daemon, and some compositors refuse synthetic input outright. Per-app profiles do not work: no Wayland protocol reports which window is focused. |

If you use Echo on a Community-tier platform and it works, saying so is a
genuinely useful contribution — the gap is verification, not code.

### Per-OS notes

- **Windows** — needs the WebView2 runtime (preinstalled on Win11; on Win10 grab
  the *Evergreen* runtime from Microsoft).
- **macOS** — **Apple Silicon only.** The ONNX Runtime behind Silero VAD and
  wake word publishes no Intel-macOS binaries, so there is no x86_64 build; an
  Intel Mac would have to compile ONNX Runtime from source. Grant **Microphone**
  and **Accessibility** permissions on first run, or Echo can hear you but can't
  type.
- **Tray icon** — Windows puts it in the notification area (possibly behind the
  overflow arrow, where you can drag it out), macOS in the menu bar, Linux in
  whatever status area the desktop provides. A few minimal Linux desktops have
  none at all; Echo logs a warning and runs without it, reachable through the
  pill and the global hotkey.
- **Linux** — the AppImage needs FUSE (`sudo apt install libfuse2` on
  Debian/Ubuntu), and text injection needs `xdotool` (X11) or `ydotool`
  (Wayland). A `.deb` is also attached to each release.

### Updating

Auto-update is built in but **switched off** until release signing is set up
(see [docs/RELEASING.md](docs/RELEASING.md)). Until then, re-run the install
command above to upgrade.

---

## Project structure

```
Echo/
├─ echo-app/                # the Tauri application
│  ├─ src/                  # React frontend (components, hooks, ipc wrappers)
│  ├─ scripts/              # build helpers (stage-runtime-deps.mjs)
│  └─ src-tauri/            # Rust backend
│     ├─ src/core/          # audio, asr, vad, dictionary, injection, telemetry, plugins
│     ├─ src/storage/       # SQLite, repositories, keychain
│     ├─ src/commands/      # Tauri IPC commands
│     ├─ src/platform/      # per-OS text injection
│     ├─ capabilities/      # Tauri permission capabilities
│     └─ resources/bin/     # bundled whisper-cli lands here at package time
├─ packaging/               # winget / homebrew / flatpak / snap manifests
├─ .github/workflows/       # CI + release matrix
├─ docs/                    # RELEASING.md, BUNDLING.md, WAKE_WORD.md
├─ CONTRIBUTING.md          # dev setup + architecture
└─ PLUGINS.md               # plugin manifest + SDK contract
```

---

## Running locally

### 1. Common prerequisites (all platforms)

- **Rust** (stable) + Cargo — https://rustup.rs
- **Node.js 20+** and npm — https://nodejs.org
- **Tauri v2 system dependencies** — the per-OS setup below covers these; the
  canonical list is at https://tauri.app/start/prerequisites/

### 2. Per-OS setup

<details open>
<summary><b>Windows</b></summary>

1. **Visual Studio C++ Build Tools** — install the *"Desktop development with
   C++"* workload (includes the MSVC compiler). https://visualstudio.microsoft.com/downloads/
2. **WebView2 Runtime** — preinstalled on Windows 11; on Windows 10 install the
   *Evergreen* runtime: https://developer.microsoft.com/microsoft-edge/webview2/
3. That's it — text injection (SendInput) and the offline `whisper-cli` both work
   with no extra tools (the Whisper engine auto-downloads on first run).

</details>

<details>
<summary><b>macOS</b></summary>

1. **Xcode Command Line Tools:**
   ```bash
   xcode-select --install
   ```
2. **Offline Whisper engine** — in dev, macOS needs `whisper-cli` on your `PATH`
   (release installers bundle it for you):
   ```bash
   brew install whisper-cpp        # provides `whisper-cli`
   ```
3. **Accessibility permission** — required so Echo can type into other apps.
   Grant it under System Settings → Privacy & Security → Accessibility (see
   [Text injection](#text-injection-per-os)). In dev the app that needs
   permission is your terminal; for an installed build it's Echo itself.

</details>

<details>
<summary><b>Linux (Debian/Ubuntu)</b></summary>

1. **Tauri + audio system libraries:**
   ```bash
   sudo apt update
   sudo apt install -y \
     libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf \
     libasound2-dev build-essential curl wget file cmake
   ```
   (Fedora/Arch equivalents: see the Tauri prerequisites page.)
2. **Text-injection tool** — pick one for your display server:
   ```bash
   sudo apt install -y xdotool     # X11
   sudo apt install -y ydotool     # Wayland — also needs the ydotoold daemon running
   ```
3. **Offline Whisper engine** — in dev, Linux needs `whisper-cli` on your `PATH`
   (release installers bundle it). Install `whisper.cpp` from your package
   manager or build it, ensuring a `whisper-cli` binary is on `PATH`.

</details>

### 3. Clone and run

```bash
git clone git@github.com:vedantnimbarte/Echo.git
cd Echo/echo-app
npm install
npm run tauri dev
```

This launches the desktop app in development mode. **The first Rust build
compiles all dependencies and can take several minutes**; subsequent runs are
incremental and fast. Three surfaces exist: a floating **pill** (always-on-top
recorder), the **Settings** window, and a **tray icon**. The pill has no window
chrome and stays out of the taskbar, so the tray is how you reach Settings or
quit once the pill is dismissed.

### 4. First-run configuration

Out of the box the app runs, but **transcription is `none`** until you pick a
backend — the onboarding wizard walks you through mic → engine → permissions →
hotkey. See [Transcription backends](#transcription-backends) below.

---

## Transcription backends

### Local Whisper (offline, default) — no build toolchain needed

The default local engine shells out to a bundled **`whisper-cli`** (whisper.cpp).
It needs **no** cmake/libclang at build time:

- **Windows** — the binary auto-downloads on first run (Settings → *Set up local
  Whisper*, or the onboarding "Transcription" step).
- **macOS / Linux (dev)** — provide `whisper-cli` on your `PATH`
  (`brew install whisper-cpp`, or your distro's whisper.cpp package). Release
  installers bundle it, so end users need nothing.

Then in the app: **Settings → Local Whisper models → Download** a model
(`tiny` / `base` / `small` / `medium`) and click **Use**. Models are saved under
the app data directory (see [Where things live](#where-things-live)).

> **Advanced — in-process Whisper.** An optional Cargo feature compiles
> whisper.cpp *into* the binary instead of shelling out. It needs `cmake` + LLVM
> `libclang` and is off by default:
> ```bash
> # Linux:   sudo apt install clang libclang-dev cmake
> # macOS:   brew install llvm
> # Windows: winget install LLVM.LLVM  &&  setx LIBCLANG_PATH "C:\Program Files\LLVM\bin"
> npm run tauri dev -- --features whisper
> ```

### Cloud providers (no native build needed)

In **Settings → Engine**, choose **A cloud provider**, then open the provider you
want. Paste a key and click **Save key** (it goes to your OS keychain), **Test
the key** to check it before you rely on it, and **Dictate with …** to send audio
there instead of to the offline engine.

| Provider | Get a key | Notes |
|---|---|---|
| **Groq** | [console.groq.com](https://console.groq.com/keys) | Usually the fastest. `whisper-large-v3-turbo` is cheaper still. |
| **OpenAI** | [platform.openai.com](https://platform.openai.com/api-keys) | `gpt-4o-mini-transcribe` beats `whisper-1` on both price and accuracy. 25 MB per request. |
| **Deepgram** | [console.deepgram.com](https://console.deepgram.com/) | The only one with **live streaming** — words appear as you speak. |
| **Mistral** | [console.mistral.ai](https://console.mistral.ai/api-keys) | Voxtral, around $0.18 per hour of audio. |
| **ElevenLabs** | [elevenlabs.io](https://elevenlabs.io/app/settings/api-keys) | Scribe; strong accuracy across 99 languages. |
| **AssemblyAI** | [assemblyai.com](https://www.assemblyai.com/app/account) | Uploads and queues — expect a few seconds even for a short phrase. |
| **Speechmatics** | [portal.speechmatics.com](https://portal.speechmatics.com/) | Strong multilingual. Also queues and polls. |
| **Azure AI Speech** | [portal.azure.com](https://portal.azure.com/) | Needs your resource **region** (e.g. `westeurope`) as well as a key. |
| **Google STT** | [console.cloud.google.com](https://console.cloud.google.com/apis/credentials) | Max 60 seconds. Google requires the key in the URL, so it can appear in proxy logs. |
| **Custom** | — | Any OpenAI-compatible endpoint — see below. |

Each provider has a **Model** field, pre-filled with a sensible default and
offering suggestions. It's free text, so a model released after your copy of Echo
still works — type its name.

Your **custom dictionary biases cloud transcription too**, not just the offline
engine, for the OpenAI-compatible providers.

If a cloud request fails — expired key, dropped wifi, rate limit — Echo retries
once, then falls back to the offline engine so you don't lose the dictation.
**Fallback only ever moves toward more privacy:** local never falls back to cloud,
however it fails.

#### Using a self-hosted or proxied endpoint

Pick **Custom (OpenAI-compatible)**, then fill in the endpoint and model. Anything
speaking OpenAI's `/audio/transcriptions` API works:

| What | Endpoint |
|---|---|
| [Speaches](https://github.com/speaches-ai/speaches) / faster-whisper | `http://localhost:8000/v1` |
| [LiteLLM](https://github.com/BerriAI/litellm) proxy | `http://localhost:4000/v1` |
| vLLM | `http://localhost:8000/v1` |
| OpenRouter | `https://openrouter.ai/api/v1` |
| Azure OpenAI | `https://<resource>.openai.azure.com/openai/deployments/<deployment>` |

A local endpoint keeps your audio on your own machine or network while still
skipping the native Whisper build. **Settings → Request log** shows every host
Echo contacted, so you can confirm where the audio actually went.

---

## Text injection (per OS)

Echo types the transcript into the focused app. Two methods, selectable in
**Settings → Text output → Insert method**:

- **Type keystrokes** (default) — universal, works everywhere.
- **Paste** — puts the text on the clipboard, sends the paste shortcut, then
  restores your clipboard. Faster and more reliable for long transcripts; note
  some apps (e.g. terminals) use a different paste shortcut.

Per-OS requirements:

| OS | Requirement |
|---|---|
| **Windows** | Works out of the box (SendInput). |
| **macOS** | Grant **Accessibility** permission (System Settings → Privacy & Security → Accessibility). Verify with **Settings → Check accessibility permission**. |
| **Linux (X11)** | `xdotool` installed. |
| **Linux (Wayland)** | `ydotool` installed **and** `ydotoold` daemon running. Note: some compositors (e.g. GNOME) restrict synthetic input. |

---

## Per-app profiles

Settings → *Per-app profiles* overrides how Echo behaves in a given app. Every
field can stay on **Global**, which inherits the setting above it — so a profile
can pin one behaviour (never auto-insert into a password manager) without
freezing the rest.

The app is identified by executable name on Windows, bundle id on macOS, and
X11 window class on Linux. *Detect* fills it in from whatever is focused.

Where the platform won't say which window is focused — **Wayland** (no protocol
exposes it), or **macOS without Automation permission** — profiles simply don't
apply and the global settings are used.

## What Echo sends, and where

Settings → *Privacy* shows whether the current configuration can reach the
network at all, plus a log of every outbound request Echo made: the host, why,
and when.

Read the claim precisely. This records **requests Echo itself made**. It is not
proof that nothing else left your machine — a process can't observe its own OS's
traffic, and a native plugin can make requests that never pass through the code
this instruments (see [PLUGINS.md](PLUGINS.md)).

## Global hotkey

Default is **`Ctrl/Cmd + Shift + Space`** to toggle recording. Change it in
**Settings → Global hotkey** using Tauri accelerator syntax (e.g.
`CommandOrControl+Alt+E`). If the hotkey doesn't fire, another app may already
own that combination — pick a different one.

## Tray icon, and starting at login

Echo runs in the background, so it needs somewhere to live. The tray icon —
notification area on Windows, menu bar on macOS, status area on Linux — opens a
menu with **Settings…** and **Quit Echo**. That is the only persistent way back
in: the pill has no window chrome and stays out of the taskbar, and Settings
hides itself once onboarding is done.

A hotkey can only answer if Echo is already running, so **Settings → Dictation →
Starting Echo** has *Start Echo when I log in*. The login item is registered
with your OS rather than recorded in `echo.db` — a registry `Run` key on
Windows, a LaunchAgent on macOS, an XDG autostart entry on Linux — and the
checkbox reads that back every time, so removing the entry with your own tools
switches the toggle off too. On a machine where policy forbids login items,
Settings shows the error it got rather than pretending it worked.

---

## Debugging & troubleshooting

### Where things live

Echo stores everything under its per-user **app data directory** (`com.echo.app`):

| OS | Path |
|---|---|
| **Windows** | `%APPDATA%\com.echo.app\` (`C:\Users\<you>\AppData\Roaming\com.echo.app`) |
| **macOS** | `~/Library/Application Support/com.echo.app/` |
| **Linux** | `~/.local/share/com.echo.app/` |

Inside it:

- `echo.db` — SQLite: settings, history, telemetry, dictionary.
- `models/` — downloaded Whisper models (`ggml-*.bin`).
- `wake/` — downloaded wake-word models (`*.onnx`), only if wake word is enabled.
- `bin/` — the downloaded `whisper-cli` (Windows).
- `plugins/` — installed plugins.

API keys are **not** here — they're in the OS keychain (Keychain on macOS,
Credential Manager on Windows, Secret Service on Linux).

### Logs

The Rust backend logs via `tracing` (the `echo` crate is at `debug` by default).

- **Dev (`npm run tauri dev`)** — logs print to the terminal you launched from.
- **Turn up verbosity** for other crates with `RUST_LOG`:
  ```bash
  RUST_LOG=echo=trace,tauri=debug npm run tauri dev     # macOS/Linux
  set RUST_LOG=echo=trace& npm run tauri dev            # Windows (cmd)
  ```
- **Frontend / WebView** — in a `tauri dev` build, right-click the window →
  *Inspect Element* to open the WebView devtools (console, network, React state).

### Quick checks

```bash
# Frontend type-check
cd echo-app && npx tsc --noEmit

# Backend compile, tests, format, lint
cd echo-app/src-tauri
cargo check
cargo test
cargo fmt --check
cargo clippy
```

### Common issues

| Symptom | Likely cause / fix |
|---|---|
| **App window is blank** | Vite dev server didn't start. Ensure `npm install` ran; check the terminal for the `localhost:1420` dev URL and for JS errors in the WebView devtools. |
| **No microphones listed / silent meter** | Grant OS microphone permission to the app (macOS: Privacy & Security → Microphone). Pick the correct device in Settings; test with the meter. |
| **Transcription does nothing (local)** | `whisper-cli` not found. Windows: run *Set up local Whisper*. macOS/Linux dev: `brew install whisper-cpp` / ensure `whisper-cli` is on `PATH`. Also confirm a model is downloaded and selected. |
| **Cloud transcription fails** | Re-check the API key in Settings (stored in keychain), provider quota, and network. Bump `RUST_LOG` to see the request error. |
| **Text doesn't appear in other apps** | macOS: grant Accessibility. Linux: install `xdotool` (X11) or run `ydotoold` (Wayland). Try switching **Insert method** between Type and Paste. |
| **Paste inserts into the wrong app / not at all** | The focused app may use a non-standard paste shortcut, or focus changed during the insert delay. Switch to **Type keystrokes**, or raise **Insert delay (ms)**. |
| **Hotkey doesn't toggle recording** | Another app owns the shortcut. Change it in Settings → Global hotkey. |
| **Can't find Echo / no way to quit** | Use the tray icon — on Windows it may be hidden behind the notification-area overflow arrow. If your Linux desktop has no status area, check `echo.log` for a tray warning; the global hotkey still works. |
| **Launch at login won't stick** | On a managed machine the login item can be blocked by policy — Settings reports the error it got. Echo reads the state back from the OS, so removing the entry with your own tools turns the toggle off, as it should. |
| **Model download stalls** | Network/proxy issue; delete the partial file in `models/` and retry. |
| **`libclang` / cmake errors at build** | Only the optional `--features whisper` path needs those — omit the feature to use the default `whisper-cli` engine. |

### Reset state

- **Re-run onboarding:** delete the `onboarding_complete` setting (or the whole
  `echo.db`) from the app data directory, then relaunch.
- **Full reset:** quit Echo and delete the app data directory above. API keys in
  the keychain are separate — remove those from your OS keychain tool if needed.

---

## Building installers

```bash
cd echo-app
npm run tauri build
```

> Auto-update artifacts are signed, so a local `tauri build` expects the updater
> key (see [`docs/RELEASING.md`](docs/RELEASING.md)). For a quick unsigned local
> build, set `bundle.createUpdaterArtifacts` to `false` in `tauri.conf.json`.

Artifacts land in `echo-app/src-tauri/target/release/bundle/`. Tagging a release
(`v*`) triggers the GitHub Actions matrix to build Windows / macOS (universal) /
Linux installers and staple the offline Whisper engine into each. See
[`docs/RELEASING.md`](docs/RELEASING.md) for the release + auto-update setup.

---

## Installing (from a release)

Echo auto-updates once installed (it checks GitHub Releases on launch). The
installers are **not yet OS-code-signed**, so the first launch shows a warning
you have to click past — this is expected for an open-source app, not a problem
with the download:

- **macOS** — the `.dmg` is quarantined, so double-clicking may say *"Echo is
  damaged and can't be opened."* Remove the quarantine flag once:
  ```bash
  xattr -cr /Applications/Echo.app
  ```
  Then grant **System Settings → Privacy & Security → Accessibility** so Echo can
  type into other apps.
- **Windows** — SmartScreen shows *"Windows protected your PC."* Click **More
  info → Run anyway**.
- **Linux** — install the `.deb`/`.rpm`/AppImage. For text injection you need
  `xdotool` (X11) or `ydotool` + a running `ydotoold` (Wayland).

---

## Privacy

- Telemetry is **local only** and **opt-in/out** in Settings — events are stored
  in SQLite on your machine and never transmitted. You can view counts and delete
  all data.
- Recorded telemetry never includes audio, transcript text, file paths, or window
  titles — only counts and coarse metadata (e.g. word count).
- API keys live in the OS keychain and are never returned to the UI.
- Cloud transcription sends your audio to the provider you configured; local
  Whisper sends nothing.

---

## Docs

- [CONTRIBUTING.md](CONTRIBUTING.md) — dev setup & architecture
- [PLUGINS.md](PLUGINS.md) — plugin manifest & SDK contract
- [RELEASING.md](docs/RELEASING.md) — cutting releases & auto-update signing
- [BUNDLING.md](docs/BUNDLING.md) — staging the offline Whisper engine
- [WAKE_WORD.md](docs/WAKE_WORD.md) — wake word + command mode, and training a custom phrase

## License

TBD.
