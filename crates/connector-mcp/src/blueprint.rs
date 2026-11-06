//! Blueprints: profiles that already exist for systems other people have
//! connected, so most people never need a model or an API key.
//!
//! A blueprint is a profile with the personal parts left out (no id, name
//! or server) plus a fingerprint: the tool names it needs. When someone adds
//! a server, the app lists the server's tools, and a blueprint whose tools
//! are all there is used as the profile, tested live, and shown for
//! approval, with no model in the loop. Any profile that says `{me}` with an
//! [`Identity`] works for everyone, which is what makes sharing possible.
//!
//! The registry is one JSON file in the repository. The app fetches it when
//! a server is added and falls back to the copy baked in at build time.
//!
//! [`Identity`]: crate::profile::Identity

use serde::{Deserialize, Serialize};
use serde_json::Value;
use taskrail_core::domain::ItemKind;

use crate::profile::{ConnectorProfile, Identity, ServerSpec};
use crate::session::ToolSummary;

/// The registry as shipped in the repository, compiled in as the fallback.
pub const BUNDLED: &str = include_str!("../../../blueprints/registry.json");

/// Where the live registry lives. The app reads it when a server is added.
pub const REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/markcastle/taskrail/main/blueprints/registry.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Match {
    /// Every one of these tools must be on the server.
    #[serde(default)]
    pub tools: Vec<String>,
    /// If the server's URL contains any of these, the blueprint is preferred
    /// over another whose tools also match.
    #[serde(default)]
    pub url_contains: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blueprint {
    /// Short, stable, used as the default connector id. Letters, digits, dashes.
    pub id: String,
    /// What people call the system: "Tuesday", "Jira Cloud".
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(rename = "match")]
    pub fingerprint: Match,
    /// The profile without `id`, `name` and `server`. Everything else as a
    /// saved profile has it, `identity` and `{me}` included.
    pub profile: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub blueprints: Vec<Blueprint>,
}

fn one() -> u32 {
    1
}

impl Registry {
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("the blueprint registry is not valid: {e}"))
    }

    /// The copy compiled into the app. Never fails: the build checked it.
    pub fn bundled() -> Self {
        Self::parse(BUNDLED).expect("the bundled blueprint registry parses")
    }

    /// The live registry, or the bundled one when it cannot be fetched in
    /// time. Says in the log which it was.
    pub async fn load(url: &str, timeout: std::time::Duration) -> Self {
        let fetched = async {
            let client = reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .map_err(|e| e.to_string())?;
            let text = client
                .get(url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?
                .text()
                .await
                .map_err(|e| e.to_string())?;
            Self::parse(&text)
        }
        .await;
        match fetched {
            Ok(registry) => {
                log::info!(
                    "blueprint registry: {} blueprints from {url}",
                    registry.blueprints.len()
                );
                registry
            }
            Err(e) => {
                let bundled = Self::bundled();
                log::info!(
                    "blueprint registry: using the {} bundled blueprints ({e})",
                    bundled.blueprints.len()
                );
                bundled
            }
        }
    }

    /// The blueprint for a server, going by the tools it offers. When more
    /// than one fits, a URL hint breaks the tie, then the order in the file.
    pub fn find(&self, tools: &[ToolSummary], url: Option<&str>) -> Option<&Blueprint> {
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        let fits: Vec<&Blueprint> = self
            .blueprints
            .iter()
            .filter(|b| !b.fingerprint.tools.is_empty())
            .filter(|b| {
                b.fingerprint
                    .tools
                    .iter()
                    .all(|t| names.contains(&t.as_str()))
            })
            .collect();
        let by_url = url.and_then(|url| {
            fits.iter().copied().find(|b| {
                b.fingerprint
                    .url_contains
                    .iter()
                    .any(|u| url.contains(u.as_str()))
            })
        });
        by_url.or_else(|| fits.first().copied())
    }
}

impl Blueprint {
    /// The profile for one person's connector: the blueprint with the
    /// personal parts filled in, validated.
    pub fn instantiate(
        &self,
        id: &str,
        name: &str,
        server: &ServerSpec,
        kind: ItemKind,
    ) -> Result<ConnectorProfile, String> {
        complete_profile(self.profile.clone(), id, name, server, kind)
    }

    /// A blueprint made from a working profile, ready to contribute. The
    /// fingerprint is the tools the profile calls, which is all a match needs.
    pub fn from_profile(profile: &ConnectorProfile, title: &str, description: &str) -> Self {
        let mut tools: Vec<String> = Vec::new();
        let mut add = |tool: &str| {
            if !tools.iter().any(|t| t == tool) {
                tools.push(tool.to_owned());
            }
        };
        add(&profile.list.call.tool);
        if let Some(each) = &profile.list.for_each {
            add(&each.tool);
        }
        if let Some(Identity { tool, .. }) = &profile.identity {
            add(tool);
        }
        if let Some(spec) = &profile.set_status {
            add(&spec.tool);
        }
        if let Some(spec) = &profile.create {
            add(&spec.tool);
        }
        let url_contains = match &profile.server {
            ServerSpec::Http { url, .. } => host_of(url).into_iter().collect(),
            ServerSpec::Stdio { .. } => vec![],
        };
        let mut value = serde_json::to_value(profile).unwrap_or(Value::Null);
        if let Value::Object(map) = &mut value {
            for key in ["id", "name", "server", "version"] {
                map.remove(key);
            }
        }
        Self {
            id: profile.id.clone(),
            title: title.to_owned(),
            description: description.to_owned(),
            homepage: None,
            fingerprint: Match {
                tools,
                url_contains,
            },
            profile: value,
        }
    }
}

