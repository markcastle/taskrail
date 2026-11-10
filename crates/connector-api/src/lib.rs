//! The contract between the rail and the systems that own its tasks.
//!
//! A connector is anything that can answer "what is on my plate?" and,
//! optionally, "mark this one as started". Jira, Trello, a calendar, a local
//! inbox and any MCP server all fit, as long as they translate into
//! [`RemoteItem`] on the way in and accept one of three statuses on the way
//! out. There is no shared task standard across those products; this crate
//! is the small one Task Rail uses instead.
//!
//! One trait rather than one per capability: connectors are held as trait
//! objects in a registry, and a single object with capability flags is
//! simpler to hold and to test than a bundle of optional traits. A connector
//! that cannot write simply reports that and keeps the default method, which
//! returns [`ConnectorError::Unsupported`].

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use taskrail_core::domain::{
    ConnectorId, Item, ItemId, ItemKind, Origin, Priority, Status, TimeWindow,
};

/// An item as the remote system describes it, before the rail has decided
/// where it goes. `status` is the remote system's own word for it; the
/// [`StatusMap`] turns that into a section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteItem {
    pub external_id: String,
    pub title: String,
    #[serde(default = "default_kind")]
    pub kind: ItemKind,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<Priority>,
    #[serde(default)]
    pub due: Option<DateTime<Utc>>,
    #[serde(default)]
    pub window: Option<TimeWindow>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    /// Where it sits in the source system, outermost first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trail: Vec<String>,
}

fn default_kind() -> ItemKind {
    ItemKind::Task
}

impl RemoteItem {
    /// The rail's id for an item from this connector. Stable across syncs.
    pub fn item_id(connector: &ConnectorId, external_id: &str) -> ItemId {
        ItemId::new(format!("{}:{}", connector.as_str(), external_id))
    }

    /// Turn this into a card. Fields the rail owns (rank, and the status if
    /// the remote one is unknown to the map) are taken from `existing` when
    /// there is one, so a sync never shuffles what the user arranged.
    pub fn to_item(
        &self,
        connector: &ConnectorId,
        map: &StatusMap,
        existing: Option<&Item>,
        rank: i64,
        now: DateTime<Utc>,
    ) -> Item {
        let status = match self.kind {
            ItemKind::Event => Status::Todo,
            ItemKind::Task => self
                .status
                .as_deref()
                .and_then(|s| map.section_for(s))
                .or(existing.map(|e| e.status))
                .unwrap_or(Status::Todo),
        };
        let completed_at = match (status, existing) {
            (Status::Done, Some(e)) if e.status == Status::Done => e.completed_at,
            (Status::Done, _) => Some(self.updated_at.unwrap_or(now)),
            _ => None,
        };
        Item {
            id: Self::item_id(connector, &self.external_id),
            kind: self.kind,
            title: self.title.clone(),
            status,
            priority: self.priority.unwrap_or_default(),
            rank: existing.map_or(rank, |e| e.rank),
            origin: Some(Origin {
                connector: connector.clone(),
                external_id: self.external_id.clone(),
                url: self.url.clone(),
            }),
            window: self.window,
            due: self.due,
            completed_at,
            updated_at: self.updated_at.unwrap_or(now),
            trail: self.trail.clone(),
        }
    }
}

/// How a remote system's status words map onto the rail's three sections,
/// and which word to send back when a card moves. Several remote statuses
/// can land in one section; each section has one canonical remote status.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusMap {
    pub to_section: BTreeMap<String, Status>,
    pub canonical: BTreeMap<Status, String>,
}

impl StatusMap {
    pub fn section_for(&self, remote_status: &str) -> Option<Status> {
        self.to_section.get(remote_status).copied().or_else(|| {
            let lower = remote_status.to_lowercase();
            self.to_section
                .iter()
                .find(|(k, _)| k.to_lowercase() == lower)
                .map(|(_, v)| *v)
        })
    }

    pub fn remote_for(&self, status: Status) -> Option<&str> {
        self.canonical.get(&status).map(String::as_str)
    }

