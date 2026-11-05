//! What the UI can ask the shell to do. Each command is a thin translation
//! from the webview's JSON world into a call on the core, followed by a
//! `rail:changed` event so every view refreshes.

use serde::{Deserialize, Serialize};
use taskrail_core::domain::{Item, ItemId, Status};
use taskrail_core::ports::ItemRepository;
use taskrail_core::settings::RailSettings;
use taskrail_core::usecases::RailView;
use tauri::{AppHandle, Emitter, State};

use crate::connectors::{demo_profile, ConnectorSummary};
use crate::state::{lock, AppState};
use crate::sync::write_back;

pub const CHANGED_EVENT: &str = "rail:changed";

type CommandResult<T> = Result<T, String>;

#[tauri::command]
pub fn rail_view(state: State<'_, AppState>, expand_top: Option<bool>) -> CommandResult<RailView> {
    lock(&state.rail).view(expand_top.unwrap_or(false))
}

#[tauri::command]
pub fn add_task(app: AppHandle, state: State<'_, AppState>, title: String) -> CommandResult<Item> {
    if title.trim().is_empty() {
        return Err("a task needs a title".into());
    }
    let item = lock(&state.rail)
        .service
        .add_task(&title)
        .map_err(|e| e.to_string())?;
    changed(&app);
    Ok(item)
}

#[tauri::command]
pub fn start_task(app: AppHandle, state: State<'_, AppState>, id: ItemId) -> CommandResult<Item> {
    let item = lock(&state.rail)
        .service
        .start(&id)
        .map_err(|e| e.to_string())?;
    write_back(&app, item.origin.clone(), item.status);
    changed(&app);
    Ok(item)
}

#[tauri::command]
pub fn complete_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ItemId,
) -> CommandResult<Item> {
    let mut rail = lock(&state.rail);
    let item = rail.service.complete(&id).map_err(|e| e.to_string())?;
    if rail.active.as_ref() == Some(&id) {
        rail.save_active(None)?;
    }
    drop(rail);
    write_back(&app, item.origin.clone(), item.status);
    changed(&app);
    Ok(item)
}

#[tauri::command]
pub fn reopen_task(app: AppHandle, state: State<'_, AppState>, id: ItemId) -> CommandResult<Item> {
    let item = lock(&state.rail)
        .service
        .reopen(&id)
        .map_err(|e| e.to_string())?;
    write_back(&app, item.origin.clone(), item.status);
    changed(&app);
    Ok(item)
}

#[derive(Debug, Deserialize)]
pub struct MoveRequest {
    pub id: ItemId,
    pub status: Status,
    pub index: usize,
}

#[tauri::command]
pub fn move_task(
    app: AppHandle,
    state: State<'_, AppState>,
    request: MoveRequest,
) -> CommandResult<Item> {
    let mut rail = lock(&state.rail);
    let before = rail
        .service
        .repository()
        .get(&request.id)
        .ok()
        .flatten()
        .map(|i| i.status);
    let item = rail
        .service
        .move_to(&request.id, request.status, request.index)
        .map_err(|e| e.to_string())?;
    if request.status != Status::InProgress && rail.active.as_ref() == Some(&request.id) {
        rail.save_active(None)?;
    }
    drop(rail);
    if before != Some(item.status) {
        write_back(&app, item.origin.clone(), item.status);
    }
    changed(&app);
    Ok(item)
}

