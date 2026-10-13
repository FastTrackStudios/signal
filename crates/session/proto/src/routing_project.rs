//! Routing channel definitions for FTS cross-project audio routing.
//!
//! These types are shared by:
//! - **Standalone routing project**: receives stems via hardware loopback channels
//! - **Combined setlist project**: receives stems via direct track-to-track sends
//!
//! The same `RoutingChannel` enum drives track creation in both contexts —
//! they differ only in how audio reaches each channel (loopback vs. receives).

use facet::Facet;

// ── Routing Channel ────────────────────────────────────────────────────────

// r[impl routing.channel.definition]
/// A routing channel — one stereo stem category.
///
/// Each variant maps to a track in the routing project. Click/Guide channels
/// belong to the `ClickGuide` group; instrument channels belong to `Tracks`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum RoutingChannel {
    Click = 0,
    Loop = 1,
    Count = 2,
    Guide = 3,
    Drums = 4,
    Percussion = 5,
    Bass = 6,
    Guitar = 7,
    Keys = 8,
    Vocals = 9,
    SFX = 10,
}

impl RoutingChannel {
    /// Human-readable track name in REAPER.
    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Click => "Click",
            Self::Loop => "Loop",
            Self::Count => "Count",
            Self::Guide => "Guide",
            Self::Drums => "Drums",
            Self::Percussion => "Percussion",
            Self::Bass => "Bass",
            Self::Guitar => "Guitar",
            Self::Keys => "Keys",
            Self::Vocals => "Vocals",
            Self::SFX => "SFX",
        }
    }

    /// Which folder group this channel belongs to.
    #[must_use]
    pub const fn group(&self) -> RoutingGroup {
        match self {
            Self::Click | Self::Loop | Self::Count | Self::Guide => RoutingGroup::ClickGuide,
            _ => RoutingGroup::Tracks,
        }
    }

    /// Default 0-based stereo pair offset for hardware loopback.
    ///
    /// The actual hardware pair index = `config.base_pair + default_loopback_pair_index()`.
    #[must_use]
    pub const fn default_loopback_pair_index(&self) -> u32 {
        match self {
            Self::Click => 0,
            Self::Loop => 1,
            Self::Count => 2,
            Self::Guide => 3,
            Self::Drums => 4,
            Self::Percussion => 5,
            Self::Bass => 6,
            Self::Guitar => 7,
            Self::Keys => 8,
            Self::Vocals => 9,
            Self::SFX => 10,
        }
    }

    /// All routing channels in canonical order.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Click,
            Self::Loop,
            Self::Count,
            Self::Guide,
            Self::Drums,
            Self::Percussion,
            Self::Bass,
            Self::Guitar,
            Self::Keys,
            Self::Vocals,
            Self::SFX,
        ]
    }

    /// Channels in the Click + Guide folder.
    #[must_use]
    pub const fn click_guide_channels() -> &'static [Self] {
        &[Self::Click, Self::Loop, Self::Count, Self::Guide]
    }

    /// Channels in the Tracks (instrument) folder.
    #[must_use]
    pub const fn track_channels() -> &'static [Self] {
        &[
            Self::Drums,
            Self::Percussion,
            Self::Bass,
            Self::Guitar,
            Self::Keys,
            Self::Vocals,
            Self::SFX,
        ]
    }
}

// ── Routing Group ──────────────────────────────────────────────────────────

// r[impl routing.channel.groups]
/// Which folder group a routing channel belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum RoutingGroup {
    /// Click + Guide folder (timing/cue channels).
    ClickGuide = 0,
    /// Tracks folder (instrument stem channels).
    Tracks = 1,
}

impl RoutingGroup {
    /// Folder name in REAPER.
    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::ClickGuide => "Click + Guide",
            Self::Tracks => "TRACKS",
        }
    }
}

// ── Loopback Config ────────────────────────────────────────────────────────

