//! A calendar as a Task Rail connector: any calendar that publishes an
//! iCalendar feed, which is most of them. Google Calendar has a "secret
//! address in iCal format" per calendar, Outlook publishes an .ics link,
//! Apple Calendar shares one. The connector fetches the feed, expands
//! recurring events for the days ahead, and hands back timed items.
//!
//! The parsing is our own and small: unfold lines, read the properties of
//! each VEVENT, resolve times through the named time zone, and let the
//! `rrule` crate expand repeating ones. It covers what feeds in the wild
//! actually use; anything exotic is skipped rather than guessed at.

use std::collections::{BTreeMap, HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use taskrail_connector_api::{
    Capabilities, Connector, ConnectorError, ConnectorInfo, ConnectorResult, RemoteItem, StatusMap,
};
use taskrail_core::domain::{ConnectorId, ItemKind, TimeWindow};

/// How far ahead events are brought onto the rail.
pub const DAYS_AHEAD: i64 = 14;

/// One calendar feed. The URL is the secret: whoever has it can read the
/// calendar, so the app keeps it in the credential store and hands it here.
pub struct IcsCalendar {
    id: ConnectorId,
    name: String,
    url: String,
    status_map: StatusMap,
    client: reqwest::Client,
}

impl IcsCalendar {
    pub fn new(id: impl Into<String>, name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: ConnectorId::new(id),
            name: name.into(),
            url: url.into(),
            status_map: StatusMap::default(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Fetch the feed as text. Google serves these as `webcal://` links in
    /// some places; that is plain HTTPS with a different scheme.
    pub async fn fetch(&self) -> ConnectorResult<String> {
        let url = self
            .url
            .strip_prefix("webcal://")
            .map(|rest| format!("https://{rest}"))
            .unwrap_or_else(|| self.url.clone());
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ConnectorError::Transport(e.to_string()))?;
        if response.status().as_u16() == 401 || response.status().as_u16() == 403 {
            return Err(ConnectorError::Auth(format!(
                "the calendar answered {}; the address may have been reset",
                response.status()
            )));
        }
        let response = response
            .error_for_status()
            .map_err(|e| ConnectorError::Transport(e.to_string()))?;
        response
            .text()
            .await
            .map_err(|e| ConnectorError::Transport(e.to_string()))
    }
}

#[async_trait]
impl Connector for IcsCalendar {
    fn info(&self) -> ConnectorInfo {
        ConnectorInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            capabilities: Capabilities {
                read: true,
                write_status: false,
                create: false,
            },
        }
    }

    fn status_map(&self) -> &StatusMap {
        &self.status_map
    }

    async fn list(&self) -> ConnectorResult<Vec<RemoteItem>> {
        let text = self.fetch().await?;
        let now = Utc::now();
        let items = events_between(&text, window_from(now), window_to(now))
            .map_err(|e| ConnectorError::Protocol(e.to_string()))?;
        log::info!(
            "{}: {} events in the next {DAYS_AHEAD} days",
            self.id.as_str(),
            items.len()
        );
        Ok(items)
    }
}

/// The start of today, local time, as UTC: the earliest an event may start
/// and still be shown.
pub fn window_from(now: DateTime<Utc>) -> DateTime<Utc> {
    let local = now.with_timezone(&Local);
    let midnight = local
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight exists");
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(now)
}

pub fn window_to(now: DateTime<Utc>) -> DateTime<Utc> {
    window_from(now) + Duration::days(DAYS_AHEAD)
}

#[derive(Debug, thiserror::Error)]
pub enum IcsError {
    #[error("not an iCalendar feed: no VCALENDAR in it")]
    NotACalendar,
}

/// One property line, split into name, parameters and value.
#[derive(Debug, Clone)]
struct Prop {
    params: BTreeMap<String, String>,
    value: String,
}

impl Prop {
    fn param(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(String::as_str)
    }
}

/// A VEVENT as a bag of properties. Repeated properties (EXDATE can be)
/// are kept in order.
#[derive(Debug, Default, Clone)]
struct Event {
    props: HashMap<String, Vec<Prop>>,
}

impl Event {
    fn first(&self, name: &str) -> Option<&Prop> {
        self.props.get(name).and_then(|v| v.first())
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.first(name).map(|p| p.value.as_str())
    }
}

/// Unfold continuation lines and split the feed into VEVENT blocks.
fn parse_events(text: &str) -> Result<Vec<Event>, IcsError> {
    if !text.contains("BEGIN:VCALENDAR") {
        return Err(IcsError::NotACalendar);
    }
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split(['\r', '\n']).filter(|l| !l.is_empty()) {
        if let Some(rest) = raw.strip_prefix([' ', '\t']) {
            if let Some(last) = lines.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        lines.push(raw.to_owned());
    }
    let mut events = Vec::new();
    let mut current: Option<Event> = None;
    let mut depth = 0;
    for line in lines {
        if line == "BEGIN:VEVENT" {
            current = Some(Event::default());
            depth = 0;
            continue;
        }
        if line == "END:VEVENT" {
            if let Some(event) = current.take() {
                events.push(event);
            }
            continue;
        }
        let Some(event) = current.as_mut() else {
            continue;
        };
        // Alarms and the like nest inside an event; their lines are not the event's.
        if line.starts_with("BEGIN:") {
            depth += 1;
            continue;
        }
        if line.starts_with("END:") {
            depth -= 1;
            continue;
        }
        if depth > 0 {
            continue;
        }
        let Some((head, value)) = split_property(&line) else {
            continue;
        };
        let mut parts = head.split(';');
        let name = parts.next().unwrap_or_default().to_ascii_uppercase();
        let mut params = BTreeMap::new();
        for param in parts {
            if let Some((k, v)) = param.split_once('=') {
                params.insert(k.to_ascii_uppercase(), v.trim_matches('"').to_owned());
            }
        }
        event.props.entry(name).or_default().push(Prop {
            params,
            value: unescape(value),
        });
    }
    Ok(events)
}

/// The colon that ends the property name and parameters, which is the first
/// colon outside double quotes.
fn split_property(line: &str) -> Option<(&str, &str)> {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => return Some((&line[..i], &line[i + 1..])),
            _ => {}
        }
    }
    None
}

