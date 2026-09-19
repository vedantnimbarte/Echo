//! The single list of dictation languages Echo offers.
//!
//! Same reason as [`crate::core::asr::catalog`]: this used to live only in
//! `SettingsPanel.tsx`, and the tray menu would have been a second copy to keep
//! in step. The settings `<select>` reads it via `dictation_languages`.
//!
//! Not the full ~99 languages Whisper knows — a picker nobody can scan is worse
//! than a short one, and the long tail is better served by pinning a code by
//! hand if it ever comes up.

use serde::Serialize;

#[derive(Serialize, Clone, Copy, specta::Type)]
pub struct Language {
    /// Whisper's code, and the value stored in the `language` setting.
    pub code: &'static str,
    pub label: &'static str,
}

const fn l(code: &'static str, label: &'static str) -> Language {
    Language { code, label }
}

/// `auto` first because it is the default: absent or empty, the pipeline
/// resolves to it (see `whisper_cli::resolve_language`).
pub const LANGUAGES: &[Language] = &[
    l("auto", "Auto-detect"),
    l("en", "English"),
    l("es", "Spanish"),
    l("fr", "French"),
    l("de", "German"),
    l("it", "Italian"),
    l("pt", "Portuguese"),
    l("nl", "Dutch"),
    l("pl", "Polish"),
    l("ru", "Russian"),
    l("uk", "Ukrainian"),
    l("tr", "Turkish"),
    l("ar", "Arabic"),
    l("hi", "Hindi"),
    l("zh", "Chinese"),
    l("ja", "Japanese"),
    l("ko", "Korean"),
];
