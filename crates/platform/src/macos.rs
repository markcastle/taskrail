//! macOS: a floating window pinned to the screen edge.
//!
//! There is no public API for a third-party app to reserve desktop space the
//! way the Dock does, so this is the honest approximation: the rail floats
//! above normal windows, joins every Space, and fills the visible height of
//! its screen. Other windows can still end up underneath it; closing that gap
//! is [`crate::herder`]'s job.
//!
//! Every method here must run on the main thread. AppKit insists, and the
//! `MainThreadMarker` check turns a mistake into an error instead of a crash.

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSScreen, NSWindow, NSWindowCollectionBehavior, NSWindowLevel};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use taskrail_core::settings::Edge;

use crate::dock::{DesktopDock, DockError, DockRequest, DockedRect, RawWindow};

/// `kCGFloatingWindowLevel`. Above normal windows, below the menu bar.
const FLOATING_WINDOW_LEVEL: NSWindowLevel = 3;

pub struct FloatingPanelDock {
    ns_window: usize,
    last: Option<DockRequest>,
}

// The pointer is only ever dereferenced on the main thread (checked at every
// use), so moving the dock between threads is fine.
unsafe impl Send for FloatingPanelDock {}

impl FloatingPanelDock {
    pub fn new(window: RawWindow) -> Result<Self, DockError> {
        let RawWindow::MacOs { ns_window } = window else {
            return Err(DockError::Unsupported);
        };
        if ns_window == 0 {
            return Err(DockError::Os("NSWindow pointer was null".into()));
        }
        Ok(Self {
            ns_window,
            last: None,
        })
    }

    fn window(&self, _mtm: MainThreadMarker) -> &NSWindow {
        // SAFETY: the shell hands us a live NSWindow that outlives the dock,
        // and the marker proves we are on the thread AppKit requires.
        unsafe { &*(self.ns_window as *const NSWindow) }
    }

    fn apply(&self, request: DockRequest) -> Result<DockedRect, DockError> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| DockError::Os("dock() must be called on the main thread".into()))?;
        let window = self.window(mtm);

        window.setLevel(FLOATING_WINDOW_LEVEL);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        window.setHidesOnDeactivate(false);

        let screen = screen_for(mtm, request.monitor.as_deref())
            .or_else(|| window.screen())
            .ok_or_else(|| DockError::Os("no screen to dock to".into()))?;
        let scale = screen.backingScaleFactor();
        let width = f64::from(request.width_px) / scale;
        let visible = screen.visibleFrame();
        let x = match request.edge {
            Edge::Right => visible.origin.x + visible.size.width - width,
            Edge::Left => visible.origin.x,
        };
        let frame = NSRect::new(
            NSPoint::new(x, visible.origin.y),
            NSSize::new(width, visible.size.height),
        );
        window.setFrame_display(frame, true);

        // AppKit's origin is bottom-left; report top-left like Windows does.
        let full = screen.frame();
        let top = full.origin.y + full.size.height - (frame.origin.y + frame.size.height);
        Ok(DockedRect {
            x: (frame.origin.x * scale).round() as i32,
            y: (top * scale).round() as i32,
            width: (frame.size.width * scale).round() as i32,
            height: (frame.size.height * scale).round() as i32,
        })
    }
}

/// The display to dock to: the one whose name matches, else the main one.
/// Names are what `NSScreen.localizedName` reports, which is also what the
/// shell lists.
fn screen_for(mtm: MainThreadMarker, monitor: Option<&str>) -> Option<Retained<NSScreen>> {
    if let Some(name) = monitor {
        let screens = NSScreen::screens(mtm);
        for screen in screens.iter() {
            if screen.localizedName().to_string() == name {
                return Some(screen);
            }
        }
    }
    NSScreen::mainScreen(mtm)
}

impl DesktopDock for FloatingPanelDock {
    fn dock(&mut self, request: DockRequest) -> Result<DockedRect, DockError> {
        self.last = Some(request.clone());
        self.apply(request)
    }

    fn relayout(&mut self) -> Result<DockedRect, DockError> {
        let request = self.last.clone().ok_or(DockError::NotDocked)?;
        self.apply(request)
    }

    fn undock(&mut self) -> Result<(), DockError> {
        // Nothing was reserved, so there is nothing to give back. The window
        // keeps its level until the shell closes it.
        self.last = None;
        Ok(())
    }
}
