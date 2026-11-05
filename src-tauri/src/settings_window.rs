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
            log::warn!("could not open settings: {e}");
        }
    })
}

/// Close the settings window if it is open. Also on the main thread, for
/// the same reason as `open`.
pub fn close(app: &AppHandle) -> tauri::Result<()> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(SETTINGS_WINDOW) {
            if let Err(e) = window.close() {
                log::warn!("could not close settings: {e}");
            }
        }
    })
}

fn open_on_main_thread(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        log::info!("settings window already open; focusing it");
        window.show()?;
        window.unminimize()?;
        window.set_focus()?;
        return Ok(());
    }
    log::info!("opening the settings window");
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
