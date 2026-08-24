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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionRecord {
    pub id: Option<i64>,
    pub text: String,
    pub language: Option<String>,
    pub provider: String,
    pub created_at: String,
}