/// A profile JSON without its personal parts becomes one with them: the way
/// discovery completes the model's answer and a blueprint becomes a
/// connector.
pub fn complete_profile(
    mut value: Value,
    id: &str,
    name: &str,
    server: &ServerSpec,
    kind: ItemKind,
) -> Result<ConnectorProfile, String> {
    let Value::Object(map) = &mut value else {
        return Err("the profile is not a JSON object".into());
    };
    map.insert("id".into(), Value::String(id.to_owned()));
    map.insert("name".into(), Value::String(name.to_owned()));
    map.insert(
        "server".into(),
        serde_json::to_value(server).map_err(|e| e.to_string())?,
    );
    map.insert(
        "kind".into(),
        serde_json::to_value(kind).map_err(|e| e.to_string())?,
    );
    map.remove("version");
    let profile: ConnectorProfile = serde_json::from_value(value)
        .map_err(|e| format!("profile does not fit the schema: {e}"))?;
    let problems = profile.validate();
    if !problems.is_empty() {
        return Err(problems.join("; "));
    }
    Ok(profile)
}

/// The host part of a URL, for naming things after a server.
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let host = rest.split('/').next()?.split('@').next_back()?;
    (!host.is_empty()).then(|| host.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::fake_tracker_profile;

    fn tool(name: &str) -> ToolSummary {
        ToolSummary {
            name: name.into(),
            description: String::new(),
            input_schema: Value::Null,
            output_schema: None,
            read_only: true,
        }
    }

    #[test]
    fn the_bundled_registry_parses_and_every_blueprint_instantiates() {
        let registry = Registry::bundled();
        assert!(!registry.blueprints.is_empty());
        for blueprint in &registry.blueprints {
            let server = ServerSpec::Http {
                url: "https://example.test/mcp".into(),
                bearer_secret: None,
                oauth_secret: None,
            };
            let profile = blueprint
                .instantiate("x", "X", &server, ItemKind::Task)
                .unwrap_or_else(|e| panic!("blueprint {} does not instantiate: {e}", blueprint.id));
            assert_eq!(profile.id, "x");
            assert!(
                !blueprint.fingerprint.tools.is_empty(),
                "blueprint {} has no tools to match on",
                blueprint.id
            );
        }
    }

    #[test]
    fn a_server_is_matched_by_its_tools_and_a_url_breaks_ties() {
        let registry = Registry::parse(
            r#"{"blueprints": [
                {"id": "a", "title": "A", "match": {"tools": ["list_things"]}, "profile": {}},
                {"id": "b", "title": "B", "match": {"tools": ["list_things", "get_thing"], "url_contains": ["b.example"]}, "profile": {}},
                {"id": "none", "title": "None", "match": {"tools": []}, "profile": {}}
            ]}"#,
        )
        .unwrap();
        let tools = [tool("list_things"), tool("get_thing"), tool("whoami")];
        assert_eq!(
            registry.find(&tools, None).unwrap().id,
            "a",
            "first fit wins without a URL"
        );
        assert_eq!(
            registry
                .find(&tools, Some("https://b.example/mcp"))
                .unwrap()
                .id,
            "b",
            "the URL picks the specific one"
        );
        assert_eq!(registry.find(&[tool("list_things")], None).unwrap().id, "a");
        assert!(
            registry.find(&[tool("whoami")], None).is_none(),
            "a blueprint without tools never matches"
        );
    }

    #[test]
    fn a_profile_becomes_a_blueprint_and_back() {
        let mut profile = fake_tracker_profile("fake");
        profile.server = ServerSpec::Http {
            url: "https://tracker.example.com/mcp".into(),
            bearer_secret: Some("connector:fake:bearer".into()),
            oauth_secret: None,
        };
        let blueprint = Blueprint::from_profile(
            &profile,
            "Fake tracker",
            "The tracker that ships with Task Rail.",
        );
        assert_eq!(
            blueprint.fingerprint.tools,
            ["list_tickets", "transition_ticket", "create_ticket"]
        );
        assert_eq!(blueprint.fingerprint.url_contains, ["tracker.example.com"]);
        let json = serde_json::to_string(&blueprint).unwrap();
        assert!(
            !json.contains("bearer"),
            "nothing about the person's server or secrets goes out"
        );
        assert!(
            !json.contains("\"id\":\"fake-tracker\",\"name\""),
            "id and name are the blueprint's, not the profile's"
        );

        let again = blueprint
            .instantiate("mine", "My tracker", &profile.server, ItemKind::Task)
            .unwrap();
        assert_eq!(again.id, "mine");
        assert_eq!(again.list, profile.list);
        assert_eq!(again.status_map, profile.status_map);
    }
}
