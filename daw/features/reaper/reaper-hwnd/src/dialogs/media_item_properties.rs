//! Media Item Properties dialog child window IDs.

use crate::ChildId;

/// Media Item Properties dialog child window IDs.
pub struct MediaItemProperties;

impl MediaItemProperties {
    /// OK - Class: Button
    pub const OK: ChildId = ChildId(1);
    /// Cancel - Class: Button
    pub const CANCEL: ChildId = ChildId(2);
    /// Position inputbox - Class: Edit
    pub const POSITION: ChildId = ChildId(1000);
    /// Position label - Class: Static
    pub const POSITION_LABEL: ChildId = ChildId(1001);
    /// Length inputbox - Class: Edit
    pub const LENGTH: ChildId = ChildId(1002);
    /// Length label - Class: Static
    pub const LENGTH_LABEL: ChildId = ChildId(1003);
    /// Offset inputbox - Class: Edit
    pub const OFFSET: ChildId = ChildId(1004);
    /// Offset label - Class: Static
    pub const OFFSET_LABEL: ChildId = ChildId(1005);
    /// Fade-in length inputbox - Class: Edit
    pub const FADE_IN: ChildId = ChildId(1006);
    /// Fade-in label - Class: Static
    pub const FADE_IN_LABEL: ChildId = ChildId(1007);
    /// Fade-out length inputbox - Class: Edit
    pub const FADE_OUT: ChildId = ChildId(1008);
    /// Fade-out label - Class: Static
    pub const FADE_OUT_LABEL: ChildId = ChildId(1009);
    /// Fade-in shape dropdown - Class: ComboBox
    pub const FADE_IN_SHAPE: ChildId = ChildId(1010);
    /// Fade-out shape dropdown - Class: ComboBox
    pub const FADE_OUT_SHAPE: ChildId = ChildId(1011);
    /// Volume inputbox - Class: Edit
    pub const VOLUME: ChildId = ChildId(1012);
    /// Volume label - Class: Static
    pub const VOLUME_LABEL: ChildId = ChildId(1013);
    /// Pan inputbox - Class: Edit
    pub const PAN: ChildId = ChildId(1014);
    /// Pan label - Class: Static
    pub const PAN_LABEL: ChildId = ChildId(1015);
    /// Mute checkbox - Class: Button
    pub const MUTE: ChildId = ChildId(1016);
    /// Lock checkbox - Class: Button
    pub const LOCK: ChildId = ChildId(1017);
    /// Loop source checkbox - Class: Button
    pub const LOOP_SOURCE: ChildId = ChildId(1018);
    /// Item name inputbox - Class: Edit
    pub const ITEM_NAME: ChildId = ChildId(1019);
    /// Item name label - Class: Static
    pub const ITEM_NAME_LABEL: ChildId = ChildId(1020);
    /// Take name inputbox - Class: Edit
    pub const TAKE_NAME: ChildId = ChildId(1021);
    /// Take name label - Class: Static
    pub const TAKE_NAME_LABEL: ChildId = ChildId(1022);
    /// Source file inputbox - Class: Edit
    pub const SOURCE_FILE: ChildId = ChildId(1023);
    /// Source file label - Class: Static
    pub const SOURCE_FILE_LABEL: ChildId = ChildId(1024);
    /// Browse source file - Class: Button
    pub const BROWSE_SOURCE: ChildId = ChildId(1025);
    /// Playback rate inputbox - Class: Edit
    pub const PLAYBACK_RATE: ChildId = ChildId(1026);
    /// Playback rate label - Class: Static
    pub const PLAYBACK_RATE_LABEL: ChildId = ChildId(1027);
    /// Pitch shift inputbox - Class: Edit
    pub const PITCH_SHIFT: ChildId = ChildId(1028);
    /// Pitch shift label - Class: Static
    pub const PITCH_SHIFT_LABEL: ChildId = ChildId(1029);
    /// Pitch shift mode dropdown - Class: ComboBox
    pub const PITCH_SHIFT_MODE: ChildId = ChildId(1030);
    /// Preserve pitch when changing rate - Class: Button
    pub const PRESERVE_PITCH: ChildId = ChildId(1031);
    /// Channel mode dropdown - Class: ComboBox
    pub const CHANNEL_MODE: ChildId = ChildId(1032);
    /// Channel mode label - Class: Static
    pub const CHANNEL_MODE_LABEL: ChildId = ChildId(1033);
    /// Take properties label - Class: Static
    pub const TAKE_PROPERTIES_LABEL: ChildId = ChildId(1100);
    /// Item properties label - Class: Static
    pub const ITEM_PROPERTIES_LABEL: ChildId = ChildId(1101);
    /// Snap offset inputbox - Class: Edit
    pub const SNAP_OFFSET: ChildId = ChildId(1200);
    /// Snap offset label - Class: Static
    pub const SNAP_OFFSET_LABEL: ChildId = ChildId(1201);
    /// Item color button - Class: Button
    pub const ITEM_COLOR: ChildId = ChildId(1300);
    /// Previous take button - Class: Button
    pub const PREV_TAKE: ChildId = ChildId(1400);
    /// Next take button - Class: Button
    pub const NEXT_TAKE: ChildId = ChildId(1401);
    /// Apply - Class: Button
    pub const APPLY: ChildId = ChildId(1144);
}
