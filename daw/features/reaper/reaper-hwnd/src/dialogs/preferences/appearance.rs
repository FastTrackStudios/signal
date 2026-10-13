//! Preferences -> Appearance page child window IDs.

use crate::ChildId;

/// Preferences -> Appearance page child window IDs.
pub struct AppearancePrefs;

impl AppearancePrefs {
    /// Item label gap inputbox - Class: Edit
    pub const ITEM_LABEL_GAP: ChildId = ChildId(1007);
    /// Show track lanes - Class: Button
    pub const SHOW_TRACK_LANES: ChildId = ChildId(1008);
    /// Cursor width inputbox - Class: Edit
    pub const CURSOR_WIDTH: ChildId = ChildId(1009);
    /// Show grid lines in arrange - Class: Button
    pub const SHOW_GRID_LINES: ChildId = ChildId(1010);
    /// Grid line opacity inputbox - Class: Edit
    pub const GRID_LINE_OPACITY: ChildId = ChildId(1011);
    /// Show tooltips - Class: Button
    pub const SHOW_TOOLTIPS: ChildId = ChildId(1012);
    /// Show envelope point values - Class: Button
    pub const SHOW_ENVELOPE_VALUES: ChildId = ChildId(1013);
    /// Theme dropdown - Class: ComboBox
    pub const THEME: ChildId = ChildId(1014);
    /// Browse theme button - Class: Button
    pub const BROWSE_THEME: ChildId = ChildId(1015);
    /// Theme label - Class: Static
    pub const THEME_LABEL: ChildId = ChildId(1016);
    /// Track height default inputbox - Class: Edit
    pub const TRACK_HEIGHT_DEFAULT: ChildId = ChildId(1017);
    /// Track height label - Class: Static
    pub const TRACK_HEIGHT_LABEL: ChildId = ChildId(1018);
    /// Show item edges - Class: Button
    pub const SHOW_ITEM_EDGES: ChildId = ChildId(1019);
    /// Tint track panels - Class: Button
    pub const TINT_TRACK_PANELS: ChildId = ChildId(1020);
    /// Display items in mono - Class: Button
    pub const DISPLAY_MONO: ChildId = ChildId(1021);
    /// Continuous scrolling in arrange - Class: Button
    pub const CONTINUOUS_SCROLLING: ChildId = ChildId(1022);
    /// Show docked toolbar handles - Class: Button
    pub const SHOW_TOOLBAR_HANDLES: ChildId = ChildId(1100);
    /// Show main menu bar - Class: Button
    pub const SHOW_MAIN_MENU: ChildId = ChildId(1101);
    /// Reset color theme to default - Class: Button
    pub const RESET_THEME: ChildId = ChildId(1200);
    /// Track spacing inputbox - Class: Edit
    pub const TRACK_SPACING: ChildId = ChildId(1300);
    /// Track spacing label - Class: Static
    pub const TRACK_SPACING_LABEL: ChildId = ChildId(1301);
    /// UI scaling dropdown - Class: ComboBox
    pub const UI_SCALING: ChildId = ChildId(1400);
    /// UI scaling label - Class: Static
    pub const UI_SCALING_LABEL: ChildId = ChildId(1401);
    /// Appearance settings label - Class: Static
    pub const APPEARANCE_LABEL: ChildId = ChildId(1837);
}