// r[impl routing.loopback.config]
/// Configuration for mapping routing channels to hardware loopback pairs.
///
/// REAPER's loopback channels start at a hardware-dependent offset.
/// `base_pair` is the 0-based index of the first loopback stereo pair.
#[derive(Clone, Debug, Default, Facet)]
pub struct LoopbackConfig {
    /// 0-based index of the first loopback stereo pair in the audio device.
    pub base_pair: u32,
}

impl LoopbackConfig {
    /// Create a config with a custom base pair offset.
    #[must_use]
    pub const fn with_base_pair(base_pair: u32) -> Self {
        Self { base_pair }
    }

    /// Get the hardware loopback stereo pair index for a channel.
    #[must_use]
    pub const fn pair_index(&self, channel: RoutingChannel) -> u32 {
        self.base_pair
            .saturating_add(channel.default_loopback_pair_index())
    }

    /// Encode the record input value for REAPER's `I_RECINPUT` track parameter.
    ///
    /// For stereo loopback input, the encoding is:
    /// `(pair_index * 2) | 1024` (1024 = stereo flag) + loopback offset.
    ///
    /// REAPER loopback inputs use channel indices starting at the loopback base.
    /// The formula: `channel_index | 1024` where `channel_index` is the first
    /// channel of the stereo pair (0-based within the loopback range).
    #[must_use]
    pub fn recinput_value(&self, channel: RoutingChannel) -> i32 {
        let pair = self.pair_index(channel);
        // Stereo input from loopback: (first_channel) | 1024 (stereo flag)
        // Loopback channels in REAPER use indices 512+
        let first_channel = 512u32.saturating_add(pair.saturating_mul(2));
        i32::try_from(first_channel | 1024).unwrap_or(i32::MAX)
    }
}

// ── Constants ──────────────────────────────────────────────────────────────

/// Filename for the routing project on disk.
pub const ROUTING_PROJECT_FILENAME: &str = "FTS-Routing.RPP";

// r[impl routing.project.extstate]
/// `ExtState` section for routing project identification.
pub const EXT_STATE_SECTION: &str = "FTS";

/// `ExtState` key to identify a project as the routing project.
pub const EXT_STATE_KEY_IS_ROUTING: &str = "is_routing_project";

/// `ExtState` value indicating this is the routing project.
pub const EXT_STATE_VALUE_TRUE: &str = "1";

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_groups() {
        for ch in RoutingChannel::click_guide_channels() {
            assert_eq!(ch.group(), RoutingGroup::ClickGuide);
        }
        for ch in RoutingChannel::track_channels() {
            assert_eq!(ch.group(), RoutingGroup::Tracks);
        }
    }

    #[test]
    fn all_channels_covered() {
        let all = RoutingChannel::all();
        let cg = RoutingChannel::click_guide_channels();
        let tr = RoutingChannel::track_channels();
        assert_eq!(all.len(), cg.len() + tr.len());
    }

    #[test]
    fn loopback_config_default() {
        let config = LoopbackConfig::default();
        assert_eq!(config.pair_index(RoutingChannel::Click), 0);
        assert_eq!(config.pair_index(RoutingChannel::SFX), 10);
    }

    #[test]
    fn loopback_config_with_offset() {
        let config = LoopbackConfig::with_base_pair(8);
        assert_eq!(config.pair_index(RoutingChannel::Click), 8);
        assert_eq!(config.pair_index(RoutingChannel::Drums), 12);
    }

    #[test]
    fn recinput_encoding() {
        let config = LoopbackConfig::default();
        // Click at pair 0: channel 512, stereo → 512 | 1024 = 1536
        assert_eq!(config.recinput_value(RoutingChannel::Click), 1536);
        // Drums at pair 4: channel 520, stereo → 520 | 1024 = 1544
        assert_eq!(config.recinput_value(RoutingChannel::Drums), 1544);
    }

    #[test]
    fn display_names() {
        assert_eq!(RoutingChannel::Click.display_name(), "Click");
        assert_eq!(RoutingChannel::SFX.display_name(), "SFX");
        assert_eq!(RoutingGroup::ClickGuide.display_name(), "Click + Guide");
        assert_eq!(RoutingGroup::Tracks.display_name(), "TRACKS");
    }
}
