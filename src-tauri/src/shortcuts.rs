//! The global quick-add shortcut. Pressing it anywhere on the desktop brings
//! the rail forward and tells the UI to open the capture box.

use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub const QUICK_ADD_EVENT: &str = "rail:quick-add";

fn quick_add_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space)
}

pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state == ShortcutState::Pressed && *shortcut == quick_add_shortcut() {
                if let Some(window) = app.get_webview_window(crate::dock::RAIL_WINDOW) {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                let _ = app.emit(QUICK_ADD_EVENT, ());
            }
        })
        .build()
}

/// Best effort: another app may already own the combination, and a rail
/// without a hotkey is still a rail.
pub fn register(app: &AppHandle) {
    if let Err(e) = app.global_shortcut().register(quick_add_shortcut()) {
        eprintln!("task rail: could not register Ctrl+Shift+Space: {e}");
    }
}
