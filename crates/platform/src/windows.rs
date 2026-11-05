//! Windows: a real reserved strip through the Application Desktop Toolbar
//! (AppBar) API.
//!
//! The sequence is the one Microsoft documents: register with `ABM_NEW`,
//! propose a rectangle with `ABM_QUERYPOS` (the system may push it inward to
//! make room for other bars), commit it with `ABM_SETPOS`, then move the
//! window into it. The system tells us through the callback message when the
//! layout changes, and we re-run the same steps.
//!
//! One rail per process, so the state the callback needs is a single global.

use std::mem::size_of;
use std::sync::Mutex;

use taskrail_core::settings::Edge;
use windows::core::w;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, HDC, HMONITOR, MONITORINFO,
    MONITORINFOEXW, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::UI::Shell::{
    DefSubclassProc, RemoveWindowSubclass, SHAppBarMessage, SetWindowSubclass, ABE_LEFT, ABE_RIGHT,
    ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, ABN_POSCHANGED, APPBARDATA,
};
use windows::Win32::UI::WindowsAndMessaging::{
    RegisterWindowMessageW, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER, WM_DISPLAYCHANGE,
};

use crate::dock::{DesktopDock, DockError, DockRequest, DockedRect, RawWindow};

const SUBCLASS_ID: usize = 0x5452_4149; // "TRAI"

#[derive(Debug, Clone)]
struct Placement {
    edge: Edge,
    width: i32,
    monitor: Option<String>,
    callback_message: u32,
}

static PLACEMENT: Mutex<Option<Placement>> = Mutex::new(None);

pub struct AppBarDock {
    hwnd: isize,
    callback_message: u32,
    registered: bool,
}

impl AppBarDock {
    pub fn new(window: RawWindow) -> Result<Self, DockError> {
        let RawWindow::Windows { hwnd } = window else {
            return Err(DockError::Unsupported);
        };
        let callback_message = unsafe { RegisterWindowMessageW(w!("TaskRailAppBarCallback")) };
        if callback_message == 0 {
            return Err(DockError::Os("RegisterWindowMessageW returned 0".into()));
        }
        Ok(Self {
            hwnd,
            callback_message,
            registered: false,
        })
    }

    fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }

    fn appbar_data(&self) -> APPBARDATA {
        APPBARDATA {
            cbSize: size_of::<APPBARDATA>() as u32,
            hWnd: self.hwnd(),
            uCallbackMessage: self.callback_message,
            ..Default::default()
        }
    }
}

impl DesktopDock for AppBarDock {
    fn dock(&mut self, request: DockRequest) -> Result<DockedRect, DockError> {
        if !self.registered {
            let mut data = self.appbar_data();
            unsafe { SHAppBarMessage(ABM_NEW, &mut data) };
            unsafe { SetWindowSubclass(self.hwnd(), Some(subclass_proc), SUBCLASS_ID, 0) }
                .ok()
                .map_err(|e| DockError::Os(format!("SetWindowSubclass: {e}")))?;
            self.registered = true;
        }
        let placement = Placement {
            edge: request.edge,
            width: request.width_px as i32,
            monitor: request.monitor.clone(),
            callback_message: self.callback_message,
        };
        *lock_placement() = Some(placement.clone());
        place(self.hwnd(), &placement)
    }

    fn relayout(&mut self) -> Result<DockedRect, DockError> {
        let placement = lock_placement().clone().ok_or(DockError::NotDocked)?;
        place(self.hwnd(), &placement)
    }

    fn undock(&mut self) -> Result<(), DockError> {
        if self.registered {
            let mut data = self.appbar_data();
            unsafe {
                SHAppBarMessage(ABM_REMOVE, &mut data);
                let _ = RemoveWindowSubclass(self.hwnd(), Some(subclass_proc), SUBCLASS_ID);
            }
            self.registered = false;
            *lock_placement() = None;
        }
        Ok(())
    }
}

impl Drop for AppBarDock {
    fn drop(&mut self) {
        let _ = self.undock();
    }
}

fn lock_placement() -> std::sync::MutexGuard<'static, Option<Placement>> {
    PLACEMENT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Every display: the device name Windows uses for it (`\\.\DISPLAY1`) and
/// its full rectangle in physical pixels.
fn monitors() -> Vec<(String, RECT)> {
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _hdc: HDC,
        _clip: *mut RECT,
        out: LPARAM,
    ) -> BOOL {
        let out = &mut *(out.0 as *mut Vec<(String, RECT)>);
        let mut info = MONITORINFOEXW {
            monitorInfo: MONITORINFO {
                cbSize: size_of::<MONITORINFOEXW>() as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        if GetMonitorInfoW(
            monitor,
            &mut info as *mut MONITORINFOEXW as *mut MONITORINFO,
        )
        .as_bool()
        {
            let len = info
                .szDevice
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(info.szDevice.len());
            let name = String::from_utf16_lossy(&info.szDevice[..len]);
            out.push((name, info.monitorInfo.rcMonitor));
        }
        BOOL::from(true)
    }
    let mut out: Vec<(String, RECT)> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut out as *mut _ as isize),
        );
    }
    out
}

/// The display to dock to: the named one if it is still attached, else the
/// primary display.
fn screen_for(monitor: Option<&str>) -> Result<RECT, DockError> {
    if let Some(name) = monitor {
        if let Some((_, rect)) = monitors().into_iter().find(|(n, _)| n == name) {
            return Ok(rect);
        }
    }
    let primary = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetMonitorInfoW(primary, &mut info) }
        .ok()
        .map_err(|e| DockError::Os(format!("GetMonitorInfoW: {e}")))?;
    Ok(info.rcMonitor)
}

/// Runs the query/set/move sequence for the given placement.
fn place(hwnd: HWND, placement: &Placement) -> Result<DockedRect, DockError> {
    let screen = screen_for(placement.monitor.as_deref())?;

    let mut data = APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: placement.callback_message,
        uEdge: match placement.edge {
            Edge::Left => ABE_LEFT,
            Edge::Right => ABE_RIGHT,
        },
        rc: screen,
        ..Default::default()
    };

    fit(&mut data.rc, placement.edge, placement.width, screen);
    unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut data) };
    // The system may have pushed our outer edge inward to make room for
    // another bar. Keep our width and accept the new edge.
    fit(&mut data.rc, placement.edge, placement.width, screen);
    unsafe { SHAppBarMessage(ABM_SETPOS, &mut data) };

    let rc = data.rc;
    let (width, height) = (rc.right - rc.left, rc.bottom - rc.top);
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            rc.left,
            rc.top,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
    .map_err(|e| DockError::Os(format!("SetWindowPos: {e}")))?;

    Ok(DockedRect {
        x: rc.left,
        y: rc.top,
        width,
        height,
    })
}

fn fit(rc: &mut RECT, edge: Edge, width: i32, screen: RECT) {
    rc.top = screen.top;
    rc.bottom = screen.bottom;
    match edge {
        Edge::Right => rc.left = rc.right - width,
        Edge::Left => rc.right = rc.left + width,
    }
}

/// Hooked into the rail window so we hear about layout changes without
/// owning the window procedure.
unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    let placement = PLACEMENT.lock().ok().and_then(|guard| guard.clone());
    if let Some(placement) = placement {
        let layout_changed = msg == WM_DISPLAYCHANGE
            || (msg == placement.callback_message && wparam.0 == ABN_POSCHANGED as usize);
        if layout_changed {
            let _ = place(hwnd, &placement);
        }
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}
