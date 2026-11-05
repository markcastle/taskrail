//! The settings window: a normal, decorated window of a sensible size. The
//! rail is a strip and stays one; anything that needs room lives here.
//!
//! Windows are created on the main thread only. The gear button reaches
//! here from a command thread and the tray from the main thread; both post
//! the same closure to the event loop, so two requests can never race to
//! build the same window.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_WINDOW: &str = "settings";

pub fn open(app: &AppHandle) -> tauri::Result<()> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Err(e) = open_on_main_thread(&handle) {
            eprintln!("task rail: could not open settings: {e}");
        }
    })
}

fn open_on_main_thread(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        window.show()?;
        window.unminimize()?;
        window.set_focus()?;
        return Ok(());
    }
    // The page tells which window it is from its label, so the URL carries
    // nothing special. A fragment here would be percent-encoded as a path.
    WebviewWindowBuilder::new(app, SETTINGS_WINDOW, WebviewUrl::App("index.html".into()))
        .title("Task Rail Settings")
        .inner_size(780.0, 640.0)
        .min_inner_size(640.0, 480.0)
        .center()
        .theme(crate::theme::window_theme(app))
        .build()?;
    Ok(())
}
