//! A live connection to one MCP server.

use std::collections::BTreeMap;

use rmcp::model::{CallToolRequestParams, ClientConfig, Tool};
use rmcp::service::RunningService;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{StreamableHttpClientTransport, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use taskrail_connector_api::ConnectorError;
use tokio::process::Command;

use crate::profile::ServerSpec;

/// What discovery hands to a model: the parts of a tool definition that
/// matter for working out what it does, nothing else.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSummary {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(default)]
    pub output_schema: Option<Value>,
    /// `true` when the server says the tool changes nothing. Discovery may
    /// call these freely; anything else needs the user's say-so.
    pub read_only: bool,
}

impl From<&Tool> for ToolSummary {
    fn from(tool: &Tool) -> Self {
        Self {
            name: tool.name.to_string(),
            description: tool.description.as_deref().unwrap_or_default().to_owned(),
            input_schema: Value::Object((*tool.input_schema).clone()),
            output_schema: tool
                .output_schema
                .as_ref()
                .map(|s| Value::Object((**s).clone())),
            read_only: tool
                .annotations
                .as_ref()
                .and_then(|a| a.read_only_hint)
                .unwrap_or(false),
        }
    }
}

pub struct McpSession {
    service: RunningService<RoleClient, ClientConfig>,
}

impl McpSession {
    /// Connect and complete the MCP handshake. `bearer` is sent as an
    /// `Authorization` header on HTTP transports when present.
    pub async fn connect(spec: &ServerSpec, bearer: Option<&str>) -> Result<Self, ConnectorError> {
        let transport_err = |e: &dyn std::fmt::Display| ConnectorError::Transport(e.to_string());
        let service = match spec {
            ServerSpec::Stdio { command, args, env } => {
                let mut cmd = Command::new(command);
                cmd.args(args);
                for (k, v) in env {
                    cmd.env(k, v);
                }
                let transport = TokioChildProcess::new(cmd).map_err(|e| transport_err(&e))?;
                ClientConfig::default()
                    .serve(transport)
                    .await
                    .map_err(|e| transport_err(&e))?
            }
            ServerSpec::Http { url, .. } => {
                let mut config = StreamableHttpClientTransportConfig::with_uri(url.clone());
                if let Some(token) = bearer {
                    config.auth_header = Some(format!("Bearer {token}"));
                }
                let transport = StreamableHttpClientTransport::from_config(config);
                ClientConfig::default()
                    .serve(transport)
                    .await
                    .map_err(|e| transport_err(&e))?
            }
        };
        Ok(Self { service })
    }

    pub fn server_name(&self) -> Option<String> {
        self.service
            .peer_info()
            .and_then(|info| info.server_info.as_ref().map(|s| s.name.to_string()))
    }

    pub async fn tools(&self) -> Result<Vec<ToolSummary>, ConnectorError> {
        let tools = self
            .service
            .list_all_tools()
            .await
            .map_err(|e| ConnectorError::Protocol(e.to_string()))?;
        Ok(tools.iter().map(ToolSummary::from).collect())
    }

    /// Call a tool and return its result as JSON (see
    /// [`crate::extract::result_to_json`]).
    pub async fn call(&self, tool: &str, arguments: &Value) -> Result<Value, ConnectorError> {
        let arguments: serde_json::Map<String, Value> = match arguments {
            Value::Object(map) => map.clone(),
            Value::Null => Default::default(),
            other => {
                return Err(ConnectorError::Invalid(format!(
                    "tool arguments must be an object, got {other}"
                )))
            }
        };
        let result = self
            .service
            .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
            .await
            .map_err(|e| ConnectorError::Protocol(e.to_string()))?;
        crate::extract::result_to_json(&result).map_err(|e| ConnectorError::Protocol(e.to_string()))
    }

    pub async fn close(self) {
        let _ = self.service.cancel().await;
    }
}

/// Environment variables a stdio server should inherit, for the UI to show.
pub fn describe(spec: &ServerSpec) -> String {
    match spec {
        ServerSpec::Stdio { command, args, .. } => {
            let mut parts = vec![command.clone()];
            parts.extend(args.iter().cloned());
            parts.join(" ")
        }
        ServerSpec::Http { url, .. } => url.clone(),
    }
}

#[allow(dead_code)]
fn _env_type_check(_: &BTreeMap<String, String>) {}
