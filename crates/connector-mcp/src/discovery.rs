//! Working out what a server is, with a model's help.
//!
//! The model sees the server's tool list and the profile format, and writes
//! a first Connector Profile. The profile is then tried for real: the list
//! tool is called, the extractors are run on what comes back, and if that
//! fails the model gets the error and a sample of the response and tries
//! again. Up to three rounds. The model never sees anything but tool
//! definitions, a trimmed sample, and its own mistakes, and it never calls
//! a tool itself: this code does, and only tools the server marks read-only
//! unless the caller says otherwise.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use taskrail_connector_api::RemoteItem;
use taskrail_core::domain::ItemKind;
use taskrail_llm::{extract_json, LlmClient, LlmError, LlmRequest};

use crate::auth::SessionAuth;
use crate::extract::extract_items;
use crate::profile::{fake_tracker_profile, ConnectorProfile, ServerSpec};
use crate::session::{McpSession, ToolSummary};

pub const MAX_ROUNDS: usize = 3;
const SAMPLE_CHARS: usize = 6_000;

#[derive(Clone)]
pub struct DiscoveryRequest {
    /// The id the profile will get. Letters, digits and dashes.
    pub id: String,
    pub name: String,
    pub server: ServerSpec,
    /// Tasks or calendar events.
    pub kind: ItemKind,
    /// Anything the user wants the model to know: "only my tickets", "the
    /// team calendar", the name of the product.
    pub hint: String,
    /// Allow validation to call a list tool the server does not mark
    /// read-only. Off by default; the UI asks the user first.
    pub allow_unannotated_list: bool,
    /// How to get let in: a static token, stored sign-in credentials, or
    /// nothing.
    pub auth: SessionAuth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryOutcome {
    pub profile: ConnectorProfile,
    /// What the list call returned through the profile, for the user to
    /// check before saving.
    pub sample: Vec<RemoteItem>,
    pub tools: Vec<ToolSummary>,
    pub rounds: usize,
    /// What went wrong on earlier rounds, for the curious.
    pub notes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("could not reach the server: {0}")]
    Server(String),
    #[error("sign in required: {0}")]
    SignInRequired(String),
    #[error("the server has no tools")]
    NoTools,
    #[error("the model could not be used: {0}")]
    Model(#[from] LlmError),
    #[error("the profile's list tool {tool:?} is not marked read-only; confirm before discovery calls it")]
    NeedsConfirmation {
        tool: String,
        profile: Box<ConnectorProfile>,
    },
    #[error("no working profile after {rounds} rounds: {last}")]
    GaveUp { rounds: usize, last: String },
}

/// Run discovery end to end.
pub async fn discover(
    request: &DiscoveryRequest,
    llm: &dyn LlmClient,
) -> Result<DiscoveryOutcome, DiscoveryError> {
    let session = McpSession::connect(&request.server, &request.auth)
        .await
        .map_err(|e| match e {
            taskrail_connector_api::ConnectorError::Auth(m) => DiscoveryError::SignInRequired(m),
            other => DiscoveryError::Server(other.to_string()),
        })?;
    let outcome = discover_with(request, llm, &session).await;
    session.close().await;
    outcome
}

async fn discover_with(
    request: &DiscoveryRequest,
    llm: &dyn LlmClient,
    session: &McpSession,
) -> Result<DiscoveryOutcome, DiscoveryError> {
    let tools = session
        .tools()
        .await
        .map_err(|e| DiscoveryError::Server(e.to_string()))?;
    if tools.is_empty() {
        return Err(DiscoveryError::NoTools);
    }

    let system = system_prompt();
    let mut user = first_prompt(request, &tools);
    let mut notes = Vec::new();
    let mut last = String::new();

    for round in 1..=MAX_ROUNDS {
        let answer = llm
            .complete(&LlmRequest {
                system: system.clone(),
                user: user.clone(),
                max_tokens: 8_000,
            })
            .await?;

        let profile = match parse_profile(&answer, request) {
            Ok(profile) => profile,
            Err(problem) => {
                notes.push(format!("round {round}: {problem}"));
                last = problem.clone();
                user = retry_prompt(&answer, &problem, None);
                continue;
            }
        };

        let list_tool = tools.iter().find(|t| t.name == profile.list.call.tool);
        match list_tool {
            None => {
                let problem = format!(
                    "list tool {:?} does not exist on this server",
                    profile.list.call.tool
                );
                notes.push(format!("round {round}: {problem}"));
                last = problem.clone();
                user = retry_prompt(&answer, &problem, None);
                continue;
            }
            Some(tool) if !tool.read_only && !request.allow_unannotated_list => {
                return Err(DiscoveryError::NeedsConfirmation {
                    tool: tool.name.clone(),
                    profile: Box::new(profile),
                });
            }
            Some(_) => {}
        }

        let raw = match session
            .call(&profile.list.call.tool, &profile.list.call.arguments)
            .await
        {
            Ok(raw) => raw,
            Err(e) => {
                let problem = format!("calling {} failed: {e}", profile.list.call.tool);
                notes.push(format!("round {round}: {problem}"));
                last = problem.clone();
                user = retry_prompt(&answer, &problem, None);
                continue;
            }
        };

        match extract_items(&profile, &raw) {
            Ok(items) if !items.is_empty() => {
                return Ok(DiscoveryOutcome {
                    profile,
                    sample: items,
                    tools,
                    rounds: round,
                    notes,
                });
            }
            Ok(_) => {
                let problem = format!(
                    "the items path {:?} matched nothing in the response",
                    profile.list.items
                );
                notes.push(format!("round {round}: {problem}"));
                last = problem.clone();
                user = retry_prompt(&answer, &problem, Some(&raw));
            }
            Err(e) => {
                let problem = e.to_string();
                notes.push(format!("round {round}: {problem}"));
                last = problem.clone();
                user = retry_prompt(&answer, &problem, Some(&raw));
            }
        }
    }

    Err(DiscoveryError::GaveUp {
        rounds: MAX_ROUNDS,
        last,
    })
}

/// The model's answer as a profile, with the parts it must not decide
/// (transport, id, name, kind) replaced by what the user gave.
fn parse_profile(answer: &str, request: &DiscoveryRequest) -> Result<ConnectorProfile, String> {
    let mut value = extract_json(answer).map_err(|e| e.to_string())?;
    let Value::Object(map) = &mut value else {
        return Err("the answer was not a JSON object".into());
    };
    map.insert("id".into(), Value::String(request.id.clone()));
    map.insert("name".into(), Value::String(request.name.clone()));
    map.insert(
        "server".into(),
        serde_json::to_value(&request.server).map_err(|e| e.to_string())?,
    );
    map.insert(
        "kind".into(),
        serde_json::to_value(request.kind).map_err(|e| e.to_string())?,
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

fn system_prompt() -> String {
    let example = fake_tracker_profile("<command>");
    let example_json = serde_json::to_string_pretty(&example).unwrap_or_default();
    format!(
        r#"You configure Task Rail, a desktop task strip, to read a person's work from an MCP server.

You will be given the server's tool list. Answer with one JSON object, a Connector Profile, and nothing else. No prose, no code fences.

The profile format, shown filled in for an example issue tracker:

{example_json}

Rules:
- "list" names the tool that returns the person's own open items, with arguments that narrow it to them where the tool allows (an assignee of "me" or similar, open items only if that option exists). Prefer tools whose annotations say they are read-only.
- "list.items" is a JSONPath, evaluated against the tool's JSON result, that selects the array of items, such as "$.tickets[*]" or "$[*]" or "$.data.issues[*]".
- "list.fields" are JSONPaths evaluated against one item. "external_id" and "title" are required. Fill "status", "priority", "due", "url" when the item has them; leave them out otherwise. For calendar events fill "start" and "end" instead of "status".
- "set_status" names the tool that changes an item's status, with "{{external_id}}" and "{{status}}" placeholders in its arguments. Leave it out if no tool can do it.
- "create" names the tool that creates an item from a title, with a "{{title}}" placeholder. Leave it out if there is none.
- "status_map.to_section" maps every status value the server might return onto "todo", "in_progress" or "done". "status_map.canonical" names the one status to send for each of the three. Guess sensible values from descriptions and enums; if the tool list gives no status values at all, use the words the descriptions suggest.
- "priority_map" maps the server's priority words onto "lowest", "low", "normal", "high" or "highest". Leave it empty if the server has no priorities.
- "poll_seconds" is how often to refresh; 60 is a good default, 300 for slow systems.
- Do not include "id", "name", "server", "kind" or "version"; they are filled in for you.
- Never invent tools or fields. Use only names that appear in the tool list and its schemas."#
    )
}

fn first_prompt(request: &DiscoveryRequest, tools: &[ToolSummary]) -> String {
    let catalogue = serde_json::to_string_pretty(tools).unwrap_or_default();
    let what = match request.kind {
        ItemKind::Task => "the person's tasks",
        ItemKind::Event => "the person's calendar events",
    };
    let hint = if request.hint.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\n\nWhat the person said about this server: {}",
            request.hint.trim()
        )
    };
    format!(
        "Write the Connector Profile for reading {what} from the server named {:?}.{hint}\n\nThe server's tools:\n{catalogue}",
        request.name
    )
}

fn retry_prompt(previous_answer: &str, problem: &str, sample: Option<&Value>) -> String {
    let mut prompt = format!(
        "Your previous profile did not work.\n\nProblem: {problem}\n\nYour previous answer:\n{previous_answer}\n"
    );
    if let Some(sample) = sample {
        let text = serde_json::to_string_pretty(sample).unwrap_or_default();
        let trimmed: String = text.chars().take(SAMPLE_CHARS).collect();
        let cut = if text.chars().count() > SAMPLE_CHARS {
            "\n... (cut)"
        } else {
            ""
        };
        prompt.push_str(&format!(
            "\nWhat the list tool actually returned (trimmed):\n{trimmed}{cut}\n"
        ));
    }
    prompt.push_str("\nAnswer with the corrected profile as one JSON object and nothing else.");
    prompt
}
