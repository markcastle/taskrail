//! Keeping the rail honest about what the remote systems say.
//!
//! Two directions, deliberately kept apart:
//!
//! - **Pull** ([`apply_remote`]): a connector's list result is merged into the
//!   repository. The remote system wins on everything it owns: title,
//!   status, priority, due date, link. The rail wins on what it owns: rank,
//!   and the active card. Items the remote no longer returns are removed.
//! - **Push** (the shell calls [`Connector::set_status`] after a local
//!   change): fire and forget from the user's point of view, with the next
//!   pull restoring remote truth if it failed.
//!
//! The connector call that fetches items is async and slow; the merge is
//! synchronous and fast. The shell does the fetch without any lock held,
//! then takes the repository lock only for the merge.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;
use taskrail_connector_api::{RemoteItem, StatusMap};
use taskrail_core::domain::{ConnectorId, Item, ItemId, Priority, Status};
use taskrail_core::ports::{ItemRepository, RepositoryError};
use taskrail_core::services::RANK_STEP;

/// What one pull did, for logs and the UI.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
}

/// Merge a connector's items into the repository. See the crate docs for
/// who wins on which field.
pub fn apply_remote(
    repo: &mut dyn ItemRepository,
    connector: &ConnectorId,
    remote: &[RemoteItem],
    map: &StatusMap,
    now: DateTime<Utc>,
) -> Result<SyncReport, RepositoryError> {
    let mut report = SyncReport::default();
    let existing: HashMap<ItemId, Item> = repo
        .all()?
        .into_iter()
        .filter(|item| {
            item.origin
                .as_ref()
                .is_some_and(|o| &o.connector == connector)
        })
        .map(|item| (item.id.clone(), item))
        .collect();

    // New items go below everything already in their section, best priority
    // first, so a sync never jumps the queue the user arranged.
    let mut bottom: HashMap<Status, i64> = HashMap::new();
    for item in repo.all()? {
        if !item.is_event() {
            let slot = bottom.entry(item.status).or_insert(item.rank);
            *slot = (*slot).max(item.rank);
        }
    }
    let mut incoming: Vec<&RemoteItem> = remote.iter().collect();
    incoming.sort_by(|a, b| {
        b.priority
            .unwrap_or(Priority::Normal)
            .cmp(&a.priority.unwrap_or(Priority::Normal))
    });

    let mut seen = HashSet::new();
    for remote_item in incoming {
        let id = RemoteItem::item_id(connector, &remote_item.external_id);
        if !seen.insert(id.clone()) {
            continue; // the remote listed it twice; keep the first
        }
        let previous = existing.get(&id);
        let section = remote_item
            .status
            .as_deref()
            .and_then(|s| map.section_for(s))
            .or(previous.map(|p| p.status))
            .unwrap_or(Status::Todo);
        let rank = match previous {
            Some(p) => p.rank,
            None => {
                let next = bottom
                    .get(&section)
                    .map_or(RANK_STEP, |max| max + RANK_STEP);
                bottom.insert(section, next);
                next
            }
        };
        let item = remote_item.to_item(connector, map, previous, rank, now);
        match previous {
            Some(p) if same_remote_fields(p, &item) => report.unchanged += 1,
            Some(_) => {
                repo.upsert(item)?;
                report.updated += 1;
            }
            None => {
                repo.upsert(item)?;
                report.added += 1;
            }
        }
    }

    for id in existing.keys() {
        if !seen.contains(id) {
            repo.remove(id)?;
            report.removed += 1;
        }
    }
    Ok(report)
}

