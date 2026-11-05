//! Light, dark or system, applied to every window's native chrome. The
//! page applies the same choice to its own colours.

use taskrail_core::settings::Theme as RailTheme;
use tauri::{AppHandle, Manager, Theme};

use crate::state::{lock, AppState};

pub fn window_theme(app: &AppHandle) -> Option<Theme> {
    let theme = lock(&app.state::<AppState>().rail).settings.theme;
    match theme {
        RailTheme::System => None,
        RailTheme::Light => Some(Theme::Light),
        RailTheme::Dark => Some(Theme::Dark),
    }
}

pub fn apply(app: &AppHandle) {
    let theme = window_theme(app);
    for window in app.webview_windows().values() {
        if let Err(e) = window.set_theme(theme) {
            eprintln!("task rail: could not set the window theme: {e}");
        }
    }
}
