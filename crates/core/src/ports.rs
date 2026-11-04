//! The seams between the core and the outside world.
//!
//! Each trait here is implemented by an adapter crate: SQLite for the
//! repository, the OS keychain for secrets, the system clock for time. Tests
//! swap in fakes. Nothing in the core knows or cares which one is behind the
//! trait, which is the whole point.

use chrono::{DateTime, Utc};

use crate::domain::{Item, ItemId};

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("storage failure: {0}")]
    Storage(String),
    #[error("no item with id {0:?}")]
    NotFound(ItemId),
}

/// Where the rail's items live between runs.
pub trait ItemRepository {
    fn all(&self) -> Result<Vec<Item>, RepositoryError>;
    fn get(&self, id: &ItemId) -> Result<Option<Item>, RepositoryError>;
    fn upsert(&mut self, item: Item) -> Result<(), RepositoryError>;
    fn remove(&mut self, id: &ItemId) -> Result<(), RepositoryError>;
}

/// Time, as something you can fake in a test.
pub trait Clock {
    fn now(&self) -> DateTime<Utc>;
}

/// The real clock. Lives here because it has no dependencies and every
/// adapter would otherwise write its own.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("secret store failure: {0}")]
    Store(String),
}

/// API keys and tokens. Backed by the OS keychain in the real app; never by a
/// plain file.
pub trait SecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, key: &str, value: &str) -> Result<(), SecretError>;
    fn delete(&self, key: &str) -> Result<(), SecretError>;
}

#[derive(Debug, thiserror::Error)]
pub enum KeyValueError {
    #[error("key-value store failure: {0}")]
    Store(String),
}

/// Small named blobs that need to survive a restart: the settings, the
/// active item. Anything bigger or more structured deserves its own port.
pub trait KeyValueStore {
    fn get_value(&self, key: &str) -> Result<Option<String>, KeyValueError>;
    fn set_value(&mut self, key: &str, value: &str) -> Result<(), KeyValueError>;
    fn remove_value(&mut self, key: &str) -> Result<(), KeyValueError>;
}
