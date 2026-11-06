//! Keeping the app current. A check shortly after start and every hour;
//! a new release is either installed straight away, if the person asked for
//! that, or announced on the rail with an Install button. Every build is
//! signed in CI and the plugin refuses anything the public key does not match,
//! so the app only ever installs what we made.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::state::{lock, AppState};

pub const UPDATE_AVAILABLE_EVENT: &str = "rail:update-available";

const FIRST_CHECK_SECONDS: u64 = 20;
const CHECK_EVERY_SECONDS: u64 = 60 * 60;

/// What the UI needs to know about a release that is newer than this one.
#[derive(Debug, Clone, Serialize)]
pub struct AvailableUpdate {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

/// The answer to "is there an update?", for the settings page.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub current: String,
    pub available: Option<AvailableUpdate>,
}

/// Check once soon after start, then every hour, for as long as the app runs.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(FIRST_CHECK_SECONDS)).await;
        loop {
            match check(&app).await {
                Ok(Some(update)) => {
                    let auto = lock(&app.state::<AppState>().rail).settings.auto_update;
                    if auto {
                        log::info!(
                            "update {} available, installing automatically",
                            update.version
                        );
                        if let Err(e) = install(&app).await {
                            log::warn!("automatic update failed: {e}");
                        }
                    } else {
                        log::info!("update {} available, telling the rail", update.version);
                        let _ = app.emit(UPDATE_AVAILABLE_EVENT, &update);
                    }
                }
                Ok(None) => log::info!("up to date"),
                Err(e) => log::warn!("update check failed: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(CHECK_EVERY_SECONDS)).await;
        }
    });
}

/// Ask the release feed whether something newer than this build exists.
pub async fn check(app: &AppHandle) -> Result<Option<AvailableUpdate>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let found = updater.check().await.map_err(|e| e.to_string())?;
    Ok(found.map(|u| AvailableUpdate {
        version: u.version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    }))
}

/// Download, verify and install the newest release, then restart into it.
/// The reserved strip is given back first: on Windows the installer ends
/// this process without going through the normal exit path.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Err("already up to date".into());
    };
    log::info!("installing update {}", update.version);
    crate::dock::release(app);
    update
        .download_and_install(|_, _| {}, || log::info!("update downloaded, installing"))
        .await
        .map_err(|e| e.to_string())?;
    log::info!("update installed, restarting");
    app.restart();
}
