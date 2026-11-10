//! The Connector Profile: everything the connector needs to know about one
//! server, written down once.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use taskrail_connector_api::StatusMap;
use taskrail_core::domain::{ItemKind, Priority};

pub const PROFILE_VERSION: u32 = 1;

/// How to reach the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum ServerSpec {
    /// Spawn a process and speak MCP over its stdin and stdout.
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    /// Streamable HTTP. `bearer_secret` names an entry in the secret store
    /// holding a static token to send. `oauth_secret` names the entry
    /// holding credentials from the MCP sign-in flow. Both are names, never
    /// the secrets themselves.
    Http {
        url: String,
        #[serde(default)]
        bearer_secret: Option<String>,
        #[serde(default)]
        oauth_secret: Option<String>,
    },
}

/// One tool call with fixed arguments. Argument strings may contain
/// `{external_id}`, `{status}` and `{title}`, filled in at call time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub tool: String,
    #[serde(default = "empty_object")]
    pub arguments: serde_json::Value,
}

fn empty_object() -> serde_json::Value {
    serde_json::Value::Object(Default::default())
}

/// Where each field of an item lives in one element of the list result.
/// Every path is JSONPath, evaluated against the item, so `$.summary` reads
/// the `summary` key. Only `external_id` and `title` are required.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldPaths {
    pub external_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// One test an element must pass to be kept. `path` is JSONPath evaluated
/// against the element; the test passes when any value it selects
/// satisfies it, so `$.assignees[*].id` works for a list of people.
///
/// - `equals`: the value matches (strings compared without regard to case,
///   numbers and booleans by their text).
/// - `contains`: the value is a string containing this text, any case.
/// - neither: the path selects something that is not null.
/// - `not`: inverts the result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub not: bool,
}

/// A parent call whose results feed the list call: "for each board, list
/// its tasks". The parent is called once; `items` selects its elements and
/// `id` reads the value that replaces `placeholder` in the list call's
/// arguments, which then runs once per element. Results are concatenated.
/// `where` keeps only the elements that pass every condition, so a profile
/// can fan out over the boards whose name contains "Platform" and no others.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForEach {
    pub tool: String,
    #[serde(default = "empty_object")]
    pub arguments: serde_json::Value,
    pub items: String,
    pub id: String,
    /// The placeholder in the list call's arguments, such as `{board_id}`.
    pub placeholder: String,
    #[serde(default, rename = "where", skip_serializing_if = "Vec::is_empty")]
    pub filter: Vec<Condition>,
}

/// Who the signed-in person is, so a profile can say "my tasks" without
/// carrying anyone's user id. `tool` is called once per connection and `id`
/// reads the value that replaces `{me}` wherever it appears in the profile.
/// This is what lets a profile written for one person become a blueprint
/// for everyone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub tool: String,
    #[serde(default = "empty_object")]
    pub arguments: serde_json::Value,
    /// JSONPath to the person's id in the tool's result, such as `$.id`.
    pub id: String,
}

/// The placeholder [`Identity`] fills in.
pub const ME: &str = "{me}";

/// One step of an item's trail: where it sits in the source system, such as
/// its board, its group or its parent task. Resolved against the list
/// result, so nothing has to be on the item itself.
///
/// Without `key`, `label` is read from the whole result once: the board's
/// name on a "get board" result. With `key`, the item's value at `key` is
/// looked up among the `items` of the result, matching on `id`, and `label`
/// is read from the element found: a task's `group_id` against the board's
/// groups, or its `parent_id` against the other tasks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrailStep {
    /// What the step is, for picks and for people: "board", "group", "parent".
    pub name: String,
    /// JSONPath to the label, on the result or on the matched element.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Something a person can decide about their own connector, declared by the
