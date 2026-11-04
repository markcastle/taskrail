//! What happens when the user does something to a card.
//!
//! [`RailService`] is the only thing that writes to the repository. It owns
//! the rules for where a card lands when it moves: new tasks go to the top,
//! a started task leads In Progress, a reopened task goes back to the top of
//! To Do, and ranks are renumbered when a drag runs out of room between two
//! neighbours. Writing status changes back to the system that owns a task
//! will hang off these same methods once connectors exist.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use chrono::{DateTime, Utc};

use crate::domain::{Item, ItemId, Status};
use crate::ports::{Clock, ItemRepository, RepositoryError};

/// Gap left between neighbouring ranks so a drop between two cards can
/// usually take the midpoint without touching anything else.
pub const RANK_STEP: i64 = 1_000;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("no item with id {0:?}")]
    NotFound(ItemId),
    #[error("{0:?} is a calendar event; the rail does not move events")]
    NotATask(ItemId),
}

pub struct RailService<R, C> {
    repo: R,
    clock: C,
}

impl<R: ItemRepository, C: Clock> RailService<R, C> {
    pub fn new(repo: R, clock: C) -> Self {
        Self { repo, clock }
    }

    pub fn repository(&self) -> &R {
        &self.repo
    }

    pub fn repository_mut(&mut self) -> &mut R {
        &mut self.repo
    }

    pub fn items(&self) -> Result<Vec<Item>, ServiceError> {
        Ok(self.repo.all()?)
    }

    /// Quick-add: a local task at the top of To Do, so the person sees it
    /// land and can drag it down if it does not belong there.
    pub fn add_task(&mut self, title: &str) -> Result<Item, ServiceError> {
        let now = self.clock.now();
        let title = title.trim();
        let id = new_local_id(now);
        let rank = self.top_rank(Status::Todo, None)?;
        let item = Item::task(id, title, Status::Todo, rank, now);
        self.repo.upsert(item.clone())?;
        Ok(item)
    }

    /// Pull a card into In Progress. It lands at the top because starting a
    /// task is the strongest signal of what you are doing next.
    pub fn start(&mut self, id: &ItemId) -> Result<Item, ServiceError> {
        self.move_to(id, Status::InProgress, 0)
    }

    /// Flick a card away. Done cards have no meaningful order; they sort by
    /// completion time.
    pub fn complete(&mut self, id: &ItemId) -> Result<Item, ServiceError> {
        let mut item = self.task(id)?;
        let now = self.clock.now();
        item.status = Status::Done;
        item.completed_at = Some(now);
        item.updated_at = now;
        self.repo.upsert(item.clone())?;
        Ok(item)
    }

    /// Bring a completed card back to the top of To Do.
    pub fn reopen(&mut self, id: &ItemId) -> Result<Item, ServiceError> {
        self.move_to(id, Status::Todo, 0)
    }

    /// Place a task at `index` within the section for `status`, counting only
    /// the tasks already in that section and not the moved one. Changes the
    /// status if the card came from elsewhere.
    pub fn move_to(
        &mut self,
        id: &ItemId,
        status: Status,
        index: usize,
    ) -> Result<Item, ServiceError> {
        let mut item = self.task(id)?;
        let now = self.clock.now();

        let mut section = self.section(status, Some(id))?;
        let index = index.min(section.len());
        let prev = index.checked_sub(1).map(|i| section[i].rank);
        let next = section.get(index).map(|item| item.rank);

        let rank = match rank_between(prev, next) {
            Some(rank) => rank,
            None => {
                // No room between the neighbours: renumber the section with
                // the moved card slotted in, then read its rank back.
                section.insert(index, item.clone());
                for (i, neighbour) in section.iter_mut().enumerate() {
                    neighbour.rank = (i as i64 + 1) * RANK_STEP;
                    if &neighbour.id != id {
                        neighbour.updated_at = now;
                        self.repo.upsert(neighbour.clone())?;
                    }
                }
                (index as i64 + 1) * RANK_STEP
            }
        };

        item.rank = rank;
        item.updated_at = now;
        if item.status != status {
            item.status = status;
            item.completed_at = (status == Status::Done).then_some(now);
        }
        self.repo.upsert(item.clone())?;
        Ok(item)
    }

