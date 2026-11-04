//! A profile plus a session, behind the [`Connector`] trait.

use async_trait::async_trait;
use taskrail_connector_api::{
    Capabilities, Connector, ConnectorError, ConnectorInfo, ConnectorResult, RemoteItem, StatusMap,
};
use taskrail_core::domain::{ConnectorId, Status};
use tokio::sync::Mutex;

use crate::extract::{extract_items, fill_arguments};
use crate::profile::ConnectorProfile;
use crate::session::McpSession;

pub struct McpConnector {
    profile: ConnectorProfile,
    bearer: Option<String>,
    session: Mutex<Option<McpSession>>,
}

impl McpConnector {
    /// Nothing is connected until the first call, so building one is free.
    pub fn new(profile: ConnectorProfile, bearer: Option<String>) -> Self {
        Self {
            profile,
            bearer,
            session: Mutex::new(None),
        }
    }

    pub fn profile(&self) -> &ConnectorProfile {
        &self.profile
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
            *guard = Some(McpSession::connect(&self.profile.server, self.bearer.as_deref()).await?);
        }
        let outcome = match guard.as_ref() {
            Some(session) => session.call(tool, arguments).await,
            None => Err(ConnectorError::Transport("no session".into())),
        };
        if matches!(outcome, Err(ConnectorError::Transport(_))) {
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
        let result = self
            .call(
                &self.profile.list.call.tool,
                &self.profile.list.call.arguments,
            )
            .await?;
        extract_items(&self.profile, &result).map_err(|e| ConnectorError::Protocol(e.to_string()))
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
