//! Preferences -> Audio -> Mute/Solo page child window IDs.

use crate::ChildId;

/// Preferences -> Audio -> Mute/Solo page child window IDs.
pub struct MuteSoloPrefs;

impl MuteSoloPrefs {
    /// Solo-in-place ganging dropdown - Class: ComboBox
    pub const SOLO_IN_PLACE_GANGING: ChildId = ChildId(1000);
    /// Track solo in place mode dropdown - Class: ComboBox
    pub const SOLO_IN_PLACE_MODE: ChildId = ChildId(1001);
    /// Solo defeats mute - Class: Button
    pub const SOLO_DEFEATS_MUTE: ChildId = ChildId(1002);
    /// Track mute/solo behavior dropdown - Class: ComboBox
    pub const MUTE_SOLO_BEHAVIOR: ChildId = ChildId(1003);
    /// Send mute behavior dropdown - Class: ComboBox
    pub const SEND_MUTE_BEHAVIOR: ChildId = ChildId(1004);
    /// Mute/solo settings label - Class: Static
    pub const MUTE_SOLO_LABEL: ChildId = ChildId(1005);
    /// Solo dimming amount inputbox - Class: Edit
    pub const SOLO_DIM_AMOUNT: ChildId = ChildId(1006);
    /// Solo dimming label - Class: Static
    pub const SOLO_DIM_LABEL: ChildId = ChildId(1007);
    /// Exclusive solo behavior - Class: Button
    pub const EXCLUSIVE_SOLO: ChildId = ChildId(1008);
}
