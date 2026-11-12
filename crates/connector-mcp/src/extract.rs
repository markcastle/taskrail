//! Reading items out of a tool result with a profile's JSONPath expressions.

use chrono::{DateTime, Utc};
use rmcp::model::CallToolResult;
use serde_json::Value;
use serde_json_path::JsonPath;
use taskrail_connector_api::RemoteItem;
use taskrail_core::domain::{ItemKind, TimeWindow};

use crate::profile::{
    Choice, Condition, ConnectorProfile, FieldPaths, ListSpec, Scope, TrailStep, ME,
};

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

/// The text a JSON value is compared by: strings as they are, everything
/// else as its JSON text, so `7` and `"7"` are the same id.
fn comparable(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Does `element` pass every condition? See [`Condition`] for the rules.
pub fn passes(element: &Value, conditions: &[Condition]) -> Result<bool, ExtractError> {
    for condition in conditions {
        let values: Vec<&Value> = path(&condition.path)?
            .query(element)
            .all()
            .into_iter()
            .filter(|v| !v.is_null())
            .collect();
        let hit = match (&condition.equals, &condition.contains) {
            (Some(wanted), _) => {
                let wanted = comparable(wanted);
                values
                    .iter()
                    .any(|v| comparable(v).eq_ignore_ascii_case(&wanted))
            }
            (None, Some(needle)) => {
                let needle = needle.to_lowercase();
                values.iter().any(|v| {
                    v.as_str()
                        .is_some_and(|s| s.to_lowercase().contains(&needle))
                })
            }
            (None, None) => !values.is_empty(),
        };
        if hit == condition.not {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Every item in the list result that passes the profile's `where`
/// conditions, as the profile describes them.
pub fn extract_items(
    profile: &ConnectorProfile,
    result: &Value,
) -> Result<Vec<RemoteItem>, ExtractError> {
    extract_with(profile, &profile.list, result)
}

/// `extract_items` with a list spec that may differ from the profile's own,
/// such as one with `{me}` filled in.
pub fn extract_with(
    profile: &ConnectorProfile,
    list: &ListSpec,
    result: &Value,
) -> Result<Vec<RemoteItem>, ExtractError> {
    extract_scoped(profile, list, result, &Scope::default())
}

/// `extract_with`, keeping only items that pass the person's picks.
pub fn extract_scoped(
    profile: &ConnectorProfile,
    list: &ListSpec,
    result: &Value,
    scope: &Scope,
) -> Result<Vec<RemoteItem>, ExtractError> {
    Ok(extract_aligned(profile, list, result, scope)?
        .into_iter()
        .map(|(item, _)| item)
        .collect())
}

/// `extract_scoped` keeping each item's per-step trail values, for the
/// scope options.
fn extract_aligned(
    profile: &ConnectorProfile,
    list: &ListSpec,
    result: &Value,
    scope: &Scope,
) -> Result<Vec<Aligned>, ExtractError> {
    let items = path(&list.items)?.query(result).all();
    let resolved = resolve_steps(&list.trail, result)?;
    let mut out = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        if !passes(item, &list.filter)? {
            continue;
        }
        let steps = steps_of(&resolved, item)?;
        if !passes_picks(profile, scope, &steps) {
            continue;
        }
        let trail = steps.iter().flatten().cloned().collect();
        out.push((
            extract_item(profile, &list.fields, item, index, trail)?,
            steps,
        ));
    }
    Ok(out)
}

fn extract_item(
    profile: &ConnectorProfile,
    fields: &FieldPaths,
    item: &Value,
    index: usize,
    trail: Vec<String>,
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
        trail,
    })
}

/// An item with its value for every trail step, in step order.
type Aligned = (RemoteItem, Vec<Option<String>>);

/// The lookup tables a result holds for the trail: for each step, the
/// constant label (no key) or the id-to-label map built from its elements.
enum Resolved {
    Constant(Option<String>),
    Lookup {
        key: String,
        table: std::collections::HashMap<String, String>,
    },
}

fn resolve_steps(steps: &[TrailStep], result: &Value) -> Result<Vec<Resolved>, ExtractError> {
    steps
        .iter()
        .map(|step| match (&step.key, &step.items, &step.id) {
            (Some(key), Some(items), Some(id)) => {
                let mut table = std::collections::HashMap::new();
                for element in path(items)?.query(result).all() {
                    if let (Some(k), Some(label)) =
                        (string_at(element, id)?, string_at(element, &step.label)?)
                    {
                        table.insert(k, label);
                    }
                }
                Ok(Resolved::Lookup {
                    key: key.clone(),
                    table,
                })
            }
            _ => Ok(Resolved::Constant(string_at(result, &step.label)?)),
        })
        .collect()
}

/// One item's value for every trail step, in step order, `None` where it
/// has nothing. The item's trail is these with the gaps dropped; picks match
/// by position.
fn steps_of(resolved: &[Resolved], item: &Value) -> Result<Vec<Option<String>>, ExtractError> {
    resolved
        .iter()
        .map(|step| match step {
            Resolved::Constant(label) => Ok(label.clone()),
            Resolved::Lookup { key, table } => {
                Ok(string_at(item, key)?.and_then(|k| table.get(&k).cloned()))
            }
        })
        .collect()
}

/// The list spec with the person's switches folded into its conditions.
pub fn list_for_scope(profile: &ConnectorProfile, scope: &Scope) -> ListSpec {
    let mut list = profile.list.clone();
    for choice in &profile.choices {
        if let Choice::Switch { filter, .. } = choice {
            if scope.switch_on(choice) {
                list.filter.extend(filter.iter().cloned());
            }
        }
    }
    list
}

/// Does an item pass the person's picks over trail steps? A pick over a
/// step keeps items whose value for that step is one of the chosen.
fn passes_picks(profile: &ConnectorProfile, scope: &Scope, steps: &[Option<String>]) -> bool {
    profile.choices.iter().all(|choice| match choice {
        Choice::Pick {
            id,
            trail: Some(step),
            ..
        } => {
            let chosen = scope.picked(id);
            if chosen.is_empty() {
                return true;
            }
            let Some(position) = profile.list.trail.iter().position(|t| &t.name == step) else {
                return true;
            };
            steps
                .get(position)
                .and_then(|v| v.as_ref())
                .is_some_and(|value| chosen.contains(value))
        }
        _ => true,
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

/// The list spec with `{me}` replaced everywhere it can appear: the list
/// and parent arguments, and the values conditions compare against.
pub fn list_for(list: &ListSpec, me: &str) -> ListSpec {
    let mut out = list.clone();
    let pairs = [(ME, me)];
    out.call.arguments = fill_placeholders(&out.call.arguments, &pairs);
    for condition in &mut out.filter {
        if let Some(value) = &condition.equals {
            condition.equals = Some(fill_placeholders(value, &pairs));
        }
    }
    if let Some(each) = &mut out.for_each {
        each.arguments = fill_placeholders(&each.arguments, &pairs);
        for condition in &mut each.filter {
            if let Some(value) = &condition.equals {
                condition.equals = Some(fill_placeholders(value, &pairs));
            }
        }
    }
    out
}

/// Ask the server who the signed-in person is, through the profile's
/// `identity`. `Ok(None)` when the profile has no identity.
pub async fn resolve_me(
    session: &crate::session::McpSession,
    profile: &ConnectorProfile,
) -> Result<Option<String>, taskrail_connector_api::ConnectorError> {
    use taskrail_connector_api::ConnectorError;
    let Some(identity) = &profile.identity else {
        return Ok(None);
    };
    let raw = session.call(&identity.tool, &identity.arguments).await?;
    let me = string_at(&raw, &identity.id)
        .map_err(|e| ConnectorError::Invalid(e.to_string()))?
        .ok_or_else(|| {
            ConnectorError::Protocol(format!(
                "{} answered, but nothing was at {:?} to say who you are",
                identity.tool, identity.id
            ))
        })?;
    log::info!("{}: {} says the person is {me}", profile.id, identity.tool);
    Ok(Some(me))
}

/// Run a profile's list call against a session, including the `for_each`
/// fan-out, and return every item. The one place the list pipeline lives;
/// the connector and discovery both use it. A profile that says `{me}`
/// learns who that is first.
pub async fn fetch_items(
    session: &crate::session::McpSession,
    profile: &ConnectorProfile,
) -> Result<Vec<RemoteItem>, taskrail_connector_api::ConnectorError> {
    fetch_items_with(session, profile, &Scope::default()).await
}

/// `fetch_items` seen through one person's scope: their switches join the
/// conditions, their container picks narrow the fan-out, their trail picks
/// narrow the items.
pub async fn fetch_items_with(
    session: &crate::session::McpSession,
    profile: &ConnectorProfile,
    scope: &Scope,
) -> Result<Vec<RemoteItem>, taskrail_connector_api::ConnectorError> {
    Ok(fetch_aligned(session, profile, scope)
        .await?
        .into_iter()
        .map(|(item, _)| item)
        .collect())
}

async fn fetch_aligned(
    session: &crate::session::McpSession,
    profile: &ConnectorProfile,
    scope: &Scope,
) -> Result<Vec<Aligned>, taskrail_connector_api::ConnectorError> {
    use taskrail_connector_api::ConnectorError;
    let scoped = list_for_scope(profile, scope);
    let list = &match resolve_me(session, profile).await? {
        Some(me) if profile.uses_me() => list_for(&scoped, &me),
        _ => scoped,
    };
    let Some(each) = &list.for_each else {
        let raw = session.call(&list.call.tool, &list.call.arguments).await?;
        return extract_aligned(profile, list, &raw, scope)
            .map_err(|e| ConnectorError::Protocol(e.to_string()));
    };
    // Container picks: the label path and chosen values, if any pick applies.
    let container_picks: Vec<(&str, &[String])> = profile
        .choices
        .iter()
        .filter_map(|c| match c {
            Choice::Pick {
                id,
                containers: Some(label),
                ..
            } if !scope.picked(id).is_empty() => Some((label.as_str(), scope.picked(id))),
            _ => None,
        })
        .collect();
    let parent = session.call(&each.tool, &each.arguments).await?;
    let elements = path(&each.items)
        .map_err(|e| ConnectorError::Invalid(e.to_string()))?
        .query(&parent)
        .all();
    let mut ids = Vec::new();
    let mut skipped = 0;
    for element in elements {
        if !passes(element, &each.filter).map_err(|e| ConnectorError::Invalid(e.to_string()))? {
            skipped += 1;
            continue;
        }
        let mut picked_out = false;
        for (label_path, chosen) in &container_picks {
            let label = string_at(element, label_path)
                .map_err(|e| ConnectorError::Invalid(e.to_string()))?;
            if !label.is_some_and(|l| chosen.contains(&l)) {
                picked_out = true;
            }
        }
        if picked_out {
            skipped += 1;
            continue;
        }
        if let Some(id) =
            string_at(element, &each.id).map_err(|e| ConnectorError::Invalid(e.to_string()))?
        {
            ids.push(id);
        }
    }
    log::info!(
        "{}: {} returned {} ids to fan out over{}",
        profile.id,
        each.tool,
        ids.len(),
        if skipped > 0 {
            format!(" ({skipped} left out by the where clause or the picks)")
        } else {
            String::new()
        }
    );
    let mut all = Vec::new();
    for id in ids {
        let arguments = fill_placeholders(
            &list.call.arguments,
            &[(each.placeholder.as_str(), id.as_str())],
        );
        let raw = session.call(&list.call.tool, &arguments).await?;
        let mut items = extract_aligned(profile, list, &raw, scope)
            .map_err(|e| ConnectorError::Protocol(e.to_string()))?;
        all.append(&mut items);
    }
    Ok(all)
}

/// What a person can pick from. Containers come from the parent call and are
/// always all of them. Trail values, such as group names, come from the
/// items themselves with every switch off, and only from the containers the
/// person has already kept to: with two boards picked out of ten, the groups
/// worth offering are those two boards', not the forty across all ten.
pub async fn scope_options(
    session: &crate::session::McpSession,
    profile: &ConnectorProfile,
    current: &Scope,
) -> Result<std::collections::BTreeMap<String, Vec<String>>, taskrail_connector_api::ConnectorError>
{
    use taskrail_connector_api::ConnectorError;
    let mut options = std::collections::BTreeMap::new();
    let wants_trail = profile
        .choices
        .iter()
        .any(|c| matches!(c, Choice::Pick { trail: Some(_), .. }));
    let mut wide = Scope::everything(profile);
    for choice in &profile.choices {
        if let Choice::Pick {
            id,
            containers: Some(_),
            ..
        } = choice
        {
            let kept = current.picked(id);
            if !kept.is_empty() {
                wide.picks.insert(id.clone(), kept.to_vec());
            }
        }
    }
    let items = if wants_trail {
        fetch_aligned(session, profile, &wide).await?
    } else {
        Vec::new()
    };
    for choice in &profile.choices {
        match choice {
            Choice::Pick {
                id,
                containers: Some(label),
                ..
            } => {
                let Some(each) = &profile.list.for_each else {
                    continue;
                };
                let parent = session.call(&each.tool, &each.arguments).await?;
                let mut names: Vec<String> = Vec::new();
                for element in path(&each.items)
                    .map_err(|e| ConnectorError::Invalid(e.to_string()))?
                    .query(&parent)
                    .all()
                {
                    if let Some(name) = string_at(element, label)
                        .map_err(|e| ConnectorError::Invalid(e.to_string()))?
                    {
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
                options.insert(id.clone(), names);
            }
            Choice::Pick {
                id,
                trail: Some(step),
                ..
            } => {
                let Some(position) = profile.list.trail.iter().position(|t| &t.name == step) else {
                    continue;
                };
                let mut values: Vec<String> = Vec::new();
                for (_, steps) in &items {
                    if let Some(Some(v)) = steps.get(position) {
                        if !values.contains(v) {
                            values.push(v.clone());
                        }
                    }
                }
                values.sort();
                options.insert(id.clone(), values);
            }
            _ => {}
        }
    }
    Ok(options)
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
    fn where_conditions_keep_only_matching_items() {
        let mut profile = fake_tracker_profile("fake");
        let board = json!({ "tickets": [
            { "ticket_id": "A", "summary": "mine", "assignees": [{ "id": "u-1" }, { "id": "u-2" }] },
            { "ticket_id": "B", "summary": "theirs", "assignees": [{ "id": "u-3" }] },
            { "ticket_id": "C", "summary": "nobody's", "assignees": [] },
            { "ticket_id": "D", "summary": "mine but archived", "assignees": [{ "id": "u-1" }], "archived": true }
        ]});
        profile.list.filter = vec![
            Condition {
                path: "$.assignees[*].id".into(),
                equals: Some(json!("U-1")),
                ..Default::default()
            },
            Condition {
                path: "$.archived".into(),
                not: true,
                ..Default::default()
            },
        ];
        let items = extract_items(&profile, &board).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].external_id, "A");

        profile.list.filter = vec![Condition {
            path: "$.summary".into(),
            contains: Some("MINE".into()),
            ..Default::default()
        }];
        let items = extract_items(&profile, &board).unwrap();
        assert_eq!(items.len(), 2, "contains ignores case");

        profile.list.filter = vec![Condition {
            path: "$.assignees[*].id".into(),
            equals: Some(json!(7)),
            ..Default::default()
        }];
        assert!(extract_items(&profile, &board).unwrap().is_empty());
        assert!(
            passes(
                &json!({ "n": "7" }),
                &[Condition {
                    path: "$.n".into(),
                    equals: Some(json!(7)),
                    ..Default::default()
                }]
            )
            .unwrap(),
            "numbers and their text compare equal"
        );
    }

    #[test]
    fn me_is_filled_in_everywhere_it_can_appear() {
        use crate::profile::ForEach;
        let mut profile = fake_tracker_profile("fake");
        profile.list.call.arguments = json!({ "assignee": "{me}", "board_id": "{board_id}" });
        profile.list.filter = vec![Condition {
            path: "$.owner".into(),
            equals: Some(json!("{me}")),
            ..Default::default()
        }];
        profile.list.for_each = Some(ForEach {
            tool: "list_boards".into(),
            arguments: json!({ "member": "{me}" }),
            items: "$.boards[*]".into(),
            id: "$.id".into(),
            placeholder: "{board_id}".into(),
            filter: vec![Condition {
                path: "$.owner".into(),
                equals: Some(json!("{me}")),
                ..Default::default()
            }],
        });
        let list = list_for(&profile.list, "u-7");
        assert_eq!(
            list.call.arguments,
            json!({ "assignee": "u-7", "board_id": "{board_id}" }),
            "other placeholders are left for later"
        );
        assert_eq!(list.filter[0].equals, Some(json!("u-7")));
        let each = list.for_each.unwrap();
        assert_eq!(each.arguments, json!({ "member": "u-7" }));
        assert_eq!(each.filter[0].equals, Some(json!("u-7")));
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
