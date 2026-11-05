//! API keys and tokens, kept in the operating system's credential store:
//! Credential Manager on Windows, the Keychain on macOS, the kernel keyring
//! on Linux (which does not survive a reboot; fine for development).
//!
//! Windows caps one credential at 2560 UTF-16 characters, and an OAuth
//! credential bundle (access token, refresh token, client id, all as JSON)
//! is routinely bigger. So a long value is split into numbered pieces under
//! `key#0`, `key#1`, ... and the entry under `key` itself records how many.
//! Reading reassembles them; deleting removes them all. Short values are
//! stored as they are.
//!
//! Nothing here ever writes a secret to the SQLite file.

use taskrail_core::ports::{SecretError, SecretStore};

const SERVICE: &str = "taskrail";
/// Comfortably under the Windows limit, counted in characters.
const CHUNK_CHARS: usize = 2000;
const CHUNKED_MARKER: &str = "taskrail-chunks:";

#[derive(Debug, Default, Clone, Copy)]
pub struct OsSecrets;

fn entry(key: &str) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(SERVICE, key).map_err(|e| SecretError::Store(e.to_string()))
}

fn read(key: &str) -> Result<Option<String>, SecretError> {
    match entry(key)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(SecretError::Store(e.to_string())),
    }
}

fn write(key: &str, value: &str) -> Result<(), SecretError> {
    entry(key)?
        .set_password(value)
        .map_err(|e| SecretError::Store(e.to_string()))
}

fn remove(key: &str) -> Result<(), SecretError> {
    match entry(key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(SecretError::Store(e.to_string())),
    }
}

fn chunk_key(key: &str, index: usize) -> String {
    format!("{key}#{index}")
}

/// Split by characters, never inside one, so every piece is valid text.
pub(crate) fn split_into_chunks(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    chars
        .chunks(CHUNK_CHARS)
        .map(|c| c.iter().collect())
        .collect()
}

pub(crate) fn chunk_count(marker: &str) -> Option<usize> {
    marker.strip_prefix(CHUNKED_MARKER)?.trim().parse().ok()
}

/// Remove numbered pieces from `from` upward until one is missing.
fn remove_chunks_from(key: &str, from: usize) -> Result<(), SecretError> {
    let mut index = from;
    while read(&chunk_key(key, index))?.is_some() {
        remove(&chunk_key(key, index))?;
        index += 1;
    }
    Ok(())
}

impl SecretStore for OsSecrets {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        let Some(head) = read(key)? else {
            return Ok(None);
        };
        let Some(count) = chunk_count(&head) else {
            return Ok(Some(head));
        };
        let mut value = String::new();
        for index in 0..count {
            let piece = read(&chunk_key(key, index))?.ok_or_else(|| {
                SecretError::Store(format!("piece {index} of {count} is missing for {key}"))
            })?;
            value.push_str(&piece);
        }
        Ok(Some(value))
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        if value.chars().count() <= CHUNK_CHARS {
            write(key, value)?;
            return remove_chunks_from(key, 0);
        }
        let pieces = split_into_chunks(value);
        for (index, piece) in pieces.iter().enumerate() {
            write(&chunk_key(key, index), piece)?;
        }
        write(key, &format!("{CHUNKED_MARKER}{}", pieces.len()))?;
        remove_chunks_from(key, pieces.len())
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        remove(key)?;
        remove_chunks_from(key, 0)
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

/// Serialized OAuth credentials for one connector, in the credential store.
pub struct KeychainCredentials {
    key: String,
}

impl KeychainCredentials {
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }
}

impl taskrail_connector_mcp::CredentialPersistence for KeychainCredentials {
    fn load(&self) -> Result<Option<String>, String> {
        OsSecrets.get(&self.key).map_err(|e| e.to_string())
    }

    fn save(&self, json: &str) -> Result<(), String> {
        OsSecrets.set(&self.key, json).map_err(|e| e.to_string())
    }

    fn clear(&self) -> Result<(), String> {
        OsSecrets.delete(&self.key).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_values_split_on_character_boundaries_and_rejoin() {
        let value: String = "é".repeat(CHUNK_CHARS * 2 + 7);
        let pieces = split_into_chunks(&value);
        assert_eq!(pieces.len(), 3);
        assert!(pieces.iter().all(|p| p.chars().count() <= CHUNK_CHARS));
        assert_eq!(pieces.concat(), value);
    }

    #[test]
    fn the_marker_names_the_number_of_pieces() {
        assert_eq!(chunk_count("taskrail-chunks:3"), Some(3));
        assert_eq!(chunk_count("sk-ant-plain-key"), None);
        assert_eq!(chunk_count("taskrail-chunks:x"), None);
    }
}