/// profile and answered per connector in a [`Scope`]. This is how one
/// blueprint serves people who use the same system differently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Choice {
    /// On or off. On, its conditions join the list's `where`.
    Switch {
        id: String,
        label: String,
        #[serde(default)]
        default: bool,
        #[serde(default, rename = "where", skip_serializing_if = "Vec::is_empty")]
        filter: Vec<Condition>,
    },
    /// A set of values to keep. Nothing chosen means everything. Either
    /// `containers` names the label path on a `for_each` element, so the
    /// pick is among boards or projects, or `trail` names a trail step, so
    /// the pick is among the values that step takes, such as group names.
    Pick {
        id: String,
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        containers: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trail: Option<String>,
    },
}

impl Choice {
    pub fn id(&self) -> &str {
        match self {
            Choice::Switch { id, .. } | Choice::Pick { id, .. } => id,
        }
    }
}

/// One person's answers to a profile's choices. Lives on the connector,
/// never in the profile or a blueprint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    #[serde(default)]
    pub switches: BTreeMap<String, bool>,
    #[serde(default)]
    pub picks: BTreeMap<String, Vec<String>>,
}

impl Scope {
    /// Is a switch on, going by the answer or else the choice's default?
    pub fn switch_on(&self, choice: &Choice) -> bool {
        match choice {
            Choice::Switch { id, default, .. } => {
                self.switches.get(id).copied().unwrap_or(*default)
            }
            Choice::Pick { .. } => false,
        }
    }

    /// The values chosen for a pick, empty meaning everything.
    pub fn picked(&self, id: &str) -> &[String] {
        self.picks.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The widest view: every switch off, nothing picked. What the options
    /// for the picks are collected from.
    pub fn everything(profile: &ConnectorProfile) -> Self {
        let mut scope = Self::default();
        for choice in &profile.choices {
            if let Choice::Switch { id, .. } = choice {
                scope.switches.insert(id.clone(), false);
            }
        }
        scope
    }
}

/// The list call and how to read its result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSpec {
    #[serde(flatten)]
    pub call: ToolSpec,
    /// JSONPath to the array of items in the result, such as `$.tickets[*]`.
    pub items: String,
    pub fields: FieldPaths,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub for_each: Option<ForEach>,
    /// Keep only the items that pass every condition. This is how a profile
    /// narrows a list to one person's work when the tool itself cannot.
    #[serde(default, rename = "where", skip_serializing_if = "Vec::is_empty")]
    pub filter: Vec<Condition>,
    /// Where each item sits, outermost first. See [`TrailStep`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trail: Vec<TrailStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorProfile {
    #[serde(default = "default_version")]
    pub version: u32,
    /// Short, stable, used in item ids. Letters, digits and dashes.
    pub id: String,
    pub name: String,
    pub server: ServerSpec,
    #[serde(default = "default_kind")]
    pub kind: ItemKind,
    pub list: ListSpec,
    /// How to learn who the person is, when the profile uses `{me}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,
    /// What each person may decide for themselves. See [`Choice`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Choice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_status: Option<ToolSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create: Option<ToolSpec>,
    #[serde(default)]
    pub status_map: StatusMap,
    /// Remote priority words to the rail's scale, matched case-insensitively.
    #[serde(default)]
    pub priority_map: BTreeMap<String, Priority>,
    /// How often to poll, in seconds.
    #[serde(default = "default_poll")]
    pub poll_seconds: u32,
}

fn default_version() -> u32 {
    PROFILE_VERSION
}

fn default_kind() -> ItemKind {
    ItemKind::Task
}

fn default_poll() -> u32 {
    60
}

impl ConnectorProfile {
    pub fn priority_for(&self, remote: &str) -> Option<Priority> {
        let lower = remote.to_lowercase();
        self.priority_map
            .iter()
            .find(|(k, _)| k.to_lowercase() == lower)
            .map(|(_, v)| *v)
    }

    /// Does any part of the list call depend on knowing who the person is?
    pub fn uses_me(&self) -> bool {
        serde_json::to_string(&self.list).is_ok_and(|json| json.contains(ME))
            || serde_json::to_string(&self.choices).is_ok_and(|json| json.contains(ME))
    }