    /// A map is usable when every section can be written back to.
    pub fn is_complete(&self) -> bool {
        [Status::Todo, Status::InProgress, Status::Done]
            .iter()
            .all(|s| self.canonical.contains_key(s))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub read: bool,
    pub write_status: bool,
    pub create: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorInfo {
    pub id: ConnectorId,
    pub name: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("could not reach the server: {0}")]
    Transport(String),
    #[error("the server answered in a way the connector did not expect: {0}")]
    Protocol(String),
    #[error("sign-in needed: {0}")]
    Auth(String),
    #[error("this connector cannot {0}")]
    Unsupported(&'static str),
    #[error("the connector is misconfigured: {0}")]
    Invalid(String),
}

pub type ConnectorResult<T> = Result<T, ConnectorError>;

#[async_trait]
pub trait Connector: Send + Sync {
    fn info(&self) -> ConnectorInfo;

    /// The status map this connector was configured with.
    fn status_map(&self) -> &StatusMap;

    /// Everything currently on the user's plate in the remote system.
    async fn list(&self) -> ConnectorResult<Vec<RemoteItem>>;

    /// Move an item to the section's canonical remote status.
    async fn set_status(&self, external_id: &str, status: Status) -> ConnectorResult<()> {
        let _ = (external_id, status);
        Err(ConnectorError::Unsupported("change a status"))
    }

    /// Create an item in the remote system from a title.
    async fn create(&self, title: &str) -> ConnectorResult<RemoteItem> {
        let _ = title;
        Err(ConnectorError::Unsupported("create items"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 4, 10, 0, 0).unwrap()
    }

    fn map() -> StatusMap {
        StatusMap {
            to_section: BTreeMap::from([
                ("Backlog".into(), Status::Todo),
                ("Selected".into(), Status::Todo),
                ("In Progress".into(), Status::InProgress),
                ("Done".into(), Status::Done),
            ]),
            canonical: BTreeMap::from([
                (Status::Todo, "Selected".into()),
                (Status::InProgress, "In Progress".into()),
                (Status::Done, "Done".into()),
            ]),
        }
    }

    fn remote(id: &str, status: &str) -> RemoteItem {
        RemoteItem {
            external_id: id.into(),
            title: format!("Item {id}"),
            kind: ItemKind::Task,
            status: Some(status.into()),
            priority: None,
            due: None,
            window: None,
            url: Some(format!("https://example.test/{id}")),
            updated_at: None,
            trail: vec![],
        }
    }

    #[test]
    fn status_map_round_trips_through_json_and_matches_case_insensitively() {
        let map = map();
        let json = serde_json::to_string(&map).unwrap();
        let back: StatusMap = serde_json::from_str(&json).unwrap();
        assert_eq!(back, map);
        assert_eq!(map.section_for("in progress"), Some(Status::InProgress));
        assert_eq!(map.section_for("Nope"), None);
        assert_eq!(map.remote_for(Status::Done), Some("Done"));
        assert!(map.is_complete());
        assert!(!StatusMap::default().is_complete());
    }

    #[test]
    fn a_new_remote_item_becomes_a_card_with_its_origin() {
        let connector = ConnectorId::new("tuesday");
        let item = remote("42", "In Progress").to_item(&connector, &map(), None, 1500, now());
        assert_eq!(item.id.as_str(), "tuesday:42");
        assert_eq!(item.status, Status::InProgress);
        assert_eq!(item.rank, 1500);
        assert_eq!(item.origin.as_ref().unwrap().external_id, "42");
        assert_eq!(item.updated_at, now());
    }

    #[test]
    fn a_resynced_item_keeps_the_users_rank() {
        let connector = ConnectorId::new("tuesday");
        let first = remote("42", "Selected").to_item(&connector, &map(), None, 1500, now());
        let arranged = Item { rank: 250, ..first };
        let again =
            remote("42", "Selected").to_item(&connector, &map(), Some(&arranged), 9999, now());
        assert_eq!(again.rank, 250);
    }

    #[test]
    fn an_unknown_remote_status_keeps_the_existing_section() {
        let connector = ConnectorId::new("tuesday");
        let first = remote("42", "In Progress").to_item(&connector, &map(), None, 1, now());
        let again = remote("42", "Weird").to_item(&connector, &map(), Some(&first), 1, now());
        assert_eq!(again.status, Status::InProgress);
        let fresh = remote("43", "Weird").to_item(&connector, &map(), None, 1, now());
        assert_eq!(fresh.status, Status::Todo);
    }

    #[test]
    fn done_items_get_a_completion_time_once() {
        let connector = ConnectorId::new("tuesday");
        let done = remote("42", "Done").to_item(&connector, &map(), None, 1, now());
        assert_eq!(done.completed_at, Some(now()));
        let later = now() + chrono::Duration::hours(2);
        let again = remote("42", "Done").to_item(&connector, &map(), Some(&done), 1, later);
        assert_eq!(
            again.completed_at,
            Some(now()),
            "completion time is kept across syncs"
        );
    }

    #[test]
    fn remote_items_deserialize_with_only_the_required_fields() {
        let item: RemoteItem =
            serde_json::from_str(r#"{"external_id":"1","title":"Minimal"}"#).unwrap();
        assert_eq!(item.kind, ItemKind::Task);
        assert_eq!(item.status, None);
    }
}
