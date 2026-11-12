//! The connectors the user has set up, and the live objects behind them.
//!
//! Profiles are plain JSON in the key-value store under one key. Live
//! connectors are built lazily and shared as `Arc`s so a sync or a write-back
//! can hold one across an `await` without holding any lock.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use taskrail_connector_api::{Capabilities, Connector};
use taskrail_connector_ics::IcsCalendar;
use taskrail_connector_mcp::profile::fake_tracker_profile;
use taskrail_connector_mcp::{ConnectorProfile, McpConnector, Scope, ServerSpec};
use taskrail_core::ports::{KeyValueStore, SecretStore};

use crate::secrets::{KeychainCredentials, OsSecrets};
use taskrail_connector_mcp::{CredentialPersistence, SessionAuth};

pub const PROFILES_KEY: &str = "connector_profiles";
/// The ids of connectors the person has paused, as a JSON list.
pub const PAUSED_KEY: &str = "connector_paused";
/// Each connector's answers to its profile's choices, as a JSON map by id.
pub const SCOPES_KEY: &str = "connector_scopes";
/// Calendar feeds, as a JSON list. Their addresses live in the credential store.
pub const FEEDS_KEY: &str = "calendar_feeds";

/// A calendar that publishes an iCalendar feed. The address is the secret
/// (whoever has it can read the calendar), so only its credential-store
/// name is kept here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarFeed {
    pub id: String,
    pub name: String,
    pub url_secret: String,
    #[serde(default = "default_feed_poll")]
    pub poll_seconds: u32,
}

fn default_feed_poll() -> u32 {
    300
}

pub fn calendar_secret_name(id: &str) -> String {
    format!("calendar:{id}:url")
}

/// What the UI shows for each connector.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorSummary {
    pub id: String,
    pub name: String,
    /// "mcp" for a profile-driven server, "calendar" for a feed.
    pub kind: String,
    pub server: String,
    pub capabilities: Capabilities,
    pub poll_seconds: u32,
    pub last_sync: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub item_count: usize,
    /// Paused: its tasks stay on the rail, nothing is pulled or written back.
    pub paused: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SyncStatus {
    pub last_sync: Option<DateTime<Utc>>,
    /// The last pull, whether it worked or not, so a failing connector is
    /// tried again after a pause and not on every tick.
    pub last_attempt: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub item_count: usize,
}

/// How long to leave a connector alone after a pull failed.
const RETRY_SECONDS: u32 = 60;

#[derive(Default)]
pub struct ConnectorRegistry {
    profiles: Vec<ConnectorProfile>,
    live: HashMap<String, Arc<McpConnector>>,
    status: HashMap<String, SyncStatus>,
    paused: Vec<String>,
    scopes: HashMap<String, Scope>,
    feeds: Vec<CalendarFeed>,
    live_feeds: HashMap<String, Arc<IcsCalendar>>,
}