    /// Problems a person should fix before the profile is used.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            problems.push("id must be letters, digits and dashes".into());
        }
        if self.list.fields.external_id.is_empty() || self.list.fields.title.is_empty() {
            problems.push("list.fields needs external_id and title".into());
        }
        if self.kind == ItemKind::Task
            && self.set_status.is_some()
            && !self.status_map.is_complete()
        {
            problems.push(
                "status_map.canonical must name a remote status for todo, in_progress and done"
                    .into(),
            );
        }
        if self.kind == ItemKind::Event
            && (self.list.fields.start.is_none() || self.list.fields.end.is_none())
        {
            problems.push("a calendar profile needs list.fields.start and list.fields.end".into());
        }
        for step in &self.list.trail {
            let lookup = [step.key.is_some(), step.items.is_some(), step.id.is_some()];
            if lookup.iter().any(|b| *b) && !lookup.iter().all(|b| *b) {
                problems.push(format!(
                    "trail step {:?} looks things up, so it needs key, items and id together",
                    step.name
                ));
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        for choice in &self.choices {
            if !ids.insert(choice.id()) {
                problems.push(format!("two choices share the id {:?}", choice.id()));
            }
            if let Choice::Pick {
                id,
                containers,
                trail,
                ..
            } = choice
            {
                match (containers, trail) {
                    (Some(_), None) if self.list.for_each.is_none() => problems.push(format!(
                        "pick {id:?} is over containers, but the list has no for_each"
                    )),
                    (None, Some(step)) if !self.list.trail.iter().any(|t| &t.name == step) => {
                        problems.push(format!(
                            "pick {id:?} is over trail step {step:?}, which the list does not have"
                        ))
                    }
                    (None, None) | (Some(_), Some(_)) => {
                        problems.push(format!("pick {id:?} needs either containers or trail"))
                    }
                    _ => {}
                }
            }
        }
        let switch_conditions = self.choices.iter().flat_map(|c| match c {
            Choice::Switch { filter, .. } => filter.iter(),
            Choice::Pick { .. } => [].iter(),
        });
        let conditions = self
            .list
            .filter
            .iter()
            .chain(
                self.list
                    .for_each
                    .iter()
                    .flat_map(|each| each.filter.iter()),
            )
            .chain(switch_conditions);
        if self.identity.is_none() && self.uses_me() {
            problems.push(
                "the profile uses {me} but has no \"identity\" saying which tool tells us who that is"
                    .into(),
            );
        }
        for condition in conditions {
            if condition.path.trim().is_empty() {
                problems.push("every \"where\" condition needs a path".into());
            }
            if condition.equals.is_some() && condition.contains.is_some() {
                problems.push(format!(
                    "the condition on {:?} must use equals or contains, not both",
                    condition.path
                ));
            }
        }
        problems
    }
}

