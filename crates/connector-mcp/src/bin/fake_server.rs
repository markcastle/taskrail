//! A small MCP server that behaves like a task tracker, over stdio.
//!
//! It exists for two reasons. The connector's tests need a real server to
//! talk to, and a person trying Task Rail before their own system is
//! reachable needs something to connect. Add it as a stdio server with this
//! binary as the command and the rail fills with its tasks.
//!
//! The tools are deliberately named unlike the rail's own vocabulary
//! ("Selected", "P2", `ticket_id`) so discovery has something to translate.

use std::sync::{Arc, Mutex};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ServerCapabilities, ServerConfig};
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct Ticket {
    ticket_id: String,
    summary: String,
    /// One of Backlog, Selected, In Progress, Done.
    state: String,
    /// P1 (highest) to P4 (lowest).
    priority: String,
    due_date: Option<String>,
    link: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct TicketList {
    tickets: Vec<Ticket>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListParams {
    /// Only tickets assigned to this person. Use "me" for the signed-in user.
    assignee: Option<String>,
    /// Leave out finished tickets.
    #[serde(default)]
    open_only: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetParams {
    ticket_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TransitionParams {
    ticket_id: String,
    /// The target state: Backlog, Selected, In Progress or Done.
    state: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateParams {
    summary: String,
}

#[derive(Clone)]
struct FakeTracker {
    tickets: Arc<Mutex<Vec<Ticket>>>,
    tool_router: ToolRouter<Self>,
}

impl FakeTracker {
    fn new() -> Self {
        let seed =
            |id: &str, summary: &str, state: &str, priority: &str, due: Option<&str>| Ticket {
                ticket_id: id.into(),
                summary: summary.into(),
                state: state.into(),
                priority: priority.into(),
                due_date: due.map(str::to_owned),
                link: format!("https://tracker.example/tickets/{id}"),
            };
        Self {
            tickets: Arc::new(Mutex::new(vec![
                seed(
                    "T-101",
                    "Prepare board deck",
                    "Selected",
                    "P1",
                    Some("2026-11-05T16:00:00Z"),
                ),
                seed(
                    "T-102",
                    "Reply to supplier",
                    "Selected",
                    "P2",
                    Some("2026-11-05T18:00:00Z"),
                ),
                seed("T-103", "Book venue", "Backlog", "P3", None),
                seed("T-104", "Update docs", "Backlog", "P4", None),
                seed("T-105", "Design dashboard", "In Progress", "P2", None),
                seed("T-106", "Review pull request", "In Progress", "P3", None),
                seed("T-107", "Triage inbox", "Done", "P3", None),
            ])),
            tool_router: Self::tool_router(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Ticket>> {
        self.tickets.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[tool_router]
impl FakeTracker {
    #[tool(
        description = "List tickets in the tracker, optionally only those assigned to someone and only open ones.",
        annotations(read_only_hint = true)
    )]
    fn list_tickets(&self, Parameters(params): Parameters<ListParams>) -> Json<TicketList> {
        let _ = params.assignee;
        let tickets = self
            .lock()
            .iter()
            .filter(|t| !params.open_only || t.state != "Done")
            .cloned()
            .collect();
        Json(TicketList { tickets })
    }

    #[tool(
        description = "Fetch one ticket by id.",
        annotations(read_only_hint = true)
    )]
    fn get_ticket(
        &self,
        Parameters(params): Parameters<GetParams>,
    ) -> Result<Json<Ticket>, String> {
        self.lock()
            .iter()
            .find(|t| t.ticket_id == params.ticket_id)
            .cloned()
            .map(Json)
            .ok_or_else(|| format!("no ticket {}", params.ticket_id))
    }

    #[tool(
        description = "Move a ticket to another state: Backlog, Selected, In Progress or Done.",
        annotations(read_only_hint = false, idempotent_hint = true)
    )]
    fn transition_ticket(
        &self,
        Parameters(params): Parameters<TransitionParams>,
    ) -> Result<Json<Ticket>, String> {
        const STATES: [&str; 4] = ["Backlog", "Selected", "In Progress", "Done"];
        if !STATES.contains(&params.state.as_str()) {
            return Err(format!("unknown state {}", params.state));
        }
        let mut tickets = self.lock();
        let ticket = tickets
            .iter_mut()
            .find(|t| t.ticket_id == params.ticket_id)
            .ok_or_else(|| format!("no ticket {}", params.ticket_id))?;
        ticket.state = params.state;
        Ok(Json(ticket.clone()))
    }

    #[tool(
        description = "Create a new ticket in the Backlog.",
        annotations(read_only_hint = false)
    )]
    fn create_ticket(&self, Parameters(params): Parameters<CreateParams>) -> Json<Ticket> {
        let mut tickets = self.lock();
        let id = format!("T-{}", 101 + tickets.len());
        let ticket = Ticket {
            ticket_id: id.clone(),
            summary: params.summary,
            state: "Backlog".into(),
            priority: "P3".into(),
            due_date: None,
            link: format!("https://tracker.example/tickets/{id}"),
        };
        tickets.push(ticket.clone());
        Json(ticket)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for FakeTracker {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = FakeTracker::new().serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