impl ConnectorRegistry {
    pub fn load(store: &dyn KeyValueStore) -> Self {
        let profiles = store
            .get_value(PROFILES_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let paused = store
            .get_value(PAUSED_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let scopes = store
            .get_value(SCOPES_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let feeds = store
            .get_value(FEEDS_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        Self {
            profiles,
            paused,
            scopes,
            feeds,
            ..Default::default()
        }
    }

    pub fn save(&self, store: &mut dyn KeyValueStore) -> Result<(), String> {
        let json = serde_json::to_string(&self.profiles).map_err(|e| e.to_string())?;
        store
            .set_value(PROFILES_KEY, &json)
            .map_err(|e| e.to_string())?;
        let paused = serde_json::to_string(&self.paused).map_err(|e| e.to_string())?;
        store
            .set_value(PAUSED_KEY, &paused)
            .map_err(|e| e.to_string())?;
        let scopes = serde_json::to_string(&self.scopes).map_err(|e| e.to_string())?;
        store
            .set_value(SCOPES_KEY, &scopes)
            .map_err(|e| e.to_string())?;
        let feeds = serde_json::to_string(&self.feeds).map_err(|e| e.to_string())?;
        store
            .set_value(FEEDS_KEY, &feeds)
            .map_err(|e| e.to_string())
    }

    pub fn feeds(&self) -> &[CalendarFeed] {
        &self.feeds
    }

    pub fn add_feed(&mut self, feed: CalendarFeed) -> Result<(), String> {
        if self.ids().iter().any(|id| id == &feed.id) {
            return Err(format!("a connector with id {:?} already exists", feed.id));
        }
        self.feeds.push(feed);
        Ok(())
    }

    /// Any connector, MCP or calendar, behind the one trait sync speaks.
    pub fn live_any(&mut self, id: &str) -> Option<Arc<dyn Connector>> {
        if let Some(c) = self.live(id) {
            return Some(c);
        }
        if let Some(c) = self.live_feeds.get(id) {
            return Some(c.clone());
        }
        let feed = self.feeds.iter().find(|f| f.id == id)?.clone();
        let url = crate::secrets::OsSecrets
            .get(&feed.url_secret)
            .ok()
            .flatten()?;
        let calendar = Arc::new(IcsCalendar::new(feed.id.clone(), feed.name.clone(), url));
        self.live_feeds.insert(id.to_owned(), calendar.clone());
        Some(calendar)
    }

    /// One connector's answers to its choices, or the defaults.
    pub fn scope(&self, id: &str) -> Scope {
        self.scopes.get(id).cloned().unwrap_or_default()
    }

    /// Replace a connector's answers. The live connector is rebuilt so the
    /// next pull sees them.
    pub fn set_scope(&mut self, id: &str, scope: Scope) {
        self.scopes.insert(id.to_owned(), scope);
        self.live.remove(id);
    }

    pub fn profile(&self, id: &str) -> Option<&ConnectorProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn is_paused(&self, id: &str) -> bool {
        self.paused.iter().any(|p| p == id)
    }

    /// Pause or resume. Returns whether anything changed.
    pub fn set_paused(&mut self, id: &str, paused: bool) -> bool {
        let was = self.is_paused(id);
        if paused && !was {
            self.paused.push(id.to_owned());
        } else if !paused && was {
            self.paused.retain(|p| p != id);
        }
        was != paused
    }

    /// The display name for a connector id, for messages.
    pub fn name_of(&self, id: &str) -> String {
        self.profiles
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
            .or_else(|| {
                self.feeds
                    .iter()
                    .find(|f| f.id == id)
                    .map(|f| f.name.clone())
            })
            .unwrap_or_else(|| id.to_owned())
    }

    pub fn profiles(&self) -> &[ConnectorProfile] {
        &self.profiles
    }

    pub fn ids(&self) -> Vec<String> {
        self.profiles
            .iter()
            .map(|p| p.id.clone())
            .chain(self.feeds.iter().map(|f| f.id.clone()))
            .collect()
    }

    /// Swap a connector's profile for a newer one with the same id. The live
    /// connector is rebuilt so the next pull uses it. False when there is no
    /// connector with that id any more.
    pub fn replace(&mut self, profile: ConnectorProfile) -> bool {
        let Some(slot) = self.profiles.iter_mut().find(|p| p.id == profile.id) else {
            return false;
        };
        self.live.remove(&profile.id);
        *slot = profile;
        true
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
        let before = self.profiles.len() + self.feeds.len();
        self.profiles.retain(|p| p.id != id);
        if let Some(feed) = self.feeds.iter().find(|f| f.id == id) {
            let _ = crate::secrets::OsSecrets.delete(&feed.url_secret);
        }
        self.feeds.retain(|f| f.id != id);
        self.live.remove(id);
        self.live_feeds.remove(id);
        self.status.remove(id);
        self.paused.retain(|p| p != id);
        self.scopes.remove(id);
        self.profiles.len() + self.feeds.len() != before
    }

    /// The live connector for a profile, built on first use.
    pub fn live(&mut self, id: &str) -> Option<Arc<McpConnector>> {
        if let Some(c) = self.live.get(id) {
            return Some(c.clone());
        }
        let profile = self.profiles.iter().find(|p| p.id == id)?.clone();
        let auth = session_auth(&profile.server);
        let scope = self.scope(id);
        let connector = Arc::new(McpConnector::with_scope(profile, auth, scope));
        self.live.insert(id.to_owned(), connector.clone());
        Some(connector)
    }

    pub fn record(&mut self, id: &str, outcome: Result<usize, String>, at: DateTime<Utc>) {
        let status = self.status.entry(id.to_owned()).or_default();
        status.last_attempt = Some(at);
        match outcome {
            Ok(count) => {
                status.last_sync = Some(at);
                status.last_error = None;
                status.item_count = count;
            }
            Err(message) => status.last_error = Some(message),
        }
    }

    /// What the last pull of a connector said went wrong, if it failed.
    pub fn last_error(&self, id: &str) -> Option<String> {
        self.status.get(id).and_then(|s| s.last_error.clone())
    }

    pub fn due_for_sync(&self, now: DateTime<Utc>) -> Vec<String> {
        let due = |id: &str, poll: u32| {
            if self.is_paused(id) {
                return false;
            }
            let Some(status) = self.status.get(id) else {
                return true;
            };
            // After a failure, try again soon but not at once; after a
            // success, when the connector's own interval has passed.
            let (since, wait) = match &status.last_error {
                Some(_) => (status.last_attempt, RETRY_SECONDS.min(poll)),
                None => (status.last_sync, poll),
            };
            match since {
                Some(last) => (now - last).num_seconds() >= i64::from(wait),
                None => true,
            }
        };
        self.profiles
            .iter()
            .filter(|p| due(&p.id, p.poll_seconds))
            .map(|p| p.id.clone())
            .chain(
                self.feeds
                    .iter()
                    .filter(|f| due(&f.id, f.poll_seconds))
                    .map(|f| f.id.clone()),
            )
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
                    kind: "mcp".into(),
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
                    paused: self.is_paused(&p.id),
                }
            })
            .chain(self.feeds.iter().map(|f| {
                let status = self.status.get(&f.id).cloned().unwrap_or_default();
                ConnectorSummary {
                    id: f.id.clone(),
                    name: f.name.clone(),
                    kind: "calendar".into(),
                    server: "iCalendar feed".into(),
                    capabilities: Capabilities {
                        read: true,
                        write_status: false,
                        create: false,
                    },
                    poll_seconds: f.poll_seconds,
                    last_sync: status.last_sync,
                    last_error: status.last_error,
                    item_count: status.item_count,
                    paused: self.is_paused(&f.id),
                }
            }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    /// A registry holding one calendar that polls every five minutes.
    fn one_feed() -> ConnectorRegistry {
        ConnectorRegistry {
            feeds: vec![CalendarFeed {
                id: "cal".into(),
                name: "Calendar".into(),
                url_secret: "calendar:cal:url".into(),
                poll_seconds: 300,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_newer_profile_takes_the_place_of_the_one_with_its_id() {
        let demo = taskrail_connector_mcp::Registry::bundled()
            .blueprints
            .into_iter()
            .find(|b| b.id == "demo-tracker")
            .unwrap();
        let server = ServerSpec::Http {
            url: "https://example.test/mcp".into(),
            bearer_secret: None,
            oauth_secret: None,
        };
        let today = demo
            .instantiate(
                "demo",
                "Demo",
                &server,
                taskrail_core::domain::ItemKind::Task,
            )
            .unwrap();
        let mut old = today.clone();
        old.list.trail.clear();
        old.choices.clear();

        let mut registry = ConnectorRegistry::default();
        registry.add(old).unwrap();
        assert!(registry.replace(today.clone()));
        assert_eq!(registry.profiles(), std::slice::from_ref(&today));

        let mut stranger = today;
        stranger.id = "someone-else".into();
        assert!(
            !registry.replace(stranger),
            "nothing with that id to replace"
        );
        assert_eq!(registry.profiles().len(), 1);
    }

    #[test]
    fn a_connector_never_pulled_is_due() {
        assert_eq!(one_feed().due_for_sync(Utc::now()), vec!["cal".to_owned()]);
    }

    #[test]
    fn after_a_good_pull_it_waits_its_own_interval() {
        let mut registry = one_feed();
        let then = Utc::now();
        registry.record("cal", Ok(3), then);
        assert!(registry
            .due_for_sync(then + Duration::seconds(299))
            .is_empty());
        assert_eq!(
            registry.due_for_sync(then + Duration::seconds(300)).len(),
            1
        );
    }

    #[test]
    fn after_a_failed_pull_it_tries_again_in_a_minute_not_at_once() {
        let mut registry = one_feed();
        let good = Utc::now();
        registry.record("cal", Ok(3), good);
        let failed = good + Duration::seconds(300);
        registry.record("cal", Err("no answer".into()), failed);
        assert!(registry
            .due_for_sync(failed + Duration::seconds(10))
            .is_empty());
        assert_eq!(
            registry.due_for_sync(failed + Duration::seconds(60)).len(),
            1
        );
        // The last good pull is still what the rail reports as "synced".
        assert_eq!(registry.status.get("cal").unwrap().last_sync, Some(good));
    }

    #[test]
    fn a_pull_that_works_again_clears_the_error() {
        let mut registry = one_feed();
        let now = Utc::now();
        registry.record("cal", Err("no answer".into()), now);
        assert_eq!(registry.last_error("cal").as_deref(), Some("no answer"));
        registry.record("cal", Ok(3), now + Duration::seconds(60));
        assert_eq!(registry.last_error("cal"), None);
    }
}
