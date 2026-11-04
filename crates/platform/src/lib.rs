//! How Task Rail gets its strip of screen.
//!
//! The rest of the app only ever sees [`DesktopDock`]. What sits behind it
//! depends on the operating system, and the two are not equals:
//!
//! - **Windows** registers an Application Desktop Toolbar (an AppBar), the
//!   same mechanism the taskbar uses. The OS shrinks the work area, so
//!   maximised windows stop at our edge. See [`windows`].
//! - **macOS** has no public way for a third-party app to reserve screen
//!   space. We float a window at the edge, above everything, on every Space.
//!   Keeping other windows from sliding underneath it is a separate, opt-in
//!   feature that needs the Accessibility permission. See [`macos`] and
//!   [`herder`].
//!
//! On any other platform [`create_dock`] returns [`DockError::Unsupported`]
//! and the shell falls back to an ordinary always-on-top window, which is
//! enough to develop the UI on Linux.

mod dock;
pub mod herder;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

pub use dock::{DesktopDock, DockError, DockRequest, DockedRect, RawWindow};

/// Picks the right implementation for the platform we were compiled for.
pub fn create_dock(window: RawWindow) -> Result<Box<dyn DesktopDock>, DockError> {
    #[cfg(windows)]
    {
        windows::AppBarDock::new(window).map(|dock| Box::new(dock) as Box<dyn DesktopDock>)
    }
    #[cfg(target_os = "macos")]
    {
        macos::FloatingPanelDock::new(window).map(|dock| Box::new(dock) as Box<dyn DesktopDock>)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = window;
        Err(DockError::Unsupported)
    }
}
