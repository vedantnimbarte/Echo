# Contributing to Echo

Thanks for your interest in Echo! This document covers dev setup and a quick
architecture tour.

## Prerequisites

- **Rust** (stable) + Cargo — https://rustup.rs
- **Node.js** 20+ and npm
- **Tauri v2 system deps** — https://tauri.app/start/prerequisites/
  - Windows: Microsoft Visual Studio C++ Build Tools + WebView2
  - macOS: Xcode Command Line Tools
  - Linux: `libwebkit2gtk-4.1-dev`, `libasound2-dev`, `librsvg2-dev`, `patchelf`
- **Optional (local Whisper):** `cmake` + LLVM/`libclang` (see README)

## Dev setup

```bash
cd echo-app
npm install
npm run tauri dev
```

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
```

For local Whisper code, build with the feature: `cargo check --features whisper`
(requires libclang).

## Architecture

Clean-architecture-ish separation in `echo-app/src-tauri/src`:

- `core/audio` — CPAL capture, mono down-mix, 16 kHz resample
- `core/vad` — voice activity detection (energy-based; Silero planned)
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

### Key rules (see `plan.md`)

1. `AppState.db` is `Mutex<Connection>` — never share the `Connection` across
   threads; drop the guard before `.await`.
2. All Tauri event names use the `echo://` prefix.
3. CPU-bound ASR work runs on `spawn_blocking`.
4. Never return raw API keys to the frontend.
5. VAD runs in the audio capture task, not the ASR task.

## Commit style

Conventional commits (`feat:`, `fix:`, `docs:`, `build:`, `ci:`), small and
focused.
