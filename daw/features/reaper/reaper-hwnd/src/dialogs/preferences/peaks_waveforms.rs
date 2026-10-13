//! Preferences -> Appearance -> Peaks/Waveforms page child window IDs.

use crate::ChildId;

/// Preferences -> Appearance -> Peaks/Waveforms page child window IDs.
pub struct PeaksWaveformsPrefs;

impl PeaksWaveformsPrefs {
    /// Display peaks in sample-level view - Class: Button
    pub const SAMPLE_LEVEL_VIEW: ChildId = ChildId(1000);
    /// Tint media items with track color - Class: Button
    pub const TINT_WITH_TRACK_COLOR: ChildId = ChildId(1001);
    /// Peaks display mode dropdown - Class: ComboBox
    pub const DISPLAY_MODE: ChildId = ChildId(1002);
    /// Display mode label - Class: Static
    pub const DISPLAY_MODE_LABEL: ChildId = ChildId(1003);
    /// Peak cache block size dropdown - Class: ComboBox
    pub const CACHE_BLOCK_SIZE: ChildId = ChildId(1004);
    /// Cache block size label - Class: Static
    pub const CACHE_BLOCK_SIZE_LABEL: ChildId = ChildId(1005);
    /// Show peaks in media items - Class: Button
    pub const SHOW_PEAKS: ChildId = ChildId(1006);
    /// Show peak values in dB - Class: Button
    pub const SHOW_DB_VALUES: ChildId = ChildId(1007);
    /// Peak display gain inputbox - Class: Edit
    pub const DISPLAY_GAIN: ChildId = ChildId(1008);
    /// Display gain label - Class: Static
    pub const DISPLAY_GAIN_LABEL: ChildId = ChildId(1009);
    /// Show spectral peaks - Class: Button
    pub const SHOW_SPECTRAL: ChildId = ChildId(1100);
    /// Spectral peaks settings label - Class: Static
    pub const SPECTRAL_LABEL: ChildId = ChildId(1101);
    /// Peak colors label - Class: Static
    pub const COLORS_LABEL: ChildId = ChildId(1200);
    /// Antialiased peaks - Class: Button
    pub const ANTIALIASED: ChildId = ChildId(1300);
    /// Peaks settings label - Class: Static
    pub const PEAKS_LABEL: ChildId = ChildId(1652);
}
