//! Cloud provider configuration: keys, per-provider settings, and construction.
//!
//! The split matters. **Secrets go to the OS keychain** ([`keychain`]); the
//! non-secret choices that sit beside them — which model, which endpoint, which
//! region — go to the ordinary settings table. Endpoints are not secrets, and
//! putting them in the keychain would make them invisible to the user and
//! unexportable for no benefit, while making the keychain entry a blob that has
//! to be parsed before a key can be read.

use std::sync::Arc;

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;
use tokio::sync::RwLock;

use crate::{
    core::asr::{
        assemblyai::AssemblyAiProvider,
        azure::AzureSpeechProvider,
        catalog::{self, ProviderKind, ProviderSpec},
        deepgram::DeepgramProvider,
        elevenlabs::ElevenLabsProvider,
        google::GoogleSttProvider,
        openai::WhisperApiProvider,
        prompt::PromptContext,
        speechmatics::SpeechmaticsProvider,
        AsrProvider,
    },
    core::dictionary::DictionaryEngine,
    error::{EchoError, Result},
    state::AppState,
    storage::{keychain, repositories},
};

/// Settings key for one of a provider's non-secret fields.
fn setting_key(provider: &str, field: &str) -> String {
    format!("cloud_{provider}_{field}")
}

/// Read a provider's stored field, treating empty as unset.
///
/// The empty check is not decoration: clearing a text input writes `""`, and an
/// empty endpoint that shadowed the catalog default would send every request to
/// `/audio/transcriptions` with no host.
fn stored(conn: &Connection, provider: &str, field: &str) -> Option<String> {
    repositories::get_setting(conn, &setting_key(provider, field))
        .unwrap_or(None)
        .filter(|s| !s.trim().is_empty())
}

/// Everything needed to build one provider, with defaults already applied.
pub struct ProviderConfig {
    pub endpoint: String,
    pub model: String,
    pub region: Option<String>,
}

/// Resolve a provider's configuration: stored value, else catalog default.
pub fn resolve_config(conn: &Connection, spec: &ProviderSpec) -> Result<ProviderConfig> {
    let endpoint = stored(conn, spec.id, "endpoint")
        .unwrap_or_else(|| spec.default_endpoint.to_string());

    if endpoint.is_empty() && !spec.needs_region {
        return Err(EchoError::Config(format!(
            "{} needs an endpoint URL. Add one in Settings → Cloud providers.",
            spec.label
        )));
    }

    let region = stored(conn, spec.id, "region");
    if spec.needs_region && region.is_none() {
        return Err(EchoError::Config(format!(
            "{} needs a region (for example westeurope). Add one in Settings → Cloud providers.",
            spec.label
        )));
    }

    let model = stored(conn, spec.id, "model")
        .or_else(|| spec.default_model().map(str::to_string))
        .unwrap_or_default();

    Ok(ProviderConfig {
        endpoint,
        model,
        region,
    })
}

/// Build a cloud ASR provider from its id, key, and stored configuration.
///
/// Takes its dependencies explicitly rather than an [`AppState`] because it is
/// also called during setup, before that state exists — the boot path
/// registers providers from the keychain while the pieces are still loose.
///
/// Bucket A providers get the dictionary and prompt context so cloud decodes
/// are biased by the same custom vocabulary the offline engine uses — without
/// them, adding a name to the dictionary silently helped only local users.
pub fn build_provider_with(
    conn: &Connection,
    dictionary: Arc<RwLock<DictionaryEngine>>,
    prompt_ctx: Arc<PromptContext>,
    provider: &str,
    key: String,
) -> Result<Arc<dyn AsrProvider>> {
    let spec = catalog::find(provider)
        .ok_or_else(|| EchoError::NotFound(format!("Unknown provider '{provider}'")))?;
    let cfg = resolve_config(conn, spec)?;

    Ok(match spec.kind {
        ProviderKind::OpenAiCompatible => Arc::new(
            WhisperApiProvider::new(spec.id, &cfg.endpoint, cfg.model, key)
                .with_dictionary(dictionary)
                .with_prompt_context(prompt_ctx),
        ),
        ProviderKind::Deepgram => Arc::new(DeepgramProvider::new(key, cfg.model)),
        ProviderKind::ElevenLabs => {
            Arc::new(ElevenLabsProvider::new(&cfg.endpoint, cfg.model, key))
        }
        ProviderKind::AzureSpeech => {
            // `resolve_config` has already refused a missing region, so this
            // unwrap is on a value it guaranteed.
            let region = cfg.region.as_deref().unwrap_or_default();
            Arc::new(AzureSpeechProvider::new(region, key))
        }
        ProviderKind::AssemblyAi => {
            Arc::new(AssemblyAiProvider::new(&cfg.endpoint, cfg.model, key))
        }
        ProviderKind::Speechmatics => {
            Arc::new(SpeechmaticsProvider::new(&cfg.endpoint, cfg.model, key))
        }
        ProviderKind::GoogleStt => Arc::new(GoogleSttProvider::new(&cfg.endpoint, cfg.model, key)),
    })
}

