//! Glue between the Tauri window and the platform dock.

use taskrail_core::settings::Edge;
use taskrail_platform::{DockError, DockRequest, RawWindow};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

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
    let (edge, width_px) = {
        let rail = lock(&state.rail);
        (rail.settings.edge, rail.current_width_px())
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let request = DockRequest {
        edge,
        width_px: (f64::from(width_px) * scale).round() as u32,
    };
    let mut guard = lock(&state.dock);
    let dock = guard.as_mut().ok_or(DockError::NotDocked)?;
    let rect = dock.dock(request)?;
    log::info!("docked at {rect:?}");
    Ok(())
}

/// The no-docking fallback: an always-on-top window at the chosen edge of the
/// primary monitor.
fn park(window: &WebviewWindow, app: &AppHandle) -> tauri::Result<()> {
    let (edge, width_px) = {
        let state = app.state::<AppState>();
        let rail = lock(&state.rail);
        (rail.settings.edge, rail.current_width_px())
    };
    let Some(monitor) = window.primary_monitor()? else {
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
