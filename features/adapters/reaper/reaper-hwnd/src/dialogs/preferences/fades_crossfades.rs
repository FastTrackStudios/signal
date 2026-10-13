//! Preferences -> Appearance -> Fades/Crossfades page child window IDs.

use crate::ChildId;

/// Preferences -> Appearance -> Fades/Crossfades page child window IDs.
pub struct FadesCrossfadesPrefs;

impl FadesCrossfadesPrefs {
    /// Show fades in items - Class: Button
    pub const SHOW_FADES: ChildId = ChildId(1000);
    /// Show crossfades - Class: Button
    pub const SHOW_CROSSFADES: ChildId = ChildId(1001);
    /// Fade display opacity inputbox - Class: Edit
    pub const FADE_OPACITY: ChildId = ChildId(1002);
    /// Fade opacity label - Class: Static
    pub const FADE_OPACITY_LABEL: ChildId = ChildId(1003);
    /// Crossfade display style dropdown - Class: ComboBox
    pub const CROSSFADE_STYLE: ChildId = ChildId(1004);
    /// Crossfade style label - Class: Static
    pub const CROSSFADE_STYLE_LABEL: ChildId = ChildId(1005);
}
