//! Keeping other windows off the rail where the OS will not do it for us.
//!
//! On macOS a floating window is only ever *above* other windows; the system
//! will still happily place a zoomed window underneath it. The plan for parity
//! is a small background task that, once the user grants the Accessibility
//! permission, watches other applications' windows through the `AXUIElement`
//! API and nudges any that overlap the rail's strip back into the free area.
//! This is exactly how tiling window managers such as yabai work.
//!
//! Not implemented yet. The trait is here so the shell can be written
//! against it and so the first implementation has a shape to fill.

use crate::DockError;

pub trait WindowHerder: Send {
    /// Start watching. Returns an error if the permission is missing, so the
    /// shell can send the user to System Settings.
    fn start(&mut self) -> Result<(), DockError>;

    /// Stop watching. Windows already moved stay where they are.
    fn stop(&mut self);
}
