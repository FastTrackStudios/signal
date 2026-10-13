//! Preferences -> Audio -> Rendering page child window IDs.

use crate::ChildId;

/// Preferences -> Audio -> Rendering page child window IDs.
pub struct RenderingPrefs;

impl RenderingPrefs {
    /// Full-speed offline rendering mode dropdown - Class: ComboBox
    pub const OFFLINE_MODE: ChildId = ChildId(1000);
    /// Offline rendering label - Class: Static
    pub const OFFLINE_LABEL: ChildId = ChildId(1001);
    /// Resample mode dropdown - Class: ComboBox
    pub const RESAMPLE_MODE: ChildId = ChildId(1002);
    /// Resample label - Class: Static
    pub const RESAMPLE_LABEL: ChildId = ChildId(1003);
    /// Use project samplerate for mixing and FX - Class: Button
    pub const USE_PROJECT_SAMPLERATE: ChildId = ChildId(1004);
    /// Rendering samplerate inputbox - Class: Edit
    pub const SAMPLERATE: ChildId = ChildId(1005);
    /// Samplerate label - Class: Static
    pub const SAMPLERATE_LABEL: ChildId = ChildId(1006);
}
