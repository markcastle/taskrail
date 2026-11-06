//! The connectors the user has set up, and the live objects behind them.
//!
//! Profiles are plain JSON in the key-value store under one key. Live
//! connectors are built lazily and shared as `Arc`s so a sync or a write-back
//! can hold one across an `await` without holding any lock.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use taskrail_connector_api::{Capabilities, Connector};
use taskrail_connector_mcp::profile::fake_tracker_profile;
use taskrail_connector_mcp::{ConnectorProfile, McpConnector, ServerSpec};
use taskrail_core::ports::{KeyValueStore, SecretStore};

use crate::secrets::{KeychainCredentials, OsSecrets};
use taskrail_connector_mcp::{CredentialPersistence, SessionAuth};

pub const PROFILES_KEY: &str = "connector_profiles";

/// What the UI shows for each connector.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorSummary {
    pub id: String,
    pub name: String,
    pub server: String,
    pub capabilities: Capabilities,
    pub poll_seconds: u32,
    pub last_sync: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub item_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SyncStatus {
    pub last_sync: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub item_count: usize,
}

#[derive(Default)]
pub struct ConnectorRegistry {
    profiles: Vec<ConnectorProfile>,
    live: HashMap<String, Arc<McpConnector>>,
    status: HashMap<String, SyncStatus>,
}

impl ConnectorRegistry {
    pub fn load(store: &dyn KeyValueStore) -> Self {
        let profiles = store
            .get_value(PROFILES_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        Self {
            profiles,
            ..Default::default()
        }
    }

    pub fn save(&self, store: &mut dyn KeyValueStore) -> Result<(), String> {
        let json = serde_json::to_string(&self.profiles).map_err(|e| e.to_string())?;
        store
            .set_value(PROFILES_KEY, &json)
            .map_err(|e| e.to_string())
    }

    pub fn profiles(&self) -> &[ConnectorProfile] {
        &self.profiles
    }

    pub fn ids(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.id.clone()).collect()
    }

    pub fn add(&mut self, profile: ConnectorProfile) -> Result<(), String> {
        let problems = profile.validate();
        if !problems.is_empty() {
            return Err(problems.join("; "));
        }
        if self.profiles.iter().any(|p| p.id == profile.id) {
            return Err(format!(
                "a connector with id {:?} already exists",
                profile.id
            ));
        }
        self.profiles.push(profile);
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        self.live.remove(id);
        self.status.remove(id);
        self.profiles.len() != before
    }

    /// The live connector for a profile, built on first use.
    pub fn live(&mut self, id: &str) -> Option<Arc<McpConnector>> {
        if let Some(c) = self.live.get(id) {
            return Some(c.clone());
        }
        let profile = self.profiles.iter().find(|p| p.id == id)?.clone();
        let auth = session_auth(&profile.server);
        let connector = Arc::new(McpConnector::new(profile, auth));
        self.live.insert(id.to_owned(), connector.clone());
        Some(connector)
    }

    pub fn record(&mut self, id: &str, outcome: Result<usize, String>, at: DateTime<Utc>) {
        let status = self.status.entry(id.to_owned()).or_default();
        match outcome {
            Ok(count) => {
                status.last_sync = Some(at);
                status.last_error = None;
                status.item_count = count;
            }
            Err(message) => status.last_error = Some(message),
        }
    }

    pub fn due_for_sync(&self, now: DateTime<Utc>) -> Vec<String> {
        self.profiles
            .iter()
            .filter(|p| {
                self.status
                    .get(&p.id)
                    .and_then(|s| s.last_sync)
                    .map_or(true, |last| {
                        (now - last).num_seconds() >= i64::from(p.poll_seconds)
                    })
            })
            .map(|p| p.id.clone())
            .collect()
    }

    pub fn summaries(&self) -> Vec<ConnectorSummary> {
        self.profiles
            .iter()
            .map(|p| {
                let status = self.status.get(&p.id).cloned().unwrap_or_default();
                let live = self.live.get(&p.id);
                ConnectorSummary {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    server: taskrail_connector_mcp::session::describe(&p.server),
                    capabilities: live.map(|c| c.info().capabilities).unwrap_or(Capabilities {
                        read: true,
                        write_status: p.set_status.is_some() && p.status_map.is_complete(),
                        create: p.create.is_some(),
                    }),
                    poll_seconds: p.poll_seconds,
                    last_sync: status.last_sync,
                    last_error: status.last_error,
                    item_count: status.item_count,
                }
            })
            .collect()
    }
}

/// The demo connector: this very binary, run as the fake tracker.
pub fn demo_profile() -> Result<ConnectorProfile, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut profile = fake_tracker_profile(exe.to_string_lossy().to_string());
    profile.server = ServerSpec::Stdio {
        command: exe.to_string_lossy().to_string(),
        args: vec!["--fake-mcp-server".into()],
        env: Default::default(),
    };
    profile.id = "demo-tracker".into();
    profile.name = "Demo tracker".into();
    profile.poll_seconds = 30;
    Ok(profile)
}

/// Where a connector's bearer token lives in the credential store.
pub fn bearer_secret_name(connector_id: &str) -> String {
    format!("connector:{connector_id}:bearer")
}

/// How to get let in to a server, from the names its spec records.
pub fn session_auth(server: &ServerSpec) -> SessionAuth {
    match server {
        ServerSpec::Http {
            bearer_secret,
            oauth_secret,
            ..
        } => SessionAuth {
            bearer: bearer_secret
                .as_deref()
                .and_then(|name| OsSecrets.get(name).ok().flatten()),
            oauth: oauth_secret.as_deref().map(|name| {
                Arc::new(KeychainCredentials::new(name)) as Arc<dyn CredentialPersistence>
            }),
        },
        ServerSpec::Stdio { .. } => SessionAuth::default(),
    }
}

/// Where a connector's sign-in credentials live in the credential store.
/// Where a connector made before credentials were keyed by server kept
/// its sign-in. Still read, never written.
pub fn oauth_secret_name(connector_id: &str) -> String {
    format!("connector:{connector_id}:oauth")
}

/// Where a server's sign-in lives: keyed by its host, because the
/// credentials are the server's, and the connector's id is not known
/// until the server has said what it is.
pub fn oauth_secret_name_for_url(url: &str) -> String {
    let host = taskrail_connector_mcp::blueprint::host_of(url).unwrap_or_else(|| url.to_owned());
    format!("oauth:{host}")
}
