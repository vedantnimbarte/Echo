# Echo PRD (Product Requirements Document)

## Version
v1.0

## Product Name
Echo

## Executive Summary

Echo is an open-source, cross-platform desktop voice input system built with Tauri, Rust, and React. Its primary purpose is to function as a universal voice keyboard that allows users to dictate text into any application with ultra-low latency (<300ms perceived latency), while maintaining privacy through local transcription and offering optional cloud-based speech recognition.

Echo is designed as a standalone product that can later integrate with the IRA ecosystem while remaining independently usable.

---

# Vision

Create the fastest, most private, and most extensible voice input layer for developers, professionals, AI users, and general productivity workflows.

Users should be able to speak anywhere on their computer and have accurate text appear instantly inside the currently focused application.

---

# Goals

## Primary Goals

- Cross-platform support
- Ultra-low latency voice input
- Native keyboard injection
- Offline-first experience
- Hybrid ASR architecture
- Custom dictionary support
- Plugin architecture
- Open-source development

## Success Metrics

- Perceived latency under 300ms
- 95%+ transcription accuracy
- Works in any text field
- Startup time under 2 seconds
- Memory usage below 500MB during active transcription

---

# Target Users

## Developers

- VS Code
- Cursor
- OpenCode
- Claude Code
- Terminal workflows

## Productivity Users

- Notes
- Documents
- Email
- Chat applications

## AI Users

- LLM interfaces
- Agent platforms
- Prompt-heavy workflows

---

# Core Product Principles

1. Offline-first
2. Privacy-focused
3. Cross-platform
4. Extensible
5. Low-latency
6. Open-source

---

# Functional Requirements

## Audio Capture

### Requirements

- Multi-microphone support
- Device switching
- Hot microphone swapping
- Device persistence

### Supported Platforms

- Windows
- Linux
- macOS

---

## Recording Modes

### Push-to-Talk

User holds configured hotkey.

Flow:

Hold → Record → Release → Transcribe → Inject

### Toggle Mode

Press once → Start recording

Press again → Stop recording

### Configuration

User may switch modes via settings.

---

## Voice Activity Detection

### Requirements

- Silence detection
- Noise rejection
- Speech segmentation
- CPU-efficient operation

### Suggested Technology

Silero VAD

---

## Speech Recognition

### Local ASR

Requirements:

- Offline operation
- Automatic model downloads
- GPU acceleration where available
- CPU fallback

Supported Models:

- Whisper Tiny
- Whisper Base
- Whisper Small
- Whisper Medium
- Future custom models

### Cloud ASR

Supported Providers:

- OpenAI
- Groq
- Deepgram

### Bring Your Own Key

Users can configure personal API keys.

### Future Hosted Service

Echo-hosted cloud transcription supported in future versions.

---

## Streaming Transcription

### Requirements

- Partial transcription
- Incremental updates
- Real-time display
- Immediate text availability

### User Experience

Speech appears while user is speaking.

---

## Language Support

### Requirements

- Multilingual support
- Automatic language detection
- Language override option
- Per-language settings

---

## Text Injection

### Requirements

- Native keyboard events
- No clipboard dependency
- Works with any focused application

### Supported Targets

- IDEs
- Browsers
- Terminals
- Chat applications
- Office applications

---

## Custom Dictionary

### Features

- Phrase replacements
- Technical vocabulary
- Project-specific terminology
- Import/export

Example:

"router file" → "src/agents/router.rs"

---

## History

### Requirements

User configurable.

Modes:

- Disabled
- Enabled

### Storage

Local-only SQLite storage.

---

## Telemetry

### Default State

Enabled by default.

### User Controls

- Disable telemetry
- View collected data
- Delete telemetry data

### Data Restrictions

No audio recording uploads without consent.

---

## Plugin System

### Goals

Enable third-party extension development.

### Plugin Categories

#### ASR Plugins

Custom transcription engines.

#### Output Plugins

Alternative output targets.

#### Dictionary Plugins

Dynamic dictionaries.

#### Audio Plugins

Audio preprocessing.

#### Integration Plugins

Third-party software integrations.

---

# Non-Functional Requirements

## Performance

- Startup under 2 seconds
- Transcription latency under 300ms
- Low CPU utilization
- Efficient memory usage

## Reliability

- Crash recovery
- Automatic restart support
- Settings persistence

## Security

- Local data encryption support
- Secure API key storage
- Sandboxed plugin execution

## Privacy

- Offline-first architecture
- User-owned data
- Transparent telemetry

---

# Technical Architecture

## Frontend

- React
- TypeScript
- Vite
- Tailwind
- Zustand
- TanStack Query

## Desktop Layer

- Tauri

## Backend

- Rust
- Tokio
- Serde

---

# High-Level Architecture

Audio Input
→ VAD
→ Streaming ASR
→ Dictionary Processor
→ Output Pipeline
→ Native Keyboard Injection

---

# Rust Workspace Layout

src-tauri/

core/
- audio
- vad
- asr
- injection
- dictionary
- plugins
- telemetry

providers/
- whispercpp
- openai
- deepgram
- groq

platform/
- windows
- linux
- macos

storage/
- sqlite
- config

---

# Database Schema

## settings

Stores global settings.

## profiles

User profiles.

## dictionary_entries

Custom phrases.

## transcription_history

Optional history.

## telemetry_events

Anonymous telemetry.

## plugins

Installed plugins.

---

# Plugin Architecture

## Design Goals

- Stable API
- Versioned interfaces
- Runtime loading
- Sandboxing

## Future SDK

Echo SDK for Rust plugin development.

---

# Distribution

## Windows

- MSI
- EXE
- Winget
- Microsoft Store

## macOS

- DMG
- Homebrew
- App Store

## Linux

- AppImage
- Deb
- RPM
- Flatpak
- Snap

---

# Security Requirements

## API Keys

Stored securely using OS keychain systems.

## Plugins

Permission-based execution.

## Updates

Signed releases.

---

# Telemetry Requirements

Collected:

- Startup metrics
- Crash reports
- Performance metrics

Never collected:

- Raw audio
- Transcripts
- Personal files

---

# Development Roadmap

## MVP (v0.1)

- Tauri
- Rust backend
- React UI
- Global hotkey
- Push-to-talk
- Toggle mode
- Streaming transcription
- Local Whisper support
- Keyboard injection
- Custom dictionary
- SQLite

## v0.2

- Cloud providers
- Auto language detection
- Telemetry
- History controls

## v0.3

- Plugin system
- SDK
- Plugin marketplace foundation

## v1.0

- Production stability
- Cross-platform packaging
- Full documentation
- Enterprise-ready architecture

---

# Future IRA Integration

Echo remains an independent application.

Future architecture:

Echo
→ Transcript Stream
→ IRA Runtime
→ Kortex Memory
→ Agent Layer

This integration must remain optional.

---

# Risks

## Technical Risks

- Cross-platform keyboard injection differences
- Streaming latency targets
- macOS accessibility permissions
- Linux desktop environment variations

## Product Risks

- Speech recognition quality
- Model download sizes
- Plugin security

---

# Long-Term Vision

Echo becomes the voice input layer for the broader IRA ecosystem while remaining a standalone open-source product that anyone can use independently.
