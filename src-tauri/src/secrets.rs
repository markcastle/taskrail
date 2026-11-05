//! API keys and tokens, kept in the operating system's credential store:
//! Credential Manager on Windows, the Keychain on macOS, the kernel keyring
//! on Linux (which does not survive a reboot; fine for development).
//!
//! Nothing here ever writes a secret to the SQLite file.

use taskrail_core::ports::{SecretError, SecretStore};

const SERVICE: &str = "taskrail";

#[derive(Debug, Default, Clone, Copy)]
pub struct OsSecrets;

fn entry(key: &str) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(SERVICE, key).map_err(|e| SecretError::Store(e.to_string()))
}

impl SecretStore for OsSecrets {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        match entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Store(e.to_string())),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        entry(key)?
            .set_password(value)
            .map_err(|e| SecretError::Store(e.to_string()))
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        match entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Store(e.to_string())),
        }
    }
}

/// Where a provider's key lives.
pub fn llm_key_name(provider: taskrail_llm::Provider) -> String {
    format!(
        "llm:{}",
        serde_json::to_value(provider)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    )
}
