//! Preferences dialog and sub-page child window IDs.

pub mod appearance;
pub mod audio;
pub mod automation;
pub mod buffering;
pub mod compatibility;
pub mod control_osc;
pub mod device;
pub mod editing_behavior;
pub mod envelope_display;
pub mod external_editors;
pub mod fades_crossfades;
pub mod general;
pub mod keyboard;
pub mod loop_recording;
pub mod media;
pub mod media_item_defaults;
pub mod midi;
pub mod midi_devices;
pub mod midi_editor;
pub mod mouse;
pub mod mouse_modifiers;
pub mod mute_solo;
pub mod paths;
pub mod peaks_waveforms;
pub mod playback;
pub mod plugins;
pub mod project;
pub mod reamote;
pub mod reascript;
pub mod recording;
pub mod rendering;
pub mod rewire_dx;
pub mod seeking;
pub mod track_control;
pub mod track_send_defaults;
pub mod video;
pub mod vst;

use crate::ChildId;

/// Preferences dialog (top-level) child window IDs.
pub struct PreferencesDialog;

impl PreferencesDialog {
    /// Current page HWND - Class: #32770
    pub const CURRENT_PAGE: ChildId = ChildId(0);
    /// OK - Class: Button
    pub const OK: ChildId = ChildId(1);
    /// Cancel - Class: Button
    pub const CANCEL: ChildId = ChildId(2);
    /// Category tree - Class: SysTreeView32
    pub const CATEGORY_TREE: ChildId = ChildId(1110);
    /// Static - Class: Static
    pub const STATIC_1: ChildId = ChildId(1111);
    /// Apply - Class: Button
    pub const APPLY: ChildId = ChildId(1144);
    /// Static - Class: Static
    pub const STATIC_2: ChildId = ChildId(1259);
    /// Find button - Class: Button
    pub const FIND: ChildId = ChildId(1311);
    /// Find inputbox - Class: Edit
    pub const FIND_INPUT: ChildId = ChildId(1312);
}
