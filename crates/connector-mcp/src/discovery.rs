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
    /// The id the profile will get. Letters, digits and dashes. Empty means
    /// "work one out from the name", kept clear of `taken_ids`.
    pub id: String,
    /// What to call the connector. Empty means "ask the server": the
    /// blueprint's title, then what the server calls itself, then its host.
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
    /// Ids already in use, so a worked-out id can step aside: "tuesday-2".
    pub taken_ids: Vec<String>,
}

/// A request with its name and id settled. The name comes from the person
/// if they gave one, else the blueprint, else the server's own handshake,
/// else the host. The id is the name as a slug, made unique.
fn settle(
    request: &DiscoveryRequest,
    session: &McpSession,
    blueprint_title: Option<&str>,
) -> DiscoveryRequest {
    let mut settled = request.clone();
    if settled.name.trim().is_empty() {
        let from_server = session.server_name().filter(|n| !n.trim().is_empty());
        let from_host = match &request.server {
            ServerSpec::Http { url, .. } => crate::blueprint::host_of(url),
            ServerSpec::Stdio { command, .. } => std::path::Path::new(command)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned()),
        };
        settled.name = blueprint_title
            .map(str::to_owned)
            .or(from_server)
            .or(from_host)
            .unwrap_or_else(|| "Connector".into());
        log::info!("discovery: no name given, calling it {:?}", settled.name);
    }
    settled.name = settled.name.trim().to_owned();
    if settled.id.trim().is_empty() {
        settled.id = unique_id(&slug(&settled.name), &request.taken_ids);
    }
    settled
}

/// Lower-case letters, digits and dashes, the way ids are spelled.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-').to_owned();
    if trimmed.is_empty() {
        "connector".into()
    } else {
        trimmed
    }
}

/// `base`, or `base-2`, `base-3`... the first that is not taken.
pub fn unique_id(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken.iter().any(|t| t == candidate))
        .expect("the integers do not run out")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryOutcome {
    /// The title of the blueprint that was used, when no model was needed.
    #[serde(default)]
    pub blueprint: Option<String>,
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
    #[error("{0}")]
    NoModel(String),
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
    let session = connect(request).await?;
    let request = settle(request, &session, None);
    let outcome = discover_with(&request, llm, &session).await;
    session.close().await;
    outcome
}

/// The way the app does it: a blueprint that fits the server's tools is
/// used, tested live and returned with no model involved. Only when none
/// fits is the model asked, and if there is no model to ask, that is the
/// error.
pub async fn discover_or_blueprint(
    request: &DiscoveryRequest,
    registry: &crate::blueprint::Registry,
    llm: Option<&dyn LlmClient>,
) -> Result<DiscoveryOutcome, DiscoveryError> {
    let session = connect(request).await?;
    let outcome = async {
        let tools = session
            .tools()
            .await
            .map_err(|e| DiscoveryError::Server(e.to_string()))?;
        if tools.is_empty() {
            return Err(DiscoveryError::NoTools);
        }
        let url = match &request.server {
            crate::profile::ServerSpec::Http { url, .. } => Some(url.as_str()),
            crate::profile::ServerSpec::Stdio { .. } => None,
        };
        let mut notes = Vec::new();
        let found = registry.find(&tools, url);
        let request = &settle(request, &session, found.map(|b| b.title.as_str()));
        if let Some(blueprint) = found {
            log::info!(
                "discovery for {:?}: the {:?} blueprint fits this server's tools",
                request.name,
                blueprint.title
            );
            match try_blueprint(blueprint, request, &session, &tools).await {
                Ok(outcome) => return Ok(outcome),
                Err(e) => {
                    log::warn!("the {:?} blueprint did not work here: {e}", blueprint.title);
                    notes.push(format!("the {} blueprint did not work here: {e}", blueprint.title));
                }
            }
        }
        let Some(llm) = llm else {
            return Err(DiscoveryError::NoModel(if notes.is_empty() {
                "no blueprint matches this server, and no API key is saved; add one under Model in settings, or write a profile by hand".into()
            } else {
                format!("{}, and no API key is saved to work out another profile; add one under Model in settings", notes.join("; "))
            }));
        };
        let mut outcome = discover_with(request, llm, &session).await?;
        notes.append(&mut outcome.notes);
        outcome.notes = notes;
        Ok(outcome)
    }
    .await;
    session.close().await;
    outcome
}

