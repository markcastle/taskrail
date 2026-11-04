use taskrail_core::settings::Edge;

/// What the shell asks for. Width is in *physical* pixels because that is
/// what both operating systems want to hear; the shell does the scaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockRequest {
    pub edge: Edge,
    pub width_px: u32,
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

    /// Re-apply the last request, for when the display layout changes.
    fn relayout(&mut self) -> Result<DockedRect, DockError>;

    /// Give the space back. Called on shutdown; also runs on drop.
    fn undock(&mut self) -> Result<(), DockError>;
}
