//! What the UI can ask the shell to do. Each command is a thin translation
//! from the webview's JSON world into a call on the core, followed by a
//! `rail:changed` event so every view refreshes.

use serde::{Deserialize, Serialize};
use taskrail_connector_api::Connector;
use taskrail_core::domain::{Item, ItemId, Status};
use taskrail_core::ports::ItemRepository;
use taskrail_core::settings::RailSettings;
use taskrail_core::usecases::RailView;
use tauri::{AppHandle, Emitter, Manager, State};

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

/// A task from a paused connector cannot change status: the change would
/// not reach the system and the next pull after resuming would undo it.
fn refuse_if_paused(state: &State<'_, AppState>, id: &ItemId) -> CommandResult<()> {
    let origin = lock(&state.rail)
        .service
        .repository()
        .get(id)
        .ok()
        .flatten()
        .and_then(|item| item.origin);
    if let Some(origin) = origin {
        let registry = lock(&state.connectors);
        if registry.is_paused(origin.connector.as_str()) {
            return Err(format!(
                "{} is paused. Resume it to change its tasks.",
                registry.name_of(origin.connector.as_str())
            ));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn start_task(app: AppHandle, state: State<'_, AppState>, id: ItemId) -> CommandResult<Item> {
    refuse_if_paused(&state, &id)?;
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
    refuse_if_paused(&state, &id)?;
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
    refuse_if_paused(&state, &id)?;
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
    if before != Some(request.status) {
        drop(rail);
        refuse_if_paused(&state, &request.id)?;
        rail = lock(&state.rail);
    }
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
        before.edge != rail.settings.edge
            || before.width_px != rail.settings.width_px
            || before.monitor != rail.settings.monitor
    };
    if needs_redock {
        crate::dock::redock(&app).map_err(|e| e.to_string())?;
    }
    crate::theme::apply(&app);
    crate::tray::reflect_sounds(lock(&state.rail).settings.sounds);
    changed(&app);
    Ok(lock(&state.rail).settings.clone())
}

/// Sounds on or off, from the tray. The settings page shows the same switch.
pub fn set_sounds(app: AppHandle, sounds: bool) -> CommandResult<()> {
    {
        let state = app.state::<AppState>();
        let mut rail = lock(&state.rail);
        let mut settings = rail.settings.clone();
        settings.sounds = sounds;
        rail.save_settings(settings)?;
    }
    log::info!("sounds {}", if sounds { "on" } else { "off" });
    crate::tray::reflect_sounds(sounds);
    changed(&app);
    Ok(())
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
        // Real work is arriving, so the welcome cards have done their job.
        let retired = crate::seed::retire(&mut rail.service).map_err(|e| e.to_string())?;
        if retired > 0 {
            log::info!("took {retired} welcome cards off the rail now that a connector is set up");
        }
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

/// What a connector's server really returns, trimmed, for anyone writing
/// or fixing a profile.
#[tauri::command]
pub async fn connector_sample(state: State<'_, AppState>, id: String) -> CommandResult<String> {
    let connector = lock(&state.connectors)
        .live(&id)
        .ok_or_else(|| format!("no connector {id}"))?;
    connector.raw_sample(6_000).await.map_err(|e| e.to_string())
}

/// What a person can decide about one connector: the profile's choices,
/// their current answers, and what each pick can be chosen from.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectorScope {
    pub choices: Vec<taskrail_connector_mcp::Choice>,
    pub scope: taskrail_connector_mcp::Scope,
    pub options: std::collections::BTreeMap<String, Vec<String>>,
}

#[tauri::command]
pub async fn get_connector_scope(
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<ConnectorScope> {
    let (choices, scope, connector) = {
        let mut registry = lock(&state.connectors);
        let profile = registry
            .profile(&id)
            .ok_or_else(|| format!("no connector {id}"))?;
        let choices = profile.choices.clone();
        let scope = registry.scope(&id);
        let connector = registry
            .live(&id)
            .ok_or_else(|| format!("no connector {id}"))?;
        (choices, scope, connector)
    };
    let options = if choices.is_empty() {
        Default::default()
    } else {
        connector.scope_options().await.map_err(|e| e.to_string())?
    };
    Ok(ConnectorScope {
        choices,
        scope,
        options,
    })
}

/// Save a person's answers for a connector and pull with them straight away.
#[tauri::command]
pub async fn set_connector_scope(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    scope: taskrail_connector_mcp::Scope,
) -> CommandResult<()> {
    {
        let mut registry = lock(&state.connectors);
        registry.set_scope(&id, scope);
        let mut rail = lock(&state.rail);
        registry.save(rail.service.repository_mut())?;
    }
    log::info!("scope for {id} changed; pulling");
    crate::sync::pull(&app, &id).await?;
    changed(&app);
    Ok(())
}

/// Pause or resume a connector. Resuming pulls straight away.
#[tauri::command]
pub async fn set_connector_paused(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    paused: bool,
) -> CommandResult<ConnectorSummary> {
    {
        let mut registry = lock(&state.connectors);
        if registry.set_paused(&id, paused) {
            let mut rail = lock(&state.rail);
            registry.save(rail.service.repository_mut())?;
            log::info!(
                "{} {}",
                registry.name_of(&id),
                if paused { "paused" } else { "resumed" }
            );
        }
    }
    if !paused {
        let _ = crate::sync::pull(&app, &id).await;
    }
    changed(&app);
    let registry = lock(&state.connectors);
    registry
        .summaries()
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("no connector {id}"))
}

/// Add a calendar by its iCalendar address. The feed is read once first, so
/// a wrong address fails here rather than quietly later; the address then
/// goes into the credential store and the calendar onto the rail.
#[tauri::command]
pub async fn add_calendar(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    url: String,
) -> CommandResult<ConnectorSummary> {
    use taskrail_core::ports::SecretStore;
    let url = url.trim().to_owned();
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("webcal://"))
    {
        return Err("a calendar address starts with https:// or webcal://".into());
    }
    let name = {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            "Calendar".to_owned()
        } else {
            trimmed.to_owned()
        }
    };
    let id = {
        let registry = lock(&state.connectors);
        taskrail_connector_mcp::discovery::unique_id(
            &taskrail_connector_mcp::discovery::slug(&name),
            &registry.ids(),
        )
    };
    let probe = taskrail_connector_ics::IcsCalendar::new(id.clone(), name.clone(), url.clone());
    let found = probe.list().await.map_err(|e| match e {
        taskrail_connector_api::ConnectorError::Protocol(m) => {
            format!("that address did not return a calendar: {m}")
        }
        other => format!("could not read the calendar: {other}"),
    })?;
    log::info!(
        "calendar {name:?} read: {} events in the next {} days",
        found.len(),
        taskrail_connector_ics::DAYS_AHEAD
    );
    let secret = crate::connectors::calendar_secret_name(&id);
    crate::secrets::OsSecrets
        .set(&secret, &url)
        .map_err(|e| e.to_string())?;
    {
        let mut registry = lock(&state.connectors);
        registry.add_feed(crate::connectors::CalendarFeed {
            id: id.clone(),
            name,
            url_secret: secret,
            poll_seconds: 300,
        })?;
        let mut rail = lock(&state.rail);
        registry.save(rail.service.repository_mut())?;
        let retired = crate::seed::retire(&mut rail.service).map_err(|e| e.to_string())?;
        if retired > 0 {
            log::info!("took {retired} welcome cards off the rail now that a connector is set up");
        }
    }
    crate::sync::pull(&app, &id).await?;
    changed(&app);
    lock(&state.connectors)
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
    SignInRequired,
}

