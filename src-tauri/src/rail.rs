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
}

/// Logical width of the rail while settings are open.
pub const EXPANDED_WIDTH_PX: u32 = 760;

/// Logical width of the strip an auto-hidden rail leaves at the edge: enough
/// to find with the pointer and to show a mark, too little to cover anything.
/// The window itself never gets this narrow; it slides past the screen edge
/// with this much left showing.
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

    /// How wide the window itself is right now. Never the strip: a window a
    /// few pixels wide does not reliably exist on Windows, so a tucked-away
    /// rail keeps its width and sits past the screen edge instead.
    pub fn window_width_px(&self) -> u32 {
        if self.expanded {
            self.rail_width_px().max(EXPANDED_WIDTH_PX)
        } else {
            self.rail_width_px()
        }
    }

    /// How much of the window is on screen: all of it, or only the strip
    /// while the rail is tucked away.
    pub fn visible_width_px(&self) -> u32 {
        if self.settings.auto_hide && !self.revealed && !self.expanded {
            STRIP_WIDTH_PX
        } else {
            self.window_width_px()
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