/// [`build_provider_with`], reading its dependencies out of the running app.
pub fn build_provider(
    state: &AppState,
    provider: &str,
    key: String,
) -> Result<Arc<dyn AsrProvider>> {
    let conn = state.db.lock().unwrap();
    build_provider_with(
        &conn,
        state.dictionary.clone(),
        state.prompt_ctx.clone(),
        provider,
        key,
    )
}

/// A provider as the settings UI needs it: shape, plus whether a key is stored
/// and what the user has chosen. Never carries the key itself.
#[derive(Serialize)]
pub struct ProviderInfo {
    #[serde(flatten)]
    pub spec: &'static ProviderSpec,
    pub key_set: bool,
    pub model: String,
    pub endpoint: String,
    pub region: Option<String>,
    /// False while a catalog row exists but its provider is not built yet.
    pub available: bool,
}

/// The provider list the settings UI renders — replacing the arrays that used
/// to be duplicated in `CloudProviders.tsx` and `SettingsPanel.tsx`.
#[tauri::command]
pub fn list_cloud_providers(state: State<'_, AppState>) -> Result<Vec<ProviderInfo>> {
    let conn = state.db.lock().unwrap();
    Ok(catalog::PROVIDERS
        .iter()
        .map(|spec| {
            let cfg = resolve_config(&conn, spec).ok();
            ProviderInfo {
                spec,
                key_set: keychain::get_api_key(spec.id).unwrap_or(None).is_some(),
                model: cfg
                    .as_ref()
                    .map(|c| c.model.clone())
                    .unwrap_or_else(|| spec.default_model().unwrap_or("").to_string()),
                endpoint: cfg
                    .as_ref()
                    .map(|c| c.endpoint.clone())
                    .unwrap_or_else(|| spec.default_endpoint.to_string()),
                region: cfg.as_ref().and_then(|c| c.region.clone()),
                // Every catalog kind is implemented; a row is unavailable
                // only when its own config is incomplete (no endpoint, no
                // region), which `resolve_config` is what decides.
                available: cfg.is_some(),
            }
        })
        .collect())
}

/// Store a provider's API key in the OS keychain and register it for immediate
/// use (no restart needed).
#[tauri::command]
pub async fn set_api_key(state: State<'_, AppState>, provider: String, key: String) -> Result<()> {
    keychain::store_api_key(&provider, &key)?;
    let p = build_provider(state.inner(), &provider, key)?;
    state.asr.register(p).await;
    Ok(())
}

/// Save one of a provider's non-secret settings (model, endpoint, region) and
/// rebuild it so the change takes effect without a restart.
#[tauri::command]
pub async fn set_provider_setting(
    state: State<'_, AppState>,
    provider: String,
    field: String,
    value: String,
) -> Result<()> {
    if !matches!(field.as_str(), "model" | "endpoint" | "region") {
        return Err(EchoError::Config(format!("Unknown provider field '{field}'")));
    }
    {
        let conn = state.db.lock().unwrap();
        repositories::set_setting(&conn, &setting_key(&provider, &field), value.trim())?;
    }

    // Only rebuild if there is a key to build with; otherwise the setting is
    // simply saved for when one arrives.
    if let Some(key) = keychain::get_api_key(&provider)? {
        let p = build_provider(state.inner(), &provider, key)?;
        state.asr.register(p).await;
    }
    Ok(())
}

