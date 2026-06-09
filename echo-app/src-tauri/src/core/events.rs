use serde::{Deserialize, Serialize};

/// All events emitted on the application event bus via Tauri's emit system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum AppEvent {
    RecordingStarted,
    RecordingStopped,
    TranscriptPartial { text: String },
    TranscriptFinal { text: String, language: Option<String> },
    DeviceChanged { device_name: String },
    ErrorOccurred { message: String },
    ModelDownloadProgress { name: String, progress: f32 },
    ModelDownloadComplete { name: String },
}

impl AppEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            AppEvent::RecordingStarted => "echo://recording-started",
            AppEvent::RecordingStopped => "echo://recording-stopped",
            AppEvent::TranscriptPartial { .. } => "echo://transcript-partial",
            AppEvent::TranscriptFinal { .. } => "echo://transcript-final",
            AppEvent::DeviceChanged { .. } => "echo://device-changed",
            AppEvent::ErrorOccurred { .. } => "echo://error",
            AppEvent::ModelDownloadProgress { .. } => "echo://model-download-progress",
            AppEvent::ModelDownloadComplete { .. } => "echo://model-download-complete",
        }
    }
}
