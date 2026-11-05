//! API keys and tokens, kept in the operating system's credential store:
//! Credential Manager on Windows, the Keychain on macOS, the kernel keyring
//! on Linux (which does not survive a reboot; fine for development).
//!
//! Windows caps one credential blob at 2560 bytes, which is 1280 UTF-16
//! code units (the keyring crate's error message says "2560 chars", which
//! is the byte count). An OAuth credential bundle (access token, refresh
//! token, client id, all as JSON) is routinely bigger. So a long value is
//! split into numbered pieces under `key#0`, `key#1`, ... and the entry
//! under `key` itself records how many. Reading reassembles them; deleting
//! removes them all. Short values are stored as they are.
//!
//! Nothing here ever writes a secret to the SQLite file.

use taskrail_core::ports::{SecretError, SecretStore};

const SERVICE: &str = "taskrail";
/// Comfortably under the Windows limit of 1280, counted in UTF-16 code
/// units because that is what Windows counts.
const CHUNK_UNITS: usize = 1000;
const CHUNKED_MARKER: &str = "taskrail-chunks:";

/// The raw store underneath: one value per key, with whatever limits the
/// platform has. The keyring is the real one; tests use memory.
pub trait Backend {
    fn read(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn write(&self, key: &str, value: &str) -> Result<(), SecretError>;
    fn remove(&self, key: &str) -> Result<(), SecretError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Keyring;

impl Backend for Keyring {
    fn read(&self, key: &str) -> Result<Option<String>, SecretError> {
        match entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Store(e.to_string())),
        }
    }

    fn write(&self, key: &str, value: &str) -> Result<(), SecretError> {
        entry(key)?
            .set_password(value)
            .map_err(|e| SecretError::Store(e.to_string()))
    }

    fn remove(&self, key: &str) -> Result<(), SecretError> {
        match entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Store(e.to_string())),
        }
    }
}

fn entry(key: &str) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(SERVICE, key).map_err(|e| SecretError::Store(e.to_string()))
}

/// A secret store that splits long values into pieces the backend accepts.
#[derive(Debug, Default, Clone, Copy)]
pub struct Chunked<B: Backend>(pub B);

/// The store the app uses.
pub type OsSecrets = Chunked<Keyring>;

#[allow(non_upper_case_globals)]
pub const OsSecrets: OsSecrets = Chunked(Keyring);

fn chunk_key(key: &str, index: usize) -> String {
    format!("{key}#{index}")
}

/// Split by characters, never inside one, so every piece is valid text.
/// How long a value is the way Windows measures it.
fn utf16_units(value: &str) -> usize {
    value.encode_utf16().count()
}

/// Split into pieces of at most `CHUNK_UNITS` UTF-16 units, never inside a
/// character, so every piece is valid text.
pub(crate) fn split_into_chunks(value: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    let mut units = 0;
    for c in value.chars() {
        let len = c.len_utf16();
        if units + len > CHUNK_UNITS {
            pieces.push(std::mem::take(&mut piece));
            units = 0;
        }
        piece.push(c);
        units += len;
    }
    if !piece.is_empty() || pieces.is_empty() {
        pieces.push(piece);
    }
    pieces
}

pub(crate) fn chunk_count(marker: &str) -> Option<usize> {
    marker.strip_prefix(CHUNKED_MARKER)?.trim().parse().ok()
}

impl<B: Backend> Chunked<B> {
    /// Remove numbered pieces from `from` upward until one is missing.
    fn remove_chunks_from(&self, key: &str, from: usize) -> Result<(), SecretError> {
        let mut index = from;
        while self.0.read(&chunk_key(key, index))?.is_some() {
            self.0.remove(&chunk_key(key, index))?;
            index += 1;
        }
        Ok(())
    }
}

