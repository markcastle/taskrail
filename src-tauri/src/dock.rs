//! Glue between the Tauri window and the platform dock.

use taskrail_core::settings::Edge;
use taskrail_platform::{DockError, DockRequest, RawWindow};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

use crate::state::{lock, AppState};

pub const RAIL_WINDOW: &str = "rail";

/// Called once from setup, on the main thread. Creates the platform dock and
/// claims the edge. On platforms without docking support the window is
/// simply parked at the edge so the UI can still be developed.
pub fn attach(app: &AppHandle) -> tauri::Result<()> {
    let window = rail_window(app)?;
    let state = app.state::<AppState>();
    match taskrail_platform::create_dock(raw_window(&window)?) {
        Ok(dock) => {
            *lock(&state.dock) = Some(dock);
            if let Err(e) = apply(app) {
                log::warn!("docking failed, falling back: {e}");
                park(&window, app)?;
            }
        }
        Err(DockError::Unsupported) => park(&window, app)?,
        Err(e) => {
            log::warn!("could not create dock, falling back: {e}");
            park(&window, app)?;
        }
    }
    Ok(())
}

/// Re-dock after a settings change. Commands run off the main thread, and
/// AppKit will not have that, so the work hops over.
pub fn redock(app: &AppHandle) -> tauri::Result<()> {
    let handle = app.clone();
    app.run_on_main_thread(move || match apply(&handle) {
        Ok(()) => {}
        Err(DockError::NotDocked) => {
            if let Ok(window) = rail_window(&handle) {
                if let Err(e) = park(&window, &handle) {
                    log::warn!("re-park failed: {e}");
                }
            }
        }
        Err(e) => log::warn!("re-dock failed: {e}"),
    })
}

/// Take the rail off the screen but keep the app running: the reserved
/// strip goes back to the desktop, the tray and the hotkey stay. Runs on
/// the main thread because the dock is a window operation.
pub fn hide(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        release(&handle);
        if let Ok(window) = rail_window(&handle) {
            if let Err(e) = window.hide() {
                log::warn!("could not hide the rail: {e}");
            }
        }
        log::info!("rail hidden");
    });
}

/// The UI listens for this to come out of its strip when the tray or the
/// shortcut asked for the rail.
pub const REVEAL_EVENT: &str = "rail:reveal";

/// Bring a hidden rail back and dock it again, and bring an auto-hidden
/// rail out of its strip. A rail that is already on screen is just focused.
pub fn show(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Ok(window) = rail_window(&handle) else {
            return;
        };
        let was_hidden = !window.is_visible().unwrap_or(true);
        let _ = window.show();
        if was_hidden {
            if let Err(e) = attach(&handle) {
                log::warn!("could not dock the rail again: {e}");
            }
            log::info!("rail shown");
        }
        let tucked = {
            let state = handle.state::<AppState>();
            let rail = lock(&state.rail);
            rail.settings.auto_hide && !rail.revealed
        };
        if tucked {
            if let Err(e) = set_revealed(&handle, true) {
                log::warn!("could not bring the rail out: {e}");
            }
            let _ = handle.emit(REVEAL_EVENT, ());
        }
        let _ = window.set_focus();
    });
}

/// Out of the strip or back into it. Only the window moves; the strip other
/// windows keep clear of stays as it is. Must run on the main thread.
pub fn set_revealed(app: &AppHandle, revealed: bool) -> Result<(), DockError> {
    {
        let state = app.state::<AppState>();
        let mut rail = lock(&state.rail);
        if rail.revealed == revealed {
            return Ok(());
        }
        rail.revealed = revealed;
    }
    log::info!("rail {}", if revealed { "out" } else { "tucked away" });
    resize(app)
}

/// Re-size the window to what the rail's state says, without touching the
/// reserved strip. For the live drag and for coming out of the strip.
pub fn resize(app: &AppHandle) -> Result<(), DockError> {
    let window = rail_window(app).map_err(|e| DockError::Os(e.to_string()))?;
    let state = app.state::<AppState>();
    let (window_px, monitor) = {
        let rail = lock(&state.rail);
        (rail.window_width_px(), rail.settings.monitor.clone())
    };
    let scale = monitor_scale(&window, monitor.as_deref());
    let physical = (f64::from(window_px) * scale).round() as u32;
    let mut guard = lock(&state.dock);
    match guard.as_mut() {
        Some(dock) => {
            dock.resize_window(physical)?;
            Ok(())
        }
        None => {
            drop(guard);
            park(&window, app).map_err(|e| DockError::Os(e.to_string()))
        }
    }
}

