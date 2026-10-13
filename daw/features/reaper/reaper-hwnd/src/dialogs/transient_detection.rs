//! Transient Detection Settings dialog child window IDs.

use crate::ChildId;

/// Transient Detection Settings dialog child window IDs.
pub struct TransientDetection;

impl TransientDetection {
    /// OK - Class: Button
    pub const OK: ChildId = ChildId(1);
    /// Cancel - Class: Button
    pub const CANCEL: ChildId = ChildId(2);
    /// Sensitivity inputbox - Class: Edit
    pub const SENSITIVITY: ChildId = ChildId(1000);
    /// Sensitivity label - Class: Static
    pub const SENSITIVITY_LABEL: ChildId = ChildId(1001);
    /// Sensitivity slider - Class: msctls_trackbar32
    pub const SENSITIVITY_SLIDER: ChildId = ChildId(1002);
    /// Threshold inputbox - Class: Edit
    pub const THRESHOLD: ChildId = ChildId(1003);
    /// Threshold label - Class: Static
    pub const THRESHOLD_LABEL: ChildId = ChildId(1004);
    /// Threshold slider - Class: msctls_trackbar32
    pub const THRESHOLD_SLIDER: ChildId = ChildId(1005);
    /// Zero-crossing snap - Class: Button
    pub const ZERO_CROSSING_SNAP: ChildId = ChildId(1006);
    /// Min slice length inputbox - Class: Edit
    pub const MIN_SLICE_LENGTH: ChildId = ChildId(1007);
    /// Min slice label - Class: Static
    pub const MIN_SLICE_LABEL: ChildId = ChildId(1008);
    /// Preview button - Class: Button
    pub const PREVIEW: ChildId = ChildId(1009);
    /// Reset to defaults - Class: Button
    pub const RESET_DEFAULTS: ChildId = ChildId(1010);
    /// Detection settings label - Class: Static
    pub const DETECTION_LABEL: ChildId = ChildId(1100);
}
