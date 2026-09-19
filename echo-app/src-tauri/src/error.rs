/*!
 * SOURCE OF TRUTH KEYWORDS: EchoError, ErrorCode, Result, is_recoverable,
 *   invalid_input, permission_required, already_in_progress
 * WHAT:  The one error type that crosses the IPC boundary, and the stable code
 *        the frontend branches on.
 * WHY:   THE FRONTEND MUST NEVER BRANCH ON MESSAGE TEXT. Before `code` existed
 *        the error serialised as a bare string, so the only way for the UI to
 *        tell "the engine is still warming up, try again in five seconds" from
 *        "your microphone is gone" was to match on prose — which breaks the
 *        first time someone improves the wording, and cannot be localised at
 *        all.
 *
 *        That distinction is not cosmetic. A failure the user can simply retry
 *        in a moment is NOT drawn as a failure: `EngineNotReady` is the single
 *        most likely error a new user will ever see, on their very first
 *        keypress, and painting the pill red there says the app is broken when
 *        it is seven seconds old. `is_recoverable` is what lets the pill make
 *        that call on a field instead of on a substring.
 * WHERE: Returned by every command through ipc/factory.rs; read by the
 *        frontend's errorMessage/errorCode helpers in src/lib/errors.ts.
 */

use serde::Serialize;
use specta::Type;
use thiserror::Error;

/**
 * SOURCE OF TRUTH KEYWORDS: ErrorCode
 * WHAT:  The closed set of failures the frontend is allowed to branch on.
 * WHY:   Stable identifiers, deliberately coarser than the error variants:
 *        the UI cares whether it can retry, whether to send the user to a
 *        settings pane, and whether to apologise — not which of four adapters
 *        produced it.
 * WHERE: Carried on every serialised EchoError.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// The input never should have reached a handler. Always a bug in the
    /// caller, never something the user can act on.
    InvalidInput,
    /// An OS grant is missing. The UI sends the user to the right pane.
    PermissionRequired,
    /// The engine is still starting. Transient by definition — see the module
    /// WHY for why this one must not be drawn as a failure.
    EngineNotReady,
    /// The same exclusive command is already running.
    AlreadyInProgress,
    AudioDevice,
    Transcription,
    Injection,
    Storage,
    Plugin,
    Config,
    NotFound,
}

#[derive(Debug, Error)]
pub enum EchoError {
    #[error("Audio device error: {0}")]
    AudioDevice(String),

    #[error("Audio stream error: {0}")]
    AudioStream(String),

    #[error("ASR provider error: {0}")]
    AsrProvider(String),

    /// The engine exists and is starting; the caller should try again shortly.
    /// Separate from `AsrProvider` precisely so the UI can tell them apart.
    #[error("{0}")]
    EngineNotReady(String),

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

    /// Raised by the command factory when an input fails its own validation.
    #[error("{0}")]
    InvalidInput(String),

    /// Raised by the command factory when an exclusive command is re-entered.
    #[error("{0}")]
    AlreadyInProgress(String),
}

impl EchoError {
    /// The stable code for this error. One arm per variant, deliberately
    /// exhaustive: a new variant must decide what the UI should do about it.
    pub fn code(&self) -> ErrorCode {
        match self {
            EchoError::AudioDevice(_) | EchoError::AudioStream(_) => ErrorCode::AudioDevice,
            EchoError::AsrProvider(_) => ErrorCode::Transcription,
            EchoError::EngineNotReady(_) => ErrorCode::EngineNotReady,
            EchoError::Injection(_) => ErrorCode::Injection,
            EchoError::Storage(_) | EchoError::Serialization(_) => ErrorCode::Storage,
            EchoError::Plugin(_) => ErrorCode::Plugin,
            EchoError::Config(_) => ErrorCode::Config,
            EchoError::NotFound(_) => ErrorCode::NotFound,
            EchoError::PermissionDenied(_) => ErrorCode::PermissionRequired,
            EchoError::InvalidInput(_) => ErrorCode::InvalidInput,
            EchoError::AlreadyInProgress(_) => ErrorCode::AlreadyInProgress,
        }
    }

    /// Whether trying the exact same thing again in a moment could work.
    ///
    /// This is the field the pill reads to decide whether to draw a failure at
    /// all. It is NOT "was this the user's fault" — a missing permission is the
    /// user's to fix but retrying without fixing it will fail identically, so
    /// it is not recoverable in this sense.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            EchoError::EngineNotReady(_) | EchoError::AlreadyInProgress(_)
        )
    }
}

// Plugin lifecycle hooks return the SDK's dependency-free error; lift it into
// the host error at the loader boundary.
impl From<echo_sdk::PluginError> for EchoError {
    fn from(e: echo_sdk::PluginError) -> Self {
        EchoError::Plugin(e.to_string())
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: SerializedError
 * WHAT:  The wire shape of an error: code, message, and whether retrying could
 *        help.
 * WHY:   A struct rather than the bare string this used to be, so the UI has
 *        something to branch on that survives a copy edit. `message` stays
 *        first-class and human — the code tells the UI what to DO, the message
 *        tells the user what happened, and neither is derivable from the other.
 * WHERE: Produced by the Serialize impl below; consumed by src/lib/errors.ts.
 */
#[derive(Debug, Serialize, Type)]
pub struct SerializedError {
    pub code: ErrorCode,
    pub message: String,
    pub recoverable: bool,
}

// Tauri commands must return serde-serializable errors.
impl serde::Serialize for EchoError {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        SerializedError {
            code: self.code(),
            message: self.to_string(),
            recoverable: self.is_recoverable(),
        }
        .serialize(serializer)
    }
}

pub type Result<T> = std::result::Result<T, EchoError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the code is that the UI never has to read the prose.
    /// If a variant's code changes, a branch somewhere in the frontend silently
    /// stops matching, so the mapping is pinned here.
    #[test]
    fn codes_are_stable() {
        assert_eq!(
            EchoError::EngineNotReady("warming".into()).code(),
            ErrorCode::EngineNotReady
        );
        assert_eq!(
            EchoError::PermissionDenied("mic".into()).code(),
            ErrorCode::PermissionRequired
        );
        assert_eq!(
            EchoError::AudioStream("gone".into()).code(),
            ErrorCode::AudioDevice
        );
    }

    /// Recoverable means "the same call could work in a moment", not "the user
    /// can fix this". A missing permission is the user's to fix and will fail
    /// identically until they do, which is why it is not in this set.
    #[test]
    fn only_transient_failures_are_recoverable() {
        assert!(EchoError::EngineNotReady("warming".into()).is_recoverable());
        assert!(EchoError::AlreadyInProgress("busy".into()).is_recoverable());

        assert!(!EchoError::PermissionDenied("mic".into()).is_recoverable());
        assert!(!EchoError::AudioDevice("unplugged".into()).is_recoverable());
        assert!(!EchoError::InvalidInput("nope".into()).is_recoverable());
    }

    /// The serialised shape is a contract with src/lib/errors.ts.
    #[test]
    fn serialises_with_a_code_the_frontend_can_branch_on() {
        let json = serde_json::to_value(EchoError::EngineNotReady(
            "Echo is still starting up.".into(),
        ))
        .expect("serialise");
        assert_eq!(json["code"], "ENGINE_NOT_READY");
        assert_eq!(json["message"], "Echo is still starting up.");
        assert_eq!(json["recoverable"], true);
    }
}
