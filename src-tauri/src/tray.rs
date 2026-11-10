//! The tray icon: the one place the rail can always be reached from, even
//! when it is hidden, and the proper way to quit an app with no title bar.

use std::sync::{Mutex, OnceLock};

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::state::{lock, AppState};

const SHOW: &str = "show";
const HIDE: &str = "hide";
const SETTINGS: &str = "settings";
const SILENCE: &str = "silence";
const LOGS: &str = "logs";
const QUIT: &str = "quit";

/// The silence item, kept so its tick can follow the setting when it is
/// changed from the settings page instead.
static SILENCE_ITEM: OnceLock<Mutex<Option<CheckMenuItem<Wry>>>> = OnceLock::new();

pub fn install(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let sounds_on = lock(&app.state::<AppState>().rail).settings.sounds;
    let show = MenuItem::with_id(app, SHOW, "Show Task Rail", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, HIDE, "Hide Task Rail", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, SETTINGS, "Settings…", true, None::<&str>)?;
    let silence = CheckMenuItem::with_id(
        app,
        SILENCE,
        "Silence Task Rail",
        true,
        !sounds_on,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, QUIT, "Quit Task Rail", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &hide,
            &settings,
            &silence,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let _ = SILENCE_ITEM.get_or_init(|| Mutex::new(None));
    if let Some(slot) = SILENCE_ITEM.get() {
        *lock(slot) = Some(silence.clone());
    }

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Task Rail")
        .on_menu_event(|app, event| match event.id().as_ref() {
            SHOW => crate::dock::show(app),
            HIDE => crate::dock::hide(app),
            SILENCE => {
                // The tick has already flipped; the setting follows it.
                let silenced = SILENCE_ITEM
                    .get()
                    .and_then(|slot| lock(slot).as_ref().and_then(|item| item.is_checked().ok()))
                    .unwrap_or(true);
                if let Err(e) = crate::commands::set_sounds(app.clone(), !silenced) {
                    log::warn!("could not change the sound setting: {e}");
                }
            }
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
                crate::dock::show(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)
}

/// Keep the tray's tick in step with the setting, however it was changed.
pub fn reflect_sounds(sounds: bool) {
    if let Some(slot) = SILENCE_ITEM.get() {
        if let Some(item) = lock(slot).as_ref() {
            let _ = item.set_checked(!sounds);
        }
    }
}
