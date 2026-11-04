//! An in-memory [`ItemRepository`]. Used by tests and by the shell before the
//! SQLite store exists. Not a cache; nothing survives a restart.

use std::collections::HashMap;

use crate::domain::{Item, ItemId};
use crate::ports::{ItemRepository, RepositoryError};

#[derive(Debug, Default, Clone)]
pub struct InMemoryItemRepository {
    items: HashMap<ItemId, Item>,
}

impl InMemoryItemRepository {
    pub fn with_items(items: impl IntoIterator<Item = Item>) -> Self {
        Self {
            items: items
                .into_iter()
                .map(|item| (item.id.clone(), item))
                .collect(),
        }
    }
}

impl ItemRepository for InMemoryItemRepository {
    fn all(&self) -> Result<Vec<Item>, RepositoryError> {
        let mut items: Vec<Item> = self.items.values().cloned().collect();
        items.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(items)
    }

    fn get(&self, id: &ItemId) -> Result<Option<Item>, RepositoryError> {
        Ok(self.items.get(id).cloned())
    }

    fn upsert(&mut self, item: Item) -> Result<(), RepositoryError> {
        self.items.insert(item.id.clone(), item);
        Ok(())
    }

    fn remove(&mut self, id: &ItemId) -> Result<(), RepositoryError> {
        self.items
            .remove(id)
            .map(|_| ())
            .ok_or_else(|| RepositoryError::NotFound(id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Status;
    use chrono::Utc;

    #[test]
    fn upsert_then_get_then_remove() {
        let mut repo = InMemoryItemRepository::default();
        let item = Item::task("a", "Write tests", Status::Todo, 100, Utc::now());
        repo.upsert(item.clone()).unwrap();
        assert_eq!(repo.get(&item.id).unwrap(), Some(item.clone()));
        repo.remove(&item.id).unwrap();
        assert!(repo.get(&item.id).unwrap().is_none());
        assert!(matches!(
            repo.remove(&item.id),
            Err(RepositoryError::NotFound(_))
        ));
    }
}
