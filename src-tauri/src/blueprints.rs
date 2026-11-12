//! Keeping connectors in step with their blueprints.
//!
//! A blueprint gets better after people have connected with it: it learns
//! to say where a task sits, it gains choices, a filter is fixed. A
//! connector made from it used to keep the edition it was made with for
//! ever, so the improvement reached only people who connected afterwards.
//! Now a connector follows its blueprint. The registry is read shortly after
//! start and every hour, and a connector whose blueprint has moved on is
//! moved on with it.
//!
//! Nothing is replaced on trust. The newer profile is first run against the
//! person's own server, with their sign-in and their choices; only if it
//! answers is it saved. One that does not is logged and the connector is
//! left exactly as it was.

use std::time::Duration;

use taskrail_connector_api::Connector;
use taskrail_connector_mcp::blueprint::{Registry, REGISTRY_URL};
use taskrail_connector_mcp::{ConnectorProfile, McpConnector};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{lock, AppState};

/// Soon after start, once the first pull has had the server to itself.
const FIRST_CHECK: Duration = Duration::from_secs(25);
const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);
const REGISTRY_TIMEOUT: Duration = Duration::from_secs(10);
const TRIAL_TIMEOUT: Duration = Duration::from_secs(120);

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            check(&app).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

async fn check(app: &AppHandle) {
    let state = app.state::<AppState>();
    // A paused connector is left alone in every way, this one included.
    let profiles: Vec<ConnectorProfile> = {
        let connectors = lock(&state.connectors);
        connectors
            .profiles()
            .iter()
            .filter(|p| !connectors.is_paused(&p.id))
            .cloned()
            .collect()
    };
    if profiles.is_empty() {
        return;
    }
    let registry = Registry::load(REGISTRY_URL, REGISTRY_TIMEOUT).await;
    for profile in profiles {
        if let Some(fresh) = registry.refreshed(&profile) {
            bring_up_to_date(app, &profile.id, fresh).await;
        }
    }
}

async fn bring_up_to_date(app: &AppHandle, id: &str, fresh: ConnectorProfile) {
    let state = app.state::<AppState>();
    let blueprint = fresh.blueprint.clone().unwrap_or_default();
    let scope = lock(&state.connectors).scope(id);
    let auth = crate::connectors::session_auth(&fresh.server);
    let trial = McpConnector::with_scope(fresh.clone(), auth, scope);
    let found = match tokio::time::timeout(TRIAL_TIMEOUT, trial.list()).await {
        Ok(Ok(items)) => items.len(),
        Ok(Err(e)) => {
            log::warn!(
                "{id}: the newer {blueprint:?} blueprint did not work against this server, so the connector stays as it was: {e}"
            );
            return;
        }
        Err(_) => {
            log::warn!(
                "{id}: the newer {blueprint:?} blueprint got no answer in time, so the connector stays as it was"
            );
            return;
        }
    };
    {
        let mut connectors = lock(&state.connectors);
        // Removed while the trial ran: nothing to bring up to date.
        if !connectors.replace(fresh) {
            return;
        }
        let mut rail = lock(&state.rail);
        if let Err(e) = connectors.save(rail.service.repository_mut()) {
            log::warn!("{id}: could not save the updated connector: {e}");
        }
    }
    log::info!(
        "{id} brought up to date with the {blueprint:?} blueprint, which finds {found} items here"
    );
    // Pull with it straight away, so what the blueprint added is on the rail.
    let _ = crate::sync::pull(app, id).await;
    let _ = app.emit(crate::commands::CHANGED_EVENT, ());
}
