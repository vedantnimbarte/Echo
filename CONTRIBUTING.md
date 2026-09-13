# Contributing to Echo

Thanks for your interest in Echo. This covers the repository layout, dev setup,
and a quick architecture tour. For what Echo *is* and how to use it, see the
[README](README.md).

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

## Prerequisites

- **Rust** (stable) + Cargo — https://rustup.rs
- **Node.js 20+** and npm — https://nodejs.org
- **Tauri v2 system dependencies** — the per-OS setup below covers these; the
  canonical list is at https://tauri.app/start/prerequisites/

## Per-OS setup

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
   [Text injection](README.md#text-injection-per-os)). In dev the app that needs
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

## Clone and run

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

## First run

Out of the box the app runs, but **transcription is `none`** until you pick a
backend — the onboarding wizard walks you through mic → engine → permissions →
hotkey. See [Transcription backends](README.md#transcription-backends) in the README.

## Checks before pushing

```bash
# Rust
cd echo-app/src-tauri
cargo check
cargo test
cargo fmt --check
cargo clippy

# TypeScript
cd echo-app
npx tsc --noEmit
npm test          # vitest
npm run i18n:check # every translation catalogue against English
```

For local Whisper code, build with the feature: `cargo check --features whisper`
(requires `cmake` and LLVM/`libclang`). The default `whisper-cli` engine needs
neither.

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
(`v*`) triggers the GitHub Actions matrix to build Windows / macOS (arm64 and Intel) /
Linux installers and staple the offline Whisper engine into each. See
[`docs/RELEASING.md`](docs/RELEASING.md) for the release + auto-update setup.

---

## Architecture

Clean-architecture-ish separation in `echo-app/src-tauri/src`:

- `core/audio` — CPAL capture, mono down-mix, 16 kHz resample
- `core/vad` — voice activity detection (Silero neural, energy fallback)
- `core/asr` — `AsrProvider` trait, `AsrManager`, Whisper + cloud providers,
  model download manager, WAV encoding
- `core/dictionary` — phrase-replacement engine
- `core/injection` + `platform/{windows,macos,linux}` — text injection
- `core/telemetry` — local-only usage events
- `core/plugins` — plugin traits + libloading loader
- `storage` — SQLite (rusqlite), repositories, keychain
- `commands` — Tauri IPC command handlers
- `state.rs` — shared `AppState`

Frontend (`echo-app/src`): React 19 + Zustand + TanStack Query, with typed IPC
wrappers in `ipc/`.

### Key rules

1. `AppState.db` is `Mutex<Connection>` — never share the `Connection` across
   threads; drop the guard before `.await`.
2. All Tauri event names use the `echo://` prefix.
3. CPU-bound ASR work runs on `spawn_blocking`.
4. Never return raw API keys to the frontend.
5. VAD runs in the audio capture task, not the ASR task.

## Commit style

Conventional commits (`feat:`, `fix:`, `docs:`, `build:`, `ci:`), small and
focused.
