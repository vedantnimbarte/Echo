# Echo System Architecture Document (SAD)

## Purpose
Defines the complete architecture of Echo.

## Core Layers

1. React UI Layer
2. Tauri IPC Layer
3. Rust Application Core
4. Plugin Runtime
5. Platform Layer

## Architecture

UI
↓
Tauri IPC
↓
Event Bus
↓
Core Services
├─ Audio
├─ VAD
├─ ASR
├─ Dictionary
├─ Injection
├─ Telemetry
├─ Plugins
└─ Storage

## Audio Pipeline

Microphone
→ Audio Buffer
→ VAD
→ Streaming ASR
→ Post Processing
→ Keyboard Injection

## Event Bus

Events:
- RecordingStarted
- RecordingStopped
- TranscriptPartial
- TranscriptFinal
- DeviceChanged
- PluginLoaded

## Plugin Runtime

Traits:
- AsrProvider
- OutputProvider
- AudioProcessor
- DictionaryProvider

## Security

- Signed plugins
- Permission model
- Keychain storage
- No transcript collection

## Future IRA Integration

Echo emits transcript stream.
IRA consumes transcript stream.