/// Report whether a provider has an API key stored. Never returns the key
/// itself (architectural rule 5).
#[tauri::command]
pub fn get_api_key_set(provider: String) -> Result<bool> {
    Ok(keychain::get_api_key(&provider)?.is_some())
}

/// Remove a provider's stored API key from the keychain.
#[tauri::command]
pub fn remove_api_key(provider: String) -> Result<()> {
    keychain::delete_api_key(&provider)
}

/// Transcribe a short silent buffer to prove the key, endpoint and model all
/// work together.
///
/// Worth a round trip: without it the first sign of a typo'd key is a sentence
/// that goes nowhere, mid-thought, with the error buried in a log. A provider
/// that returns empty text for silence has still answered the only question
/// being asked, so empty is a pass.
#[tauri::command]
pub async fn test_api_key(state: State<'_, AppState>, provider: String) -> Result<String> {
    let key = keychain::get_api_key(&provider)?
        .ok_or_else(|| EchoError::Config("Save an API key first.".into()))?;
    let p = build_provider(state.inner(), &provider, key)?;

    // One second of silence at 16 kHz — long enough to be a valid request,
    // short enough to cost nothing worth mentioning.
    let silence = vec![0.0f32; 16_000];
    p.transcribe(silence, Some("en")).await.map(|_| "ok".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::asr::catalog;

    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn an_unconfigured_provider_falls_back_to_its_catalog_defaults() {
        let conn = memory_db();
        let spec = catalog::find("openai").unwrap();
        let cfg = resolve_config(&conn, spec).unwrap();
        assert_eq!(cfg.endpoint, "https://api.openai.com/v1");
        assert_eq!(cfg.model, "whisper-1");
        assert!(cfg.region.is_none());
    }

    #[test]
    fn a_stored_choice_wins_over_the_default() {
        let conn = memory_db();
        repositories::set_setting(&conn, "cloud_openai_model", "gpt-4o-transcribe").unwrap();
        let cfg = resolve_config(&conn, catalog::find("openai").unwrap()).unwrap();
        assert_eq!(cfg.model, "gpt-4o-transcribe");
    }

    #[test]
    fn clearing_a_field_restores_the_default_rather_than_blanking_it() {
        // Emptying the input writes "", which must not shadow the default and
        // send requests to a hostless URL.
        let conn = memory_db();
        repositories::set_setting(&conn, "cloud_openai_endpoint", "   ").unwrap();
        let cfg = resolve_config(&conn, catalog::find("openai").unwrap()).unwrap();
        assert_eq!(cfg.endpoint, "https://api.openai.com/v1");
    }

    #[test]
    fn the_custom_provider_refuses_to_run_without_an_endpoint() {
        let conn = memory_db();
        let spec = catalog::find("custom").unwrap();
        assert!(resolve_config(&conn, spec).is_err(), "should demand an endpoint");

        repositories::set_setting(&conn, "cloud_custom_endpoint", "http://localhost:8000/v1")
            .unwrap();
        repositories::set_setting(&conn, "cloud_custom_model", "Systran/faster-whisper-small")
            .unwrap();
        let cfg = resolve_config(&conn, spec).unwrap();
        assert_eq!(cfg.endpoint, "http://localhost:8000/v1");
        assert_eq!(cfg.model, "Systran/faster-whisper-small");
    }

    #[test]
    fn azure_demands_a_region_because_the_host_depends_on_it() {
        let conn = memory_db();
        let spec = catalog::find("azure").unwrap();
        assert!(resolve_config(&conn, spec).is_err());

        repositories::set_setting(&conn, "cloud_azure_region", "westeurope").unwrap();
        let cfg = resolve_config(&conn, spec).unwrap();
        assert_eq!(cfg.region.as_deref(), Some("westeurope"));
    }

    #[test]
    fn settings_keys_do_not_collide_between_providers() {
        assert_eq!(setting_key("openai", "model"), "cloud_openai_model");
        assert_ne!(
            setting_key("openai", "model"),
            setting_key("openai_extra", "model")
        );
    }
}
