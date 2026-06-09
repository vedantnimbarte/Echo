use keyring::Entry;

use crate::error::{EchoError, Result};

const SERVICE: &str = "com.echo.app";

fn entry(provider: &str) -> Result<Entry> {
    Entry::new(SERVICE, provider).map_err(|e| EchoError::Config(e.to_string()))
}

/// Store an API key for a provider in the OS keychain.
pub fn store_api_key(provider: &str, key: &str) -> Result<()> {
    entry(provider)?
        .set_password(key)
        .map_err(|e| EchoError::Config(e.to_string()))
}

/// Retrieve an API key, returning `None` if no entry exists.
pub fn get_api_key(provider: &str) -> Result<Option<String>> {
    match entry(provider)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(EchoError::Config(e.to_string())),
    }
}

/// Delete a provider's API key. Succeeds even if no entry exists.
pub fn delete_api_key(provider: &str) -> Result<()> {
    match entry(provider)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(EchoError::Config(e.to_string())),
    }
}
