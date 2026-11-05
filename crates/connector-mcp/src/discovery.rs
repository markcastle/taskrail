//! Working out what a server is, with a model's help.
//!
//! The model sees the server's tool list and the profile format. It may
//! answer with a profile, or it may first ask to *explore*: call a few
//! read-only tools and see what real data looks like, which is how it
//! learns that a tracker keeps work on boards, what a board id looks like,
//! or which statuses exist. Each profile it writes is tried for real
//! through the same pipeline the connector uses; failures go back to the
//! model with a trimmed sample of what the server said. The model never
//! calls a tool itself: this code does, and only tools that are read-only
//! by annotation or by name, unless the caller says otherwise.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use taskrail_connector_api::{ConnectorError, RemoteItem};
use taskrail_core::domain::ItemKind;
use taskrail_llm::{extract_json, LlmClient, LlmError, LlmMessage, LlmRequest, Role};

use crate::auth::SessionAuth;
use crate::extract::fetch_items;
use crate::profile::{fake_tracker_profile, ConnectorProfile, ServerSpec};
use crate::session::{McpSession, ToolSummary};

/// Model calls in total, exploration included.
pub const MAX_ROUNDS: usize = 8;
/// How many times the model may explore before it must write a profile.
pub const MAX_EXPLORATIONS: usize = 4;
/// Tool calls allowed in one exploration.
const MAX_CALLS_PER_EXPLORATION: usize = 3;
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
    /// Allow tool calls the server does not mark read-only and whose names
    /// do not say so either. Off by default; the UI asks the user first.
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
    /// What the model looked at and what went wrong on earlier rounds.
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
            ConnectorError::Auth(m) => DiscoveryError::SignInRequired(m),
            other => DiscoveryError::Server(other.to_string()),
        })?;
    let outcome = discover_with(request, llm, &session).await;
    session.close().await;
    outcome
}

#[derive(Debug, Deserialize)]
struct ExploreCall {
    tool: String,
    #[serde(default)]
    arguments: Value,
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
    log::info!(
        "discovery for {:?} starting with {} tools and {}",
        request.name,
        tools.len(),
        llm.describe()
    );

    let system = system_prompt();
    // The whole exchange so far, so the tool list and the format given at
    // the start stay in front of the model through explorations and retries.
    let mut messages = vec![LlmMessage {
        role: Role::User,
        content: first_prompt(request, &tools),
    }];
    let mut notes = Vec::new();
    let mut last = String::new();
    let mut explorations = 0;

