use std::sync::Arc;

use tauri::State;

use crate::{
    core::asr::{deepgram::DeepgramProvider, openai::WhisperApiProvider, AsrProvider},
    error::{EchoError, Result},
    state::AppState,
    storage::keychain,
};

/// Build a cloud ASR provider instance from its name and API key.
pub fn build_provider(provider: &str, key: String) -> Result<Arc<dyn AsrProvider>> {
    Ok(match provider {
        "openai" => Arc::new(WhisperApiProvider::openai(key)),
        "groq" => Arc::new(WhisperApiProvider::groq(key)),
        "deepgram" => Arc::new(DeepgramProvider::new(key)),
        _ => return Err(EchoError::NotFound(format!("Unknown provider '{provider}'"))),
    })
}

/// Store a provider's API key in the OS keychain and register it for immediate
/// use (no restart needed).
#[tauri::command]
pub async fn set_api_key(
    state: State<'_, AppState>,
    provider: String,
    key: String,
) -> Result<()> {
    keychain::store_api_key(&provider, &key)?;
    let p = build_provider(&provider, key)?;
    state.asr.register(p).await;
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
