//! Preferences -> Automation page child window IDs.

use crate::ChildId;

/// Preferences -> Automation page child window IDs.
pub struct AutomationPrefs;

impl AutomationPrefs {
    /// Default automation mode dropdown - Class: ComboBox
    pub const DEFAULT_MODE: ChildId = ChildId(1000);
    /// Mode label - Class: Static
    pub const MODE_LABEL: ChildId = ChildId(1001);
    /// Reduce envelope point density when recording - Class: Button
    pub const REDUCE_POINT_DENSITY: ChildId = ChildId(1002);
    /// Automation thinning threshold inputbox - Class: Edit
    pub const THINNING_THRESHOLD: ChildId = ChildId(1003);
    /// Thinning label - Class: Static
    pub const THINNING_LABEL: ChildId = ChildId(1004);
    /// Write automation on touch - Class: Button
    pub const WRITE_ON_TOUCH: ChildId = ChildId(1005);
    /// Auto-add envelopes when writing - Class: Button
    pub const AUTO_ADD_ENVELOPES: ChildId = ChildId(1006);
    /// Smooth automation transitions - Class: Button
    pub const SMOOTH_TRANSITIONS: ChildId = ChildId(1007);
    /// Automation recording label - Class: Static
    pub const RECORDING_LABEL: ChildId = ChildId(1100);
}
