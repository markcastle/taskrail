//! The things Task Rail reasons about.
//!
//! A rail shows *items*. Most are tasks, which you move between sections.
//! Some are calendar events, which are pinned to a time and only ever come
//! and go. Everything here is plain data; the behaviour lives in
//! [`crate::usecases`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Identifies an item inside the rail. It stays the same for the item's whole
/// life, even if the remote system renames or moves the underlying record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemId(String);

impl ItemId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Identifies one configured connector: one Tuesday workspace, one calendar,
/// the local inbox.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectorId(String);

impl ConnectorId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What kind of thing an item is. Tasks move between sections. Events sit at
/// a time and cannot be started or completed from the rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Task,
    Event,
}

/// The three states every connector's statuses collapse into. The UI calls
/// `Todo` "Top Priorities", but underneath it is just the work not yet begun.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Todo,
    InProgress,
    Done,
}

/// Coarse priority as remote systems express it. The rail's own ordering is a
/// finer thing (see [`Item::rank`]); priority seeds that order for newly
/// synced items and shows as a badge.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Lowest,
    Low,
    #[default]
    Normal,
    High,
    Highest,
}

/// Where an item came from, so that a change made in the rail can be written
/// back to the system that owns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub connector: ConnectorId,
    pub external_id: String,
    pub url: Option<String>,
}

/// The span of time an event occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// One card on the rail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub kind: ItemKind,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    /// Position within its section; lower ranks sit higher. Ranks are sparse
    /// integers so a drag between two neighbours rarely forces renumbering.
    pub rank: i64,
    /// `None` for tasks that live only in the rail.
    pub origin: Option<Origin>,
    /// Always `Some` for events, always `None` for tasks.
    pub window: Option<TimeWindow>,
    /// When a task is due, if the source system says. Events use `window`.
    pub due: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl Item {
    /// A task with sensible defaults. Chain the `with_*` builders for the rest.
    pub fn task(
        id: impl Into<String>,
        title: impl Into<String>,
        status: Status,
        rank: i64,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: ItemId::new(id),
            kind: ItemKind::Task,
            title: title.into(),
            status,
            priority: Priority::Normal,
            rank,
            origin: None,
            window: None,
            due: None,
            completed_at: None,
            updated_at,
        }
    }

    /// A calendar event. Events are always `Todo`; the rail never changes them.
    pub fn event(
        id: impl Into<String>,
        title: impl Into<String>,
        window: TimeWindow,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: ItemId::new(id),
            kind: ItemKind::Event,
            title: title.into(),
            status: Status::Todo,
            priority: Priority::Normal,
            rank: 0,
            origin: None,
            window: Some(window),
            due: None,
            completed_at: None,
            updated_at,
        }
    }

    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_origin(mut self, origin: Origin) -> Self {
        self.origin = Some(origin);
        self
    }

    pub fn due(mut self, at: DateTime<Utc>) -> Self {
        self.due = Some(at);
        self
    }

    /// Marks the item done at the given moment.
    pub fn completed_at(mut self, at: DateTime<Utc>) -> Self {
        self.status = Status::Done;
        self.completed_at = Some(at);
        self
    }

    pub fn is_event(&self) -> bool {
        self.kind == ItemKind::Event
    }
}
