use std::sync::Mutex;

use taskrail_core::domain::{Item, ItemId};
use taskrail_core::settings::RailSettings;
use taskrail_platform::DesktopDock;

/// Everything the shell holds between commands.
///
/// Items are an in-memory fixture until the SQLite store lands in milestone
/// two. Settings are defaults until the settings file lands with it.
pub struct AppState {
    pub settings: Mutex<RailSettings>,
    pub items: Mutex<Vec<Item>>,
    pub active: Mutex<Option<ItemId>>,
    pub dock: Mutex<Option<Box<dyn DesktopDock>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            settings: Mutex::new(RailSettings::default()),
            items: Mutex::new(crate::fixtures::items(chrono::Utc::now())),
            active: Mutex::new(Some(ItemId::new("design-dashboard"))),
            dock: Mutex::new(None),
        }
    }
}
