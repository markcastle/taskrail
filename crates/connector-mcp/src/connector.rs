//! A profile plus a session, behind the [`Connector`] trait.

use async_trait::async_trait;
use taskrail_connector_api::{
    Capabilities, Connector, ConnectorError, ConnectorInfo, ConnectorResult, RemoteItem, StatusMap,
};
use taskrail_core::domain::{ConnectorId, Status};
use tokio::sync::Mutex;

use crate::auth::SessionAuth;
use crate::extract::{extract_items, fill_arguments};

use crate::profile::{ConnectorProfile, Scope};
use crate::session::McpSession;

pub struct McpConnector {
    profile: ConnectorProfile,
    auth: SessionAuth,
    scope: Scope,
    session: Mutex<Option<McpSession>>,
}

impl McpConnector {
    /// Nothing is connected until the first call, so building one is free.
    pub fn new(profile: ConnectorProfile, auth: SessionAuth) -> Self {
        Self::with_scope(profile, auth, Scope::default())
    }

    /// A connector seen through one person's answers to the profile's choices.
    pub fn with_scope(profile: ConnectorProfile, auth: SessionAuth, scope: Scope) -> Self {
        Self {
            profile,
            auth,
            scope,
            session: Mutex::new(None),
        }
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// What the person could pick from, for each pick the profile declares.
    pub async fn scope_options(
        &self,
    ) -> ConnectorResult<std::collections::BTreeMap<String, Vec<String>>> {
        let mut guard = self.session.lock().await;
        if guard.is_none() {
            *guard = Some(McpSession::connect(&self.profile.server, &self.auth).await?);
        }
        match guard.as_ref() {
            Some(session) => {
                crate::extract::scope_options(session, &self.profile, &self.scope).await
            }
            None => Err(ConnectorError::Transport("no session".into())),
        }
    }

    pub fn profile(&self) -> &ConnectorProfile {
        &self.profile
    }

    /// What the server actually returns, for people writing or fixing a
    /// profile: the parent call's result when there is a fan-out, and the
    /// list call's result for the first element. Trimmed, as JSON text.
    pub async fn raw_sample(&self, max_chars: usize) -> ConnectorResult<String> {
        let list = &self.profile.list;
        let trim = |value: &serde_json::Value| {
            let text = serde_json::to_string_pretty(value).unwrap_or_default();
            if text.chars().count() > max_chars {
                let cut: String = text.chars().take(max_chars).collect();
                format!("{cut}\n... (cut at {max_chars} characters)")
            } else {
                text
            }
        };
        let mut out = String::new();
        let arguments = match &list.for_each {
            Some(each) => {
                let parent = self.call(&each.tool, &each.arguments).await?;
                out.push_str(&format!("=== {} ===\n{}\n\n", each.tool, trim(&parent)));
                let first = crate::extract::first(&parent, &each.items)
                    .ok()
                    .flatten()
                    .and_then(|element| {
                        crate::extract::first(element, &each.id)
                            .ok()
                            .flatten()
                            .map(|id| match id {
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            })
                    });
                match first {
                    Some(id) => crate::extract::fill_placeholders(
                        &list.call.arguments,
                        &[(each.placeholder.as_str(), id.as_str())],
                    ),
                    None => {
                        out.push_str("(no element found to fan out over)\n");
                        return Ok(out);
                    }
                }
            }
            None => list.call.arguments.clone(),
        };
        let raw = self.call(&list.call.tool, &arguments).await?;
        out.push_str(&format!("=== {} ===\n{}\n", list.call.tool, trim(&raw)));
        Ok(out)
    }

    /// Call a tool on a connected session, connecting first if needed. A
    /// transport failure drops the session so the next call reconnects.
    async fn call(
        &self,
        tool: &str,
        arguments: &serde_json::Value,
    ) -> ConnectorResult<serde_json::Value> {
        let mut guard = self.session.lock().await;
        if guard.is_none() {
            *guard = Some(McpSession::connect(&self.profile.server, &self.auth).await?);
        }
        let outcome = match guard.as_ref() {
            Some(session) => session.call(tool, arguments).await,
            None => Err(ConnectorError::Transport("no session".into())),
        };
        if matches!(
            outcome,
            Err(ConnectorError::Transport(_) | ConnectorError::Auth(_))
        ) {
            if let Some(session) = guard.take() {
                session.close().await;
            }
        }
        outcome
    }
}

#[async_trait]
impl Connector for McpConnector {
    fn info(&self) -> ConnectorInfo {
        ConnectorInfo {
            id: ConnectorId::new(self.profile.id.clone()),
            name: self.profile.name.clone(),
            capabilities: Capabilities {
                read: true,
                write_status: self.profile.set_status.is_some()
                    && self.profile.status_map.is_complete(),
                create: self.profile.create.is_some(),
            },
        }
    }

    fn status_map(&self) -> &StatusMap {
        &self.profile.status_map
    }

    async fn list(&self) -> ConnectorResult<Vec<RemoteItem>> {
        let mut guard = self.session.lock().await;
        if guard.is_none() {
            *guard = Some(McpSession::connect(&self.profile.server, &self.auth).await?);
        }
        let outcome = match guard.as_ref() {
            Some(session) => {
                crate::extract::fetch_items_with(session, &self.profile, &self.scope).await
            }
            None => Err(ConnectorError::Transport("no session".into())),
        };
        if matches!(
            outcome,
            Err(ConnectorError::Transport(_) | ConnectorError::Auth(_))
        ) {
            if let Some(session) = guard.take() {
                session.close().await;
            }
        }
        outcome
    }

    async fn set_status(&self, external_id: &str, status: Status) -> ConnectorResult<()> {
        let spec = self
            .profile
            .set_status
            .as_ref()
            .ok_or(ConnectorError::Unsupported("change a status"))?;
        let remote = self.profile.status_map.remote_for(status).ok_or_else(|| {
            ConnectorError::Invalid(format!("no remote status is mapped for {status:?}"))
        })?;
        let arguments = fill_arguments(&spec.arguments, external_id, remote, "");
        self.call(&spec.tool, &arguments).await?;
        Ok(())
    }

    async fn create(&self, title: &str) -> ConnectorResult<RemoteItem> {
        let spec = self
            .profile
            .create
            .as_ref()
            .ok_or(ConnectorError::Unsupported("create items"))?;
        let arguments = fill_arguments(&spec.arguments, "", "", title);
        let result = self.call(&spec.tool, &arguments).await?;
        // The create result is usually the new item itself. Read it with the
        // list field paths wrapped as a one-item list.
        let mut single_profile = self.profile.clone();
        single_profile.list.items = "$.items[*]".into();
        extract_items(&single_profile, &serde_json::json!({ "items": [result] }))
            .ok()
            .and_then(|mut items| items.pop())
            .ok_or_else(|| ConnectorError::Protocol("create returned nothing recognisable".into()))
    }
}
