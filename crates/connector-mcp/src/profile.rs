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
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// The list call and how to read its result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSpec {
    #[serde(flatten)]
    pub call: ToolSpec,
    /// JSONPath to the array of items in the result, such as `$.tickets[*]`.
    pub items: String,
    pub fields: FieldPaths,
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
    #[serde(default)]
    pub set_status: Option<ToolSpec>,
    #[serde(default)]
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
        },
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
    fn validation_catches_the_common_mistakes() {
        let mut profile = fake_tracker_profile("fake");
        profile.id = "bad id!".into();
        profile.status_map.canonical.clear();
        let problems = profile.validate();
        assert_eq!(problems.len(), 2, "{problems:?}");
    }
}
