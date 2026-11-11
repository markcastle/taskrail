use taskrail_core::settings::Edge;

/// What the shell asks for. Widths are in *physical* pixels because that is
/// what both operating systems want to hear; the shell does the scaling.
/// `monitor` is the platform's name for a display, as the shell lists them;
/// `None`, or a name that no longer exists, means the primary display.
///
/// `width_px` is the strip reserved for the rail, the part other windows
/// keep clear of. `window_px` is how wide the rail is, which may be more
/// than the strip when it is out over the desktop. `visible_px` is how much
/// of it shows, measured from the screen edge inward: all of it normally,
/// and only a sliver while the rail is tucked away. How a platform hides
/// the rest is its own business (Windows clips the window in place, macOS
/// makes the window that narrow); what it must never do is put the hidden
/// part on a neighbouring display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockRequest {
    pub edge: Edge,
    pub width_px: u32,
    pub window_px: u32,
    pub visible_px: u32,
    pub monitor: Option<String>,
}

impl DockRequest {
    /// A window exactly as wide as its strip, all of it on screen.
    pub fn plain(edge: Edge, width_px: u32, monitor: Option<String>) -> Self {
        Self {
            edge,
            width_px,
            window_px: width_px,
            visible_px: width_px,
            monitor,
        }
    }
}

/// Where the rail ended up, in physical pixels with a top-left origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockedRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The native window handle, as plain integers so the value is `Send`. Only
/// the platform module that matches knows what to do with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawWindow {
    Windows { hwnd: isize },
    MacOs { ns_window: usize },
    Unsupported,
}

#[derive(Debug, thiserror::Error)]
pub enum DockError {
    #[error("desktop docking is not supported on this platform")]
    Unsupported,
    #[error("the rail is not docked")]
    NotDocked,
    #[error("operating system call failed: {0}")]
    Os(String),
}

/// A strip of screen that belongs to the rail.
///
/// Implementations may require calls on the main thread (macOS does). The
/// shell guarantees that; this trait only promises `Send` so it can live in
/// shared state.
pub trait DesktopDock: Send {
    /// Claim the edge. Safe to call again with a new request to move or
    /// resize the rail.
    fn dock(&mut self, request: DockRequest) -> Result<DockedRect, DockError>;

    /// Change only the window, keeping the reserved strip as it is. Cheap
    /// enough to call on every pointer move while the rail is dragged wider,
    /// and what the rail uses to come out of its strip and go back into it.
    /// `visible_px` is how much of the rail shows, from the edge inward.
    fn resize_window(&mut self, window_px: u32, visible_px: u32) -> Result<DockedRect, DockError>;

    /// Re-apply the last request, for when the display layout changes.
    fn relayout(&mut self) -> Result<DockedRect, DockError>;

    /// Give the space back. Called on shutdown; also runs on drop.
    fn undock(&mut self) -> Result<(), DockError>;
}