#[tauri::command]
pub fn remove_task(app: AppHandle, state: State<'_, AppState>, id: ItemId) -> CommandResult<()> {
    let mut rail = lock(&state.rail);
    rail.service.remove(&id).map_err(|e| e.to_string())?;
    if rail.active.as_ref() == Some(&id) {
        rail.save_active(None)?;
    }
    drop(rail);
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_active(
    app: AppHandle,
    state: State<'_, AppState>,
    id: Option<ItemId>,
) -> CommandResult<()> {
    lock(&state.rail).save_active(id)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> RailSettings {
    lock(&state.rail).settings.clone()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: RailSettings,
) -> CommandResult<RailSettings> {
    let needs_redock = {
        let mut rail = lock(&state.rail);
        let before = rail.settings.clone();
        rail.save_settings(settings)?;
        before.edge != rail.settings.edge || before.width_px != rail.settings.width_px
    };
    if needs_redock {
        crate::dock::redock(&app).map_err(|e| e.to_string())?;
    }
    changed(&app);
    Ok(lock(&state.rail).settings.clone())
}

#[tauri::command]
pub fn list_connectors(state: State<'_, AppState>) -> Vec<ConnectorSummary> {
    lock(&state.connectors).summaries()
}

/// Add a connector from a profile the user pasted or discovery produced,
/// then pull from it straight away.
#[tauri::command]
pub async fn add_connector_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_json: String,
    bearer: Option<String>,
) -> CommandResult<ConnectorSummary> {
    let mut profile: taskrail_connector_mcp::ConnectorProfile =
        serde_json::from_str(&profile_json).map_err(|e| format!("not a valid profile: {e}"))?;
    store_bearer(&mut profile, bearer)?;
    add_and_pull(&app, &state, profile).await
}

/// A pasted token goes to the credential store and the profile only keeps
/// the name it was stored under.
fn store_bearer(
    profile: &mut taskrail_connector_mcp::ConnectorProfile,
    bearer: Option<String>,
) -> CommandResult<()> {
    use taskrail_core::ports::SecretStore;
    let Some(token) = bearer.filter(|t| !t.trim().is_empty()) else {
        return Ok(());
    };
    if let taskrail_connector_mcp::ServerSpec::Http { bearer_secret, .. } = &mut profile.server {
        let name = crate::connectors::bearer_secret_name(&profile.id);
        crate::secrets::OsSecrets
            .set(&name, token.trim())
            .map_err(|e| e.to_string())?;
        *bearer_secret = Some(name);
    }
    Ok(())
}

#[tauri::command]
pub async fn add_demo_connector(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<ConnectorSummary> {
    add_and_pull(&app, &state, demo_profile()?).await
}

async fn add_and_pull(
    app: &AppHandle,
    state: &State<'_, AppState>,
    profile: taskrail_connector_mcp::ConnectorProfile,
) -> CommandResult<ConnectorSummary> {
    let id = profile.id.clone();
    {
        let mut registry = lock(&state.connectors);
        registry.add(profile)?;
        let mut rail = lock(&state.rail);
        registry.save(rail.service.repository_mut())?;
    }
    crate::sync::pull(app, &id).await?;
    changed(app);
    let registry = lock(&state.connectors);
    registry
        .summaries()
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| "connector vanished".to_owned())
}

/// Remove a connector and every card that came from it.
#[tauri::command]
pub fn remove_connector(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<()> {
    let mut registry = lock(&state.connectors);
    if !registry.remove(&id) {
        return Err(format!("no connector {id}"));
    }
    let mut rail = lock(&state.rail);
    registry.save(rail.service.repository_mut())?;
    let gone: Vec<ItemId> = rail
        .service
        .items()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|i| {
            i.origin
                .as_ref()
                .is_some_and(|o| o.connector.as_str() == id)
        })
        .map(|i| i.id)
        .collect();
    for item_id in gone {
        rail.service.remove(&item_id).map_err(|e| e.to_string())?;
    }
    if rail
        .active
        .as_ref()
        .is_some_and(|a| a.as_str().starts_with(&format!("{id}:")))
    {
        rail.save_active(None)?;
    }
    drop(rail);
    drop(registry);
    changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn sync_connector(app: AppHandle, id: String) -> CommandResult<usize> {
    crate::sync::pull(&app, &id).await
}

#[tauri::command]
pub fn get_model_settings(app: AppHandle) -> crate::model::ModelSettings {
    crate::model::settings(&app)
}

#[tauri::command]
pub fn save_model_settings(
    app: AppHandle,
    config: taskrail_llm::LlmConfig,
    api_key: Option<String>,
) -> CommandResult<crate::model::ModelSettings> {
    crate::model::save(&app, config, api_key)
}

#[tauri::command]
pub async fn test_model(app: AppHandle) -> CommandResult<String> {
    crate::model::test(&app).await
}

/// What discovery hands back: a profile to review, or a pause because the
/// model picked a list tool the server does not promise is read-only.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DiscoveryResult {
    Ok(taskrail_connector_mcp::DiscoveryOutcome),
    NeedsConfirmation {
        tool: String,
        profile: taskrail_connector_mcp::ConnectorProfile,
    },
}

#[derive(Debug, Deserialize)]
pub struct DiscoverRequest {
    pub id: String,
    pub name: String,
    pub server: taskrail_connector_mcp::ServerSpec,
    #[serde(default = "task_kind")]
    pub kind: taskrail_core::domain::ItemKind,
    #[serde(default)]
    pub hint: String,
    #[serde(default)]
    pub allow_unannotated_list: bool,
    #[serde(default)]
    pub bearer: Option<String>,
}

fn task_kind() -> taskrail_core::domain::ItemKind {
    taskrail_core::domain::ItemKind::Task
}

#[tauri::command]
pub async fn discover_connector(
    app: AppHandle,
    request: DiscoverRequest,
) -> CommandResult<DiscoveryResult> {
    let llm = crate::model::client(&app)?;
    let request = taskrail_connector_mcp::DiscoveryRequest {
        id: request.id,
        name: request.name,
        server: request.server,
        kind: request.kind,
        hint: request.hint,
        allow_unannotated_list: request.allow_unannotated_list,
        bearer: request.bearer.filter(|t| !t.trim().is_empty()),
    };
    match taskrail_connector_mcp::discover(&request, llm.as_ref()).await {
        Ok(outcome) => Ok(DiscoveryResult::Ok(outcome)),
        Err(taskrail_connector_mcp::DiscoveryError::NeedsConfirmation { tool, profile }) => {
            Ok(DiscoveryResult::NeedsConfirmation {
                tool,
                profile: *profile,
            })
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Quit from the UI. The exit handler in `lib.rs` releases the strip.
#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

fn changed(app: &AppHandle) {
    if let Err(e) = app.emit(CHANGED_EVENT, ()) {
        eprintln!("task rail: could not emit {CHANGED_EVENT}: {e}");
    }
}
