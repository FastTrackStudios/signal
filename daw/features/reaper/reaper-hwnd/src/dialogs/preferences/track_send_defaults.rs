//! Preferences -> Track/Send Defaults page child window IDs.

use crate::ChildId;

/// Preferences -> Track/Send Defaults page child window IDs.
pub struct TrackSendDefaultsPrefs;

impl TrackSendDefaultsPrefs {
    /// Default track channel count dropdown - Class: ComboBox
    pub const DEFAULT_TRACK_CHANNELS: ChildId = ChildId(1000);
    /// Default send/receive channel count dropdown - Class: ComboBox
    pub const DEFAULT_SEND_CHANNELS: ChildId = ChildId(1001);
    /// Record mode for new tracks dropdown - Class: ComboBox
    pub const RECORD_MODE: ChildId = ChildId(1002);
    /// Track label - Class: Static
    pub const TRACK_LABEL: ChildId = ChildId(1003);
    /// New track default volume inputbox - Class: Edit
    pub const DEFAULT_VOLUME: ChildId = ChildId(1004);
    /// New track default pan inputbox - Class: Edit
    pub const DEFAULT_PAN: ChildId = ChildId(1005);
    /// Automatic record arm when track selected - Class: Button
    pub const AUTO_RECORD_ARM: ChildId = ChildId(1006);
    /// Set track name from first FX - Class: Button
    pub const NAME_FROM_FIRST_FX: ChildId = ChildId(1007);
    /// Send volume/pan label - Class: Static
    pub const SEND_VOLUME_PAN_LABEL: ChildId = ChildId(1008);
}