/// `resize`, from a command thread.
pub fn resize_later(app: &AppHandle) -> tauri::Result<()> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Err(e) = resize(&handle) {
            log::warn!("re-size failed: {e}");
        }
    })
}

/// Undock and drop the platform dock. Safe to call more than once.
pub fn release(app: &AppHandle) {
    let state = app.state::<AppState>();
    let taken = lock(&state.dock).take();
    if let Some(mut dock) = taken {
        if let Err(e) = dock.undock() {
            log::warn!("undock failed: {e}");
        }
    }
}

fn apply(app: &AppHandle) -> Result<(), DockError> {
    let window = rail_window(app).map_err(|e| DockError::Os(e.to_string()))?;
    let state = app.state::<AppState>();
    let (edge, width_px, window_px, monitor) = {
        let rail = lock(&state.rail);
        (
            rail.settings.edge,
            rail.reserved_width_px(),
            rail.window_width_px(),
            rail.settings.monitor.clone(),
        )
    };
    let scale = monitor_scale(&window, monitor.as_deref());
    let request = DockRequest {
        edge,
        width_px: (f64::from(width_px) * scale).round() as u32,
        window_px: (f64::from(window_px) * scale).round() as u32,
        monitor,
    };
    let mut guard = lock(&state.dock);
    let dock = guard.as_mut().ok_or(DockError::NotDocked)?;
    let rect = dock.dock(request)?;
    log::info!("docked at {rect:?}");
    Ok(())
}

/// The scale factor of the chosen monitor, so the width in logical pixels
/// comes out right on a mixed-DPI desk.
fn monitor_scale(window: &WebviewWindow, monitor: Option<&str>) -> f64 {
    chosen_monitor(window, monitor)
        .map(|m| m.scale_factor())
        .unwrap_or_else(|| window.scale_factor().unwrap_or(1.0))
}

/// The monitor the settings name, or the primary one.
fn chosen_monitor(window: &WebviewWindow, monitor: Option<&str>) -> Option<tauri::Monitor> {
    if let Some(name) = monitor {
        if let Ok(all) = window.available_monitors() {
            if let Some(m) = all
                .into_iter()
                .find(|m| m.name().map(String::as_str) == Some(name))
            {
                return Some(m);
            }
        }
    }
    window.primary_monitor().ok().flatten()
}

/// What the settings page lists.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MonitorInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub primary: bool,
}

pub fn monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    let Ok(window) = rail_window(app) else {
        return Vec::new();
    };
    let primary = window
        .primary_monitor()
        .ok()
        .flatten()
        .and_then(|m| m.name().cloned());
    window
        .available_monitors()
        .map(|all| {
            all.into_iter()
                .map(|m| MonitorInfo {
                    name: m.name().cloned().unwrap_or_else(|| "Display".into()),
                    width: m.size().width,
                    height: m.size().height,
                    x: m.position().x,
                    y: m.position().y,
                    scale: m.scale_factor(),
                    primary: m.name().cloned() == primary,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The no-docking fallback: an always-on-top window at the chosen edge of the
/// chosen monitor.
fn park(window: &WebviewWindow, app: &AppHandle) -> tauri::Result<()> {
    let (edge, width_px, monitor) = {
        let state = app.state::<AppState>();
        let rail = lock(&state.rail);
        (
            rail.settings.edge,
            rail.window_width_px(),
            rail.settings.monitor.clone(),
        )
    };
    let Some(monitor) = chosen_monitor(window, monitor.as_deref()) else {
        return Ok(());
    };
    let scale = monitor.scale_factor();
    let width = (f64::from(width_px) * scale).round() as u32;
    let size = monitor.size();
    let origin = monitor.position();
    let x = match edge {
        Edge::Right => origin.x + size.width as i32 - width as i32,
        Edge::Left => origin.x,
    };
    window.set_always_on_top(true)?;
    window.set_size(PhysicalSize::new(width, size.height))?;
    window.set_position(PhysicalPosition::new(x, origin.y))?;
    Ok(())
}

fn rail_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    app.get_webview_window(RAIL_WINDOW)
        .ok_or_else(|| tauri::Error::WindowNotFound)
}

#[allow(unused_variables)]
fn raw_window(window: &WebviewWindow) -> tauri::Result<RawWindow> {
    #[cfg(windows)]
    {
        Ok(RawWindow::Windows {
            hwnd: window.hwnd()?.0 as isize,
        })
    }
    #[cfg(target_os = "macos")]
    {
        Ok(RawWindow::MacOs {
            ns_window: window.ns_window()? as usize,
        })
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Ok(RawWindow::Unsupported)
    }
}
