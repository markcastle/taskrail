//! The global quick-add shortcut. Pressing it anywhere on the desktop brings
//! the rail forward and tells the UI to open the capture box.

use tauri::{AppHandle, Emitter, Wry};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub const QUICK_ADD_EVENT: &str = "rail:quick-add";

fn quick_add_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space)
}

pub fn plugin() -> tauri::plugin::TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state == ShortcutState::Pressed && *shortcut == quick_add_shortcut() {
                // A hidden rail comes back docked, then takes the new task.
                crate::dock::show(app);
                let _ = app.emit(QUICK_ADD_EVENT, ());
            }
        })
        .build()
}

/// Best effort: another app may already own the combination, and a rail
/// without a hotkey is still a rail.
pub fn register(app: &AppHandle) {
    if let Err(e) = app.global_shortcut().register(quick_add_shortcut()) {
        log::warn!("could not register Ctrl+Shift+Space: {e}");
    }
}
