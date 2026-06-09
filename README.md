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
- ⌨️ **Text injection** into the focused app (Windows / macOS / Linux)
- 📖 **Custom dictionary** with replacements, enable/disable, JSON import/export
- ⚡ **Global hotkey** to toggle recording from anywhere
- 🧩 **Plugin system** (experimental) for custom ASR / output / audio / dictionary
- 📊 **Local-only telemetry**, opt-in, viewable and deletable — nothing leaves your device
- 🕘 **History** of past transcriptions
- 🔐 **API keys stored in the OS keychain**, never in plain files

---

## Project structure

```
Echo/
├─ echo-app/                # the Tauri application
│  ├─ src/                  # React frontend
│  └─ src-tauri/            # Rust backend
│     ├─ src/core/          # audio, asr, vad, dictionary, injection, telemetry, plugins
│     ├─ src/storage/       # SQLite, repositories, keychain
│     ├─ src/commands/      # Tauri IPC commands
│     └─ src/platform/      # per-OS text injection
├─ packaging/               # winget / homebrew / flatpak / snap manifests
├─ .github/workflows/       # release CI matrix
├─ plan.md                  # phase-by-phase implementation plan & status
├─ CONTRIBUTING.md          # dev setup + architecture
└─ PLUGINS.md               # plugin manifest + SDK contract
```

---

## Prerequisites

1. **Rust** (stable) — https://rustup.rs
2. **Node.js 20+** and npm
3. **Tauri v2 system dependencies** — https://tauri.app/start/prerequisites/
   - **Windows:** Visual Studio C++ Build Tools + WebView2 runtime
   - **macOS:** Xcode Command Line Tools
   - **Linux:** `libwebkit2gtk-4.1-dev`, `libasound2-dev`, `librsvg2-dev`, `patchelf`

---

## Run locally

```bash
git clone git@github.com:vedantnimbarte/Echo.git
cd Echo/echo-app
npm install
npm run tauri dev
```

This launches the desktop app in development mode. The first Rust build downloads
and compiles dependencies and can take several minutes.

To type-check without running:

```bash
# Frontend
npx tsc --noEmit
# Backend
cd src-tauri && cargo check
```

---

## ⚙️ Local configuration you’ll need to do

Out of the box the app runs, but **transcription is set to `none`** until you
pick a provider. You also need to satisfy a few platform requirements depending
on what you use.

### 1. Choose a transcription backend

**Option A — Local Whisper (offline).**
Local Whisper is compiled behind an **off-by-default Cargo feature** because
`whisper-rs` builds whisper.cpp from source, which needs extra tooling:

- **cmake** (Windows: bundled with VS Build Tools "C++ CMake tools")
- **LLVM / libclang** — set `LIBCLANG_PATH` if not auto-detected
  - Windows: install LLVM (e.g. `winget install LLVM.LLVM`), then
    `setx LIBCLANG_PATH "C:\Program Files\LLVM\bin"`
  - macOS: `brew install llvm` (libclang ships with it)
  - Linux: `sudo apt install clang libclang-dev cmake`

Then run with the feature enabled:

```bash
cd echo-app
npm run tauri dev -- --features whisper          # dev
npm run tauri build -- --features whisper        # release
```

In the app: **Settings → Local Whisper models → Download** (tiny/base/small/medium),
then click **Use**. Models are saved under the app data directory.

> Without the `whisper` feature the app still builds and runs — you just can’t
> select a local model (use a cloud provider instead).

**Option B — Cloud provider (no native build needed).**
In **Settings → Cloud provider API keys**, paste a key for OpenAI, Groq, or
Deepgram and click **Save** (stored in your OS keychain). Then choose it under
**ASR Provider**. Get keys here:

- OpenAI: https://platform.openai.com/api-keys
- Groq: https://console.groq.com/keys
- Deepgram: https://console.deepgram.com/

### 2. Text injection requirements (per OS)

- **Windows:** works out of the box (SendInput).
- **macOS:** grant **Accessibility** permission —
  System Settings → Privacy & Security → Accessibility → enable Echo.
  Use **Settings → Check accessibility permission** to verify.
- **Linux:** install a typing tool —
  - X11: `sudo apt install xdotool`
  - Wayland: install `ydotool` **and** run the `ydotoold` daemon.

You can toggle injection and add a delay in **Settings → Text injection**.

### 3. Global hotkey

Default is **`Ctrl/Cmd + Shift + Space`** to toggle recording. Change it in
**Settings → Global hotkey** (uses Tauri accelerator syntax, e.g.
`CommandOrControl+Alt+E`).

---

## Building installers

```bash
cd echo-app
npm run tauri build
```

Artifacts land in `echo-app/src-tauri/target/release/bundle/`. Tagging a release
(`v*`) triggers the GitHub Actions matrix to build Windows / macOS (universal) /
Linux installers. Code-signing certificates are not yet configured.

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
| 1 | Audio pipeline (VAD, device select) | ✅ (Silero VAD planned) |
| 2 | Local ASR (Whisper) | ✅ (build needs libclang) |
| 3 | Text injection (Win/macOS/Linux) | ✅ |
| 4 | Dictionaries | ✅ |
| 5 | Cloud ASR (OpenAI/Groq/Deepgram) | ✅ (Deepgram HTTP; WS planned) |
| 6 | Telemetry | ✅ |
| 7 | Plugin system | ✅ (SDK crate planned) |
| 8 | Packaging | ✅ (signing TBD) |
| 9 | v1 launch (hotkey, CSP, docs) | ✅ (perf/signing TBD) |

---

## Docs

- [CONTRIBUTING.md](CONTRIBUTING.md) — dev setup & architecture
- [PLUGINS.md](PLUGINS.md) — plugin manifest & SDK contract
- [plan.md](plan.md) — implementation plan

## License

TBD.
