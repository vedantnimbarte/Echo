# Echo — Universal Voice Keyboard

Echo is a privacy-first, cross-platform **voice keyboard**: press a hotkey, speak,
and Echo transcribes your speech and types it into whatever app is focused.
Transcription runs **locally** (Whisper) or via **cloud providers** (OpenAI,
Groq, Deepgram) — your choice.

Built with **Rust · Tauri v2 · React 19 · TypeScript · TailwindCSS v4 · SQLite**.

---

## Features

- 🎙️ **Live capture** with device selection and voice-activity detection (VAD)
- 🧠 **Local transcription** via Whisper (whisper.cpp) — fully offline
- ☁️ **Cloud transcription** via OpenAI Whisper, Groq, or Deepgram
- ⌨️ **Text injection** into the focused app — type keystrokes *or* clipboard-paste
- 📖 **Custom dictionary** with replacements, enable/disable, JSON import/export
- ⚡ **Global hotkey** to toggle recording from anywhere
- 🔄 **Auto-update** from GitHub Releases (signed)
- 🧩 **Plugin system** (experimental) for custom ASR / output / audio / dictionary
- 📊 **Local-only telemetry**, opt-in, viewable and deletable — nothing leaves your device
- 🕘 **History** of past transcriptions
- 🔐 **API keys stored in the OS keychain**, never in plain files

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
├─ docs/                    # RELEASING.md, BUNDLING.md
├─ plan.md                  # phase-by-phase implementation plan & status
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
incremental and fast. Two windows exist: a floating **pill** (always-on-top
recorder) and the **Settings** window.

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

### Cloud provider (no native build needed)

In **Settings → Cloud provider API keys**, paste a key and click **Save** (stored
in your OS keychain), then choose the provider under **ASR Provider**:

- OpenAI: https://platform.openai.com/api-keys
- Groq: https://console.groq.com/keys
- Deepgram: https://console.deepgram.com/ *(currently HTTP; streaming WS planned)*

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

## Global hotkey

Default is **`Ctrl/Cmd + Shift + Space`** to toggle recording. Change it in
**Settings → Global hotkey** using Tauri accelerator syntax (e.g.
`CommandOrControl+Alt+E`). If the hotkey doesn't fire, another app may already
own that combination — pick a different one.

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

## Status

See [`plan.md`](plan.md) for the full phase breakdown. Summary:

| Phase | Area | Status |
|---|---|---|
| 0 | Foundation | ✅ |
| 1 | Audio pipeline (VAD, device select) | ✅ |
| 2 | Local ASR (Whisper) | ✅ (`whisper-cli` default; `--features whisper` optional) |
| 3 | Text injection (Win/macOS/Linux) | ✅ (type + paste) |
| 4 | Dictionaries | ✅ |
| 5 | Cloud ASR (OpenAI/Groq/Deepgram) | ✅ (Deepgram HTTP; WS planned) |
| 6 | Telemetry | ✅ |
| 7 | Plugin system | ✅ |
| 8 | Packaging | ✅ (offline engine bundled in CI; OS code-signing TBD) |
| 9 | v1 launch (hotkey, CSP, auto-update, docs) | ✅ (OS code-signing TBD) |

---

## Docs

- [CONTRIBUTING.md](CONTRIBUTING.md) — dev setup & architecture
- [PLUGINS.md](PLUGINS.md) — plugin manifest & SDK contract
- [RELEASING.md](docs/RELEASING.md) — cutting releases & auto-update signing
- [BUNDLING.md](docs/BUNDLING.md) — staging the offline Whisper engine
- [plan.md](plan.md) — implementation plan

## License

TBD.