    for round in 1..=MAX_ROUNDS {
        log::info!("discovery round {round}: asking the model");
        let answer = llm
            .complete(&LlmRequest {
                system: system.clone(),
                messages: messages.clone(),
                max_tokens: 8_000,
            })
            .await
            .map_err(|e| {
                log::warn!("discovery round {round}: the model call failed: {e}");
                e
            })?;
        messages.push(LlmMessage {
            role: Role::Assistant,
            content: answer.clone(),
        });
        let mut reply = |content: String| {
            messages.push(LlmMessage {
                role: Role::User,
                content,
            });
        };

        let value = match extract_json(&answer) {
            Ok(value) => value,
            Err(e) => {
                let problem = e.to_string();
                log::warn!("discovery round {round}: {problem}");
                log::info!(
                    "discovery round {round}: the answer began: {}",
                    answer
                        .chars()
                        .take(300)
                        .collect::<String>()
                        .replace('\n', " ")
                );
                notes.push(format!("round {round}: {problem}"));
                last = problem.clone();
                reply(retry_prompt(&problem, None));
                continue;
            }
        };

        // The model wants to look before it writes.
        if let Some(calls) = value.get("explore").and_then(Value::as_array) {
            if explorations >= MAX_EXPLORATIONS {
                let problem =
                    format!("you have explored {MAX_EXPLORATIONS} times; write the profile now");
                log::info!("discovery round {round}: {problem}");
                notes.push(format!("round {round}: {problem}"));
                last = problem.clone();
                reply(retry_prompt(&problem, None));
                continue;
            }
            explorations += 1;
            let mut results = Vec::new();
            for call in calls.iter().take(MAX_CALLS_PER_EXPLORATION) {
                let Ok(call) = serde_json::from_value::<ExploreCall>(call.clone()) else {
                    results.push((
                        "?".to_owned(),
                        "not a valid call: needs \"tool\" and \"arguments\"".to_owned(),
                    ));
                    continue;
                };
                let result = match tools.iter().find(|t| t.name == call.tool) {
                    None => format!("refused: no tool named {:?}", call.tool),
                    Some(tool) if !tool.read_only && !request.allow_unannotated_list => {
                        format!("refused: {:?} is not known to be read-only, so discovery will not call it", call.tool)
                    }
                    Some(_) => match session.call(&call.tool, &call.arguments).await {
                        Ok(raw) => trim(&serde_json::to_string_pretty(&raw).unwrap_or_default()),
                        Err(e) => format!("the call failed: {e}"),
                    },
                };
                log::info!(
                    "discovery round {round}: explored {} ({} chars back)",
                    call.tool,
                    result.len()
                );
                notes.push(format!("round {round}: looked at {}", call.tool));
                results.push((call.tool, result));
            }
            reply(explore_prompt(&results, MAX_EXPLORATIONS - explorations));
            continue;
        }

        let profile = match parse_profile(value.clone(), request) {
            Ok(profile) => profile,
            Err(problem) => {
                log::warn!("discovery round {round}: {problem}");
                log::info!(
                    "discovery round {round}: the answer's top-level keys were {:?}",
                    value
                        .as_object()
                        .map(|o| o.keys().cloned().collect::<Vec<_>>())
                        .unwrap_or_default()
                );
                notes.push(format!("round {round}: {problem}"));
                last = problem.clone();
                reply(retry_prompt(&problem, None));
                continue;
            }
        };

        // Every tool the profile will call during validation must exist and
        // be safe to call.
        let mut calls_to_check = vec![profile.list.call.tool.clone()];
        if let Some(each) = &profile.list.for_each {
            calls_to_check.push(each.tool.clone());
        }
        let mut blocked = None;
        for name in &calls_to_check {
            match tools.iter().find(|t| &t.name == name) {
                None => {
                    blocked = Some(Err(format!("tool {name:?} does not exist on this server")));
                    break;
                }
                Some(tool) if !tool.read_only && !request.allow_unannotated_list => {
                    blocked = Some(Ok(tool.name.clone()));
                    break;
                }
                Some(_) => {}
            }
        }
        match blocked {
            Some(Err(problem)) => {
                log::warn!("discovery round {round}: {problem}");
                notes.push(format!("round {round}: {problem}"));
                last = problem.clone();
                reply(retry_prompt(&problem, None));
                continue;
            }
            Some(Ok(tool)) => {
                return Err(DiscoveryError::NeedsConfirmation {
                    tool,
                    profile: Box::new(profile),
                });
            }
            None => {}
        }

        // A raw sample for the retry prompt: the parent call when fanning
        // out, the list call otherwise.
        let sample_call = profile
            .list
            .for_each
            .as_ref()
            .map(|e| (e.tool.clone(), e.arguments.clone()))
            .unwrap_or_else(|| {
                (
                    profile.list.call.tool.clone(),
                    profile.list.call.arguments.clone(),
                )
            });

        match fetch_items(session, &profile).await {
            Ok(items) if !items.is_empty() => {
                log::info!(
                    "discovery round {round}: {} items through {}{}",
                    items.len(),
                    profile.list.call.tool,
                    profile
                        .list
                        .for_each
                        .as_ref()
                        .map(|e| format!(" for each of {}", e.tool))
                        .unwrap_or_default()
                );
                return Ok(DiscoveryOutcome {
                    profile,
                    sample: items,
                    tools,
                    rounds: round,
                    notes,
                });
            }
            Ok(_) => {
                let problem = format!("the items path {:?} matched nothing", profile.list.items);
                log::warn!("discovery round {round}: {problem}");
                notes.push(format!("round {round}: {problem}"));
                last = problem.clone();
                let raw = session.call(&sample_call.0, &sample_call.1).await.ok();
                reply(retry_prompt(&problem, raw.as_ref()));
            }
            Err(e) => {
                let problem = e.to_string();
                log::warn!("discovery round {round}: {problem}");
                notes.push(format!("round {round}: {problem}"));
                last = problem.clone();
                let raw = session.call(&sample_call.0, &sample_call.1).await.ok();
                reply(retry_prompt(&problem, raw.as_ref()));
            }
        }
    }

    log::warn!("discovery gave up after {MAX_ROUNDS} rounds: {last}");
    Err(DiscoveryError::GaveUp {
        rounds: MAX_ROUNDS,
        last,
    })
}

