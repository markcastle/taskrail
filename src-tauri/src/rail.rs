//! The shell's view of the core: one service over one store, plus the two
//! things the core leaves to us to persist, settings and the active item.

use std::path::PathBuf;

use taskrail_core::domain::ItemId;
use taskrail_core::ports::{KeyValueStore, SystemClock};
use taskrail_core::services::RailService;
use taskrail_core::settings::RailSettings;
use taskrail_core::usecases::{build_rail_view, RailView};
use taskrail_store_sqlite::SqliteStore;

const SETTINGS_KEY: &str = "settings";
const ACTIVE_KEY: &str = "active";
const SEEDED_KEY: &str = "seeded";

pub struct Rail {
    pub service: RailService<SqliteStore, SystemClock>,
    pub settings: RailSettings,
    pub active: Option<ItemId>,
    /// While settings are open the rail widens to make room. Not persisted.
    pub expanded: bool,
}

/// Logical width of the rail while settings are open.
pub const EXPANDED_WIDTH_PX: u32 = 760;

impl Rail {
    pub fn open(path: Option<PathBuf>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut store = match &path {
            Some(path) => SqliteStore::open(path)?,
            None => {
                log::warn!("no data directory, running in memory");
                SqliteStore::in_memory()?
            }
        };

        let settings = store
            .get_value(SETTINGS_KEY)?
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let active = store.get_value(ACTIVE_KEY)?.map(ItemId::new);

        if store.get_value(SEEDED_KEY)?.is_none() {
            store.set_value(SEEDED_KEY, "1")?;
            let mut rail = Self {
                service: RailService::new(store, SystemClock),
                settings,
                active,
                expanded: false,
            };
            crate::seed::welcome(&mut rail.service)?;
            return Ok(rail);
        }

        Ok(Self {
            service: RailService::new(store, SystemClock),
            settings,
            active,
            expanded: false,
        })
    }

    /// The width the dock should use right now.
    pub fn current_width_px(&self) -> u32 {
        if self.expanded {
            self.settings.width_px.max(EXPANDED_WIDTH_PX)
        } else {
            self.settings.width_px
        }
    }

    pub fn view(&self, expand_top: bool) -> Result<RailView, String> {
        let items = self.service.items().map_err(|e| e.to_string())?;
        Ok(build_rail_view(
            &items,
            self.active.as_ref(),
            chrono::Utc::now(),
            &self.settings,
            expand_top,
        ))
    }

    pub fn save_settings(&mut self, settings: RailSettings) -> Result<(), String> {
        let json = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
        self.service
            .repository_mut()
            .set_value(SETTINGS_KEY, &json)
            .map_err(|e| e.to_string())?;
        self.settings = settings;
        Ok(())
    }

    pub fn save_active(&mut self, active: Option<ItemId>) -> Result<(), String> {
        let store = self.service.repository_mut();
        match &active {
            Some(id) => store.set_value(ACTIVE_KEY, id.as_str()),
            None => store.remove_value(ACTIVE_KEY),
        }
        .map_err(|e| e.to_string())?;
        self.active = active;
        Ok(())
    }
}
