//! Pulling from connectors on a schedule, and pushing status changes back.

use std::sync::Arc;

use serde::Serialize;
use taskrail_connector_api::Connector;
use taskrail_core::domain::{Origin, Status};
use taskrail_sync::apply_remote;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{lock, AppState};

pub const SYNC_ERROR_EVENT: &str = "rail:sync-error";
/// A pull finished, well or badly. The rail re-reads its connectors' status.
pub const SYNCED_EVENT: &str = "rail:synced";
const TICK_SECONDS: u64 = 10;

/// The longest one pull may take, end to end. Each call a connector makes
/// has its own, shorter limit; this is the backstop for anything those miss,
/// because a pull that never returns stops every pull after it.
const PULL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Debug, Clone, Serialize)]
pub struct SyncError {
    pub connector: String,
    pub message: String,
}

/// A tick that took this much longer than it should means the computer was
/// asleep, or as good as: nothing ran, and every open connection is stale.
const AWAY_SECONDS: i64 = 60;

/// Wakes every few seconds and pulls from whichever connectors are due.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let due = lock(&app.state::<AppState>().connectors).due_for_sync(chrono::Utc::now());
            for id in due {
                // Each pull runs as its own task, so that one going badly
                // wrong takes down that pull and not this loop. A loop that
                // dies does so silently, and nothing syncs again until the
                // app is restarted.
                let task = tauri::async_runtime::spawn({
                    let app = app.clone();
                    let id = id.clone();
                    // Failures are already recorded and reported inside `pull`.
                    async move { pull(&app, &id).await }
                });
                if let Err(e) = task.await {
                    let message = format!("the sync stopped unexpectedly: {e}");
                    lock(&app.state::<AppState>().connectors).record(
                        &id,
                        Err(message.clone()),
                        chrono::Utc::now(),
                    );
                    report(&app, &id, &message);
                }
            }
            let asleep_from = chrono::Utc::now();
            tokio::time::sleep(std::time::Duration::from_secs(TICK_SECONDS)).await;
            let away = (chrono::Utc::now() - asleep_from).num_seconds();
            if away >= AWAY_SECONDS {
                log::info!("back after {away} seconds away: reconnecting and syncing now");
                let open = lock(&app.state::<AppState>().connectors).woke();
                for connector in open {
                    connector.reset().await;
                }
            }
        }
    });
}

/// One pull: list without holding any lock, then merge under the rail lock.
pub async fn pull(app: &AppHandle, id: &str) -> Result<usize, String> {
    let state = app.state::<AppState>();
    let Some(connector) = lock(&state.connectors).live_any(id) else {
        return Err(format!("no connector {id}"));
    };
    let outcome = match tokio::time::timeout(PULL_TIMEOUT, pull_with(app, connector)).await {
        Ok(outcome) => outcome,
        Err(_) => Err(format!(
            "gave up after {} minutes without an answer",
            PULL_TIMEOUT.as_secs() / 60
        )),
    };
    let before = lock(&state.connectors).last_error(id);
    lock(&state.connectors).record(id, outcome.clone(), chrono::Utc::now());
    // The footer's "Synced a minute ago" is this, so say so every time and
    // not only when a task changed.
    let _ = app.emit(SYNCED_EVENT, id);
    match &outcome {
        // Said once when it starts, not on every retry while it lasts.
        Err(message) if before.as_deref() != Some(message.as_str()) => report(app, id, message),
        Err(message) => log::warn!("sync error on {id}, still: {message}"),
        Ok(_) if before.is_some() => {
            log::info!("{id} is syncing again");
            let _ = app.emit(crate::commands::CHANGED_EVENT, ());
        }
        Ok(_) => {}
    }
    outcome
}

async fn pull_with(app: &AppHandle, connector: Arc<dyn Connector>) -> Result<usize, String> {
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
    log::info!(
        "pulled {count} items from {}: {} added, {} updated, {} removed, {} unchanged",
        connector.info().id.as_str(),
        report.added,
        report.updated,
        report.removed,
        report.unchanged
    );
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
        let Some(connector) = lock(&state.connectors).live_any(&id) else {
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
    log::warn!("sync error on {connector}: {message}");
    let _ = app.emit(
        SYNC_ERROR_EVENT,
        SyncError {
            connector: connector.to_owned(),
            message: message.to_owned(),
        },
    );
}