/// Everything the remote system owns. `updated_at` is left out on purpose:
/// a changed timestamp with nothing else different is not worth a write.
fn same_remote_fields(a: &Item, b: &Item) -> bool {
    a.title == b.title
        && a.status == b.status
        && a.priority == b.priority
        && a.due == b.due
        && a.window == b.window
        && a.origin == b.origin
        && a.kind == b.kind
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::collections::BTreeMap;
    use taskrail_core::domain::ItemKind;
    use taskrail_core::memory::InMemoryItemRepository;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 4, 10, 0, 0).unwrap()
    }

    fn map() -> StatusMap {
        StatusMap {
            to_section: BTreeMap::from([
                ("Open".into(), Status::Todo),
                ("Doing".into(), Status::InProgress),
                ("Closed".into(), Status::Done),
            ]),
            canonical: BTreeMap::from([
                (Status::Todo, "Open".into()),
                (Status::InProgress, "Doing".into()),
                (Status::Done, "Closed".into()),
            ]),
        }
    }

    fn remote(id: &str, title: &str, status: &str, priority: Priority) -> RemoteItem {
        RemoteItem {
            external_id: id.into(),
            title: title.into(),
            kind: ItemKind::Task,
            status: Some(status.into()),
            priority: Some(priority),
            due: None,
            window: None,
            url: None,
            updated_at: None,
        }
    }

    fn connector() -> ConnectorId {
        ConnectorId::new("t")
    }

    fn todo_order(repo: &InMemoryItemRepository) -> Vec<String> {
        let mut items: Vec<Item> = repo
            .all()
            .unwrap()
            .into_iter()
            .filter(|i| i.status == Status::Todo)
            .collect();
        items.sort_by_key(|i| i.rank);
        items
            .into_iter()
            .map(|i| i.id.as_str().to_owned())
            .collect()
    }

    #[test]
    fn first_pull_adds_everything_below_local_items_best_priority_first() {
        let mut repo = InMemoryItemRepository::with_items([Item::task(
            "local-1",
            "Mine",
            Status::Todo,
            1000,
            now(),
        )]);
        let remote = [
            remote("a", "Low", "Open", Priority::Low),
            remote("b", "Urgent", "Open", Priority::Highest),
            remote("c", "Doing it", "Doing", Priority::Normal),
        ];
        let report = apply_remote(&mut repo, &connector(), &remote, &map(), now()).unwrap();
        assert_eq!(
            report,
            SyncReport {
                added: 3,
                ..Default::default()
            }
        );
        assert_eq!(todo_order(&repo), ["local-1", "t:b", "t:a"]);
        assert_eq!(
            repo.get(&ItemId::new("t:c")).unwrap().unwrap().status,
            Status::InProgress
        );
    }

    #[test]
    fn a_second_pull_updates_remote_fields_and_keeps_rank() {
        let mut repo = InMemoryItemRepository::default();
        let first = [remote("a", "Old title", "Open", Priority::Normal)];
        apply_remote(&mut repo, &connector(), &first, &map(), now()).unwrap();
        let mut arranged = repo.get(&ItemId::new("t:a")).unwrap().unwrap();
        arranged.rank = 42;
        repo.upsert(arranged).unwrap();

        let second = [remote("a", "New title", "Doing", Priority::High)];
        let report = apply_remote(&mut repo, &connector(), &second, &map(), now()).unwrap();
        assert_eq!(report.updated, 1);
        let item = repo.get(&ItemId::new("t:a")).unwrap().unwrap();
        assert_eq!(item.title, "New title");
        assert_eq!(item.status, Status::InProgress);
        assert_eq!(item.priority, Priority::High);
        assert_eq!(item.rank, 42);
    }

    #[test]
    fn unchanged_items_are_not_rewritten_and_missing_ones_are_removed() {
        let mut repo = InMemoryItemRepository::default();
        let first = [
            remote("a", "A", "Open", Priority::Normal),
            remote("b", "B", "Open", Priority::Normal),
        ];
        apply_remote(&mut repo, &connector(), &first, &map(), now()).unwrap();
        let second = [remote("a", "A", "Open", Priority::Normal)];
        let report = apply_remote(&mut repo, &connector(), &second, &map(), now()).unwrap();
        assert_eq!(
            report,
            SyncReport {
                unchanged: 1,
                removed: 1,
                ..Default::default()
            }
        );
        assert!(repo.get(&ItemId::new("t:b")).unwrap().is_none());
    }

    #[test]
    fn other_connectors_and_local_items_are_left_alone() {
        let other = ConnectorId::new("other");
        let mut repo = InMemoryItemRepository::with_items([
            Item::task("local-1", "Mine", Status::Todo, 1000, now()),
            remote("z", "Theirs", "Open", Priority::Normal).to_item(
                &other,
                &map(),
                None,
                1000,
                now(),
            ),
        ]);
        apply_remote(&mut repo, &connector(), &[], &map(), now()).unwrap();
        assert_eq!(repo.all().unwrap().len(), 2);
    }

    #[test]
    fn a_duplicate_external_id_in_one_pull_is_kept_once() {
        let mut repo = InMemoryItemRepository::default();
        let remote = [
            remote("a", "First", "Open", Priority::Normal),
            remote("a", "Again", "Open", Priority::Normal),
        ];
        let report = apply_remote(&mut repo, &connector(), &remote, &map(), now()).unwrap();
        assert_eq!(report.added, 1);
        assert_eq!(
            repo.get(&ItemId::new("t:a")).unwrap().unwrap().title,
            "First"
        );
    }
}
