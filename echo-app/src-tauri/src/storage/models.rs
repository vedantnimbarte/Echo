use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: Option<i64>,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryEntry {
    pub id: Option<i64>,
    pub phrase: String,
    pub replacement: String,
    pub enabled: bool,
    pub profile_id: Option<i64>,
    pub created_at: String,
}

/// A voice snippet: say `trigger` as a whole utterance, get `body` verbatim.
/// See [`crate::core::snippets`] for why this is not a dictionary entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: Option<i64>,
    pub trigger: String,
    /// Delivered exactly as stored — line breaks, numbers and punctuation
    /// included. Never formatted.
    pub body: String,
    pub enabled: bool,
}

/// Per-app overrides. `None` on an override field means "inherit the global
/// setting", so a profile can pin one behaviour without freezing the rest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppProfile {
    pub id: Option<i64>,
    /// Lowercased executable name, bundle id, or window class, matched exactly.
    pub app_match: String,
    /// Human-friendly name for the list; falls back to `app_match`.
    pub label: Option<String>,
    pub auto_inject: Option<bool>,
    pub injection_method: Option<String>,
    /// Type partial transcripts into this app as you speak. `None` inherits
    /// the global setting.
    pub stream_partials: Option<bool>,
    /// Run the formatting pass in this app. `None` inherits the global setting.
    pub formatting: Option<bool>,
    /// Dictionary profile to apply while this app is focused.
    pub profile_id: Option<i64>,
    pub enabled: bool,
}

/// One outbound request Echo made.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressRecord {
    pub id: Option<i64>,
    pub host: String,
    pub purpose: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TranscriptionRecord {
    pub id: Option<i64>,
    pub text: String,
    pub language: Option<String>,
    pub provider: String,
    pub created_at: String,
    /// How long you spoke, from the speech the VAD actually forwarded. `None`
    /// for rows written before this was measured, and for any utterance whose
    /// duration could not be established — a missing number, not a zero.
    #[serde(default)]
    pub duration_ms: Option<i64>,
    /// The app that was focused when the text was delivered, lowercased, in
    /// the same form per-app profiles match on.
    #[serde(default)]
    pub app: Option<String>,
    /// Words the dictionary rewrote.
    #[serde(default)]
    pub dictionary_fixes: i64,
    /// Words the clean-up pass changed — fillers dropped, punctuation spoken,
    /// numbers written as digits.
    #[serde(default)]
    pub cleanup_fixes: i64,
}
