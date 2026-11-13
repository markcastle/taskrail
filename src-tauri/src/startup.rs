//! Opening when the person signs in to their computer.
//!
//! The setting is the truth and the operating system is made to agree with
//! it: at every start, and whenever the switch in Settings is moved. On
//! Windows that is an entry under the user's own Run key, which needs no
//! administrator; on macOS a launch agent in the user's Library. Both are
//! per user and both are removed by turning the switch off.

use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use crate::state::{lock, AppState};

pub fn plugin() -> tauri::plugin::TauriPlugin<Wry> {
    tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None)
}

/// Make the operating system agree with the setting. An error is for the
/// person to see: the setting says one thing and the system could not be
/// made to match.
pub fn apply(app: &AppHandle) -> Result<(), String> {
    // A build run from source must not put itself in the way of the
    // installed app, or leave an entry pointing into a target directory.
    if cfg!(debug_assertions) {
        return Ok(());
    }
    let wanted = lock(&app.state::<AppState>().rail).settings.start_at_login;
    let launcher = app.autolaunch();
    let registered = launcher
        .is_enabled()
        .map_err(|e| format!("could not tell whether Task Rail starts at login: {e}"))?;
    if wanted == registered {
        return Ok(());
    }
    if wanted {
        launcher
            .enable()
            .map_err(|e| format!("could not set Task Rail to start at login: {e}"))?;
        log::info!("set to open at login");
    } else {
        launcher
            .disable()
            .map_err(|e| format!("could not stop Task Rail starting at login: {e}"))?;
        log::info!("no longer opens at login");
    }
    Ok(())
}