#[derive(Debug, Deserialize)]
pub struct DiscoverRequest {
    /// Empty: worked out from the name, kept clear of existing connectors.
    #[serde(default)]
    pub id: String,
    /// Empty: the blueprint's title, else what the server calls itself.
    #[serde(default)]
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
    /// A blueprint to use instead of the registry: one someone sent, or one
    /// being tested before it is merged.
    #[serde(default)]
    pub blueprint: Option<String>,
}

fn task_kind() -> taskrail_core::domain::ItemKind {
    taskrail_core::domain::ItemKind::Task
}

#[tauri::command]
pub async fn discover_connector(
    app: AppHandle,
    request: DiscoverRequest,
) -> CommandResult<DiscoveryResult> {
    use taskrail_core::ports::SecretStore;
    let llm = crate::model::client_if_configured(&app)?;
    let own_blueprint = match request.blueprint.as_deref().map(str::trim) {
        Some(json) if !json.is_empty() => Some(
            serde_json::from_str::<taskrail_connector_mcp::Blueprint>(json)
                .map_err(|e| format!("that is not a blueprint: {e}"))?,
        ),
        _ => None,
    };
    let mut server = request.server;
    let mut auth = taskrail_connector_mcp::SessionAuth {
        bearer: request.bearer.filter(|t| !t.trim().is_empty()),
        oauth: None,
    };
    // A completed sign-in for this server is picked up here and recorded in
    // the spec, so the saved profile knows where its credentials live. Older
    // connectors kept theirs under their id; those are still honoured.
    if let taskrail_connector_mcp::ServerSpec::Http {
        url, oauth_secret, ..
    } = &mut server
    {
        let mut candidates = vec![crate::connectors::oauth_secret_name_for_url(url)];
        if !request.id.trim().is_empty() {
            candidates.push(crate::connectors::oauth_secret_name(&request.id));
        }
        if let Some(name) = candidates
            .into_iter()
            .find(|name| crate::secrets::OsSecrets.get(name).ok().flatten().is_some())
        {
            *oauth_secret = Some(name.clone());
            auth.oauth = Some(std::sync::Arc::new(
                crate::secrets::KeychainCredentials::new(name),
            ));
        }
    }
    let taken_ids = lock(&app.state::<AppState>().connectors).ids();
    let request = taskrail_connector_mcp::DiscoveryRequest {
        id: request.id,
        name: request.name,
        server,
        kind: request.kind,
        hint: request.hint,
        allow_unannotated_list: request.allow_unannotated_list,
        auth,
        taken_ids,
    };
    // A pasted blueprint is the whole registry for this attempt. Otherwise
    // the live registry, or the one built in. Either way a blueprint that
    // fits is tried before any model.
    let registry = match own_blueprint {
        Some(blueprint) => {
            log::info!(
                "discovery for {:?}: using the pasted {:?} blueprint",
                request.name,
                blueprint.title
            );
            taskrail_connector_mcp::Registry {
                version: 1,
                blueprints: vec![blueprint],
            }
        }
        None => {
            taskrail_connector_mcp::Registry::load(
                taskrail_connector_mcp::blueprint::REGISTRY_URL,
                std::time::Duration::from_secs(5),
            )
            .await
        }
    };
    match taskrail_connector_mcp::discover_or_blueprint(&request, &registry, llm.as_deref()).await {
        Ok(outcome) => {
            log::info!(
                "discovery for {:?} done in {} rounds with {} sample items{}",
                outcome.profile.name,
                outcome.rounds,
                outcome.sample.len(),
                outcome
                    .blueprint
                    .as_ref()
                    .map(|b| format!(" through the {b:?} blueprint"))
                    .unwrap_or_default()
            );
            Ok(DiscoveryResult::Ok(outcome))
        }
        Err(taskrail_connector_mcp::DiscoveryError::NeedsConfirmation { tool, profile }) => {
            Ok(DiscoveryResult::NeedsConfirmation {
                tool,
                profile: *profile,
            })
        }
        Err(taskrail_connector_mcp::DiscoveryError::SignInRequired(_)) => {
            Ok(DiscoveryResult::SignInRequired)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// A saved connector's profile as a blueprint others could use: no server,
/// no secrets, the tools it calls as the fingerprint. Pretty JSON, ready to
/// paste into the registry or an issue.
#[tauri::command]
pub fn export_blueprint(state: State<'_, AppState>, id: String) -> CommandResult<String> {
    let registry = lock(&state.connectors);
    let profile = registry
        .profiles()
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("no connector {id}"))?;
    let blueprint = taskrail_connector_mcp::Blueprint::from_profile(
        profile,
        &profile.name,
        "What this system is and what the profile pulls.",
    );
    serde_json::to_string_pretty(&blueprint).map_err(|e| e.to_string())
}

/// Open one of this project's GitHub pages in the browser, such as the
/// prefilled blueprint submission form. Only our own pages: the UI never
/// gets to open an arbitrary address.
#[tauri::command]
pub fn open_project_page(url: String) -> CommandResult<()> {
    const OURS: &str = "https://github.com/markcastle/taskrail/";
    if !url.starts_with(OURS) {
        return Err(format!("only pages under {OURS} can be opened"));
    }
    open::that(&url).map_err(|e| e.to_string())
}

/// The MCP sign-in flow for one server: open the browser, wait for it to
/// come back to a local port, keep the credentials in the credential store.
/// Blocks until the person finishes or five minutes pass.
#[tauri::command]
pub async fn sign_in(url: String) -> CommandResult<()> {
    let store = std::sync::Arc::new(crate::secrets::KeychainCredentials::new(
        crate::connectors::oauth_secret_name_for_url(&url),
    ));
    let id = crate::connectors::oauth_secret_name_for_url(&url);
    log::info!("sign-in for {id} starting at {url}");
    let start = taskrail_connector_mcp::auth::begin(&url, store)
        .await
        .map_err(|e| format!("could not start the sign-in: {e}"))?;
    log::info!("sign-in for {id}: opening the browser");
    if let Err(e) = open::that(&start.auth_url) {
        return Err(format!(
            "could not open your browser ({e}); visit this address yourself: {}",
            start.auth_url
        ));
    }
    start
        .finish(std::time::Duration::from_secs(300))
        .await
        .map_err(|e| format!("sign-in failed: {e}"))?;
    log::info!("sign-in for {id} complete");
    Ok(())
}

/// Settings live inside the rail. Opening them widens the strip; closing
/// them gives the width back. The tray uses the event to ask the page to
/// open them.
pub const OPEN_SETTINGS_EVENT: &str = "rail:open-settings";

#[tauri::command]
pub fn open_settings(app: AppHandle) -> CommandResult<()> {
    crate::dock::show(&app);
    app.emit(OPEN_SETTINGS_EVENT, ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_monitors(app: AppHandle) -> Vec<crate::dock::MonitorInfo> {
    crate::dock::monitors(&app)
}

#[tauri::command]
pub fn expand_rail(
    app: AppHandle,
    state: State<'_, AppState>,
    expanded: bool,
) -> CommandResult<()> {
    {
        let mut rail = lock(&state.rail);
        if rail.expanded == expanded {
            return Ok(());
        }
        rail.expanded = expanded;
    }
    log::info!(
        "rail {}",
        if expanded {
            "widened for settings"
        } else {
            "back to normal width"
        }
    );
    crate::dock::redock(&app).map_err(|e| e.to_string())
}

/// Ask the release feed now, for the settings page's "Check for updates".
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> CommandResult<crate::update::UpdateStatus> {
    let available = crate::update::check(&app).await?;
    Ok(crate::update::UpdateStatus {
        current: env!("CARGO_PKG_VERSION").to_owned(),
        available,
    })
}

/// Install the newest release and restart into it.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> CommandResult<()> {
    crate::update::install(&app).await
}

#[tauri::command]
pub fn app_version() -> String {
    format!(
        "{} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("TASKRAIL_COMMIT")
    )
}

#[tauri::command]
pub fn read_log(app: AppHandle, lines: Option<usize>) -> String {
    crate::logs::tail(&app, lines.unwrap_or(400))
}

#[tauri::command]
pub fn log_from_ui(window: tauri::Window, level: crate::logs::UiLevel, message: String) {
    crate::logs::from_ui(window.label(), level, &message);
}

#[tauri::command]
pub fn open_log_folder(app: AppHandle) -> CommandResult<()> {
    let path = crate::logs::file(&app).ok_or("no log directory on this platform")?;
    let dir = path.parent().ok_or("no log directory")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    open::that(dir).map_err(|e| e.to_string())
}

/// Quit from the UI. The exit handler in `lib.rs` releases the strip.
#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

fn changed(app: &AppHandle) {
    if let Err(e) = app.emit(CHANGED_EVENT, ()) {
        log::warn!("could not emit {CHANGED_EVENT}: {e}");
    }
}
