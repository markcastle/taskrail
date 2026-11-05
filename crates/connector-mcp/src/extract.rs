//! Reading items out of a tool result with a profile's JSONPath expressions.

use chrono::{DateTime, Utc};
use rmcp::model::CallToolResult;
use serde_json::Value;
use serde_json_path::JsonPath;
use taskrail_connector_api::RemoteItem;
use taskrail_core::domain::{ItemKind, TimeWindow};

use crate::profile::{ConnectorProfile, FieldPaths};

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("the tool reported an error: {0}")]
    ToolError(String),
    #[error("the result was not JSON and had no structured content")]
    NotJson,
    #[error("bad JSONPath {path:?}: {reason}")]
    BadPath { path: String, reason: String },
    #[error("item {index} has no {field} at {path:?}")]
    MissingField {
        index: usize,
        field: &'static str,
        path: String,
    },
}

/// The JSON a tool call produced. Structured content wins; otherwise the
/// text blocks are tried as JSON, first one at a time and then joined.
pub fn result_to_json(result: &CallToolResult) -> Result<Value, ExtractError> {
    if result.is_error == Some(true) {
        return Err(ExtractError::ToolError(text_of(result)));
    }
    if let Some(value) = &result.structured_content {
        return Ok(value.clone());
    }
    let texts: Vec<&str> = result
        .content
        .iter()
        .filter_map(|block| block.as_text().map(|t| t.text.as_str()))
        .collect();
    for text in &texts {
        if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
            return Ok(value);
        }
    }
    serde_json::from_str(texts.join("\n").trim()).map_err(|_| ExtractError::NotJson)
}

fn text_of(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn path(expr: &str) -> Result<JsonPath, ExtractError> {
    JsonPath::parse(expr).map_err(|e| ExtractError::BadPath {
        path: expr.to_owned(),
        reason: e.to_string(),
    })
}

/// First match of `expr` in `value`, if any and not null.
pub fn first<'a>(value: &'a Value, expr: &str) -> Result<Option<&'a Value>, ExtractError> {
    Ok(path(expr)?.query(value).first().filter(|v| !v.is_null()))
}

fn string_at(value: &Value, expr: &str) -> Result<Option<String>, ExtractError> {
    Ok(first(value, expr)?.map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }))
}

fn time_at(value: &Value, expr: &str) -> Result<Option<DateTime<Utc>>, ExtractError> {
    Ok(string_at(value, expr)?.and_then(|s| parse_time(&s)))
}

/// Dates as remote systems write them: RFC 3339 with or without a zone,
/// a bare date, or epoch seconds or milliseconds.
pub fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    let text = text.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(naive.and_utc());
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S") {
        return Some(naive.and_utc());
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return date.and_hms_opt(0, 0, 0).map(|n| n.and_utc());
    }
    if let Ok(n) = text.parse::<i64>() {
        let secs = if n.abs() > 10_000_000_000 {
            n / 1000
        } else {
            n
        };
        return DateTime::from_timestamp(secs, 0);
    }
    None
}

/// Every item in the list result, as the profile describes them.
pub fn extract_items(
    profile: &ConnectorProfile,
    result: &Value,
) -> Result<Vec<RemoteItem>, ExtractError> {
    let items = path(&profile.list.items)?.query(result).all();
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| extract_item(profile, &profile.list.fields, item, index))
        .collect()
}

fn extract_item(
    profile: &ConnectorProfile,
    fields: &FieldPaths,
    item: &Value,
    index: usize,
) -> Result<RemoteItem, ExtractError> {
    let required = |field: &'static str, expr: &str| {
        string_at(item, expr)?.ok_or_else(|| ExtractError::MissingField {
            index,
            field,
            path: expr.to_owned(),
        })
    };
    let optional = |expr: &Option<String>| match expr {
        Some(expr) => string_at(item, expr),
        None => Ok(None),
    };
    let optional_time = |expr: &Option<String>| match expr {
        Some(expr) => time_at(item, expr),
        None => Ok(None),
    };

    let window = match (optional_time(&fields.start)?, optional_time(&fields.end)?) {
        (Some(start), Some(end)) => Some(TimeWindow { start, end }),
        (Some(start), None) => Some(TimeWindow { start, end: start }),
        _ => None,
    };
    let kind = if profile.kind == ItemKind::Event && window.is_some() {
        ItemKind::Event
    } else {
        ItemKind::Task
    };

    Ok(RemoteItem {
        external_id: required("external_id", &fields.external_id)?,
        title: required("title", &fields.title)?,
        kind,
        status: optional(&fields.status)?,
        priority: optional(&fields.priority)?.and_then(|p| profile.priority_for(&p)),
        due: optional_time(&fields.due)?,
        window,
        url: optional(&fields.url)?,
        updated_at: optional_time(&fields.updated_at)?,
    })
}

/// Fill `{external_id}`, `{status}` and `{title}` placeholders anywhere in
/// a tool's argument template.
pub fn fill_arguments(template: &Value, external_id: &str, status: &str, title: &str) -> Value {
    fill_placeholders(
        template,
        &[
            ("{external_id}", external_id),
            ("{status}", status),
            ("{title}", title),
        ],
    )
}

