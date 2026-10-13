//! Preferences -> Envelope Display page child window IDs.

use crate::ChildId;

/// Preferences -> Envelope Display page child window IDs.
pub struct EnvelopeDisplayPrefs;

impl EnvelopeDisplayPrefs {
    /// Show envelope values on mouseover - Class: Button
    pub const SHOW_VALUES_ON_MOUSEOVER: ChildId = ChildId(1000);
    /// Show envelope point names - Class: Button
    pub const SHOW_POINT_NAMES: ChildId = ChildId(1001);
    /// Default envelope point shape dropdown - Class: ComboBox
    pub const DEFAULT_POINT_SHAPE: ChildId = ChildId(1002);
    /// Point shape label - Class: Static
    pub const POINT_SHAPE_LABEL: ChildId = ChildId(1003);
    /// Envelope lane height inputbox - Class: Edit
    pub const LANE_HEIGHT: ChildId = ChildId(1004);
    /// Lane height label - Class: Static
    pub const LANE_HEIGHT_LABEL: ChildId = ChildId(1005);
    /// Show fader-scaled envelope values - Class: Button
    pub const FADER_SCALED_VALUES: ChildId = ChildId(1006);
    /// Envelope opacity inputbox - Class: Edit
    pub const OPACITY: ChildId = ChildId(1007);
    /// Opacity label - Class: Static
    pub const OPACITY_LABEL: ChildId = ChildId(1008);
    /// Reduce envelope point size at low zoom - Class: Button
    pub const REDUCE_POINT_SIZE_LOW_ZOOM: ChildId = ChildId(1009);
    /// Envelope display label - Class: Static
    pub const ENVELOPE_DISPLAY_LABEL: ChildId = ChildId(1100);
}