/// The profile for the fake tracker that ships with the repo. Doubles as the
/// worked example of what discovery is expected to produce.
pub fn fake_tracker_profile(command: impl Into<String>) -> ConnectorProfile {
    use taskrail_core::domain::Status;
    ConnectorProfile {
        version: PROFILE_VERSION,
        id: "fake-tracker".into(),
        name: "Fake tracker".into(),
        server: ServerSpec::Stdio {
            command: command.into(),
            args: vec![],
            env: BTreeMap::new(),
        },
        kind: ItemKind::Task,
        list: ListSpec {
            call: ToolSpec {
                tool: "list_tickets".into(),
                arguments: serde_json::json!({ "assignee": "me", "open_only": false }),
            },
            items: "$.tickets[*]".into(),
            fields: FieldPaths {
                external_id: "$.ticket_id".into(),
                title: "$.summary".into(),
                status: Some("$.state".into()),
                priority: Some("$.priority".into()),
                due: Some("$.due_date".into()),
                url: Some("$.link".into()),
                ..FieldPaths::default()
            },
            for_each: None,
            filter: vec![],
            trail: vec![],
        },
        identity: None,
        choices: vec![],
        set_status: Some(ToolSpec {
            tool: "transition_ticket".into(),
            arguments: serde_json::json!({ "ticket_id": "{external_id}", "state": "{status}" }),
        }),
        create: Some(ToolSpec {
            tool: "create_ticket".into(),
            arguments: serde_json::json!({ "summary": "{title}" }),
        }),
        status_map: StatusMap {
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
        },
        priority_map: BTreeMap::from([
            ("P1".into(), Priority::Highest),
            ("P2".into(), Priority::High),
            ("P3".into(), Priority::Normal),
            ("P4".into(), Priority::Low),
        ]),
        poll_seconds: 60,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_profile_round_trips_and_validates() {
        let profile = fake_tracker_profile("fake");
        let json = serde_json::to_string_pretty(&profile).unwrap();
        let back: ConnectorProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, profile);
        assert!(profile.validate().is_empty());
        assert_eq!(profile.priority_for("p2"), Some(Priority::High));
    }

    #[test]
    fn a_minimal_profile_fills_defaults() {
        let json = r#"{
            "id": "x", "name": "X",
            "server": { "transport": "http", "url": "https://example.test/mcp" },
            "list": { "tool": "list", "items": "$[*]", "fields": { "external_id": "$.id", "title": "$.name" } }
        }"#;
        let profile: ConnectorProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.version, PROFILE_VERSION);
        assert_eq!(profile.poll_seconds, 60);
        assert!(profile.set_status.is_none());
        assert!(profile.validate().is_empty());
    }

    #[test]
    fn where_clauses_parse_and_stay_out_of_the_json_when_empty() {
        let json = r#"{
            "id": "x", "name": "X",
            "server": { "transport": "http", "url": "https://example.test/mcp" },
            "list": {
                "tool": "get_board", "arguments": { "board_id": "{board_id}" }, "items": "$.tasks[*]",
                "fields": { "external_id": "$.id", "title": "$.name" },
                "where": [ { "path": "$.assignees[*].id", "equals": "u-1" }, { "path": "$.archived", "not": true } ],
                "for_each": {
                    "tool": "list_boards", "arguments": {}, "items": "$.boards[*]", "id": "$.id", "placeholder": "{board_id}",
                    "where": [ { "path": "$.name", "contains": "platform" } ]
                }
            }
        }"#;
        let profile: ConnectorProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.list.filter.len(), 2);
        assert!(profile.list.filter[1].not);
        assert_eq!(
            profile.list.for_each.as_ref().unwrap().filter[0]
                .contains
                .as_deref(),
            Some("platform")
        );
        assert!(profile.validate().is_empty());

        let plain = serde_json::to_string(&fake_tracker_profile("fake")).unwrap();
        assert!(
            !plain.contains("\"where\""),
            "an empty filter is not written out"
        );

        let mut bad = profile.clone();
        bad.list.filter[0].contains = Some("x".into());
        bad.list.filter[1].path = String::new();
        assert_eq!(bad.validate().len(), 2, "{:?}", bad.validate());
    }

    #[test]
    fn me_needs_an_identity() {
        let mut profile = fake_tracker_profile("fake");
        profile.list.filter = vec![Condition {
            path: "$.assignee".into(),
            equals: Some(serde_json::json!("{me}")),
            ..Default::default()
        }];
        assert!(profile.uses_me());
        assert_eq!(profile.validate().len(), 1, "{:?}", profile.validate());
        profile.identity = Some(Identity {
            tool: "whoami".into(),
            arguments: serde_json::json!({}),
            id: "$.id".into(),
        });
        assert!(profile.validate().is_empty());
        let json = serde_json::to_string(&fake_tracker_profile("fake")).unwrap();
        assert!(
            !json.contains("identity"),
            "absent identity is not written out"
        );
    }

    #[test]
    fn validation_catches_the_common_mistakes() {
        let mut profile = fake_tracker_profile("fake");
        profile.id = "bad id!".into();
        profile.status_map.canonical.clear();
        let problems = profile.validate();
        assert_eq!(problems.len(), 2, "{problems:?}");
    }
}
