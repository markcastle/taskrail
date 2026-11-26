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

/// The narrowest the rail goes before titles stop fitting, and the widest
/// before it stops being a rail. The slider and the drag handle share them.
pub const WIDTH_MIN_PX: u32 = 240;
pub const WIDTH_MAX_PX: u32 = 600;

/// How often connected systems are asked what has changed, in minutes.
/// Five by default: fresh enough for a list of tasks, and one request every
/// five minutes is a load no server notices. The rail does not lean on it
/// to feel current: changes made on the rail are sent at once, a click on
/// the footer syncs now, and it syncs when the computer wakes.
pub const SYNC_MINUTES_DEFAULT: u32 = 5;
pub const SYNC_MINUTES_MIN: u32 = 1;
pub const SYNC_MINUTES_MAX: u32 = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RailSettings {
    pub edge: Edge,
    /// Rail width in logical pixels. Platforms convert to physical pixels.
    pub width_px: u32,
    /// Tuck the rail away to a thin strip at the edge, and bring it out when
    /// the pointer rests on the strip. Off by default: the rail's whole idea
    /// is to be in view.
    #[serde(default)]
    pub auto_hide: bool,
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
    /// Open when the person signs in to their computer. On unless they turn
    /// it off: a rail that has to be remembered and started by hand is a
    /// to-do list with extra steps.
    #[serde(default = "yes")]
    pub start_at_login: bool,
    /// Install new releases without asking. Off until the person turns it
    /// on: an update that changes things under you is a surprise.
    #[serde(default)]
    pub auto_update: bool,
    /// Motion on the rail: rows sliding away, arriving, settling. On by
    /// default; off here or through the system's reduce-motion setting.
    #[serde(default = "yes")]
    pub animations: bool,
    /// A quiet tick on complete and a softer one on start. Off until asked
    /// for: sound from an app nobody expected to make a sound is a jolt.
    #[serde(default)]
    pub sounds: bool,
    /// Minutes between pulls from connected systems. Read it through
    /// [`RailSettings::sync_seconds`], which clamps it.
    #[serde(default = "sync_minutes_default")]
    pub sync_minutes: u32,
}

fn yes() -> bool {
    true
}

fn sync_minutes_default() -> u32 {
    SYNC_MINUTES_DEFAULT
}

impl Default for RailSettings {
    fn default() -> Self {
        Self {
            edge: Edge::Right,
            width_px: 320,
            auto_hide: false,
            top_limit: TOP_LIMIT_DEFAULT,
            event_lead_minutes: 60,
            theme: Theme::System,
            monitor: None,
            start_at_login: true,
            auto_update: false,
            animations: true,
            sounds: false,
            sync_minutes: SYNC_MINUTES_DEFAULT,
        }
    }
}

impl RailSettings {
    pub fn effective_top_limit(&self) -> usize {
        self.top_limit.clamp(1, TOP_LIMIT_MAX) as usize
    }

    /// Seconds between pulls, whatever was asked for kept within reason.
    pub fn sync_seconds(&self) -> u32 {
        self.sync_minutes.clamp(SYNC_MINUTES_MIN, SYNC_MINUTES_MAX) * 60
    }

    /// The width inside the range the rail can live with, whatever was
    /// asked for.
    pub fn clamp_width(width_px: u32) -> u32 {
        width_px.clamp(WIDTH_MIN_PX, WIDTH_MAX_PX)
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
        assert!(!settings.auto_hide);
        assert!(
            settings.start_at_login,
            "someone who never saw the switch gets the default, which is on"
        );
        assert_eq!(settings.sync_seconds(), 300, "and syncs every five minutes");
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"theme\":\"system\""));
    }

    #[test]
    fn the_sync_interval_is_kept_within_reason() {
        let every = |minutes| RailSettings {
            sync_minutes: minutes,
            ..RailSettings::default()
        };
        assert_eq!(every(0).sync_seconds(), 60);
        assert_eq!(every(15).sync_seconds(), 900);
        assert_eq!(every(10_000).sync_seconds(), 3600);
    }

    #[test]
    fn a_width_outside_the_range_is_pulled_back_in() {
        assert_eq!(RailSettings::clamp_width(100), WIDTH_MIN_PX);
        assert_eq!(RailSettings::clamp_width(320), 320);
        assert_eq!(RailSettings::clamp_width(9000), WIDTH_MAX_PX);
    }
}
