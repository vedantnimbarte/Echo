# Echo Technical Design Specification (TDS)

## Rust Workspace

src-tauri/
core/
providers/
platform/
storage/
plugins/

## Core Traits

trait AsrProvider
trait AudioSource
trait OutputProvider
trait Plugin

## Database

settings
profiles
dictionary_entries
history
plugins
telemetry

## IPC Commands

start_recording()
stop_recording()
get_devices()
set_provider()
install_plugin()

## Audio Service

Responsibilities:
- Capture
- Buffering
- Resampling
- Streaming

## ASR Manager

Responsibilities:
- Provider selection
- Model management
- Streaming results

## Dictionary Engine

Pipeline:
Transcript
→ Normalize
→ Replace
→ Output

## Output Engine

Targets:
- Keyboard
- Plugin outputs

## Plugin SDK

Future crate:
echo-sdk

Capabilities:
- Register provider
- Register output
- Register commands
