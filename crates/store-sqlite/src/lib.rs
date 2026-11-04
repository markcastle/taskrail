//! Where the rail's data lives between runs.
//!
//! One SQLite file holds two tables. `items` keeps each item as JSON in a
//! `data` column, with the few fields the queries need (`status`, `rank`)
//! copied out beside it. That makes adding a field to [`Item`] a one-line
//! change in the core and a no-op here. `kv` holds small named blobs such as
//! the settings and the active item id.
//!
//! The bundled SQLite is compiled in, so there is nothing to install on the
//! user's machine. Everything here is synchronous; the shell calls it from
//! command handlers, which Tauri already runs off the main thread.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use taskrail_core::domain::{Item, ItemId};
use taskrail_core::ports::{ItemRepository, KeyValueError, KeyValueStore, RepositoryError};

const SCHEMA_VERSION: i32 = 1;

pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    /// Open or create the database at `path`, migrating it if needed.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, rusqlite::Error> {
        Self::from_connection(Connection::open(path)?)
    }

    /// A throwaway database for tests and for running without a data dir.
    pub fn in_memory() -> Result<Self, rusqlite::Error> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, rusqlite::Error> {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), rusqlite::Error> {
        let version: i32 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            self.conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS items (
                    id     TEXT PRIMARY KEY,
                    status TEXT NOT NULL,
                    rank   INTEGER NOT NULL,
                    data   TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS items_status_rank ON items (status, rank);
                 CREATE TABLE IF NOT EXISTS kv (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                 );",
            )?;
        }
        if version != SCHEMA_VERSION {
            self.conn
                .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(())
    }
}

fn storage_err(e: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::Storage(e.to_string())
}

fn decode(data: String) -> Result<Item, RepositoryError> {
    serde_json::from_str(&data).map_err(storage_err)
}

impl ItemRepository for SqliteStore {
    fn all(&self) -> Result<Vec<Item>, RepositoryError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT data FROM items ORDER BY status, rank, id")
            .map_err(storage_err)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(storage_err)?;
        rows.map(|row| row.map_err(storage_err).and_then(decode))
            .collect()
    }

    fn get(&self, id: &ItemId) -> Result<Option<Item>, RepositoryError> {
        self.conn
            .query_row(
                "SELECT data FROM items WHERE id = ?1",
                params![id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(storage_err)?
            .map(decode)
            .transpose()
    }

    fn upsert(&mut self, item: Item) -> Result<(), RepositoryError> {
        let data = serde_json::to_string(&item).map_err(storage_err)?;
        let status = serde_json::to_value(item.status)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        self.conn
            .execute(
                "INSERT INTO items (id, status, rank, data) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status,
                                               rank = excluded.rank,
                                               data = excluded.data",
                params![item.id.as_str(), status, item.rank, data],
            )
            .map_err(storage_err)?;
        Ok(())
    }

    fn remove(&mut self, id: &ItemId) -> Result<(), RepositoryError> {
        let changed = self
            .conn
            .execute("DELETE FROM items WHERE id = ?1", params![id.as_str()])
            .map_err(storage_err)?;
        if changed == 0 {
            return Err(RepositoryError::NotFound(id.clone()));
        }
        Ok(())
    }
}

fn kv_err(e: impl std::fmt::Display) -> KeyValueError {
    KeyValueError::Store(e.to_string())
}

impl KeyValueStore for SqliteStore {
    fn get_value(&self, key: &str) -> Result<Option<String>, KeyValueError> {
        self.conn
            .query_row("SELECT value FROM kv WHERE key = ?1", params![key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(kv_err)
    }

    fn set_value(&mut self, key: &str, value: &str) -> Result<(), KeyValueError> {
        self.conn
            .execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(kv_err)?;
        Ok(())
    }

    fn remove_value(&mut self, key: &str) -> Result<(), KeyValueError> {
        self.conn
            .execute("DELETE FROM kv WHERE key = ?1", params![key])
            .map_err(kv_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};
    use taskrail_core::domain::{Priority, Status, TimeWindow};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 4, 10, 0, 0).unwrap()
    }

    #[test]
    fn items_round_trip_with_every_field() {
        let mut store = SqliteStore::in_memory().unwrap();
        let task = Item::task("t1", "Write the store", Status::InProgress, 1500, now())
            .with_priority(Priority::High)
            .due(now() + Duration::hours(4));
        let event = Item::event(
            "e1",
            "Standup",
            TimeWindow {
                start: now(),
                end: now() + Duration::minutes(15),
            },
            now(),
        );
        store.upsert(task.clone()).unwrap();
        store.upsert(event.clone()).unwrap();

        assert_eq!(store.get(&task.id).unwrap(), Some(task.clone()));
        assert_eq!(store.get(&event.id).unwrap(), Some(event.clone()));
        assert_eq!(store.all().unwrap().len(), 2);
    }

    #[test]
    fn upsert_replaces_and_remove_removes() {
        let mut store = SqliteStore::in_memory().unwrap();
        let task = Item::task("t1", "First title", Status::Todo, 1000, now());
        store.upsert(task.clone()).unwrap();
        let renamed = Item {
            title: "Second title".into(),
            ..task.clone()
        };
        store.upsert(renamed.clone()).unwrap();
        assert_eq!(store.get(&task.id).unwrap().unwrap().title, "Second title");

        store.remove(&task.id).unwrap();
        assert!(store.get(&task.id).unwrap().is_none());
        assert!(matches!(
            store.remove(&task.id),
            Err(RepositoryError::NotFound(_))
        ));
    }

    #[test]
    fn key_values_persist_and_overwrite() {
        let mut store = SqliteStore::in_memory().unwrap();
        assert_eq!(store.get_value("active").unwrap(), None);
        store.set_value("active", "t1").unwrap();
        store.set_value("active", "t2").unwrap();
        assert_eq!(store.get_value("active").unwrap().as_deref(), Some("t2"));
        store.remove_value("active").unwrap();
        assert_eq!(store.get_value("active").unwrap(), None);
    }

    #[test]
    fn a_file_survives_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rail.sqlite");
        {
            let mut store = SqliteStore::open(&path).unwrap();
            store
                .upsert(Item::task("t1", "Persist me", Status::Todo, 1000, now()))
                .unwrap();
            store.set_value("settings", "{}").unwrap();
        }
        let store = SqliteStore::open(&path).unwrap();
        assert_eq!(store.all().unwrap().len(), 1);
        assert_eq!(store.get_value("settings").unwrap().as_deref(), Some("{}"));
    }
}
