//! The settings window: a normal, decorated window of a sensible size. The
//! rail is a strip and stays one; anything that needs room lives here.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_WINDOW: &str = "settings";

pub fn open(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        window.show()?;
        window.unminimize()?;
        window.set_focus()?;
        return Ok(());
    }
    WebviewWindowBuilder::new(
        app,
        SETTINGS_WINDOW,
        WebviewUrl::App("index.html#settings".into()),
    )
    .title("Task Rail Settings")
    .inner_size(780.0, 640.0)
    .min_inner_size(640.0, 480.0)
    .center()
    .build()?;
    Ok(())
}
