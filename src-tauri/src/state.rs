use std::sync::Mutex;

use taskrail_platform::DesktopDock;

use crate::rail::Rail;

/// Everything the shell holds between commands.
pub struct AppState {
    pub rail: Mutex<Rail>,
    pub dock: Mutex<Option<Box<dyn DesktopDock>>>,
}

impl AppState {
    pub fn new(rail: Rail) -> Self {
        Self {
            rail: Mutex::new(rail),
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