fn unescape(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

/// When something happens, as the feed wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    /// A whole day, by local date.
    Day(NaiveDate),
    /// An instant.
    At(DateTime<Utc>),
}

/// Read a DTSTART or DTEND: a date, a UTC instant, an instant in a named
/// zone, or a floating time taken as this machine's local time.
fn parse_when(prop: &Prop) -> Option<When> {
    let value = prop.value.trim();
    if prop.param("VALUE") == Some("DATE") || value.len() == 8 {
        return NaiveDate::parse_from_str(value, "%Y%m%d")
            .ok()
            .map(When::Day);
    }
    if let Some(utc) = value.strip_suffix('Z') {
        let naive = NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%S").ok()?;
        return Some(When::At(Utc.from_utc_datetime(&naive)));
    }
    let naive = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    let at = match prop
        .param("TZID")
        .and_then(|tz| tz.parse::<chrono_tz::Tz>().ok())
    {
        Some(tz) => tz
            .from_local_datetime(&naive)
            .earliest()?
            .with_timezone(&Utc),
        None => Local
            .from_local_datetime(&naive)
            .earliest()?
            .with_timezone(&Utc),
    };
    Some(When::At(at))
}

/// A day as a window: local midnight to the next.
fn day_window(day: NaiveDate) -> Option<TimeWindow> {
    let start = Local
        .from_local_datetime(&day.and_hms_opt(0, 0, 0)?)
        .earliest()?
        .with_timezone(&Utc);
    Some(TimeWindow {
        start,
        end: start + Duration::days(1),
    })
}

