//! Rendering Progress dialog child window IDs.

use crate::ChildId;

/// Rendering Progress dialog child window IDs.
pub struct RenderingProgress;

impl RenderingProgress {
    /// Close button - Class: Button
    pub const CLOSE: ChildId = ChildId(1);
    /// Cancel button - Class: Button
    pub const CANCEL: ChildId = ChildId(2);
    /// Output filename path - Class: Edit
    pub const OUTPUT_FILENAME: ChildId = ChildId(1007);
    /// Time elapsed/error status - Class: Static
    pub const TIME_ELAPSED: ChildId = ChildId(1036);
    /// Render status area - Class: Button
    pub const RENDER_STATUS: ChildId = ChildId(1039);
    /// Output format/error - Class: Static
    pub const OUTPUT_FORMAT: ChildId = ChildId(1040);
    /// Auto close when finished - Class: Button
    pub const AUTO_CLOSE: ChildId = ChildId(1042);
    /// Peaks area - Class: REAPERminipeaks
    pub const PEAKS: ChildId = ChildId(1165);
    /// VU channel 1 - Class: REAPERhorzvu
    pub const VU_CHANNEL_1: ChildId = ChildId(1166);
    /// VU channel 2 - Class: REAPERhorzvu
    pub const VU_CHANNEL_2: ChildId = ChildId(1167);
    /// VU channel 3 - Class: REAPERhorzvu
    pub const VU_CHANNEL_3: ChildId = ChildId(1168);
    /// VU channel 4 - Class: REAPERhorzvu
    pub const VU_CHANNEL_4: ChildId = ChildId(1169);
    /// VU channel 5 - Class: REAPERhorzvu
    pub const VU_CHANNEL_5: ChildId = ChildId(1170);
    /// VU channel 6 - Class: REAPERhorzvu
    pub const VU_CHANNEL_6: ChildId = ChildId(1171);
    /// VU channel 7 - Class: REAPERhorzvu
    pub const VU_CHANNEL_7: ChildId = ChildId(1172);
    /// VU channel 8 - Class: REAPERhorzvu
    pub const VU_CHANNEL_8: ChildId = ChildId(1173);
    /// VU channel 9 - Class: REAPERhorzvu
    pub const VU_CHANNEL_9: ChildId = ChildId(1174);
    /// VU channel 10 - Class: REAPERhorzvu
    pub const VU_CHANNEL_10: ChildId = ChildId(1175);
    /// VU channel 11 - Class: REAPERhorzvu
    pub const VU_CHANNEL_11: ChildId = ChildId(1176);
    /// VU channel 12 - Class: REAPERhorzvu
    pub const VU_CHANNEL_12: ChildId = ChildId(1177);
    /// VU channel 13 - Class: REAPERhorzvu
    pub const VU_CHANNEL_13: ChildId = ChildId(1178);
    /// VU channel 14 - Class: REAPERhorzvu
    pub const VU_CHANNEL_14: ChildId = ChildId(1179);
    /// VU channel 15 - Class: REAPERhorzvu
    pub const VU_CHANNEL_15: ChildId = ChildId(1180);
    /// VU channel 16 - Class: REAPERhorzvu
    pub const VU_CHANNEL_16: ChildId = ChildId(1181);
    /// VU channel 17 - Class: REAPERhorzvu
    pub const VU_CHANNEL_17: ChildId = ChildId(1182);
    /// VU channel 18 - Class: REAPERhorzvu
    pub const VU_CHANNEL_18: ChildId = ChildId(1183);
    /// VU channel 19 - Class: REAPERhorzvu
    pub const VU_CHANNEL_19: ChildId = ChildId(1184);
    /// VU channel 20 - Class: REAPERhorzvu
    pub const VU_CHANNEL_20: ChildId = ChildId(1185);
    /// VU channel 21 - Class: REAPERhorzvu
    pub const VU_CHANNEL_21: ChildId = ChildId(1186);
    /// VU channel 22 - Class: REAPERhorzvu
    pub const VU_CHANNEL_22: ChildId = ChildId(1187);
    /// VU channel 23 - Class: REAPERhorzvu
    pub const VU_CHANNEL_23: ChildId = ChildId(1188);
    /// VU channel 24 - Class: REAPERhorzvu
    pub const VU_CHANNEL_24: ChildId = ChildId(1189);
    /// Launch file button - Class: Button
    pub const LAUNCH_FILE: ChildId = ChildId(1228);
    /// Show in explorer button - Class: Button
    pub const SHOW_IN_EXPLORER: ChildId = ChildId(1229);
    /// Shup file button - Class: Button
    pub const SHUP_FILE: ChildId = ChildId(1230);
    /// Output file button - Class: Button
    pub const OUTPUT_FILE: ChildId = ChildId(40249);
}