impl<B: Backend> SecretStore for Chunked<B> {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        let Some(head) = self.0.read(key)? else {
            return Ok(None);
        };
        let Some(count) = chunk_count(&head) else {
            return Ok(Some(head));
        };
        let mut value = String::new();
        for index in 0..count {
            let piece = self.0.read(&chunk_key(key, index))?.ok_or_else(|| {
                SecretError::Store(format!("piece {index} of {count} is missing for {key}"))
            })?;
            value.push_str(&piece);
        }
        Ok(Some(value))
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        if utf16_units(value) <= CHUNK_UNITS {
            self.0.write(key, value)?;
            return self.remove_chunks_from(key, 0);
        }
        let pieces = split_into_chunks(value);
        log::info!(
            "storing {key} as {} pieces ({} UTF-16 units)",
            pieces.len(),
            utf16_units(value)
        );
        for (index, piece) in pieces.iter().enumerate() {
            self.0.write(&chunk_key(key, index), piece)?;
        }
        self.0
            .write(key, &format!("{CHUNKED_MARKER}{}", pieces.len()))?;
        self.remove_chunks_from(key, pieces.len())
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.0.remove(key)?;
        self.remove_chunks_from(key, 0)
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
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// Behaves like Windows Credential Manager: refuses anything over 2560
    /// UTF-16 units.
    #[derive(Default)]
    struct WindowsLike(RefCell<HashMap<String, String>>);

    impl Backend for WindowsLike {
        fn read(&self, key: &str) -> Result<Option<String>, SecretError> {
            Ok(self.0.borrow().get(key).cloned())
        }
        fn write(&self, key: &str, value: &str) -> Result<(), SecretError> {
            if value.encode_utf16().count() > 2560 {
                return Err(SecretError::Store(
                    "longer than platform limit of 2560 chars".into(),
                ));
            }
            self.0.borrow_mut().insert(key.to_owned(), value.to_owned());
            Ok(())
        }
        fn remove(&self, key: &str) -> Result<(), SecretError> {
            self.0.borrow_mut().remove(key);
            Ok(())
        }
    }

    #[test]
    fn an_oauth_bundle_survives_the_windows_limit() {
        let store = Chunked(WindowsLike::default());
        let bundle = format!("{{\"client_id\":\"abc\",\"token_response\":{{\"access_token\":\"{}\",\"refresh_token\":\"{}\"}}}}", "a".repeat(3000), "r".repeat(1500));
        store.set("connector:tuesday:oauth", &bundle).unwrap();
        assert_eq!(
            store.get("connector:tuesday:oauth").unwrap().as_deref(),
            Some(bundle.as_str())
        );
        assert!(
            store.0 .0.borrow().len() >= 4,
            "head plus at least three pieces"
        );

        // A shorter replacement clears the leftover pieces.
        store.set("connector:tuesday:oauth", "short").unwrap();
        assert_eq!(
            store.get("connector:tuesday:oauth").unwrap().as_deref(),
            Some("short")
        );
        assert_eq!(store.0 .0.borrow().len(), 1);

        store.delete("connector:tuesday:oauth").unwrap();
        assert_eq!(store.get("connector:tuesday:oauth").unwrap(), None);
        assert!(store.0 .0.borrow().is_empty());
    }

    #[test]
    fn long_values_split_on_character_boundaries_and_rejoin() {
        let value: String = "é".repeat(CHUNK_UNITS * 2 + 7);
        let pieces = split_into_chunks(&value);
        assert_eq!(pieces.len(), 3);
        assert!(pieces.iter().all(|p| utf16_units(p) <= CHUNK_UNITS));
        assert_eq!(pieces.concat(), value);

        // Characters outside the BMP take two units and are never split.
        let wide: String = "😀".repeat(CHUNK_UNITS);
        let pieces = split_into_chunks(&wide);
        assert_eq!(pieces.len(), 2);
        assert!(pieces.iter().all(|p| utf16_units(p) <= CHUNK_UNITS));
        assert_eq!(pieces.concat(), wide);
    }

    #[test]
    fn the_marker_names_the_number_of_pieces() {
        assert_eq!(chunk_count("taskrail-chunks:3"), Some(3));
        assert_eq!(chunk_count("sk-ant-plain-key"), None);
        assert_eq!(chunk_count("taskrail-chunks:x"), None);
    }
}
