//! Pulling from connectors on a schedule, and pushing status changes back.

use std::sync::Arc;

use serde::Serialize;
use taskrail_connector_api::Connector;
use taskrail_connector_mcp::McpConnector;
use taskrail_core::domain::{Origin, Status};
use taskrail_sync::apply_remote;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{lock, AppState};

pub const SYNC_ERROR_EVENT: &str = "rail:sync-error";
const TICK_SECONDS: u64 = 10;

#[derive(Debug, Clone, Serialize)]
pub struct SyncError {
    pub connector: String,
    pub message: String,
}

/// Wakes every few seconds and pulls from whichever connectors are due.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let due = lock(&app.state::<AppState>().connectors).due_for_sync(chrono::Utc::now());
            for id in due {
                // Failures are already recorded and reported inside `pull`.
                let _ = pull(&app, &id).await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(TICK_SECONDS)).await;
        }
    });
}

/// One pull: list without holding any lock, then merge under the rail lock.
pub async fn pull(app: &AppHandle, id: &str) -> Result<usize, String> {
    let state = app.state::<AppState>();
    let Some(connector) = lock(&state.connectors).live(id) else {
        return Err(format!("no connector {id}"));
    };
    let outcome = pull_with(app, connector).await;
    lock(&state.connectors).record(id, outcome.clone(), chrono::Utc::now());
    if let Err(message) = &outcome {
        report(app, id, message);
    }
    outcome
}

async fn pull_with(app: &AppHandle, connector: Arc<McpConnector>) -> Result<usize, String> {
    let remote = connector.list().await.map_err(|e| e.to_string())?;
    let count = remote.len();
    let state = app.state::<AppState>();
    let report = {
        let mut rail = lock(&state.rail);
        let repo = rail.service.repository_mut();
        apply_remote(
            repo,
            &connector.info().id,
            &remote,
            connector.status_map(),
            chrono::Utc::now(),
        )
        .map_err(|e| e.to_string())?
    };
    if report.added + report.updated + report.removed > 0 {
        let _ = app.emit(crate::commands::CHANGED_EVENT, ());
    }
    Ok(count)
}

/// Tell the remote system about a status change made in the rail. Runs in
/// the background; if it fails, the next pull restores the remote truth and
/// the UI is told what happened.
pub fn write_back(app: &AppHandle, origin: Option<Origin>, status: Status) {
    let Some(origin) = origin else { return };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let id = origin.connector.as_str().to_owned();
        let state = app.state::<AppState>();
        let Some(connector) = lock(&state.connectors).live(&id) else {
            return;
        };
        if !connector.info().capabilities.write_status {
            return;
        }
        if let Err(e) = connector.set_status(&origin.external_id, status).await {
            report(
                &app,
                &id,
                &format!("could not update {}: {e}", origin.external_id),
            );
            let _ = pull(&app, &id).await;
        }
    });
}

fn report(app: &AppHandle, connector: &str, message: &str) {
    eprintln!("task rail: sync error on {connector}: {message}");
    let _ = app.emit(
        SYNC_ERROR_EVENT,
        SyncError {
            connector: connector.to_owned(),
            message: message.to_owned(),
        },
    );
}