    pub fn remove(&mut self, id: &ItemId) -> Result<(), ServiceError> {
        Ok(self.repo.remove(id)?)
    }

    fn task(&self, id: &ItemId) -> Result<Item, ServiceError> {
        let item = self
            .repo
            .get(id)?
            .ok_or_else(|| ServiceError::NotFound(id.clone()))?;
        if item.is_event() {
            return Err(ServiceError::NotATask(id.clone()));
        }
        Ok(item)
    }

    /// Tasks in one section, in rank order, optionally leaving one out.
    fn section(&self, status: Status, except: Option<&ItemId>) -> Result<Vec<Item>, ServiceError> {
        let mut items: Vec<Item> = self
            .repo
            .all()?
            .into_iter()
            .filter(|item| !item.is_event() && item.status == status)
            .filter(|item| Some(&item.id) != except)
            .collect();
        items.sort_by_key(|item| item.rank);
        Ok(items)
    }

    fn top_rank(&self, status: Status, except: Option<&ItemId>) -> Result<i64, ServiceError> {
        let section = self.section(status, except)?;
        Ok(section
            .first()
            .map_or(RANK_STEP, |first| first.rank - RANK_STEP))
    }
}

/// The midpoint between two ranks, or `None` when they are adjacent and the
/// section needs renumbering. Open ends get a full step of room.
pub fn rank_between(prev: Option<i64>, next: Option<i64>) -> Option<i64> {
    match (prev, next) {
        (None, None) => Some(RANK_STEP),
        (Some(prev), None) => Some(prev + RANK_STEP),
        (None, Some(next)) => Some(next - RANK_STEP),
        (Some(prev), Some(next)) if next - prev >= 2 => Some(prev + (next - prev) / 2),
        _ => None,
    }
}

