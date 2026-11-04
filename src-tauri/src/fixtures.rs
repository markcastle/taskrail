//! Sample items so the rail has something to show before any connector
//! exists. Mirrors the mockups. Goes away when the store lands.

use chrono::{DateTime, Duration, Utc};
use taskrail_core::domain::{Item, Priority, Status, TimeWindow};

pub fn items(now: DateTime<Utc>) -> Vec<Item> {
    let event = |id: &str, title: &str, starts_in: i64, lasts: i64| {
        let start = now + Duration::minutes(starts_in);
        Item::event(
            id,
            title,
            TimeWindow {
                start,
                end: start + Duration::minutes(lasts),
            },
            now,
        )
    };
    vec![
        event("q1-board-meeting", "Q1 board meeting", 40, 60),
        event("team-sync", "Team sync", 240, 30),
        Item::task(
            "prepare-board-deck",
            "Prepare board deck",
            Status::Todo,
            100,
            now,
        )
        .with_priority(Priority::Highest)
        .due(now + Duration::hours(6)),
        Item::task(
            "reply-to-supplier",
            "Reply to supplier",
            Status::Todo,
            200,
            now,
        )
        .with_priority(Priority::High)
        .due(now + Duration::hours(8)),
        Item::task("book-venue", "Book venue", Status::Todo, 300, now),
        Item::task("update-docs", "Update docs", Status::Todo, 400, now)
            .with_priority(Priority::Low),
        Item::task(
            "plan-next-sprint",
            "Plan next sprint",
            Status::Todo,
            500,
            now,
        ),
        Item::task("release-notes", "Release notes", Status::Todo, 600, now),
        Item::task("deploy-to-dev", "Deploy to dev", Status::Todo, 700, now),
        Item::task(
            "design-dashboard",
            "Design dashboard",
            Status::InProgress,
            100,
            now,
        ),
        Item::task(
            "review-pull-request",
            "Review pull request",
            Status::InProgress,
            200,
            now,
        ),
        Item::task("triage-inbox", "Triage inbox", Status::Todo, 100, now)
            .completed_at(now - Duration::hours(2)),
        Item::task(
            "update-project-plan",
            "Update project plan",
            Status::Todo,
            200,
            now,
        )
        .completed_at(now - Duration::hours(5)),
    ]
}
