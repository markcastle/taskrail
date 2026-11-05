//! The tray icon: the one place the rail can always be reached from, even
//! when it is hidden, and the proper way to quit an app with no title bar.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const SHOW: &str = "show";
const SETTINGS: &str = "settings";
const LOGS: &str = "logs";
const QUIT: &str = "quit";

pub fn install(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let show = MenuItem::with_id(app, SHOW, "Show Task Rail", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, SETTINGS, "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit Task Rail", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Task Rail")
        .on_menu_event(|app, event| match event.id().as_ref() {
            SHOW => show_rail(app),
            SETTINGS => {
                if let Err(e) = crate::commands::open_settings(app.clone()) {
                    log::warn!("could not open settings: {e}");
                }
            }
            LOGS => {
                if let Err(e) = crate::commands::open_log_folder(app.clone()) {
                    log::warn!("could not open the logs folder: {e}");
                }
            }
            QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Only a completed left click. A right click also arrives here,
            // and focusing the rail on it would close the menu it just opened.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_rail(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)
}

fn show_rail(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(crate::dock::RAIL_WINDOW) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