/// ISO 8601 durations as feeds write them: P1D, PT1H30M, PT45M.
fn parse_duration(text: &str) -> Option<Duration> {
    let text = text.trim().trim_start_matches(['+', '-']);
    let rest = text.strip_prefix('P')?;
    let (days_part, time_part) = match rest.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (rest, None),
    };
    let mut total = Duration::zero();
    let mut number = String::new();
    for c in days_part.chars() {
        if c.is_ascii_digit() {
            number.push(c);
        } else {
            let n: i64 = number.parse().ok()?;
            number.clear();
            total += match c {
                'W' => Duration::weeks(n),
                'D' => Duration::days(n),
                _ => return None,
            };
        }
    }
    if let Some(time) = time_part {
        for c in time.chars() {
            if c.is_ascii_digit() {
                number.push(c);
            } else {
                let n: i64 = number.parse().ok()?;
                number.clear();
                total += match c {
                    'H' => Duration::hours(n),
                    'M' => Duration::minutes(n),
                    'S' => Duration::seconds(n),
                    _ => return None,
                };
            }
        }
    }
    Some(total)
}

/// The start and end of an event as written, before any recurrence.
fn base_window(event: &Event) -> Option<(When, TimeWindow)> {
    let start_prop = event.first("DTSTART")?;
    let start = parse_when(start_prop)?;
    let window = match start {
        When::Day(day) => {
            let mut w = day_window(day)?;
            if let Some(When::Day(end_day)) = event.first("DTEND").and_then(parse_when) {
                if let Some(end_w) = day_window(end_day) {
                    w.end = end_w.start;
                }
            }
            w
        }
        When::At(at) => {
            let end = match event.first("DTEND").and_then(parse_when) {
                Some(When::At(end)) => end,
                Some(When::Day(day)) => day_window(day).map(|w| w.start).unwrap_or(at),
                None => match event.value("DURATION").and_then(parse_duration) {
                    Some(d) => at + d,
                    None => at,
                },
            };
            TimeWindow {
                start: at,
                end: end.max(at),
            }
        }
    };
    Some((start, window))
}

