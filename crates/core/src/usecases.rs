//! Turning a pile of items into what the rail shows.
//!
//! This is where the product rules live: the fold, the order inside each
//! section, which calendar events get pulled in and when, what counts as
//! "done today". If a behaviour question starts with "what should the rail
//! show when…", the answer is a test in this file.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::domain::{Item, ItemId, Status};
use crate::settings::RailSettings;

/// Everything the rail needs to draw itself, already ordered and trimmed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RailView {
    /// The next calendar event that has not finished yet, if any.
    pub up_next: Option<Item>,
    /// Top Priorities, already cut at the fold. Events close to starting come
    /// first, then tasks in rank order.
    pub top: Vec<Item>,
    /// How many items sit behind the fold. Always `later.len()`.
    pub top_hidden: usize,
    /// The items behind the fold, in the order they would have followed.
    pub later: Vec<Item>,
    /// Never capped. The active item, if any, is first.
    pub in_progress: Vec<Item>,
    /// The one item the user is focused on. Always a member of `in_progress`.
    pub active: Option<ItemId>,
    /// Completed since midnight UTC, newest first.
    pub done_today: Vec<Item>,
}

/// `expand_top` lifts the fold, for when the person taps "more".
pub fn build_rail_view(
    items: &[Item],
    active: Option<&ItemId>,
    now: DateTime<Utc>,
    settings: &RailSettings,
    expand_top: bool,
) -> RailView {
    let lead = Duration::minutes(i64::from(settings.event_lead_minutes));

    let mut upcoming: Vec<&Item> = items
        .iter()
        .filter(|item| item.is_event())
        .filter(|item| item.window.is_some_and(|w| w.end > now))
        .collect();
    upcoming.sort_by_key(|item| item.window.map(|w| w.start));
    let up_next = upcoming.first().map(|item| (*item).clone());

    let mut top: Vec<Item> = upcoming
        .iter()
        .filter(|item| item.window.is_some_and(|w| w.start <= now + lead))
        .map(|item| (*item).clone())
        .collect();

    let mut todo: Vec<Item> = items
        .iter()
        .filter(|item| !item.is_event() && item.status == Status::Todo)
        .cloned()
        .collect();
    todo.sort_by(|a, b| a.rank.cmp(&b.rank).then(b.priority.cmp(&a.priority)));
    top.extend(todo);

    let limit = if expand_top {
        top.len()
    } else {
        settings.effective_top_limit()
    };
    let later = top.split_off(limit.min(top.len()));
    let top_hidden = later.len();

    let mut in_progress: Vec<Item> = items
        .iter()
        .filter(|item| !item.is_event() && item.status == Status::InProgress)
        .cloned()
        .collect();
    in_progress.sort_by(|a, b| {
        let a_active = Some(&a.id) == active;
        let b_active = Some(&b.id) == active;
        b_active.cmp(&a_active).then(a.rank.cmp(&b.rank))
    });
    let active = active
        .filter(|id| in_progress.iter().any(|item| &item.id == *id))
        .cloned();

    let today = now.date_naive();
    let mut done_today: Vec<Item> = items
        .iter()
        .filter(|item| !item.is_event() && item.status == Status::Done)
        .filter(|item| item.completed_at.is_some_and(|t| t.date_naive() == today))
        .cloned()
        .collect();
    done_today.sort_by_key(|item| std::cmp::Reverse(item.completed_at));

    RailView {
        up_next,
        top,
        top_hidden,
        later,
        in_progress,
        active,
        done_today,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Priority, TimeWindow};
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 4, 10, 0, 0).unwrap()
    }

    fn todo(id: &str, rank: i64) -> Item {
        Item::task(id, id, Status::Todo, rank, now())
    }

    fn event(id: &str, starts_in_minutes: i64, lasts_minutes: i64) -> Item {
        let start = now() + Duration::minutes(starts_in_minutes);
        let window = TimeWindow {
            start,
            end: start + Duration::minutes(lasts_minutes),
        };
        Item::event(id, id, window, now())
    }

    fn view(items: &[Item], active: Option<&ItemId>) -> RailView {
        build_rail_view(items, active, now(), &RailSettings::default(), false)
    }

    #[test]
    fn top_priorities_fold_at_the_limit_in_rank_order() {
        let items: Vec<Item> = (1..=7).map(|n| todo(&format!("t{n}"), n * 100)).collect();
        let v = view(&items, None);
        let shown: Vec<&str> = v.top.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(shown, ["t1", "t2", "t3", "t4", "t5"]);
        assert_eq!(v.top_hidden, 2);
    }

    #[test]
    fn the_fold_never_reveals_more_than_ten() {
        let items: Vec<Item> = (1..=12).map(|n| todo(&format!("t{n}"), n)).collect();
        let settings = RailSettings {
            top_limit: 50,
            ..RailSettings::default()
        };
        let v = build_rail_view(&items, None, now(), &settings, false);
        assert_eq!(v.top.len(), 10);
        assert_eq!(v.top_hidden, 2);
    }

    #[test]
    fn expanding_lifts_the_fold() {
        let items: Vec<Item> = (1..=12).map(|n| todo(&format!("t{n}"), n)).collect();
        let v = build_rail_view(&items, None, now(), &RailSettings::default(), true);
        assert_eq!(v.top.len(), 12);
        assert_eq!(v.top_hidden, 0);
    }

    #[test]
    fn equal_rank_breaks_ties_by_priority() {
        let items = vec![
            todo("low", 100).with_priority(Priority::Low),
            todo("high", 100).with_priority(Priority::Highest),
        ];
        let v = view(&items, None);
        assert_eq!(v.top[0].id.as_str(), "high");
    }

    #[test]
    fn an_event_about_to_start_leads_top_priorities() {
        let items = vec![todo("t1", 100), event("standup", 30, 15), todo("t2", 200)];
        let v = view(&items, None);
        assert_eq!(v.top[0].id.as_str(), "standup");
        assert_eq!(v.up_next.as_ref().map(|i| i.id.as_str()), Some("standup"));
    }

    #[test]
    fn a_distant_event_is_only_up_next() {
        let items = vec![todo("t1", 100), event("review", 180, 30)];
        let v = view(&items, None);
        assert_eq!(v.top.len(), 1);
        assert_eq!(v.up_next.as_ref().map(|i| i.id.as_str()), Some("review"));
    }

    #[test]
    fn an_event_that_has_ended_is_gone() {
        let items = vec![event("yesterday", -120, 30)];
        let v = view(&items, None);
        assert!(v.top.is_empty());
        assert!(v.up_next.is_none());
    }

    #[test]
    fn an_event_in_progress_still_counts_as_up_next() {
        let items = vec![event("running", -10, 30)];
        let v = view(&items, None);
        assert_eq!(v.up_next.as_ref().map(|i| i.id.as_str()), Some("running"));
        assert_eq!(v.top.len(), 1, "a running event is as close as it gets");
    }

    #[test]
    fn the_active_item_leads_in_progress() {
        let items = vec![
            Item::task("a", "a", Status::InProgress, 100, now()),
            Item::task("b", "b", Status::InProgress, 200, now()),
        ];
        let active = ItemId::new("b");
        let v = view(&items, Some(&active));
        let order: Vec<&str> = v.in_progress.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(order, ["b", "a"]);
        assert_eq!(v.active, Some(active));
    }

    #[test]
    fn an_active_id_that_is_not_in_progress_is_dropped() {
        let items = vec![todo("a", 100)];
        let active = ItemId::new("a");
        let v = view(&items, Some(&active));
        assert_eq!(v.active, None);
    }

    #[test]
    fn done_today_excludes_yesterday_and_is_newest_first() {
        let items = vec![
            todo("early", 1).completed_at(now() - Duration::hours(3)),
            todo("late", 2).completed_at(now() - Duration::hours(1)),
            todo("old", 3).completed_at(now() - Duration::days(1)),
        ];
        let v = view(&items, None);
        let order: Vec<&str> = v.done_today.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(order, ["late", "early"]);
    }
}
