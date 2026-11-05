//! User-facing settings for the rail itself.
//!
//! Persisting these is the shell's job. This module only says what the
//! settings are and what they default to, so the core can be tested against
//! any combination.

use serde::{Deserialize, Serialize};

/// Which edge of the screen the rail docks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    Left,
    Right,
}

/// Light, dark, or whatever the operating system says.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// How many Top Priorities show before the fold, unless the user changes it.
pub const TOP_LIMIT_DEFAULT: u8 = 5;

/// The most the fold will ever reveal. The rail is a next-action queue, not a
/// backlog, and ten is where that stops being true.
pub const TOP_LIMIT_MAX: u8 = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RailSettings {
    pub edge: Edge,
    /// Rail width in logical pixels. Platforms convert to physical pixels.
    pub width_px: u32,
    /// Requested fold limit. Read it through
    /// [`RailSettings::effective_top_limit`], which clamps it.
    pub top_limit: u8,
    /// An event this close to starting is pulled into Top Priorities.
    pub event_lead_minutes: u32,
    #[serde(default)]
    pub theme: Theme,
    /// The monitor to dock to, by the name the platform reports. `None`
    /// means the primary monitor, and a name that no longer exists falls
    /// back to it too.
    #[serde(default)]
    pub monitor: Option<String>,
}

impl Default for RailSettings {
    fn default() -> Self {
        Self {
            edge: Edge::Right,
            width_px: 320,
            top_limit: TOP_LIMIT_DEFAULT,
            event_lead_minutes: 60,
            theme: Theme::System,
            monitor: None,
        }
    }
}

impl RailSettings {
    pub fn effective_top_limit(&self) -> usize {
        self.top_limit.clamp(1, TOP_LIMIT_MAX) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_saved_before_themes_existed_still_load() {
        let old = r#"{"edge":"left","width_px":300,"top_limit":7,"event_lead_minutes":30}"#;
        let settings: RailSettings = serde_json::from_str(old).unwrap();
        assert_eq!(settings.theme, Theme::System);
        assert_eq!(settings.monitor, None);
        assert_eq!(settings.width_px, 300);
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"theme\":\"system\""));
    }
}