/// The occurrences of a repeating event inside the window, by handing the
/// event's own DTSTART, RRULE, RDATE and EXDATE lines to the rrule crate.
fn occurrences(event: &Event, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<DateTime<Utc>> {
    let Some(start) = event.first("DTSTART") else {
        return vec![];
    };
    let mut text = String::new();
    let mut line = |name: &str, prop: &Prop| {
        text.push_str(name);
        for (k, v) in &prop.params {
            if k == "TZID" || k == "VALUE" {
                text.push(';');
                text.push_str(k);
                text.push('=');
                text.push_str(v);
            }
        }
        text.push(':');
        text.push_str(&prop.value);
        text.push('\n');
    };
    line("DTSTART", start);
    for name in ["RRULE", "RDATE", "EXDATE"] {
        for prop in event.props.get(name).into_iter().flatten() {
            line(name, prop);
        }
    }
    let Ok(set) = text.parse::<rrule::RRuleSet>() else {
        log::warn!(
            "could not read a repeat rule: {}",
            text.replace('\n', " | ")
        );
        return vec![];
    };
    let tz = rrule::Tz::UTC;
    let result = set
        .after(from.with_timezone(&tz))
        .before(to.with_timezone(&tz))
        .all(500);
    result
        .dates
        .into_iter()
        .map(|d| d.with_timezone(&Utc))
        .collect()
}

/// Every event in the feed that touches the window, repeating ones expanded,
/// as rail items. Cancelled events are left out, and an instance that the
/// feed overrides (a moved meeting) appears once, in its new place.
pub fn events_between(
    text: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<RemoteItem>, IcsError> {
    let events = parse_events(text)?;
    // Overridden instances: (uid, original start) pairs that a RECURRENCE-ID event replaces.
    let overridden: HashSet<(String, DateTime<Utc>)> = events
        .iter()
        .filter_map(|e| {
            let uid = e.value("UID")?.to_owned();
            let at = match parse_when(e.first("RECURRENCE-ID")?)? {
                When::At(at) => at,
                When::Day(day) => day_window(day)?.start,
            };
            Some((uid, at))
        })
        .collect();

    let mut items = Vec::new();
    for event in &events {
        if event
            .value("STATUS")
            .is_some_and(|s| s.eq_ignore_ascii_case("CANCELLED"))
        {
            continue;
        }
        let Some(uid) = event.value("UID") else {
            continue;
        };
        let title = event
            .value("SUMMARY")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("(untitled)")
            .to_owned();
        let Some((start, window)) = base_window(event) else {
            continue;
        };
        let length = window.end - window.start;
        let url = event.value("URL").map(str::to_owned);
        let updated_at = event
            .first("LAST-MODIFIED")
            .or_else(|| event.first("DTSTAMP"))
            .and_then(parse_when)
            .and_then(|w| match w {
                When::At(at) => Some(at),
                When::Day(_) => None,
            });

        let starts: Vec<(DateTime<Utc>, bool)> =
            if event.props.contains_key("RRULE") || event.props.contains_key("RDATE") {
                occurrences(event, from - length, to)
                    .into_iter()
                    .filter(|at| !overridden.contains(&(uid.to_owned(), *at)))
                    .map(|at| (at, true))
                    .collect()
            } else {
                vec![(window.start, false)]
            };
        for (at, instance) in starts {
            let w = match start {
                // A repeating all-day event: each occurrence is a whole local day.
                When::Day(_) if instance => {
                    match day_window(at.with_timezone(&Local).date_naive()) {
                        Some(w) => TimeWindow {
                            start: w.start,
                            end: w.start + length,
                        },
                        None => continue,
                    }
                }
                _ => TimeWindow {
                    start: at,
                    end: at + length,
                },
            };
            if w.end <= from || w.start >= to {
                continue;
            }
            let external_id = if instance {
                format!("{uid}@{}", w.start.format("%Y%m%dT%H%M%SZ"))
            } else {
                uid.to_owned()
            };
            items.push(RemoteItem {
                external_id,
                title: title.clone(),
                kind: ItemKind::Event,
                status: None,
                priority: None,
                due: None,
                window: Some(w),
                url: url.clone(),
                updated_at,
                trail: Vec::new(),
            });
        }
    }
    items.sort_by_key(|i| i.window.map(|w| w.start));
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\n\
BEGIN:VEVENT\r\nUID:one@test\r\nDTSTAMP:20261101T090000Z\r\nDTSTART:20261110T090000Z\r\nDTEND:20261110T093000Z\r\nSUMMARY:Standup\\, with the team\r\nURL:https://meet.example/standup\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:weekly@test\r\nDTSTART;TZID=Europe/London:20261103T140000\r\nDTEND;TZID=Europe/London:20261103T150000\r\nRRULE:FREQ=WEEKLY;BYDAY=TU\r\nEXDATE;TZID=Europe/London:20261117T140000\r\nSUMMARY:Planning\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:weekly@test\r\nRECURRENCE-ID;TZID=Europe/London:20261110T140000\r\nDTSTART;TZID=Europe/London:20261110T160000\r\nDTEND;TZID=Europe/London:20261110T170000\r\nSUMMARY:Planning (moved)\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:allday@test\r\nDTSTART;VALUE=DATE:20261112\r\nDTEND;VALUE=DATE:20261113\r\nSUMMARY:Conference\r\nBEGIN:VALARM\r\nTRIGGER:-PT15M\r\nACTION:DISPLAY\r\nDESCRIPTION:Reminder\r\nEND:VALARM\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:gone@test\r\nDTSTART:20261111T100000Z\r\nDURATION:PT45M\r\nSTATUS:CANCELLED\r\nSUMMARY:Cancelled thing\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:long@test\r\nDTSTART:20261111T100000Z\r\nDURATION:PT1H30M\r\nSUMMARY:A long\r\n  folded summary line\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    fn window() -> (DateTime<Utc>, DateTime<Utc>) {
        let from = Utc.with_ymd_and_hms(2026, 11, 9, 0, 0, 0).unwrap();
        (from, from + Duration::days(14))
    }

    #[test]
    fn single_events_come_through_with_their_window_and_link() {
        let (from, to) = window();
        let items = events_between(FEED, from, to).unwrap();
        let standup = items.iter().find(|i| i.external_id == "one@test").unwrap();
        assert_eq!(
            standup.title, "Standup, with the team",
            "escapes are undone"
        );
        let w = standup.window.unwrap();
        assert_eq!(
            w.start,
            Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap()
        );
        assert_eq!(w.end - w.start, Duration::minutes(30));
        assert_eq!(standup.url.as_deref(), Some("https://meet.example/standup"));
        assert_eq!(standup.kind, ItemKind::Event);
    }

    #[test]
    fn repeating_events_expand_with_exceptions_and_overrides_honoured() {
        let (from, to) = window();
        let items = events_between(FEED, from, to).unwrap();
        let planning: Vec<&RemoteItem> = items
            .iter()
            .filter(|i| i.external_id.starts_with("weekly@test"))
            .collect();
        // Tuesdays in the window: 10 Nov (moved), 17 Nov (excluded). The moved one
        // appears as its own event at 16:00 London, and the 14:00 instance does not.
        let titles: Vec<&str> = planning.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["Planning (moved)"], "{planning:?}");
        let moved = planning[0].window.unwrap();
        assert_eq!(
            moved.start,
            Utc.with_ymd_and_hms(2026, 11, 10, 16, 0, 0).unwrap(),
            "London is on GMT in November"
        );
        assert_eq!(moved.end - moved.start, Duration::hours(1));
    }

    #[test]
    fn repeating_events_expand_when_nothing_overrides_them() {
        let from = Utc.with_ymd_and_hms(2026, 11, 23, 0, 0, 0).unwrap();
        let items = events_between(FEED, from, from + Duration::days(14)).unwrap();
        let planning: Vec<&RemoteItem> = items
            .iter()
            .filter(|i| i.external_id.starts_with("weekly@test"))
            .collect();
        assert_eq!(planning.len(), 2, "24 Nov and 1 Dec: {planning:?}");
        assert!(planning[0].external_id.ends_with("@20261124T140000Z"));
        assert_eq!(planning[0].title, "Planning");
    }

    #[test]
    fn all_day_cancelled_folded_and_duration_cases() {
        let (from, to) = window();
        let items = events_between(FEED, from, to).unwrap();
        let conference = items
            .iter()
            .find(|i| i.external_id == "allday@test")
            .unwrap();
        let w = conference.window.unwrap();
        assert_eq!(w.end - w.start, Duration::days(1), "a whole day");
        assert!(
            items.iter().all(|i| i.external_id != "gone@test"),
            "cancelled events stay out"
        );
        let long = items.iter().find(|i| i.external_id == "long@test").unwrap();
        assert_eq!(long.title, "A long folded summary line");
        let w = long.window.unwrap();
        assert_eq!(
            w.end - w.start,
            Duration::minutes(90),
            "DURATION is honoured"
        );
        // Alarm lines inside the event did not leak into it.
        assert!(long.url.is_none());
    }

    #[test]
    fn outside_the_window_nothing_comes_back_and_a_non_feed_is_refused() {
        let from = Utc.with_ymd_and_hms(2027, 6, 1, 0, 0, 0).unwrap();
        let items = events_between(FEED, from, from + Duration::days(14)).unwrap();
        let non_repeating: Vec<&RemoteItem> = items
            .iter()
            .filter(|i| !i.external_id.contains("@2"))
            .collect();
        assert!(non_repeating.is_empty(), "{non_repeating:?}");
        assert!(events_between(
            "<html>not a calendar</html>",
            from,
            from + Duration::days(1)
        )
        .is_err());
    }

    #[test]
    fn durations_parse() {
        assert_eq!(parse_duration("PT1H30M"), Some(Duration::minutes(90)));
        assert_eq!(parse_duration("P1D"), Some(Duration::days(1)));
        assert_eq!(parse_duration("P1W"), Some(Duration::weeks(1)));
        assert_eq!(parse_duration("PT45M"), Some(Duration::minutes(45)));
        assert_eq!(parse_duration("nonsense"), None);
    }
}