/// Replace every placeholder in every string of a template. A string that
/// is exactly one placeholder becomes the raw value, so an id stays an id.
pub fn fill_placeholders(template: &Value, pairs: &[(&str, &str)]) -> Value {
    match template {
        Value::String(s) => {
            let mut out = s.clone();
            for (from, to) in pairs {
                out = out.replace(from, to);
            }
            Value::String(out)
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| fill_placeholders(v, pairs)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), fill_placeholders(v, pairs)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Run a profile's list call against a session, including the `for_each`
/// fan-out, and return every item. The one place the list pipeline lives;
/// the connector and discovery both use it.
pub async fn fetch_items(
    session: &crate::session::McpSession,
    profile: &ConnectorProfile,
) -> Result<Vec<RemoteItem>, taskrail_connector_api::ConnectorError> {
    use taskrail_connector_api::ConnectorError;
    let list = &profile.list;
    let Some(each) = &list.for_each else {
        let raw = session.call(&list.call.tool, &list.call.arguments).await?;
        return extract_items(profile, &raw).map_err(|e| ConnectorError::Protocol(e.to_string()));
    };
    let parent = session.call(&each.tool, &each.arguments).await?;
    let elements = path(&each.items)
        .map_err(|e| ConnectorError::Invalid(e.to_string()))?
        .query(&parent)
        .all();
    let mut ids = Vec::new();
    for element in elements {
        if let Some(id) =
            string_at(element, &each.id).map_err(|e| ConnectorError::Invalid(e.to_string()))?
        {
            ids.push(id);
        }
    }
    log::info!(
        "{}: {} returned {} ids to fan out over",
        profile.id,
        each.tool,
        ids.len()
    );
    let mut all = Vec::new();
    for id in ids {
        let arguments = fill_placeholders(
            &list.call.arguments,
            &[(each.placeholder.as_str(), id.as_str())],
        );
        let raw = session.call(&list.call.tool, &arguments).await?;
        let mut items =
            extract_items(profile, &raw).map_err(|e| ConnectorError::Protocol(e.to_string()))?;
        all.append(&mut items);
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::fake_tracker_profile;
    use rmcp::model::ContentBlock;
    use serde_json::json;
    use taskrail_core::domain::Priority;

    fn sample() -> Value {
        json!({ "tickets": [
            { "ticket_id": "T-1", "summary": "First", "state": "Selected", "priority": "P1", "due_date": "2026-11-05T16:00:00Z", "link": "https://x/1" },
            { "ticket_id": 2, "summary": "Second", "state": "Done", "priority": "weird", "due_date": null, "link": "https://x/2" }
        ]})
    }

    #[test]
    fn items_come_out_with_every_mapped_field() {
        let profile = fake_tracker_profile("fake");
        let items = extract_items(&profile, &sample()).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].external_id, "T-1");
        assert_eq!(items[0].priority, Some(Priority::Highest));
        assert_eq!(
            items[0].due.unwrap().to_rfc3339(),
            "2026-11-05T16:00:00+00:00"
        );
        assert_eq!(items[1].external_id, "2", "numbers become strings");
        assert_eq!(
            items[1].priority, None,
            "unknown priorities are dropped, not guessed"
        );
        assert_eq!(items[1].due, None);
    }

    #[test]
    fn a_missing_required_field_names_the_item_and_path() {
        let profile = fake_tracker_profile("fake");
        let err =
            extract_items(&profile, &json!({ "tickets": [{ "summary": "no id" }] })).unwrap_err();
        assert!(
            matches!(
                err,
                ExtractError::MissingField {
                    index: 0,
                    field: "external_id",
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn structured_content_wins_then_text_json() {
        let mut structured = CallToolResult::success(vec![ContentBlock::text("not json")]);
        structured.structured_content = Some(json!({ "a": 1 }));
        assert_eq!(result_to_json(&structured).unwrap(), json!({ "a": 1 }));

        let text = CallToolResult::success(vec![ContentBlock::text("  {\"b\": 2}  ")]);
        assert_eq!(result_to_json(&text).unwrap(), json!({ "b": 2 }));

        let prose = CallToolResult::success(vec![ContentBlock::text("Here are your tasks: ...")]);
        assert!(matches!(result_to_json(&prose), Err(ExtractError::NotJson)));

        let failed = CallToolResult::error(vec![ContentBlock::text("boom")]);
        assert!(matches!(result_to_json(&failed), Err(ExtractError::ToolError(m)) if m == "boom"));
    }

    #[test]
    fn times_in_common_shapes_parse() {
        assert!(parse_time("2026-11-05T16:00:00Z").is_some());
        assert!(parse_time("2026-11-05T16:00:00+01:00").is_some());
        assert!(parse_time("2026-11-05T16:00:00").is_some());
        assert!(parse_time("2026-11-05").is_some());
        assert_eq!(
            parse_time("1762358400").unwrap().to_rfc3339(),
            "2025-11-05T16:00:00+00:00"
        );
        assert_eq!(parse_time("1762358400000"), parse_time("1762358400"));
        assert!(parse_time("next tuesday").is_none());
    }

    #[test]
    fn any_placeholder_can_be_filled() {
        let filled = fill_placeholders(
            &json!({ "board_id": "{board_id}", "x": "{other}" }),
            &[("{board_id}", "b-1")],
        );
        assert_eq!(filled, json!({ "board_id": "b-1", "x": "{other}" }));
    }

    #[test]
    fn placeholders_fill_anywhere_in_the_template() {
        let filled = fill_arguments(
            &json!({ "ticket_id": "{external_id}", "fields": { "state": "{status}" }, "tags": ["{title}"], "n": 1 }),
            "T-9",
            "Done",
            "Hello",
        );
        assert_eq!(
            filled,
            json!({ "ticket_id": "T-9", "fields": { "state": "Done" }, "tags": ["Hello"], "n": 1 })
        );
    }
}
