//! Configuration types for click track, count-in, and section guide behavior.

use facet::Facet;

/// Configuration for click track subdivisions.
#[derive(Debug, Clone, Facet)]
pub struct ClickConfig {
    /// Enable beat-level clicks (quarter notes in 4/4).
    pub beat_enabled: bool,
    /// Enable eighth note subdivision clicks.
    pub eighth_enabled: bool,
    /// Enable sixteenth note subdivision clicks.
    pub sixteenth_enabled: bool,
    /// Enable triplet subdivision clicks.
    pub triplet_enabled: bool,
    /// Enable measure accent on beat 1 (replaces the regular beat click).
    pub accent_enabled: bool,
}

impl Default for ClickConfig {
    fn default() -> Self {
        Self {
            beat_enabled: true,
            eighth_enabled: false,
            sixteenth_enabled: false,
            triplet_enabled: false,
            accent_enabled: true,
        }
    }
}

/// Configuration for count-in behavior during section transitions.
#[derive(Debug, Clone, Facet)]
pub struct CountInConfig {
    /// Enable count-in voice.
    pub enabled: bool,
    /// Offset count numbers by one (count starts from 0 instead of 1).
    pub offset_by_one: bool,
    /// Use full count patterns for odd time signatures (e.g., 7/8, 9/8).
    pub full_count_odd_time: bool,
}

impl Default for CountInConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            offset_by_one: true,
            full_count_odd_time: true,
        }
    }
}

/// Configuration for section guide cue behavior.
#[derive(Debug, Clone, Facet)]
pub struct GuideConfig {
    /// Enable section guide cues (e.g., "Verse", "Chorus" announcements).
    pub enabled: bool,
    /// Guide cue replaces the click on beat 1 of the count-in.
    pub replace_beat_one: bool,
}

impl Default for GuideConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            replace_beat_one: true,
        }
    }
}
