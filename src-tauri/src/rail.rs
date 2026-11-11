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
    /// With auto-hide on, whether the rail is out over the desktop right now
    /// or tucked into its strip. Always true with auto-hide off.
    pub revealed: bool,
    /// A width being dragged out, shown live but not yet saved.
    pub preview_width_px: Option<u32>,
    /// The page says the rail is in use (someone is typing in it, or
    /// dragging a row), so an auto-hidden rail must stay out.
    pub held: bool,
    /// Hidden from the tray: off the screen altogether, strip included.
    pub tray_hidden: bool,
    /// Mid-way between strip and rail: how much shows right now while the
    /// rail is being wiped out of its strip or back into it.
    pub wipe_px: Option<u32>,
}

/// Logical width of the rail while settings are open.
pub const EXPANDED_WIDTH_PX: u32 = 760;

/// Logical width of the strip an auto-hidden rail leaves at the edge: enough
/// to find with the pointer and to show a mark, too little to cover anything.
pub const STRIP_WIDTH_PX: u32 = 8;

impl Rail {
    pub fn open(path: Option<PathBuf>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut store = match &path {
            Some(path) => SqliteStore::open(path)?,
            None => {
                log::warn!("no data directory, running in memory");
                SqliteStore::in_memory()?
            }
        };

        let settings: RailSettings = store
            .get_value(SETTINGS_KEY)?
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let active = store.get_value(ACTIVE_KEY)?.map(ItemId::new);

        if store.get_value(SEEDED_KEY)?.is_none() {
            store.set_value(SEEDED_KEY, "1")?;
            let revealed = !settings.auto_hide;
            let mut rail = Self {
                service: RailService::new(store, SystemClock),
                settings,
                active,
                expanded: false,
                revealed,
                preview_width_px: None,
                held: false,
                tray_hidden: false,
                wipe_px: None,
            };
            crate::seed::welcome(&mut rail.service)?;
            return Ok(rail);
        }

        let revealed = !settings.auto_hide;
        Ok(Self {
            service: RailService::new(store, SystemClock),
            settings,
            active,
            expanded: false,
            revealed,
            preview_width_px: None,
            held: false,
            tray_hidden: false,
            wipe_px: None,
        })
    }

    /// The width being shown as the rail's own: the saved one, or the one
    /// mid-drag.
    fn rail_width_px(&self) -> u32 {
        self.preview_width_px.unwrap_or(self.settings.width_px)
    }

    /// The strip other windows keep clear of. With auto-hide on that is a
    /// sliver; settings still take the room they need.
    pub fn reserved_width_px(&self) -> u32 {
        if self.expanded {
            self.rail_width_px().max(EXPANDED_WIDTH_PX)
        } else if self.settings.auto_hide {
            STRIP_WIDTH_PX
        } else {
            self.rail_width_px()
        }
    }

    /// How wide the rail is right now, tucked away or not. How a platform
    /// hides a tucked-away rail is the dock's business.
    pub fn window_width_px(&self) -> u32 {
        if self.expanded {
            self.rail_width_px().max(EXPANDED_WIDTH_PX)
        } else {
            self.rail_width_px()
        }
    }

    /// How much of the rail shows, from the screen edge inward: all of it,
    /// or only the strip while it is tucked away.
    pub fn visible_width_px(&self) -> u32 {
        if !self.settings.auto_hide || self.expanded {
            return self.window_width_px();
        }
        match self.wipe_px {
            Some(px) => px.clamp(STRIP_WIDTH_PX, self.window_width_px()),
            None if self.revealed => self.window_width_px(),
            None => STRIP_WIDTH_PX,
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

    pub fn save_settings(&mut self, mut settings: RailSettings) -> Result<(), String> {
        settings.width_px = RailSettings::clamp_width(settings.width_px);
        if !settings.auto_hide {
            self.revealed = true;
        }
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
