//! A small MCP server that behaves like a task tracker, over stdio.
//!
//! It exists for two reasons. The connector's tests need a real server to
//! talk to, and a person trying Task Rail before their own system is
//! reachable needs something to connect. The shell runs it in-process when
//! launched with `--fake-mcp-server`, so the demo connector needs no extra
//! binary on the user's machine.
//!
//! The tools are deliberately named unlike the rail's own vocabulary
//! ("Selected", "P2", `ticket_id`) so discovery has something to translate.

use std::sync::{Arc, Mutex};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct Ticket {
    ticket_id: String,
    /// The board this ticket sits on.
    board_id: String,
    summary: String,
    /// One of Backlog, Selected, In Progress, Done.
    state: String,
    /// P1 (highest) to P4 (lowest).
    priority: String,
    due_date: Option<String>,
    link: String,
    /// The person this ticket is assigned to. The signed-in user is "mark".
    assignee: String,
    /// The group on the board this ticket sits in, if any. Subtasks have none.
    group_id: Option<String>,
    /// For a subtask, the ticket it belongs to.
    parent_id: Option<String>,
    /// Set when the ticket has been put away. Still returned, so clients learn to filter.
    archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
struct Group {
    id: String,
    name: String,
}

const GROUP_NOW: &str = "g-now";
const GROUP_LATER: &str = "g-later";

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

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
struct Board {
    board_id: String,
    name: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct BoardList {
    boards: Vec<Board>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct BoardParams {
    /// A board id from list_boards.
    board_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct BoardView {
    board: Board,
    groups: Vec<Group>,
    tasks: Vec<Ticket>,
}

const BOARD_WORK: &str = "b-1111";
const BOARD_HOME: &str = "b-2222";

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
                board_id: if !("T-105".."T-108").contains(&id) {
                    BOARD_WORK.into()
                } else {
                    BOARD_HOME.into()
                },
                summary: summary.into(),
                state: state.into(),
                priority: priority.into(),
                due_date: due.map(str::to_owned),
                link: format!("https://tracker.example/tickets/{id}"),
                // Two tickets belong to someone else, so filters have
                // something to leave out.
                assignee: if id == "T-103" || id == "T-106" {
                    "sam".into()
                } else {
                    "mark".into()
                },
                // Two groups on the Work board; the Home board has none.
                group_id: match id {
                    "T-101" | "T-102" => Some(GROUP_NOW.into()),
                    "T-103" | "T-104" => Some(GROUP_LATER.into()),
                    _ => None,
                },
                parent_id: None,
                archived_at: None,
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
                // A subtask of T-101, and a ticket that was put away.
                Ticket {
                    parent_id: Some("T-101".into()),
                    group_id: None,
                    ..seed("T-108", "Draft the slides", "Selected", "P2", None)
                },
                Ticket {
                    archived_at: Some("2026-10-01T09:00:00Z".into()),
                    ..seed("T-109", "Old idea", "Backlog", "P4", None)
                },
            ])),
            tool_router: Self::tool_router(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Ticket>> {
        self.tickets.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Me {
    id: String,
    name: String,
}

#[tool_router]
impl FakeTracker {
    #[tool(
        description = "Who the signed-in person is.",
        annotations(read_only_hint = true)
    )]
    fn whoami(&self) -> Json<Me> {
        Json(Me {
            id: "mark".into(),
            name: "Mark Castle".into(),
        })
    }

    #[tool(
        description = "List tickets in the tracker, optionally only those assigned to someone and only open ones.",
        annotations(read_only_hint = true)
    )]
    fn list_tickets(&self, Parameters(params): Parameters<ListParams>) -> Json<TicketList> {
        let wanted = params
            .assignee
            .as_deref()
            .map(|a| if a == "me" { "mark" } else { a });
        let tickets = self
            .lock()
            .iter()
            .filter(|t| !params.open_only || t.state != "Done")
            .filter(|t| wanted.map_or(true, |w| t.assignee == w))
            .cloned()
            .collect();
        Json(TicketList { tickets })
    }

    #[tool(
        description = "List the boards tickets are organised on.",
        annotations(read_only_hint = true)
    )]
    fn list_boards(&self) -> Json<BoardList> {
        Json(BoardList {
            boards: vec![
                Board {
                    board_id: BOARD_WORK.into(),
                    name: "Work".into(),
                },
                Board {
                    board_id: BOARD_HOME.into(),
                    name: "Home".into(),
                },
            ],
        })
    }

    #[tool(
        description = "A board and every ticket on it.",
        annotations(read_only_hint = true)
    )]
    fn get_board(
        &self,
        Parameters(params): Parameters<BoardParams>,
    ) -> Result<Json<BoardView>, String> {
        let name = match params.board_id.as_str() {
            BOARD_WORK => "Work",
            BOARD_HOME => "Home",
            other => return Err(format!("no board {other}")),
        };
        let tasks = self
            .lock()
            .iter()
            .filter(|t| t.board_id == params.board_id)
            .cloned()
            .collect();
        let groups = if params.board_id == BOARD_WORK {
            vec![
                Group {
                    id: GROUP_NOW.into(),
                    name: "Now".into(),
                },
                Group {
                    id: GROUP_LATER.into(),
                    name: "Later".into(),
                },
            ]
        } else {
            vec![]
        };
        Ok(Json(BoardView {
            board: Board {
                board_id: params.board_id,
                name: name.into(),
            },
            groups,
            tasks,
        }))
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
            board_id: BOARD_WORK.into(),
            summary: params.summary,
            state: "Backlog".into(),
            priority: "P3".into(),
            due_date: None,
            link: format!("https://tracker.example/tickets/{id}"),
            assignee: "mark".into(),
            group_id: Some(GROUP_LATER.into()),
            parent_id: None,
            archived_at: None,
        };
        tickets.push(ticket.clone());
        Json(ticket)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for FakeTracker {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            Implementation::new("fake-tracker", env!("CARGO_PKG_VERSION"))
                .with_title("Fake tracker"),
        )
    }
}

/// Serve the fake tracker on this process's stdin and stdout until the
/// client goes away.
pub async fn serve_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = FakeTracker::new().serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