async fn connect(request: &DiscoveryRequest) -> Result<McpSession, DiscoveryError> {
    McpSession::connect(&request.server, &request.auth)
        .await
        .map_err(|e| match e {
            ConnectorError::Auth(m) => DiscoveryError::SignInRequired(m),
            other => DiscoveryError::Server(other.to_string()),
        })
}

async fn try_blueprint(
    blueprint: &crate::blueprint::Blueprint,
    request: &DiscoveryRequest,
    session: &McpSession,
    tools: &[ToolSummary],
) -> Result<DiscoveryOutcome, String> {
    let profile =
        blueprint.instantiate(&request.id, &request.name, &request.server, request.kind)?;
    let sample = fetch_items(session, &profile)
        .await
        .map_err(|e| e.to_string())?;
    if sample.is_empty() {
        return Err("it found no items for you".into());
    }
    log::info!(
        "discovery for {:?}: {} items through the {:?} blueprint",
        request.name,
        sample.len(),
        blueprint.title
    );
    Ok(DiscoveryOutcome {
        blueprint: Some(blueprint.title.clone()),
        profile,
        sample,
        tools: tools.to_vec(),
        rounds: 0,
        notes: vec![],
    })
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
                    blueprint: None,
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
fn parse_profile(value: Value, request: &DiscoveryRequest) -> Result<ConnectorProfile, String> {
    if !value.is_object() {
        return Err("the answer was not a JSON object".into());
    }
    crate::blueprint::complete_profile(
        value,
        &request.id,
        &request.name,
        &request.server,
        request.kind,
    )
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
- "list.where" keeps only the items that pass every condition, for when the tool cannot narrow the list itself. Each condition is {{"path": <JSONPath on one item>, "equals": <value>}} or {{"path": ..., "contains": <text, any case>}}; a condition with neither keeps items where the path has a value, and "not": true inverts one. For example, to keep the person's own tasks: {{"where": [{{"path": "$.assignees[*].id", "equals": "<their user id>"}}]}}. Find their id with a whoami-style tool and the assignee field by looking at a real item; never guess either.
- "list.for_each.where" works the same way on the containers, so {{"where": [{{"path": "$.name", "contains": "platform"}}]}} fans out over the Platform boards only.
- When the person asks for their own items, or items from one project or board, honour that: narrow with the tool's own arguments where it has them, otherwise with "where". A profile that returns everyone's work from every board is wrong if they asked for theirs.
- Never write the person's own user id into the profile. Write "{{me}}" where it would go, and add "identity": {{"tool": "whoami", "arguments": {{}}, "id": "$.id"}} naming the tool that returns the signed-in person and the path to their id in its result. The app fills "{{me}}" in at connect time, and the profile then works for anyone.
- "list.trail" says where an item sits, outermost first, so the rail can show "Board › Group › Parent". Each step is {{"name": "board", "label": "$.board.name"}} for a value on the whole result, or {{"name": "group", "label": "$.name", "key": "$.group_id", "items": "$.groups[*]", "id": "$.id"}} to look the item's key up among elements of the result. A parent task is a lookup into the items themselves. Add the steps the result supports.
- "choices" let each person decide for themselves, with controls, instead of the profile deciding for everyone. Put the "assigned to me" condition in a switch rather than in "where": {{"kind": "switch", "id": "mine", "label": "Only tasks assigned to me", "default": true, "where": [...]}}. With a for_each, add {{"kind": "pick", "id": "boards", "label": "Boards", "containers": "$.name"}} so they can keep to some boards, and with a trail step for groups add {{"kind": "pick", "id": "groups", "label": "Groups", "trail": "group"}}. Keep "where" for what nobody would want, such as archived items.
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
