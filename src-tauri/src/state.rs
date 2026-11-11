use std::sync::Mutex;

use taskrail_core::autohide::AutoHide;
use taskrail_platform::{DesktopDock, DockedRect};

use crate::connectors::ConnectorRegistry;
use crate::rail::Rail;

/// Everything the shell holds between commands.
pub struct AppState {
    pub rail: Mutex<Rail>,
    pub connectors: Mutex<ConnectorRegistry>,
    pub dock: Mutex<Option<Box<dyn DesktopDock>>>,
    /// When an auto-hidden rail is out and when it is tucked away.
    pub autohide: Mutex<AutoHide>,
    /// Where the rail's window is and its display's scale, for telling
    /// whether the pointer is on the rail's edge.
    pub window_rect: Mutex<Option<(DockedRect, f64)>>,
}

impl AppState {
    pub fn new(rail: Rail, connectors: ConnectorRegistry) -> Self {
        let out = rail.revealed;
        Self {
            autohide: Mutex::new(AutoHide::new(out)),
            window_rect: Mutex::new(None),
            rail: Mutex::new(rail),
            connectors: Mutex::new(connectors),
            dock: Mutex::new(None),
        }
    }
}

/// A poisoned mutex here means a command panicked mid-update. The data is
/// plain values on top of a transactional store, so carrying on is safer
/// than taking the whole rail down.
pub fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
