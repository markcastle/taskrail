use taskrail_core::settings::Edge;

/// What the shell asks for. Widths are in *physical* pixels because that is
/// what both operating systems want to hear; the shell does the scaling.
/// `monitor` is the platform's name for a display, as the shell lists them;
/// `None`, or a name that no longer exists, means the primary display.
///
/// `width_px` is the strip reserved for the rail, the part other windows
/// keep clear of. `window_px` is how wide the window itself is, which may
/// be more than the strip when the rail is out over the desktop. `visible_px`
/// is how much of the window is on screen: the whole of it normally, and
/// only its inner edge when the rail is tucked away, with the rest pushed
/// past the screen edge. The window never gets narrower than the rail,
/// because windows a few pixels wide do not reliably exist on Windows.
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

    /// Move only the window, keeping the reserved strip as it is. Cheap
    /// enough to call on every pointer move while the rail is dragged wider,
    /// and what the rail uses to slide out over the desktop and back off
    /// the edge. `visible_px` is how much of the window stays on screen.
    fn resize_window(&mut self, window_px: u32, visible_px: u32) -> Result<DockedRect, DockError>;

    /// Re-apply the last request, for when the display layout changes.
    fn relayout(&mut self) -> Result<DockedRect, DockError>;

    /// Give the space back. Called on shutdown; also runs on drop.
    fn undock(&mut self) -> Result<(), DockError>;
}