/// The model's answer as a profile, with the parts it must not decide
/// (transport, id, name, kind) replaced by what the user gave.
fn parse_profile(mut value: Value, request: &DiscoveryRequest) -> Result<ConnectorProfile, String> {
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

fn trim(text: &str) -> String {
    let trimmed: String = text.chars().take(SAMPLE_CHARS).collect();
    if text.chars().count() > SAMPLE_CHARS {
        format!("{trimmed}\n... (cut)")
    } else {
        trimmed
    }
}

fn system_prompt() -> String {
    let example = fake_tracker_profile("<command>");
    let example_json = serde_json::to_string_pretty(&example).unwrap_or_default();
    format!(
        r#"You configure Task Rail, a desktop task strip, to read a person's work from an MCP server.

You will be given the server's tool list. Your job is to write one JSON object, a Connector Profile. You have two kinds of answer, each a single JSON object with no prose and no code fences:

1. EXPLORE. If you need to see real data first, such as which boards or projects exist, what an id looks like, or which statuses are in use, answer with
   {{"explore": [{{"tool": "list_boards", "arguments": {{}}}}]}}
   You get the results back and can answer again. Up to {MAX_CALLS_PER_EXPLORATION} calls per exploration, up to {MAX_EXPLORATIONS} explorations in total. Only tools that are read-only are called; the rest are refused. Explore before guessing an id: never invent ids.

2. THE PROFILE, in this format, shown filled in for an example issue tracker:

{example_json}

Rules for the profile:
- "list" names the tool that returns the person's items, with arguments that narrow it to them where the tool allows (an assignee of "me" or similar, open items only if that option exists).
- "list.items" is a JSONPath, evaluated against the tool's JSON result, that selects the array of items, such as "$.tickets[*]", "$[*]" or "$.data.issues[*]".
- "list.fields" are JSONPaths evaluated against one item. "external_id" and "title" are required. Fill "status", "priority", "due", "url" when the item has them; leave them out otherwise. For calendar events fill "start" and "end" instead of "status".
- When items live inside containers, such as boards, projects or lists, and no tool lists items across all of them, use "list.for_each": {{"tool": "list_boards", "arguments": {{}}, "items": "$.boards[*]", "id": "$.board_id", "placeholder": "{{board_id}}"}} and put the same placeholder in the list call's arguments, for example {{"board_id": "{{board_id}}"}}. The parent tool is called once, the list tool once per element, and the results are joined. If the person's hint names one container, you may instead put its real id straight into the arguments, but only an id you have seen.
- "set_status" names the tool that changes an item's status, with "{{external_id}}" and "{{status}}" placeholders in its arguments. Leave it out if no tool can do it.
- "create" names the tool that creates an item from a title, with a "{{title}}" placeholder. Leave it out if there is none.
- "status_map.to_section" maps every status value the server might return onto "todo", "in_progress" or "done". "status_map.canonical" names the one status to send for each of the three. Use real values you have seen; if you have seen none, use the words the descriptions suggest.
- "priority_map" maps the server's priority words onto "lowest", "low", "normal", "high" or "highest". Leave it empty if the server has no priorities.
- "poll_seconds" is how often to refresh; 60 is a good default, 300 for slow systems.
- Do not include "id", "name", "server", "kind" or "version"; they are filled in for you.
- Never invent tools, fields or ids. Use only names that appear in the tool list and its schemas, and only ids you have seen in exploration results."#
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

fn explore_prompt(results: &[(String, String)], remaining: usize) -> String {
    let mut prompt = String::from("Results of your exploration:\n");
    for (tool, result) in results {
        prompt.push_str(&format!("\n=== {tool} ===\n{result}\n"));
    }
    prompt.push_str(&format!(
        "\nYou may explore {remaining} more {} if you still need to, or answer with the profile as one JSON object and nothing else.",
        if remaining == 1 { "time" } else { "times" }
    ));
    prompt
}

fn retry_prompt(problem: &str, sample: Option<&Value>) -> String {
    let mut prompt = format!("Your previous answer did not work.\n\nProblem: {problem}\n");
    if let Some(sample) = sample {
        let text = serde_json::to_string_pretty(sample).unwrap_or_default();
        prompt.push_str(&format!(
            "\nWhat the server actually returned (trimmed):\n{}\n",
            trim(&text)
        ));
    }
    prompt.push_str("\nAnswer with a corrected profile as one JSON object and nothing else, or explore first if you need to see more.");
    prompt
}
