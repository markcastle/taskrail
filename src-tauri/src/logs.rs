//! Where the app writes down what it is doing, and how the UI reads it.
//!
//! One log file in the per-user log directory, rotated by the plugin, with
//! the same lines on stderr for the dev loop. The settings window has a
//! Logs page that tails the file, and the tray can open the folder.

use std::path::PathBuf;

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_log::{Target, TargetKind};

pub const FILE_STEM: &str = "taskrail";

pub fn plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_log::Builder::new()
        .targets([
            Target::new(TargetKind::Stdout),
            Target::new(TargetKind::LogDir {
                file_name: Some(FILE_STEM.into()),
            }),
        ])
        .level(log::LevelFilter::Info)
        .build()
}

pub fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_log_dir()
        .ok()
        .map(|dir| dir.join(format!("{FILE_STEM}.log")))
}

/// The last `lines` lines of the log, newest last.
pub fn tail(app: &AppHandle, lines: usize) -> String {
    let Some(path) = file(app) else {
        return "no log directory on this platform".into();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let all: Vec<&str> = text.lines().collect();
            let start = all.len().saturating_sub(lines);
            all[start..].join("\n")
        }
        Err(e) => format!("no log yet at {}: {e}", path.display()),
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiLevel {
    Info,
    Warn,
    Error,
}

/// A line from the web side, so UI failures land next to shell ones.
pub fn from_ui(window: &str, level: UiLevel, message: &str) {
    match level {
        UiLevel::Info => log::info!(target: "ui", "[{window}] {message}"),
        UiLevel::Warn => log::warn!(target: "ui", "[{window}] {message}"),
        UiLevel::Error => log::error!(target: "ui", "[{window}] {message}"),
    }
}