/// Ids for tasks that live only in the rail. Time-ordered for readability,
/// with a dash of randomness so two quick-adds in one instant never collide.
fn new_local_id(now: DateTime<Utc>) -> String {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_i64(now.timestamp_nanos_opt().unwrap_or_default());
    format!(
        "local-{}-{:08x}",
        now.format("%Y%m%d%H%M%S"),
        hasher.finish() as u32
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TimeWindow;
    use crate::memory::InMemoryItemRepository;
    use chrono::{Duration, TimeZone};

    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 4, 10, 0, 0).unwrap()
    }

    fn service(items: Vec<Item>) -> RailService<InMemoryItemRepository, FixedClock> {
        RailService::new(InMemoryItemRepository::with_items(items), FixedClock(now()))
    }

    fn todo(id: &str, rank: i64) -> Item {
        Item::task(id, id, Status::Todo, rank, now() - Duration::days(1))
    }

    fn order(
        service: &RailService<InMemoryItemRepository, FixedClock>,
        status: Status,
    ) -> Vec<String> {
        service
            .section(status, None)
            .unwrap()
            .into_iter()
            .map(|item| item.id.as_str().to_owned())
            .collect()
    }

    #[test]
    fn quick_add_lands_at_the_top_of_todo() {
        let mut s = service(vec![todo("a", 1000), todo("b", 2000)]);
        let added = s.add_task("  Reply to supplier ").unwrap();
        assert_eq!(added.title, "Reply to supplier");
        assert!(added.id.as_str().starts_with("local-"));
        assert_eq!(order(&s, Status::Todo)[0], added.id.as_str());
    }

    #[test]
    fn two_quick_adds_get_different_ids() {
        let mut s = service(vec![]);
        let a = s.add_task("one").unwrap();
        let b = s.add_task("two").unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(s.items().unwrap().len(), 2);
    }

    #[test]
    fn starting_a_task_leads_in_progress() {
        let mut s = service(vec![
            todo("a", 1000),
            Item::task("busy", "busy", Status::InProgress, 1000, now()),
        ]);
        let started = s.start(&ItemId::new("a")).unwrap();
        assert_eq!(started.status, Status::InProgress);
        assert_eq!(started.updated_at, now());
        assert_eq!(order(&s, Status::InProgress), ["a", "busy"]);
        assert!(order(&s, Status::Todo).is_empty());
    }

    #[test]
    fn completing_stamps_the_time() {
        let mut s = service(vec![todo("a", 1000)]);
        let done = s.complete(&ItemId::new("a")).unwrap();
        assert_eq!(done.status, Status::Done);
        assert_eq!(done.completed_at, Some(now()));
    }

    #[test]
    fn reopening_clears_completion_and_goes_to_the_top() {
        let mut s = service(vec![
            todo("a", 1000),
            todo("done", 1000).completed_at(now() - Duration::hours(1)),
        ]);
        let reopened = s.reopen(&ItemId::new("done")).unwrap();
        assert_eq!(reopened.status, Status::Todo);
        assert_eq!(reopened.completed_at, None);
        assert_eq!(order(&s, Status::Todo), ["done", "a"]);
    }

    #[test]
    fn dragging_within_a_section_takes_the_midpoint() {
        let mut s = service(vec![todo("a", 1000), todo("b", 2000), todo("c", 3000)]);
        let moved = s.move_to(&ItemId::new("c"), Status::Todo, 1).unwrap();
        assert_eq!(moved.rank, 1500);
        assert_eq!(order(&s, Status::Todo), ["a", "c", "b"]);
    }

    #[test]
    fn dragging_to_the_end_goes_past_the_last_card() {
        let mut s = service(vec![todo("a", 1000), todo("b", 2000)]);
        s.move_to(&ItemId::new("a"), Status::Todo, 5).unwrap();
        assert_eq!(order(&s, Status::Todo), ["b", "a"]);
    }

    #[test]
    fn a_full_gap_renumbers_the_section() {
        let mut s = service(vec![todo("a", 10), todo("b", 11), todo("c", 12)]);
        let moved = s.move_to(&ItemId::new("c"), Status::Todo, 1).unwrap();
        assert_eq!(order(&s, Status::Todo), ["a", "c", "b"]);
        let ranks: Vec<i64> = s
            .section(Status::Todo, None)
            .unwrap()
            .iter()
            .map(|i| i.rank)
            .collect();
        assert_eq!(ranks, [1000, 2000, 3000]);
        assert_eq!(moved.rank, 2000);
    }

    #[test]
    fn dragging_into_in_progress_changes_status() {
        let mut s = service(vec![
            todo("a", 1000),
            Item::task("x", "x", Status::InProgress, 1000, now()),
            Item::task("y", "y", Status::InProgress, 2000, now()),
        ]);
        let moved = s.move_to(&ItemId::new("a"), Status::InProgress, 1).unwrap();
        assert_eq!(moved.status, Status::InProgress);
        assert_eq!(order(&s, Status::InProgress), ["x", "a", "y"]);
    }

    #[test]
    fn events_cannot_be_moved() {
        let window = TimeWindow {
            start: now(),
            end: now() + Duration::hours(1),
        };
        let mut s = service(vec![Item::event("standup", "standup", window, now())]);
        let err = s.start(&ItemId::new("standup")).unwrap_err();
        assert!(matches!(err, ServiceError::NotATask(_)));
    }

    #[test]
    fn unknown_ids_are_reported() {
        let mut s = service(vec![]);
        let err = s.complete(&ItemId::new("ghost")).unwrap_err();
        assert!(matches!(err, ServiceError::NotFound(_)));
    }

    #[test]
    fn rank_between_cases() {
        assert_eq!(rank_between(None, None), Some(RANK_STEP));
        assert_eq!(rank_between(Some(1000), None), Some(2000));
        assert_eq!(rank_between(None, Some(1000)), Some(0));
        assert_eq!(rank_between(Some(1000), Some(2000)), Some(1500));
        assert_eq!(rank_between(Some(1000), Some(1001)), None);
    }
}
