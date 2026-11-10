//! Any MCP server, as a Task Rail connector.
//!
//! The pieces, in the order they are used:
//!
//! - [`profile`] is the saved description of one server: how to reach it,
//!   which tool lists the user's items, how to read each field out of the
//!   result, which tool changes a status, and how its status words map onto
//!   the rail's sections. A profile is plain JSON that a person can read and
//!   edit. Discovery writes one with a model's help; presets ship with the app.
//! - [`session`] holds the live connection, over a child process or HTTP,
//!   and turns tool results into JSON values. [`auth`] gets it let in when
//!   the server wants a sign-in.
//! - [`extract`] applies a profile's JSONPath expressions to those values.
//! - [`connector`] puts the three together behind the [`Connector`] trait.
//!
//! Nothing in this crate calls a model. Once a profile exists, sync is
//! deterministic.
//!
//! [`Connector`]: taskrail_connector_api::Connector

pub mod auth;
pub mod blueprint;
pub mod connector;
pub mod discovery;
pub mod extract;
pub mod fake_server;
pub mod profile;
pub mod session;

pub use auth::{CredentialPersistence, SessionAuth};
pub use blueprint::{Blueprint, Registry};
pub use connector::McpConnector;
pub use discovery::{
    discover, discover_or_blueprint, DiscoveryError, DiscoveryOutcome, DiscoveryRequest,
};
pub use profile::{
    Choice, Condition, ConnectorProfile, FieldPaths, ForEach, Identity, ListSpec, Scope,
    ServerSpec, ToolSpec, TrailStep,
};
pub use session::{McpSession, ToolSummary};
