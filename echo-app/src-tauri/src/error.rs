use thiserror::Error;

#[derive(Debug, Error)]
pub enum EchoError {
    #[error("Audio device error: {0}")]
    AudioDevice(String),

    #[error("Audio stream error: {0}")]
    AudioStream(String),

    #[error("ASR provider error: {0}")]
    AsrProvider(String),

    #[error("Injection error: {0}")]
    Injection(String),

    #[error("Storage error: {0}")]
    Storage(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

// Plugin lifecycle hooks return the SDK's dependency-free error; lift it into
// the host error at the loader boundary.
impl From<echo_sdk::PluginError> for EchoError {
    fn from(e: echo_sdk::PluginError) -> Self {
        EchoError::Plugin(e.to_string())
    }
}

// Tauri commands must return serde-serializable errors.
impl serde::Serialize for EchoError {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, EchoError>;
