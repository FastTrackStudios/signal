//! Traced theme artwork — GENERATED, do not edit.
//!
//! ```sh
//! cargo run -p daw-theme-art --example codegen
//! ```
//!
//! Geometry is traced from the source art exactly; colour is
//! reinterpreted at render time via the theme ramp — so a palette
//! change does NOT require regenerating this, only a change to the
//! source artwork does.
//!
//! 1021 images, 337897 rects, 3.9 MB of packed rect data.

use crate::art_data::ArtData;

/// Packed rects: x, y, w, h as u16 LE, then rgba.
pub static BLOB: &[u8] = include_bytes!("art.bin");

/// `animation_toolbar_armed.png` — 30x360, 2263 rects, 1 sprite cell(s).
pub const ANIMATION_TOOLBAR_ARMED: ArtData = ArtData {
    name: "animation_toolbar_armed",
    width: 30,
    height: 360,
    offset: 0,
    count: 2263,
    cells: 1,
    blob: BLOB,
};
/// `animation_toolbar_highlight.png` — 30x360, 6500 rects, 1 sprite cell(s).
pub const ANIMATION_TOOLBAR_HIGHLIGHT: ArtData = ArtData {
    name: "animation_toolbar_highlight",
    width: 30,
    height: 360,
    offset: 27156,
    count: 6500,
    cells: 1,
    blob: BLOB,
};
/// `custom_comping.png` — 60x15, 108 rects, 4 sprite cell(s).
pub const CUSTOM_COMPING: ArtData = ArtData {
    name: "custom_comping",
    width: 60,
    height: 15,
    offset: 105156,
    count: 108,
    cells: 4,
    blob: BLOB,
};
/// `custom_envcp_arm_bg.png` — 22x22, 48 rects, 1 sprite cell(s).
pub const CUSTOM_ENVCP_ARM_BG: ArtData = ArtData {
    name: "custom_envcp_arm_bg",
    width: 22,
    height: 22,
    offset: 106452,
    count: 48,
    cells: 1,
    blob: BLOB,
};
/// `custom_fixed_lanes_off.png` — 60x24, 102 rects, 3 sprite cell(s).
pub const CUSTOM_FIXED_LANES_OFF: ArtData = ArtData {
    name: "custom_fixed_lanes_off",
    width: 60,
    height: 24,
    offset: 107028,
    count: 102,
    cells: 3,
    blob: BLOB,
};
/// `custom_fixed_lanes_on.png` — 60x24, 102 rects, 3 sprite cell(s).
pub const CUSTOM_FIXED_LANES_ON: ArtData = ArtData {
    name: "custom_fixed_lanes_on",
    width: 60,
    height: 24,
    offset: 108252,
    count: 102,
    cells: 3,
    blob: BLOB,
};
/// `custom_master_track_pin_off.png` — 57x19, 142 rects, 3 sprite cell(s).
pub const CUSTOM_MASTER_TRACK_PIN_OFF: ArtData = ArtData {
    name: "custom_master_track_pin_off",
    width: 57,
    height: 19,
    offset: 109476,
    count: 142,
    cells: 3,
    blob: BLOB,
};
/// `custom_master_track_pin_on.png` — 57x19, 143 rects, 3 sprite cell(s).
pub const CUSTOM_MASTER_TRACK_PIN_ON: ArtData = ArtData {
    name: "custom_master_track_pin_on",
    width: 57,
    height: 19,
    offset: 111180,
    count: 143,
    cells: 3,
    blob: BLOB,
};
/// `custom_mcp_folder_1-1.png` — 16x21, 48 rects, 1 sprite cell(s).
pub const CUSTOM_MCP_FOLDER_1_1: ArtData = ArtData {
    name: "custom_mcp_folder_1-1",
    width: 16,
    height: 21,
    offset: 112896,
    count: 48,
    cells: 1,
    blob: BLOB,
};
/// `custom_mcp_folder_1-2.png` — 16x21, 43 rects, 1 sprite cell(s).
pub const CUSTOM_MCP_FOLDER_1_2: ArtData = ArtData {
    name: "custom_mcp_folder_1-2",
    width: 16,
    height: 21,
    offset: 113472,
    count: 43,
    cells: 1,
    blob: BLOB,
};
/// `custom_mcp_folder_1-4.png` — 16x21, 33 rects, 1 sprite cell(s).
pub const CUSTOM_MCP_FOLDER_1_4: ArtData = ArtData {
    name: "custom_mcp_folder_1-4",
    width: 16,
    height: 21,
    offset: 113988,
    count: 33,
    cells: 1,
    blob: BLOB,
};
/// `custom_mcp_folder_1-8.png` — 16x21, 24 rects, 1 sprite cell(s).
pub const CUSTOM_MCP_FOLDER_1_8: ArtData = ArtData {
    name: "custom_mcp_folder_1-8",
    width: 16,
    height: 21,
    offset: 114384,
    count: 24,
    cells: 1,
    blob: BLOB,
};
/// `custom_mcp_folder_start.png` — 11x20, 24 rects, 1 sprite cell(s).
pub const CUSTOM_MCP_FOLDER_START: ArtData = ArtData {
    name: "custom_mcp_folder_start",
    width: 11,
    height: 20,
    offset: 114672,
    count: 24,
    cells: 1,
    blob: BLOB,
};
/// `custom_mcp_sel_gradient.png` — 3x42, 27 rects, 1 sprite cell(s).
pub const CUSTOM_MCP_SEL_GRADIENT: ArtData = ArtData {
    name: "custom_mcp_sel_gradient",
    width: 3,
    height: 42,
    offset: 114960,
    count: 27,
    cells: 1,
    blob: BLOB,
};
/// `custom_tcp_namebg.png` — 24x24, 43 rects, 1 sprite cell(s).
pub const CUSTOM_TCP_NAMEBG: ArtData = ArtData {
    name: "custom_tcp_namebg",
    width: 24,
    height: 24,
    offset: 115284,
    count: 43,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_divider.png` — 1x1, 0 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_DIVIDER: ArtData = ArtData {
    name: "custom_track_divider",
    width: 1,
    height: 1,
    offset: 115800,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_folder_1-1.png` — 18x14, 43 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_FOLDER_1_1: ArtData = ArtData {
    name: "custom_track_folder_1-1",
    width: 18,
    height: 14,
    offset: 115800,
    count: 43,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_folder_1-2.png` — 18x14, 31 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_FOLDER_1_2: ArtData = ArtData {
    name: "custom_track_folder_1-2",
    width: 18,
    height: 14,
    offset: 116316,
    count: 31,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_folder_1-4.png` — 18x14, 23 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_FOLDER_1_4: ArtData = ArtData {
    name: "custom_track_folder_1-4",
    width: 18,
    height: 14,
    offset: 116688,
    count: 23,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_folder_1-8.png` — 18x14, 19 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_FOLDER_1_8: ArtData = ArtData {
    name: "custom_track_folder_1-8",
    width: 18,
    height: 14,
    offset: 116964,
    count: 19,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_folder_half_1-1.png` — 18x14, 29 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_FOLDER_HALF_1_1: ArtData = ArtData {
    name: "custom_track_folder_half_1-1",
    width: 18,
    height: 14,
    offset: 117192,
    count: 29,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_folder_half_1-2.png` — 18x14, 21 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_FOLDER_HALF_1_2: ArtData = ArtData {
    name: "custom_track_folder_half_1-2",
    width: 18,
    height: 14,
    offset: 117540,
    count: 21,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_folder_half_1-4.png` — 18x14, 17 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_FOLDER_HALF_1_4: ArtData = ArtData {
    name: "custom_track_folder_half_1-4",
    width: 18,
    height: 14,
    offset: 117792,
    count: 17,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_folder_half_1-8.png` — 18x14, 13 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_FOLDER_HALF_1_8: ArtData = ArtData {
    name: "custom_track_folder_half_1-8",
    width: 18,
    height: 14,
    offset: 117996,
    count: 13,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_folder_recarm.png` — 20x20, 48 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_FOLDER_RECARM: ArtData = ArtData {
    name: "custom_track_folder_recarm",
    width: 20,
    height: 20,
    offset: 118152,
    count: 48,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_io_darker.png` — 30x22, 99 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_IO_DARKER: ArtData = ArtData {
    name: "custom_track_io_darker",
    width: 30,
    height: 22,
    offset: 118728,
    count: 99,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_io_text_off.png` — 32x22, 46 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_IO_TEXT_OFF: ArtData = ArtData {
    name: "custom_track_io_text_off",
    width: 32,
    height: 22,
    offset: 119916,
    count: 46,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_io_text_on.png` — 32x22, 46 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_IO_TEXT_ON: ArtData = ArtData {
    name: "custom_track_io_text_on",
    width: 32,
    height: 22,
    offset: 120468,
    count: 46,
    cells: 1,
    blob: BLOB,
};
/// `custom_track_pin_off.png` — 57x19, 142 rects, 3 sprite cell(s).
pub const CUSTOM_TRACK_PIN_OFF: ArtData = ArtData {
    name: "custom_track_pin_off",
    width: 57,
    height: 19,
    offset: 121020,
    count: 142,
    cells: 3,
    blob: BLOB,
};
/// `custom_track_pin_on.png` — 57x19, 188 rects, 3 sprite cell(s).
pub const CUSTOM_TRACK_PIN_ON: ArtData = ArtData {
    name: "custom_track_pin_on",
    width: 57,
    height: 19,
    offset: 122724,
    count: 188,
    cells: 3,
    blob: BLOB,
};
/// `custom_track_recarm_bg.png` — 24x24, 50 rects, 1 sprite cell(s).
pub const CUSTOM_TRACK_RECARM_BG: ArtData = ArtData {
    name: "custom_track_recarm_bg",
    width: 24,
    height: 24,
    offset: 124980,
    count: 50,
    cells: 1,
    blob: BLOB,
};
/// `custom_transport_edit_bg.png` — 32x32, 77 rects, 1 sprite cell(s).
pub const CUSTOM_TRANSPORT_EDIT_BG: ArtData = ArtData {
    name: "custom_transport_edit_bg",
    width: 32,
    height: 32,
    offset: 125580,
    count: 77,
    cells: 1,
    blob: BLOB,
};
/// `custom_transport_edit_div.png` — 1x1, 1 rects, 1 sprite cell(s).
pub const CUSTOM_TRANSPORT_EDIT_DIV: ArtData = ArtData {
    name: "custom_transport_edit_div",
    width: 1,
    height: 1,
    offset: 126504,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `custom_transport_sel_end.png` — 6x6, 11 rects, 1 sprite cell(s).
pub const CUSTOM_TRANSPORT_SEL_END: ArtData = ArtData {
    name: "custom_transport_sel_end",
    width: 6,
    height: 6,
    offset: 126516,
    count: 11,
    cells: 1,
    blob: BLOB,
};
/// `custom_transport_sel_start.png` — 6x6, 11 rects, 1 sprite cell(s).
pub const CUSTOM_TRANSPORT_SEL_START: ArtData = ArtData {
    name: "custom_transport_sel_start",
    width: 6,
    height: 6,
    offset: 126648,
    count: 11,
    cells: 1,
    blob: BLOB,
};
/// `envcp_arm_off.png` — 60x20, 416 rects, 3 sprite cell(s).
pub const ENVCP_ARM_OFF: ArtData = ArtData {
    name: "envcp_arm_off",
    width: 60,
    height: 20,
    offset: 126780,
    count: 416,
    cells: 3,
    blob: BLOB,
};
/// `envcp_arm_on.png` — 60x20, 663 rects, 3 sprite cell(s).
pub const ENVCP_ARM_ON: ArtData = ArtData {
    name: "envcp_arm_on",
    width: 60,
    height: 20,
    offset: 131772,
    count: 663,
    cells: 3,
    blob: BLOB,
};
/// `envcp_bg.png` — 48x12, 4 rects, 4 sprite cell(s).
pub const ENVCP_BG: ArtData = ArtData {
    name: "envcp_bg",
    width: 48,
    height: 12,
    offset: 139728,
    count: 4,
    cells: 4,
    blob: BLOB,
};
/// `envcp_bgsel.png` — 48x12, 7 rects, 4 sprite cell(s).
pub const ENVCP_BGSEL: ArtData = ArtData {
    name: "envcp_bgsel",
    width: 48,
    height: 12,
    offset: 139776,
    count: 7,
    cells: 4,
    blob: BLOB,
};
/// `envcp_bypass_off.png` — 45x20, 189 rects, 3 sprite cell(s).
pub const ENVCP_BYPASS_OFF: ArtData = ArtData {
    name: "envcp_bypass_off",
    width: 45,
    height: 20,
    offset: 139860,
    count: 189,
    cells: 3,
    blob: BLOB,
};
/// `envcp_bypass_on.png` — 45x20, 189 rects, 3 sprite cell(s).
pub const ENVCP_BYPASS_ON: ArtData = ArtData {
    name: "envcp_bypass_on",
    width: 45,
    height: 20,
    offset: 142128,
    count: 189,
    cells: 3,
    blob: BLOB,
};
/// `envcp_fader.png` — 23x29, 108 rects, 1 sprite cell(s).
pub const ENVCP_FADER: ArtData = ArtData {
    name: "envcp_fader",
    width: 23,
    height: 29,
    offset: 144396,
    count: 108,
    cells: 1,
    blob: BLOB,
};
/// `envcp_faderbg.png` — 19x24, 19 rects, 1 sprite cell(s).
pub const ENVCP_FADERBG: ArtData = ArtData {
    name: "envcp_faderbg",
    width: 19,
    height: 24,
    offset: 145692,
    count: 19,
    cells: 1,
    blob: BLOB,
};
/// `envcp_hide.png` — 108x20, 531 rects, 3 sprite cell(s).
pub const ENVCP_HIDE: ArtData = ArtData {
    name: "envcp_hide",
    width: 108,
    height: 20,
    offset: 145920,
    count: 531,
    cells: 3,
    blob: BLOB,
};
/// `envcp_knob_small.png` — 25x26, 188 rects, 1 sprite cell(s).
pub const ENVCP_KNOB_SMALL: ArtData = ArtData {
    name: "envcp_knob_small",
    width: 25,
    height: 26,
    offset: 152292,
    count: 188,
    cells: 1,
    blob: BLOB,
};
/// `envcp_learn.png` — 90x20, 422 rects, 1 sprite cell(s).
pub const ENVCP_LEARN: ArtData = ArtData {
    name: "envcp_learn",
    width: 90,
    height: 20,
    offset: 154548,
    count: 422,
    cells: 1,
    blob: BLOB,
};
/// `envcp_learn_on.png` — 90x20, 453 rects, 3 sprite cell(s).
pub const ENVCP_LEARN_ON: ArtData = ArtData {
    name: "envcp_learn_on",
    width: 90,
    height: 20,
    offset: 159612,
    count: 453,
    cells: 3,
    blob: BLOB,
};
/// `envcp_namebg.png` — 22x24, 1 rects, 1 sprite cell(s).
pub const ENVCP_NAMEBG: ArtData = ArtData {
    name: "envcp_namebg",
    width: 22,
    height: 24,
    offset: 165048,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `envcp_parammod.png` — 90x20, 480 rects, 1 sprite cell(s).
pub const ENVCP_PARAMMOD: ArtData = ArtData {
    name: "envcp_parammod",
    width: 90,
    height: 20,
    offset: 165060,
    count: 480,
    cells: 1,
    blob: BLOB,
};
/// `envcp_parammod_on.png` — 90x20, 510 rects, 3 sprite cell(s).
pub const ENVCP_PARAMMOD_ON: ArtData = ArtData {
    name: "envcp_parammod_on",
    width: 90,
    height: 20,
    offset: 170820,
    count: 510,
    cells: 3,
    blob: BLOB,
};
/// `fixed_lanes_big.png` — 60x20, 210 rects, 3 sprite cell(s).
pub const FIXED_LANES_BIG: ArtData = ArtData {
    name: "fixed_lanes_big",
    width: 60,
    height: 20,
    offset: 176940,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `fixed_lanes_hidden.png` — 60x20, 27 rects, 3 sprite cell(s).
pub const FIXED_LANES_HIDDEN: ArtData = ArtData {
    name: "fixed_lanes_hidden",
    width: 60,
    height: 20,
    offset: 179460,
    count: 27,
    cells: 3,
    blob: BLOB,
};
/// `fixed_lanes_one.png` — 60x20, 198 rects, 3 sprite cell(s).
pub const FIXED_LANES_ONE: ArtData = ArtData {
    name: "fixed_lanes_one",
    width: 60,
    height: 20,
    offset: 179784,
    count: 198,
    cells: 3,
    blob: BLOB,
};
/// `fixed_lanes_small.png` — 60x20, 221 rects, 3 sprite cell(s).
pub const FIXED_LANES_SMALL: ArtData = ArtData {
    name: "fixed_lanes_small",
    width: 60,
    height: 20,
    offset: 182160,
    count: 221,
    cells: 3,
    blob: BLOB,
};
/// `folder_end.png` — 3x3, 0 rects, 1 sprite cell(s).
pub const FOLDER_END: ArtData = ArtData {
    name: "folder_end",
    width: 3,
    height: 3,
    offset: 184812,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `folder_indent.png` — 3x3, 0 rects, 1 sprite cell(s).
pub const FOLDER_INDENT: ArtData = ArtData {
    name: "folder_indent",
    width: 3,
    height: 3,
    offset: 184812,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `folder_start.png` — 3x3, 0 rects, 1 sprite cell(s).
pub const FOLDER_START: ArtData = ArtData {
    name: "folder_start",
    width: 3,
    height: 3,
    offset: 184812,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `gen_back.png` — 60x20, 264 rects, 4 sprite cell(s).
pub const GEN_BACK: ArtData = ArtData {
    name: "gen_back",
    width: 60,
    height: 20,
    offset: 184812,
    count: 264,
    cells: 4,
    blob: BLOB,
};
/// `gen_back_on.png` — 60x20, 261 rects, 4 sprite cell(s).
pub const GEN_BACK_ON: ArtData = ArtData {
    name: "gen_back_on",
    width: 60,
    height: 20,
    offset: 187980,
    count: 261,
    cells: 4,
    blob: BLOB,
};
/// `gen_end.png` — 72x24, 328 rects, 4 sprite cell(s).
pub const GEN_END: ArtData = ArtData {
    name: "gen_end",
    width: 72,
    height: 24,
    offset: 191112,
    count: 328,
    cells: 4,
    blob: BLOB,
};
/// `gen_env.png` — 60x20, 264 rects, 1 sprite cell(s).
pub const GEN_ENV: ArtData = ArtData {
    name: "gen_env",
    width: 60,
    height: 20,
    offset: 195048,
    count: 264,
    cells: 1,
    blob: BLOB,
};
/// `gen_env_latch.png` — 60x20, 926 rects, 1 sprite cell(s).
pub const GEN_ENV_LATCH: ArtData = ArtData {
    name: "gen_env_latch",
    width: 60,
    height: 20,
    offset: 198216,
    count: 926,
    cells: 1,
    blob: BLOB,
};
/// `gen_env_preview.png` — 60x20, 940 rects, 1 sprite cell(s).
pub const GEN_ENV_PREVIEW: ArtData = ArtData {
    name: "gen_env_preview",
    width: 60,
    height: 20,
    offset: 209328,
    count: 940,
    cells: 1,
    blob: BLOB,
};
/// `gen_env_read.png` — 60x20, 929 rects, 1 sprite cell(s).
pub const GEN_ENV_READ: ArtData = ArtData {
    name: "gen_env_read",
    width: 60,
    height: 20,
    offset: 220608,
    count: 929,
    cells: 1,
    blob: BLOB,
};
/// `gen_env_touch.png` — 60x20, 947 rects, 1 sprite cell(s).
pub const GEN_ENV_TOUCH: ArtData = ArtData {
    name: "gen_env_touch",
    width: 60,
    height: 20,
    offset: 231756,
    count: 947,
    cells: 1,
    blob: BLOB,
};
/// `gen_env_write.png` — 60x20, 927 rects, 1 sprite cell(s).
pub const GEN_ENV_WRITE: ArtData = ArtData {
    name: "gen_env_write",
    width: 60,
    height: 20,
    offset: 243120,
    count: 927,
    cells: 1,
    blob: BLOB,
};
/// `gen_forward.png` — 60x20, 261 rects, 4 sprite cell(s).
pub const GEN_FORWARD: ArtData = ArtData {
    name: "gen_forward",
    width: 60,
    height: 20,
    offset: 254244,
    count: 261,
    cells: 4,
    blob: BLOB,
};
/// `gen_forward_on.png` — 60x20, 261 rects, 4 sprite cell(s).
pub const GEN_FORWARD_ON: ArtData = ArtData {
    name: "gen_forward_on",
    width: 60,
    height: 20,
    offset: 257376,
    count: 261,
    cells: 4,
    blob: BLOB,
};
/// `gen_home.png` — 72x24, 326 rects, 4 sprite cell(s).
pub const GEN_HOME: ArtData = ArtData {
    name: "gen_home",
    width: 72,
    height: 24,
    offset: 260508,
    count: 326,
    cells: 4,
    blob: BLOB,
};
/// `gen_io.png` — 60x20, 528 rects, 3 sprite cell(s).
pub const GEN_IO: ArtData = ArtData {
    name: "gen_io",
    width: 60,
    height: 20,
    offset: 264420,
    count: 528,
    cells: 3,
    blob: BLOB,
};
/// `gen_knob_bg_small.png` — 18x20, 87 rects, 1 sprite cell(s).
pub const GEN_KNOB_BG_SMALL: ArtData = ArtData {
    name: "gen_knob_bg_small",
    width: 18,
    height: 20,
    offset: 270756,
    count: 87,
    cells: 1,
    blob: BLOB,
};
/// `gen_midi_off.png` — 60x20, 408 rects, 1 sprite cell(s).
pub const GEN_MIDI_OFF: ArtData = ArtData {
    name: "gen_midi_off",
    width: 60,
    height: 20,
    offset: 271800,
    count: 408,
    cells: 1,
    blob: BLOB,
};
/// `gen_midi_on.png` — 60x20, 879 rects, 4 sprite cell(s).
pub const GEN_MIDI_ON: ArtData = ArtData {
    name: "gen_midi_on",
    width: 60,
    height: 20,
    offset: 276696,
    count: 879,
    cells: 4,
    blob: BLOB,
};
/// `gen_mono.png` — 60x20, 939 rects, 4 sprite cell(s).
pub const GEN_MONO: ArtData = ArtData {
    name: "gen_mono",
    width: 60,
    height: 20,
    offset: 287244,
    count: 939,
    cells: 4,
    blob: BLOB,
};
/// `gen_mute_off.png` — 60x20, 337 rects, 1 sprite cell(s).
pub const GEN_MUTE_OFF: ArtData = ArtData {
    name: "gen_mute_off",
    width: 60,
    height: 20,
    offset: 298512,
    count: 337,
    cells: 1,
    blob: BLOB,
};
/// `gen_mute_on.png` — 60x20, 601 rects, 4 sprite cell(s).
pub const GEN_MUTE_ON: ArtData = ArtData {
    name: "gen_mute_on",
    width: 60,
    height: 20,
    offset: 302556,
    count: 601,
    cells: 4,
    blob: BLOB,
};
/// `gen_panbg_horz.png` — 24x22, 35 rects, 1 sprite cell(s).
pub const GEN_PANBG_HORZ: ArtData = ArtData {
    name: "gen_panbg_horz",
    width: 24,
    height: 22,
    offset: 309768,
    count: 35,
    cells: 1,
    blob: BLOB,
};
/// `gen_panbg_horz_dark.png` — 24x22, 35 rects, 1 sprite cell(s).
pub const GEN_PANBG_HORZ_DARK: ArtData = ArtData {
    name: "gen_panbg_horz_dark",
    width: 24,
    height: 22,
    offset: 310188,
    count: 35,
    cells: 1,
    blob: BLOB,
};
/// `gen_panthumb_horz.png` — 19x29, 110 rects, 1 sprite cell(s).
pub const GEN_PANTHUMB_HORZ: ArtData = ArtData {
    name: "gen_panthumb_horz",
    width: 19,
    height: 29,
    offset: 310608,
    count: 110,
    cells: 1,
    blob: BLOB,
};
/// `gen_pause.png` — 72x24, 279 rects, 4 sprite cell(s).
pub const GEN_PAUSE: ArtData = ArtData {
    name: "gen_pause",
    width: 72,
    height: 24,
    offset: 311928,
    count: 279,
    cells: 4,
    blob: BLOB,
};
/// `gen_pause_on.png` — 72x24, 157 rects, 4 sprite cell(s).
pub const GEN_PAUSE_ON: ArtData = ArtData {
    name: "gen_pause_on",
    width: 72,
    height: 24,
    offset: 315276,
    count: 157,
    cells: 4,
    blob: BLOB,
};
/// `gen_phase_inv.png` — 60x20, 501 rects, 4 sprite cell(s).
pub const GEN_PHASE_INV: ArtData = ArtData {
    name: "gen_phase_inv",
    width: 60,
    height: 20,
    offset: 317160,
    count: 501,
    cells: 4,
    blob: BLOB,
};
/// `gen_phase_norm.png` — 60x20, 429 rects, 1 sprite cell(s).
pub const GEN_PHASE_NORM: ArtData = ArtData {
    name: "gen_phase_norm",
    width: 60,
    height: 20,
    offset: 323172,
    count: 429,
    cells: 1,
    blob: BLOB,
};
/// `gen_play.png` — 72x24, 501 rects, 4 sprite cell(s).
pub const GEN_PLAY: ArtData = ArtData {
    name: "gen_play",
    width: 72,
    height: 24,
    offset: 328320,
    count: 501,
    cells: 4,
    blob: BLOB,
};
/// `gen_play_on.png` — 72x24, 1149 rects, 4 sprite cell(s).
pub const GEN_PLAY_ON: ArtData = ArtData {
    name: "gen_play_on",
    width: 72,
    height: 24,
    offset: 334332,
    count: 1149,
    cells: 4,
    blob: BLOB,
};
/// `gen_refresh.png` — 60x20, 383 rects, 4 sprite cell(s).
pub const GEN_REFRESH: ArtData = ArtData {
    name: "gen_refresh",
    width: 60,
    height: 20,
    offset: 348120,
    count: 383,
    cells: 4,
    blob: BLOB,
};
/// `gen_repeat_off.png` — 72x24, 501 rects, 4 sprite cell(s).
pub const GEN_REPEAT_OFF: ArtData = ArtData {
    name: "gen_repeat_off",
    width: 72,
    height: 24,
    offset: 352716,
    count: 501,
    cells: 4,
    blob: BLOB,
};
/// `gen_repeat_on.png` — 72x24, 747 rects, 4 sprite cell(s).
pub const GEN_REPEAT_ON: ArtData = ArtData {
    name: "gen_repeat_on",
    width: 72,
    height: 24,
    offset: 358728,
    count: 747,
    cells: 4,
    blob: BLOB,
};
/// `gen_solo_off.png` — 60x20, 360 rects, 1 sprite cell(s).
pub const GEN_SOLO_OFF: ArtData = ArtData {
    name: "gen_solo_off",
    width: 60,
    height: 20,
    offset: 367692,
    count: 360,
    cells: 1,
    blob: BLOB,
};
/// `gen_solo_on.png` — 60x19, 377 rects, 4 sprite cell(s).
pub const GEN_SOLO_ON: ArtData = ArtData {
    name: "gen_solo_on",
    width: 60,
    height: 19,
    offset: 372012,
    count: 377,
    cells: 4,
    blob: BLOB,
};
/// `gen_stereo.png` — 60x20, 519 rects, 1 sprite cell(s).
pub const GEN_STEREO: ArtData = ArtData {
    name: "gen_stereo",
    width: 60,
    height: 20,
    offset: 376536,
    count: 519,
    cells: 1,
    blob: BLOB,
};
/// `gen_stop.png` — 72x24, 213 rects, 4 sprite cell(s).
pub const GEN_STOP: ArtData = ArtData {
    name: "gen_stop",
    width: 72,
    height: 24,
    offset: 382764,
    count: 213,
    cells: 4,
    blob: BLOB,
};
/// `gen_up.png` — 60x20, 285 rects, 4 sprite cell(s).
pub const GEN_UP: ArtData = ArtData {
    name: "gen_up",
    width: 60,
    height: 20,
    offset: 385320,
    count: 285,
    cells: 4,
    blob: BLOB,
};
/// `gen_volbg_horz.png` — 24x22, 35 rects, 1 sprite cell(s).
pub const GEN_VOLBG_HORZ: ArtData = ArtData {
    name: "gen_volbg_horz",
    width: 24,
    height: 22,
    offset: 388740,
    count: 35,
    cells: 1,
    blob: BLOB,
};
/// `gen_volbg_horz_dark.png` — 24x22, 35 rects, 1 sprite cell(s).
pub const GEN_VOLBG_HORZ_DARK: ArtData = ArtData {
    name: "gen_volbg_horz_dark",
    width: 24,
    height: 22,
    offset: 389160,
    count: 35,
    cells: 1,
    blob: BLOB,
};
/// `gen_volbg_vert.png` — 22x24, 35 rects, 1 sprite cell(s).
pub const GEN_VOLBG_VERT: ArtData = ArtData {
    name: "gen_volbg_vert",
    width: 22,
    height: 24,
    offset: 389580,
    count: 35,
    cells: 1,
    blob: BLOB,
};
/// `gen_volbg_vert_dark.png` — 22x24, 35 rects, 1 sprite cell(s).
pub const GEN_VOLBG_VERT_DARK: ArtData = ArtData {
    name: "gen_volbg_vert_dark",
    width: 22,
    height: 24,
    offset: 390000,
    count: 35,
    cells: 1,
    blob: BLOB,
};
/// `gen_volthumb_horz.png` — 25x29, 110 rects, 1 sprite cell(s).
pub const GEN_VOLTHUMB_HORZ: ArtData = ArtData {
    name: "gen_volthumb_horz",
    width: 25,
    height: 29,
    offset: 390420,
    count: 110,
    cells: 1,
    blob: BLOB,
};
/// `gen_volthumb_vert.png` — 21x31, 201 rects, 1 sprite cell(s).
pub const GEN_VOLTHUMB_VERT: ArtData = ArtData {
    name: "gen_volthumb_vert",
    width: 21,
    height: 31,
    offset: 391740,
    count: 201,
    cells: 1,
    blob: BLOB,
};
/// `global_bypass.png` — 180x26, 934 rects, 3 sprite cell(s).
pub const GLOBAL_BYPASS: ArtData = ArtData {
    name: "global_bypass",
    width: 180,
    height: 26,
    offset: 394152,
    count: 934,
    cells: 3,
    blob: BLOB,
};
/// `global_latch.png` — 180x26, 1028 rects, 3 sprite cell(s).
pub const GLOBAL_LATCH: ArtData = ArtData {
    name: "global_latch",
    width: 180,
    height: 26,
    offset: 405360,
    count: 1028,
    cells: 3,
    blob: BLOB,
};
/// `global_off.png` — 180x26, 701 rects, 1 sprite cell(s).
pub const GLOBAL_OFF: ArtData = ArtData {
    name: "global_off",
    width: 180,
    height: 26,
    offset: 417696,
    count: 701,
    cells: 1,
    blob: BLOB,
};
/// `global_preview.png` — 180x26, 978 rects, 3 sprite cell(s).
pub const GLOBAL_PREVIEW: ArtData = ArtData {
    name: "global_preview",
    width: 180,
    height: 26,
    offset: 426108,
    count: 978,
    cells: 3,
    blob: BLOB,
};
/// `global_read.png` — 180x26, 975 rects, 3 sprite cell(s).
pub const GLOBAL_READ: ArtData = ArtData {
    name: "global_read",
    width: 180,
    height: 26,
    offset: 437844,
    count: 975,
    cells: 3,
    blob: BLOB,
};
/// `global_touch.png` — 180x26, 1065 rects, 3 sprite cell(s).
pub const GLOBAL_TOUCH: ArtData = ArtData {
    name: "global_touch",
    width: 180,
    height: 26,
    offset: 449544,
    count: 1065,
    cells: 3,
    blob: BLOB,
};
/// `global_trim.png` — 180x26, 948 rects, 3 sprite cell(s).
pub const GLOBAL_TRIM: ArtData = ArtData {
    name: "global_trim",
    width: 180,
    height: 26,
    offset: 462324,
    count: 948,
    cells: 3,
    blob: BLOB,
};
/// `global_write.png` — 180x26, 1043 rects, 3 sprite cell(s).
pub const GLOBAL_WRITE: ArtData = ArtData {
    name: "global_write",
    width: 180,
    height: 26,
    offset: 473700,
    count: 1043,
    cells: 3,
    blob: BLOB,
};
/// `item_bg.png` — 8x8, 7 rects, 1 sprite cell(s).
pub const ITEM_BG: ArtData = ArtData {
    name: "item_bg",
    width: 8,
    height: 8,
    offset: 486216,
    count: 7,
    cells: 1,
    blob: BLOB,
};
/// `item_bg_sel.png` — 8x8, 7 rects, 1 sprite cell(s).
pub const ITEM_BG_SEL: ArtData = ArtData {
    name: "item_bg_sel",
    width: 8,
    height: 8,
    offset: 486300,
    count: 7,
    cells: 1,
    blob: BLOB,
};
/// `item_env_off.png` — 42x14, 117 rects, 3 sprite cell(s).
pub const ITEM_ENV_OFF: ArtData = ArtData {
    name: "item_env_off",
    width: 42,
    height: 14,
    offset: 486384,
    count: 117,
    cells: 3,
    blob: BLOB,
};
/// `item_env_off_hidpi.png` — 84x28, 270 rects, 3 sprite cell(s).
pub const ITEM_ENV_OFF_HIDPI: ArtData = ArtData {
    name: "item_env_off_hidpi",
    width: 84,
    height: 28,
    offset: 487788,
    count: 270,
    cells: 3,
    blob: BLOB,
};
/// `item_env_on.png` — 42x14, 144 rects, 3 sprite cell(s).
pub const ITEM_ENV_ON: ArtData = ArtData {
    name: "item_env_on",
    width: 42,
    height: 14,
    offset: 491028,
    count: 144,
    cells: 3,
    blob: BLOB,
};
/// `item_env_on_hidpi.png` — 84x28, 282 rects, 3 sprite cell(s).
pub const ITEM_ENV_ON_HIDPI: ArtData = ArtData {
    name: "item_env_on_hidpi",
    width: 84,
    height: 28,
    offset: 492756,
    count: 282,
    cells: 3,
    blob: BLOB,
};
/// `item_fx_off.png` — 42x14, 181 rects, 3 sprite cell(s).
pub const ITEM_FX_OFF: ArtData = ArtData {
    name: "item_fx_off",
    width: 42,
    height: 14,
    offset: 496140,
    count: 181,
    cells: 3,
    blob: BLOB,
};
/// `item_fx_off_hidpi.png` — 84x28, 448 rects, 3 sprite cell(s).
pub const ITEM_FX_OFF_HIDPI: ArtData = ArtData {
    name: "item_fx_off_hidpi",
    width: 84,
    height: 28,
    offset: 498312,
    count: 448,
    cells: 3,
    blob: BLOB,
};
/// `item_fx_on.png` — 42x14, 206 rects, 3 sprite cell(s).
pub const ITEM_FX_ON: ArtData = ArtData {
    name: "item_fx_on",
    width: 42,
    height: 14,
    offset: 503688,
    count: 206,
    cells: 3,
    blob: BLOB,
};
/// `item_fx_on_hidpi.png` — 84x28, 450 rects, 3 sprite cell(s).
pub const ITEM_FX_ON_HIDPI: ArtData = ArtData {
    name: "item_fx_on_hidpi",
    width: 84,
    height: 28,
    offset: 506160,
    count: 450,
    cells: 3,
    blob: BLOB,
};
/// `item_group.png` — 42x14, 187 rects, 3 sprite cell(s).
pub const ITEM_GROUP: ArtData = ArtData {
    name: "item_group",
    width: 42,
    height: 14,
    offset: 511560,
    count: 187,
    cells: 3,
    blob: BLOB,
};
/// `item_group_hidpi.png` — 84x28, 424 rects, 3 sprite cell(s).
pub const ITEM_GROUP_HIDPI: ArtData = ArtData {
    name: "item_group_hidpi",
    width: 84,
    height: 28,
    offset: 513804,
    count: 424,
    cells: 3,
    blob: BLOB,
};
/// `item_group_sel.png` — 42x14, 213 rects, 3 sprite cell(s).
pub const ITEM_GROUP_SEL: ArtData = ArtData {
    name: "item_group_sel",
    width: 42,
    height: 14,
    offset: 518892,
    count: 213,
    cells: 3,
    blob: BLOB,
};
/// `item_group_sel_hidpi.png` — 84x28, 428 rects, 3 sprite cell(s).
pub const ITEM_GROUP_SEL_HIDPI: ArtData = ArtData {
    name: "item_group_sel_hidpi",
    width: 84,
    height: 28,
    offset: 521448,
    count: 428,
    cells: 3,
    blob: BLOB,
};
/// `item_lock_off.png` — 42x14, 117 rects, 3 sprite cell(s).
pub const ITEM_LOCK_OFF: ArtData = ArtData {
    name: "item_lock_off",
    width: 42,
    height: 14,
    offset: 526584,
    count: 117,
    cells: 3,
    blob: BLOB,
};
/// `item_lock_off_hidpi.png` — 84x28, 231 rects, 3 sprite cell(s).
pub const ITEM_LOCK_OFF_HIDPI: ArtData = ArtData {
    name: "item_lock_off_hidpi",
    width: 84,
    height: 28,
    offset: 527988,
    count: 231,
    cells: 3,
    blob: BLOB,
};
/// `item_lock_on.png` — 42x14, 141 rects, 3 sprite cell(s).
pub const ITEM_LOCK_ON: ArtData = ArtData {
    name: "item_lock_on",
    width: 42,
    height: 14,
    offset: 530760,
    count: 141,
    cells: 3,
    blob: BLOB,
};
/// `item_lock_on_hidpi.png` — 84x28, 231 rects, 3 sprite cell(s).
pub const ITEM_LOCK_ON_HIDPI: ArtData = ArtData {
    name: "item_lock_on_hidpi",
    width: 84,
    height: 28,
    offset: 532452,
    count: 231,
    cells: 3,
    blob: BLOB,
};
/// `item_mute_off.png` — 42x14, 193 rects, 3 sprite cell(s).
pub const ITEM_MUTE_OFF: ArtData = ArtData {
    name: "item_mute_off",
    width: 42,
    height: 14,
    offset: 535224,
    count: 193,
    cells: 3,
    blob: BLOB,
};
/// `item_mute_off_hidpi.png` — 84x28, 462 rects, 3 sprite cell(s).
pub const ITEM_MUTE_OFF_HIDPI: ArtData = ArtData {
    name: "item_mute_off_hidpi",
    width: 84,
    height: 28,
    offset: 537540,
    count: 462,
    cells: 3,
    blob: BLOB,
};
/// `item_mute_on.png` — 42x14, 387 rects, 3 sprite cell(s).
pub const ITEM_MUTE_ON: ArtData = ArtData {
    name: "item_mute_on",
    width: 42,
    height: 14,
    offset: 543084,
    count: 387,
    cells: 3,
    blob: BLOB,
};
/// `item_mute_on_hidpi.png` — 84x28, 1068 rects, 3 sprite cell(s).
pub const ITEM_MUTE_ON_HIDPI: ArtData = ArtData {
    name: "item_mute_on_hidpi",
    width: 84,
    height: 28,
    offset: 547728,
    count: 1068,
    cells: 3,
    blob: BLOB,
};
/// `item_note_off.png` — 42x14, 162 rects, 3 sprite cell(s).
pub const ITEM_NOTE_OFF: ArtData = ArtData {
    name: "item_note_off",
    width: 42,
    height: 14,
    offset: 560544,
    count: 162,
    cells: 3,
    blob: BLOB,
};
/// `item_note_off_hidpi.png` — 84x28, 388 rects, 3 sprite cell(s).
pub const ITEM_NOTE_OFF_HIDPI: ArtData = ArtData {
    name: "item_note_off_hidpi",
    width: 84,
    height: 28,
    offset: 562488,
    count: 388,
    cells: 3,
    blob: BLOB,
};
/// `item_note_on.png` — 42x14, 189 rects, 3 sprite cell(s).
pub const ITEM_NOTE_ON: ArtData = ArtData {
    name: "item_note_on",
    width: 42,
    height: 14,
    offset: 567144,
    count: 189,
    cells: 3,
    blob: BLOB,
};
/// `item_note_on_hidpi.png` — 84x28, 390 rects, 3 sprite cell(s).
pub const ITEM_NOTE_ON_HIDPI: ArtData = ArtData {
    name: "item_note_on_hidpi",
    width: 84,
    height: 28,
    offset: 569412,
    count: 390,
    cells: 3,
    blob: BLOB,
};
/// `item_pooled.png` — 42x14, 60 rects, 3 sprite cell(s).
pub const ITEM_POOLED: ArtData = ArtData {
    name: "item_pooled",
    width: 42,
    height: 14,
    offset: 574092,
    count: 60,
    cells: 3,
    blob: BLOB,
};
/// `item_pooled_hidpi.png` — 84x28, 114 rects, 3 sprite cell(s).
pub const ITEM_POOLED_HIDPI: ArtData = ArtData {
    name: "item_pooled_hidpi",
    width: 84,
    height: 28,
    offset: 574812,
    count: 114,
    cells: 3,
    blob: BLOB,
};
/// `item_pooled_on.png` — 42x14, 84 rects, 3 sprite cell(s).
pub const ITEM_POOLED_ON: ArtData = ArtData {
    name: "item_pooled_on",
    width: 42,
    height: 14,
    offset: 576180,
    count: 84,
    cells: 3,
    blob: BLOB,
};
/// `item_pooled_on_hidpi.png` — 84x28, 114 rects, 3 sprite cell(s).
pub const ITEM_POOLED_ON_HIDPI: ArtData = ArtData {
    name: "item_pooled_on_hidpi",
    width: 84,
    height: 28,
    offset: 577188,
    count: 114,
    cells: 3,
    blob: BLOB,
};
/// `item_props.png` — 42x14, 69 rects, 3 sprite cell(s).
pub const ITEM_PROPS: ArtData = ArtData {
    name: "item_props",
    width: 42,
    height: 14,
    offset: 578556,
    count: 69,
    cells: 3,
    blob: BLOB,
};
/// `item_props_hidpi.png` — 84x28, 183 rects, 3 sprite cell(s).
pub const ITEM_PROPS_HIDPI: ArtData = ArtData {
    name: "item_props_hidpi",
    width: 84,
    height: 28,
    offset: 579384,
    count: 183,
    cells: 3,
    blob: BLOB,
};
/// `item_props_on.png` — 42x14, 69 rects, 3 sprite cell(s).
pub const ITEM_PROPS_ON: ArtData = ArtData {
    name: "item_props_on",
    width: 42,
    height: 14,
    offset: 581580,
    count: 69,
    cells: 3,
    blob: BLOB,
};
/// `item_props_on_hidpi.png` — 84x28, 183 rects, 3 sprite cell(s).
pub const ITEM_PROPS_ON_HIDPI: ArtData = ArtData {
    name: "item_props_on_hidpi",
    width: 84,
    height: 28,
    offset: 582408,
    count: 183,
    cells: 3,
    blob: BLOB,
};
/// `item_rank.png` — 42x14, 294 rects, 3 sprite cell(s).
pub const ITEM_RANK: ArtData = ArtData {
    name: "item_rank",
    width: 42,
    height: 14,
    offset: 584604,
    count: 294,
    cells: 3,
    blob: BLOB,
};
/// `item_rank_down.png` — 42x14, 354 rects, 3 sprite cell(s).
pub const ITEM_RANK_DOWN: ArtData = ArtData {
    name: "item_rank_down",
    width: 42,
    height: 14,
    offset: 588132,
    count: 354,
    cells: 3,
    blob: BLOB,
};
/// `item_rank_down_hidpi.png` — 84x28, 846 rects, 3 sprite cell(s).
pub const ITEM_RANK_DOWN_HIDPI: ArtData = ArtData {
    name: "item_rank_down_hidpi",
    width: 84,
    height: 28,
    offset: 592380,
    count: 846,
    cells: 3,
    blob: BLOB,
};
/// `item_rank_hidpi.png` — 84x28, 744 rects, 3 sprite cell(s).
pub const ITEM_RANK_HIDPI: ArtData = ArtData {
    name: "item_rank_hidpi",
    width: 84,
    height: 28,
    offset: 602532,
    count: 744,
    cells: 3,
    blob: BLOB,
};
/// `item_rank_up.png` — 42x14, 259 rects, 3 sprite cell(s).
pub const ITEM_RANK_UP: ArtData = ArtData {
    name: "item_rank_up",
    width: 42,
    height: 14,
    offset: 611460,
    count: 259,
    cells: 3,
    blob: BLOB,
};
/// `item_rank_up_hidpi.png` — 84x28, 645 rects, 3 sprite cell(s).
pub const ITEM_RANK_UP_HIDPI: ArtData = ArtData {
    name: "item_rank_up_hidpi",
    width: 84,
    height: 28,
    offset: 614568,
    count: 645,
    cells: 3,
    blob: BLOB,
};
/// `item_timebase_beat.png` — 42x14, 210 rects, 3 sprite cell(s).
pub const ITEM_TIMEBASE_BEAT: ArtData = ArtData {
    name: "item_timebase_beat",
    width: 42,
    height: 14,
    offset: 622308,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `item_timebase_beat_hidpi.png` — 84x28, 516 rects, 3 sprite cell(s).
pub const ITEM_TIMEBASE_BEAT_HIDPI: ArtData = ArtData {
    name: "item_timebase_beat_hidpi",
    width: 84,
    height: 28,
    offset: 624828,
    count: 516,
    cells: 3,
    blob: BLOB,
};
/// `item_timebase_beat_on.png` — 42x14, 232 rects, 3 sprite cell(s).
pub const ITEM_TIMEBASE_BEAT_ON: ArtData = ArtData {
    name: "item_timebase_beat_on",
    width: 42,
    height: 14,
    offset: 631020,
    count: 232,
    cells: 3,
    blob: BLOB,
};
/// `item_timebase_beat_on_hidpi.png` — 84x28, 516 rects, 1 sprite cell(s).
pub const ITEM_TIMEBASE_BEAT_ON_HIDPI: ArtData = ArtData {
    name: "item_timebase_beat_on_hidpi",
    width: 84,
    height: 28,
    offset: 633804,
    count: 516,
    cells: 1,
    blob: BLOB,
};
/// `item_timebase_time.png` — 42x14, 220 rects, 3 sprite cell(s).
pub const ITEM_TIMEBASE_TIME: ArtData = ArtData {
    name: "item_timebase_time",
    width: 42,
    height: 14,
    offset: 639996,
    count: 220,
    cells: 3,
    blob: BLOB,
};
/// `item_timebase_time_hidpi.png` — 84x28, 556 rects, 3 sprite cell(s).
pub const ITEM_TIMEBASE_TIME_HIDPI: ArtData = ArtData {
    name: "item_timebase_time_hidpi",
    width: 84,
    height: 28,
    offset: 642636,
    count: 556,
    cells: 3,
    blob: BLOB,
};
/// `item_timebase_time_on.png` — 42x14, 244 rects, 3 sprite cell(s).
pub const ITEM_TIMEBASE_TIME_ON: ArtData = ArtData {
    name: "item_timebase_time_on",
    width: 42,
    height: 14,
    offset: 649308,
    count: 244,
    cells: 3,
    blob: BLOB,
};
/// `item_timebase_time_on_hidpi.png` — 84x28, 564 rects, 1 sprite cell(s).
pub const ITEM_TIMEBASE_TIME_ON_HIDPI: ArtData = ArtData {
    name: "item_timebase_time_on_hidpi",
    width: 84,
    height: 28,
    offset: 652236,
    count: 564,
    cells: 1,
    blob: BLOB,
};
/// `item_volknob.png` — 48x18, 271 rects, 2 sprite cell(s).
pub const ITEM_VOLKNOB: ArtData = ArtData {
    name: "item_volknob",
    width: 48,
    height: 18,
    offset: 659004,
    count: 271,
    cells: 2,
    blob: BLOB,
};
/// `item_volknob_hidpi.png` — 84x28, 673 rects, 2 sprite cell(s).
pub const ITEM_VOLKNOB_HIDPI: ArtData = ArtData {
    name: "item_volknob_hidpi",
    width: 84,
    height: 28,
    offset: 662256,
    count: 673,
    cells: 2,
    blob: BLOB,
};
/// `lane_solo_down.png` — 60x15, 72 rects, 3 sprite cell(s).
pub const LANE_SOLO_DOWN: ArtData = ArtData {
    name: "lane_solo_down",
    width: 60,
    height: 15,
    offset: 670332,
    count: 72,
    cells: 3,
    blob: BLOB,
};
/// `lane_solo_off.png` — 57x24, 465 rects, 3 sprite cell(s).
pub const LANE_SOLO_OFF: ArtData = ArtData {
    name: "lane_solo_off",
    width: 57,
    height: 24,
    offset: 671196,
    count: 465,
    cells: 3,
    blob: BLOB,
};
/// `lane_solo_off_indicator.png` — 24x8, 146 rects, 3 sprite cell(s).
pub const LANE_SOLO_OFF_INDICATOR: ArtData = ArtData {
    name: "lane_solo_off_indicator",
    width: 24,
    height: 8,
    offset: 676776,
    count: 146,
    cells: 3,
    blob: BLOB,
};
/// `lane_solo_on.png` — 57x24, 345 rects, 3 sprite cell(s).
pub const LANE_SOLO_ON: ArtData = ArtData {
    name: "lane_solo_on",
    width: 57,
    height: 24,
    offset: 678528,
    count: 345,
    cells: 3,
    blob: BLOB,
};
/// `lane_solo_on_indicator.png` — 24x8, 110 rects, 3 sprite cell(s).
pub const LANE_SOLO_ON_INDICATOR: ArtData = ArtData {
    name: "lane_solo_on_indicator",
    width: 24,
    height: 8,
    offset: 682668,
    count: 110,
    cells: 3,
    blob: BLOB,
};
/// `lane_solo_up.png` — 60x15, 66 rects, 3 sprite cell(s).
pub const LANE_SOLO_UP: ArtData = ArtData {
    name: "lane_solo_up",
    width: 60,
    height: 15,
    offset: 683988,
    count: 66,
    cells: 3,
    blob: BLOB,
};
/// `mcp_bg.png` — 4x4, 1 rects, 1 sprite cell(s).
pub const MCP_BG: ArtData = ArtData {
    name: "mcp_bg",
    width: 4,
    height: 4,
    offset: 684780,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `mcp_bgsel.png` — 4x3, 2 rects, 1 sprite cell(s).
pub const MCP_BGSEL: ArtData = ArtData {
    name: "mcp_bgsel",
    width: 4,
    height: 3,
    offset: 684792,
    count: 2,
    cells: 1,
    blob: BLOB,
};
/// `mcp_env.png` — 65x32, 351 rects, 3 sprite cell(s).
pub const MCP_ENV: ArtData = ArtData {
    name: "mcp_env",
    width: 65,
    height: 32,
    offset: 684816,
    count: 351,
    cells: 3,
    blob: BLOB,
};
/// `mcp_env_latch.png` — 65x32, 356 rects, 3 sprite cell(s).
pub const MCP_ENV_LATCH: ArtData = ArtData {
    name: "mcp_env_latch",
    width: 65,
    height: 32,
    offset: 689028,
    count: 356,
    cells: 3,
    blob: BLOB,
};
/// `mcp_env_preview.png` — 65x32, 342 rects, 3 sprite cell(s).
pub const MCP_ENV_PREVIEW: ArtData = ArtData {
    name: "mcp_env_preview",
    width: 65,
    height: 32,
    offset: 693300,
    count: 342,
    cells: 3,
    blob: BLOB,
};
/// `mcp_env_read.png` — 65x32, 363 rects, 3 sprite cell(s).
pub const MCP_ENV_READ: ArtData = ArtData {
    name: "mcp_env_read",
    width: 65,
    height: 32,
    offset: 697404,
    count: 363,
    cells: 3,
    blob: BLOB,
};
/// `mcp_env_touch.png` — 65x32, 267 rects, 3 sprite cell(s).
pub const MCP_ENV_TOUCH: ArtData = ArtData {
    name: "mcp_env_touch",
    width: 65,
    height: 32,
    offset: 701760,
    count: 267,
    cells: 3,
    blob: BLOB,
};
/// `mcp_env_write.png` — 65x32, 381 rects, 3 sprite cell(s).
pub const MCP_ENV_WRITE: ArtData = ArtData {
    name: "mcp_env_write",
    width: 65,
    height: 32,
    offset: 704964,
    count: 381,
    cells: 3,
    blob: BLOB,
};
/// `mcp_extmixbg.png` — 3x3, 3 rects, 1 sprite cell(s).
pub const MCP_EXTMIXBG: ArtData = ArtData {
    name: "mcp_extmixbg",
    width: 3,
    height: 3,
    offset: 709536,
    count: 3,
    cells: 1,
    blob: BLOB,
};
/// `mcp_extmixbgsel.png` — 3x3, 1 rects, 1 sprite cell(s).
pub const MCP_EXTMIXBGSEL: ArtData = ArtData {
    name: "mcp_extmixbgsel",
    width: 3,
    height: 3,
    offset: 709572,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fcomp_off.png` — 149x23, 95 rects, 3 sprite cell(s).
pub const MCP_FCOMP_OFF: ArtData = ArtData {
    name: "mcp_fcomp_off",
    width: 149,
    height: 23,
    offset: 709584,
    count: 95,
    cells: 3,
    blob: BLOB,
};
/// `mcp_fcomp_tiny.png` — 149x23, 111 rects, 1 sprite cell(s).
pub const MCP_FCOMP_TINY: ArtData = ArtData {
    name: "mcp_fcomp_tiny",
    width: 149,
    height: 23,
    offset: 710724,
    count: 111,
    cells: 1,
    blob: BLOB,
};
/// `mcp_folder_last.png` — 63x21, 69 rects, 3 sprite cell(s).
pub const MCP_FOLDER_LAST: ArtData = ArtData {
    name: "mcp_folder_last",
    width: 63,
    height: 21,
    offset: 712056,
    count: 69,
    cells: 3,
    blob: BLOB,
};
/// `mcp_folder_on.png` — 55x15, 2 rects, 1 sprite cell(s).
pub const MCP_FOLDER_ON: ArtData = ArtData {
    name: "mcp_folder_on",
    width: 55,
    height: 15,
    offset: 712884,
    count: 2,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fx_dis.png` — 86x22, 372 rects, 3 sprite cell(s).
pub const MCP_FX_DIS: ArtData = ArtData {
    name: "mcp_fx_dis",
    width: 86,
    height: 22,
    offset: 712908,
    count: 372,
    cells: 3,
    blob: BLOB,
};
/// `mcp_fx_empty.png` — 86x22, 378 rects, 3 sprite cell(s).
pub const MCP_FX_EMPTY: ArtData = ArtData {
    name: "mcp_fx_empty",
    width: 86,
    height: 22,
    offset: 717372,
    count: 378,
    cells: 3,
    blob: BLOB,
};
/// `mcp_fx_in_empty.png` — 225x12, 421 rects, 1 sprite cell(s).
pub const MCP_FX_IN_EMPTY: ArtData = ArtData {
    name: "mcp_fx_in_empty",
    width: 225,
    height: 12,
    offset: 721908,
    count: 421,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fx_in_empty_ol_2.png` — 225x12, 421 rects, 1 sprite cell(s).
pub const MCP_FX_IN_EMPTY_OL_2: ArtData = ArtData {
    name: "mcp_fx_in_empty_ol_2",
    width: 225,
    height: 12,
    offset: 726960,
    count: 421,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fx_in_norm.png` — 225x12, 435 rects, 1 sprite cell(s).
pub const MCP_FX_IN_NORM: ArtData = ArtData {
    name: "mcp_fx_in_norm",
    width: 225,
    height: 12,
    offset: 732012,
    count: 435,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fx_in_norm_ol_2.png` — 225x12, 435 rects, 1 sprite cell(s).
pub const MCP_FX_IN_NORM_OL_2: ArtData = ArtData {
    name: "mcp_fx_in_norm_ol_2",
    width: 225,
    height: 12,
    offset: 737232,
    count: 435,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fx_norm.png` — 86x22, 381 rects, 3 sprite cell(s).
pub const MCP_FX_NORM: ArtData = ArtData {
    name: "mcp_fx_norm",
    width: 86,
    height: 22,
    offset: 742452,
    count: 381,
    cells: 3,
    blob: BLOB,
};
/// `mcp_fxlist_bg.png` — 44x6, 1 rects, 1 sprite cell(s).
pub const MCP_FXLIST_BG: ArtData = ArtData {
    name: "mcp_fxlist_bg",
    width: 44,
    height: 6,
    offset: 747024,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fxlist_byp.png` — 38x53, 115 rects, 1 sprite cell(s).
pub const MCP_FXLIST_BYP: ArtData = ArtData {
    name: "mcp_fxlist_byp",
    width: 38,
    height: 53,
    offset: 747036,
    count: 115,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fxlist_empty.png` — 38x53, 75 rects, 1 sprite cell(s).
pub const MCP_FXLIST_EMPTY: ArtData = ArtData {
    name: "mcp_fxlist_empty",
    width: 38,
    height: 53,
    offset: 748416,
    count: 75,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fxlist_norm.png` — 38x53, 185 rects, 1 sprite cell(s).
pub const MCP_FXLIST_NORM: ArtData = ArtData {
    name: "mcp_fxlist_norm",
    width: 38,
    height: 53,
    offset: 749316,
    count: 185,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fxlist_off.png` — 38x53, 115 rects, 1 sprite cell(s).
pub const MCP_FXLIST_OFF: ArtData = ArtData {
    name: "mcp_fxlist_off",
    width: 38,
    height: 53,
    offset: 751536,
    count: 115,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fxparm_bg.png` — 44x6, 1 rects, 1 sprite cell(s).
pub const MCP_FXPARM_BG: ArtData = ArtData {
    name: "mcp_fxparm_bg",
    width: 44,
    height: 6,
    offset: 752916,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fxparm_byp.png` — 38x50, 230 rects, 1 sprite cell(s).
pub const MCP_FXPARM_BYP: ArtData = ArtData {
    name: "mcp_fxparm_byp",
    width: 38,
    height: 50,
    offset: 752928,
    count: 230,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fxparm_empty.png` — 38x50, 123 rects, 1 sprite cell(s).
pub const MCP_FXPARM_EMPTY: ArtData = ArtData {
    name: "mcp_fxparm_empty",
    width: 38,
    height: 50,
    offset: 755688,
    count: 123,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fxparm_knob_stack.png` — 23x1150, 5036 rects, 1 sprite cell(s).
pub const MCP_FXPARM_KNOB_STACK: ArtData = ArtData {
    name: "mcp_fxparm_knob_stack",
    width: 23,
    height: 1150,
    offset: 757164,
    count: 5036,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fxparm_norm.png` — 38x50, 213 rects, 1 sprite cell(s).
pub const MCP_FXPARM_NORM: ArtData = ArtData {
    name: "mcp_fxparm_norm",
    width: 38,
    height: 50,
    offset: 817596,
    count: 213,
    cells: 1,
    blob: BLOB,
};
/// `mcp_fxparm_off.png` — 38x50, 215 rects, 1 sprite cell(s).
pub const MCP_FXPARM_OFF: ArtData = ArtData {
    name: "mcp_fxparm_off",
    width: 38,
    height: 50,
    offset: 820152,
    count: 215,
    cells: 1,
    blob: BLOB,
};
/// `mcp_iconbg.png` — 6x11, 4 rects, 1 sprite cell(s).
pub const MCP_ICONBG: ArtData = ArtData {
    name: "mcp_iconbg",
    width: 6,
    height: 11,
    offset: 822732,
    count: 4,
    cells: 1,
    blob: BLOB,
};
/// `mcp_iconbgsel.png` — 6x11, 6 rects, 1 sprite cell(s).
pub const MCP_ICONBGSEL: ArtData = ArtData {
    name: "mcp_iconbgsel",
    width: 6,
    height: 11,
    offset: 822780,
    count: 6,
    cells: 1,
    blob: BLOB,
};
/// `mcp_idxbg.png` — 1x1, 0 rects, 1 sprite cell(s).
pub const MCP_IDXBG: ArtData = ArtData {
    name: "mcp_idxbg",
    width: 1,
    height: 1,
    offset: 822852,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `mcp_idxbg_sel.png` — 1x1, 0 rects, 1 sprite cell(s).
pub const MCP_IDXBG_SEL: ArtData = ArtData {
    name: "mcp_idxbg_sel",
    width: 1,
    height: 1,
    offset: 822852,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `mcp_io.png` — 71x32, 563 rects, 3 sprite cell(s).
pub const MCP_IO: ArtData = ArtData {
    name: "mcp_io",
    width: 71,
    height: 32,
    offset: 822852,
    count: 563,
    cells: 3,
    blob: BLOB,
};
/// `mcp_io_dis.png` — 71x32, 513 rects, 3 sprite cell(s).
pub const MCP_IO_DIS: ArtData = ArtData {
    name: "mcp_io_dis",
    width: 71,
    height: 32,
    offset: 829608,
    count: 513,
    cells: 3,
    blob: BLOB,
};
/// `mcp_io_r.png` — 71x32, 594 rects, 3 sprite cell(s).
pub const MCP_IO_R: ArtData = ArtData {
    name: "mcp_io_r",
    width: 71,
    height: 32,
    offset: 835764,
    count: 594,
    cells: 3,
    blob: BLOB,
};
/// `mcp_io_r_dis.png` — 71x32, 552 rects, 3 sprite cell(s).
pub const MCP_IO_R_DIS: ArtData = ArtData {
    name: "mcp_io_r_dis",
    width: 71,
    height: 32,
    offset: 842892,
    count: 552,
    cells: 3,
    blob: BLOB,
};
/// `mcp_io_s.png` — 71x32, 585 rects, 3 sprite cell(s).
pub const MCP_IO_S: ArtData = ArtData {
    name: "mcp_io_s",
    width: 71,
    height: 32,
    offset: 849516,
    count: 585,
    cells: 3,
    blob: BLOB,
};
/// `mcp_io_s_dis.png` — 71x32, 554 rects, 3 sprite cell(s).
pub const MCP_IO_S_DIS: ArtData = ArtData {
    name: "mcp_io_s_dis",
    width: 71,
    height: 32,
    offset: 856536,
    count: 554,
    cells: 3,
    blob: BLOB,
};
/// `mcp_io_s_r.png` — 71x32, 618 rects, 3 sprite cell(s).
pub const MCP_IO_S_R: ArtData = ArtData {
    name: "mcp_io_s_r",
    width: 71,
    height: 32,
    offset: 863184,
    count: 618,
    cells: 3,
    blob: BLOB,
};
/// `mcp_io_s_r_dis.png` — 71x32, 581 rects, 3 sprite cell(s).
pub const MCP_IO_S_R_DIS: ArtData = ArtData {
    name: "mcp_io_s_r_dis",
    width: 71,
    height: 32,
    offset: 870600,
    count: 581,
    cells: 3,
    blob: BLOB,
};
/// `mcp_main_namebg.png` — 8x9, 8 rects, 2 sprite cell(s).
pub const MCP_MAIN_NAMEBG: ArtData = ArtData {
    name: "mcp_main_namebg",
    width: 8,
    height: 9,
    offset: 877572,
    count: 8,
    cells: 2,
    blob: BLOB,
};
/// `mcp_main_namebg_sel.png` — 8x9, 8 rects, 2 sprite cell(s).
pub const MCP_MAIN_NAMEBG_SEL: ArtData = ArtData {
    name: "mcp_main_namebg_sel",
    width: 8,
    height: 9,
    offset: 877668,
    count: 8,
    cells: 2,
    blob: BLOB,
};
/// `mcp_mainbg.png` — 6x6, 3 rects, 1 sprite cell(s).
pub const MCP_MAINBG: ArtData = ArtData {
    name: "mcp_mainbg",
    width: 6,
    height: 6,
    offset: 877764,
    count: 3,
    cells: 1,
    blob: BLOB,
};
/// `mcp_mainbgsel.png` — 6x6, 4 rects, 1 sprite cell(s).
pub const MCP_MAINBGSEL: ArtData = ArtData {
    name: "mcp_mainbgsel",
    width: 6,
    height: 6,
    offset: 877800,
    count: 4,
    cells: 1,
    blob: BLOB,
};
/// `mcp_mainextmixbg.png` — 9x8, 1 rects, 1 sprite cell(s).
pub const MCP_MAINEXTMIXBG: ArtData = ArtData {
    name: "mcp_mainextmixbg",
    width: 9,
    height: 8,
    offset: 877848,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `mcp_mainextmixbgsel.png` — 9x8, 2 rects, 1 sprite cell(s).
pub const MCP_MAINEXTMIXBGSEL: ArtData = ArtData {
    name: "mcp_mainextmixbgsel",
    width: 9,
    height: 8,
    offset: 877860,
    count: 2,
    cells: 1,
    blob: BLOB,
};
/// `mcp_master_vol_label.png` — 43x5, 1 rects, 1 sprite cell(s).
pub const MCP_MASTER_VOL_LABEL: ArtData = ArtData {
    name: "mcp_master_vol_label",
    width: 43,
    height: 5,
    offset: 877884,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `mcp_master_volbg.png` — 129x52, 13 rects, 3 sprite cell(s).
pub const MCP_MASTER_VOLBG: ArtData = ArtData {
    name: "mcp_master_volbg",
    width: 129,
    height: 52,
    offset: 877896,
    count: 13,
    cells: 3,
    blob: BLOB,
};
/// `mcp_master_volthumb.png` — 27x53, 434 rects, 1 sprite cell(s).
pub const MCP_MASTER_VOLTHUMB: ArtData = ArtData {
    name: "mcp_master_volthumb",
    width: 27,
    height: 53,
    offset: 878052,
    count: 434,
    cells: 1,
    blob: BLOB,
};
/// `mcp_monitor_auto.png` — 63x20, 633 rects, 3 sprite cell(s).
pub const MCP_MONITOR_AUTO: ArtData = ArtData {
    name: "mcp_monitor_auto",
    width: 63,
    height: 20,
    offset: 883260,
    count: 633,
    cells: 3,
    blob: BLOB,
};
/// `mcp_monitor_off.png` — 63x20, 840 rects, 3 sprite cell(s).
pub const MCP_MONITOR_OFF: ArtData = ArtData {
    name: "mcp_monitor_off",
    width: 63,
    height: 20,
    offset: 890856,
    count: 840,
    cells: 3,
    blob: BLOB,
};
/// `mcp_monitor_on.png` — 63x20, 682 rects, 3 sprite cell(s).
pub const MCP_MONITOR_ON: ArtData = ArtData {
    name: "mcp_monitor_on",
    width: 63,
    height: 20,
    offset: 900936,
    count: 682,
    cells: 3,
    blob: BLOB,
};
/// `mcp_mono.png` — 77x35, 1332 rects, 3 sprite cell(s).
pub const MCP_MONO: ArtData = ArtData {
    name: "mcp_mono",
    width: 77,
    height: 35,
    offset: 909120,
    count: 1332,
    cells: 3,
    blob: BLOB,
};
/// `mcp_mute_off.png` — 63x20, 357 rects, 3 sprite cell(s).
pub const MCP_MUTE_OFF: ArtData = ArtData {
    name: "mcp_mute_off",
    width: 63,
    height: 20,
    offset: 925104,
    count: 357,
    cells: 3,
    blob: BLOB,
};
/// `mcp_mute_on.png` — 63x20, 714 rects, 3 sprite cell(s).
pub const MCP_MUTE_ON: ArtData = ArtData {
    name: "mcp_mute_on",
    width: 63,
    height: 20,
    offset: 929388,
    count: 714,
    cells: 3,
    blob: BLOB,
};
/// `mcp_namebg.png` — 3x3, 0 rects, 1 sprite cell(s).
pub const MCP_NAMEBG: ArtData = ArtData {
    name: "mcp_namebg",
    width: 3,
    height: 3,
    offset: 937956,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `mcp_pan_knob_large.png` — 28x29, 306 rects, 1 sprite cell(s).
pub const MCP_PAN_KNOB_LARGE: ArtData = ArtData {
    name: "mcp_pan_knob_large",
    width: 28,
    height: 29,
    offset: 937956,
    count: 306,
    cells: 1,
    blob: BLOB,
};
/// `mcp_pan_knob_small.png` — 24x25, 246 rects, 1 sprite cell(s).
pub const MCP_PAN_KNOB_SMALL: ArtData = ArtData {
    name: "mcp_pan_knob_small",
    width: 24,
    height: 25,
    offset: 941628,
    count: 246,
    cells: 1,
    blob: BLOB,
};
/// `mcp_pan_label.png` — 4x4, 0 rects, 1 sprite cell(s).
pub const MCP_PAN_LABEL: ArtData = ArtData {
    name: "mcp_pan_label",
    width: 4,
    height: 4,
    offset: 944580,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `mcp_panbg.png` — 69x11, 33 rects, 1 sprite cell(s).
pub const MCP_PANBG: ArtData = ArtData {
    name: "mcp_panbg",
    width: 69,
    height: 11,
    offset: 944580,
    count: 33,
    cells: 1,
    blob: BLOB,
};
/// `mcp_panthumb.png` — 13x23, 81 rects, 1 sprite cell(s).
pub const MCP_PANTHUMB: ArtData = ArtData {
    name: "mcp_panthumb",
    width: 13,
    height: 23,
    offset: 944976,
    count: 81,
    cells: 1,
    blob: BLOB,
};
/// `mcp_phase_inv.png` — 48x18, 518 rects, 3 sprite cell(s).
pub const MCP_PHASE_INV: ArtData = ArtData {
    name: "mcp_phase_inv",
    width: 48,
    height: 18,
    offset: 945948,
    count: 518,
    cells: 3,
    blob: BLOB,
};
/// `mcp_phase_norm.png` — 48x18, 495 rects, 3 sprite cell(s).
pub const MCP_PHASE_NORM: ArtData = ArtData {
    name: "mcp_phase_norm",
    width: 48,
    height: 18,
    offset: 952164,
    count: 495,
    cells: 3,
    blob: BLOB,
};
/// `mcp_recarm_auto.png` — 108x24, 595 rects, 3 sprite cell(s).
pub const MCP_RECARM_AUTO: ArtData = ArtData {
    name: "mcp_recarm_auto",
    width: 108,
    height: 24,
    offset: 958104,
    count: 595,
    cells: 3,
    blob: BLOB,
};
/// `mcp_recarm_auto_norec.png` — 108x24, 627 rects, 3 sprite cell(s).
pub const MCP_RECARM_AUTO_NOREC: ArtData = ArtData {
    name: "mcp_recarm_auto_norec",
    width: 108,
    height: 24,
    offset: 965244,
    count: 627,
    cells: 3,
    blob: BLOB,
};
/// `mcp_recarm_auto_on.png` — 108x24, 627 rects, 3 sprite cell(s).
pub const MCP_RECARM_AUTO_ON: ArtData = ArtData {
    name: "mcp_recarm_auto_on",
    width: 108,
    height: 24,
    offset: 972768,
    count: 627,
    cells: 3,
    blob: BLOB,
};
/// `mcp_recarm_norec.png` — 108x24, 701 rects, 3 sprite cell(s).
pub const MCP_RECARM_NOREC: ArtData = ArtData {
    name: "mcp_recarm_norec",
    width: 108,
    height: 24,
    offset: 980292,
    count: 701,
    cells: 3,
    blob: BLOB,
};
/// `mcp_recarm_off.png` — 108x24, 560 rects, 3 sprite cell(s).
pub const MCP_RECARM_OFF: ArtData = ArtData {
    name: "mcp_recarm_off",
    width: 108,
    height: 24,
    offset: 988704,
    count: 560,
    cells: 3,
    blob: BLOB,
};
/// `mcp_recarm_on.png` — 108x24, 564 rects, 3 sprite cell(s).
pub const MCP_RECARM_ON: ArtData = ArtData {
    name: "mcp_recarm_on",
    width: 108,
    height: 24,
    offset: 995424,
    count: 564,
    cells: 3,
    blob: BLOB,
};
/// `mcp_recinput.png` — 25x18, 64 rects, 1 sprite cell(s).
pub const MCP_RECINPUT: ArtData = ArtData {
    name: "mcp_recinput",
    width: 25,
    height: 18,
    offset: 1002192,
    count: 64,
    cells: 1,
    blob: BLOB,
};
/// `mcp_recmode_in.png` — 125x18, 330 rects, 1 sprite cell(s).
pub const MCP_RECMODE_IN: ArtData = ArtData {
    name: "mcp_recmode_in",
    width: 125,
    height: 18,
    offset: 1002960,
    count: 330,
    cells: 1,
    blob: BLOB,
};
/// `mcp_recmode_off.png` — 125x18, 577 rects, 1 sprite cell(s).
pub const MCP_RECMODE_OFF: ArtData = ArtData {
    name: "mcp_recmode_off",
    width: 125,
    height: 18,
    offset: 1006920,
    count: 577,
    cells: 1,
    blob: BLOB,
};
/// `mcp_recmode_out.png` — 125x18, 467 rects, 3 sprite cell(s).
pub const MCP_RECMODE_OUT: ArtData = ArtData {
    name: "mcp_recmode_out",
    width: 125,
    height: 18,
    offset: 1013844,
    count: 467,
    cells: 3,
    blob: BLOB,
};
/// `mcp_send_knob_stack.png` — 23x1150, 5036 rects, 1 sprite cell(s).
pub const MCP_SEND_KNOB_STACK: ArtData = ArtData {
    name: "mcp_send_knob_stack",
    width: 23,
    height: 1150,
    offset: 1019448,
    count: 5036,
    cells: 1,
    blob: BLOB,
};
/// `mcp_sendlist_bg.png` — 44x6, 1 rects, 1 sprite cell(s).
pub const MCP_SENDLIST_BG: ArtData = ArtData {
    name: "mcp_sendlist_bg",
    width: 44,
    height: 6,
    offset: 1079880,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `mcp_sendlist_empty.png` — 38x50, 125 rects, 1 sprite cell(s).
pub const MCP_SENDLIST_EMPTY: ArtData = ArtData {
    name: "mcp_sendlist_empty",
    width: 38,
    height: 50,
    offset: 1079892,
    count: 125,
    cells: 1,
    blob: BLOB,
};
/// `mcp_sendlist_meter.png` — 38x26, 25 rects, 1 sprite cell(s).
pub const MCP_SENDLIST_METER: ArtData = ArtData {
    name: "mcp_sendlist_meter",
    width: 38,
    height: 26,
    offset: 1081392,
    count: 25,
    cells: 1,
    blob: BLOB,
};
/// `mcp_sendlist_midihw.png` — 38x50, 219 rects, 1 sprite cell(s).
pub const MCP_SENDLIST_MIDIHW: ArtData = ArtData {
    name: "mcp_sendlist_midihw",
    width: 38,
    height: 50,
    offset: 1081692,
    count: 219,
    cells: 1,
    blob: BLOB,
};
/// `mcp_sendlist_mute.png` — 38x50, 219 rects, 1 sprite cell(s).
pub const MCP_SENDLIST_MUTE: ArtData = ArtData {
    name: "mcp_sendlist_mute",
    width: 38,
    height: 50,
    offset: 1084320,
    count: 219,
    cells: 1,
    blob: BLOB,
};
/// `mcp_sendlist_norm.png` — 38x50, 210 rects, 1 sprite cell(s).
pub const MCP_SENDLIST_NORM: ArtData = ArtData {
    name: "mcp_sendlist_norm",
    width: 38,
    height: 50,
    offset: 1086948,
    count: 210,
    cells: 1,
    blob: BLOB,
};
/// `mcp_solo_off.png` — 63x20, 413 rects, 3 sprite cell(s).
pub const MCP_SOLO_OFF: ArtData = ArtData {
    name: "mcp_solo_off",
    width: 63,
    height: 20,
    offset: 1089468,
    count: 413,
    cells: 3,
    blob: BLOB,
};
/// `mcp_solo_on.png` — 63x20, 489 rects, 3 sprite cell(s).
pub const MCP_SOLO_ON: ArtData = ArtData {
    name: "mcp_solo_on",
    width: 63,
    height: 20,
    offset: 1094424,
    count: 489,
    cells: 3,
    blob: BLOB,
};
/// `mcp_solodefeat_on.png` — 63x20, 513 rects, 3 sprite cell(s).
pub const MCP_SOLODEFEAT_ON: ArtData = ArtData {
    name: "mcp_solodefeat_on",
    width: 63,
    height: 20,
    offset: 1100292,
    count: 513,
    cells: 3,
    blob: BLOB,
};
/// `mcp_stereo.png` — 77x35, 863 rects, 3 sprite cell(s).
pub const MCP_STEREO: ArtData = ArtData {
    name: "mcp_stereo",
    width: 77,
    height: 35,
    offset: 1106448,
    count: 863,
    cells: 3,
    blob: BLOB,
};
/// `mcp_volbg.png` — 23x55, 5 rects, 1 sprite cell(s).
pub const MCP_VOLBG: ArtData = ArtData {
    name: "mcp_volbg",
    width: 23,
    height: 55,
    offset: 1116804,
    count: 5,
    cells: 1,
    blob: BLOB,
};
/// `mcp_volbg_horz.png` — 4x4, 0 rects, 1 sprite cell(s).
pub const MCP_VOLBG_HORZ: ArtData = ArtData {
    name: "mcp_volbg_horz",
    width: 4,
    height: 4,
    offset: 1116864,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `mcp_volthumb.png` — 27x53, 434 rects, 1 sprite cell(s).
pub const MCP_VOLTHUMB: ArtData = ArtData {
    name: "mcp_volthumb",
    width: 27,
    height: 53,
    offset: 1116864,
    count: 434,
    cells: 1,
    blob: BLOB,
};
/// `mcp_wid_label.png` — 4x4, 0 rects, 1 sprite cell(s).
pub const MCP_WID_LABEL: ArtData = ArtData {
    name: "mcp_wid_label",
    width: 4,
    height: 4,
    offset: 1122072,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `mcp_width_knob_large.png` — 28x29, 306 rects, 1 sprite cell(s).
pub const MCP_WIDTH_KNOB_LARGE: ArtData = ArtData {
    name: "mcp_width_knob_large",
    width: 28,
    height: 29,
    offset: 1122072,
    count: 306,
    cells: 1,
    blob: BLOB,
};
/// `mcp_width_knob_small.png` — 24x25, 246 rects, 1 sprite cell(s).
pub const MCP_WIDTH_KNOB_SMALL: ArtData = ArtData {
    name: "mcp_width_knob_small",
    width: 24,
    height: 25,
    offset: 1125744,
    count: 246,
    cells: 1,
    blob: BLOB,
};
/// `mcp_widthbg.png` — 69x11, 33 rects, 1 sprite cell(s).
pub const MCP_WIDTHBG: ArtData = ArtData {
    name: "mcp_widthbg",
    width: 69,
    height: 11,
    offset: 1128696,
    count: 33,
    cells: 1,
    blob: BLOB,
};
/// `mcp_widththumb.png` — 13x23, 81 rects, 1 sprite cell(s).
pub const MCP_WIDTHTHUMB: ArtData = ArtData {
    name: "mcp_widththumb",
    width: 13,
    height: 23,
    offset: 1129092,
    count: 81,
    cells: 1,
    blob: BLOB,
};
/// `meter_automute.png` — 20x20, 309 rects, 4 sprite cell(s).
pub const METER_AUTOMUTE: ArtData = ArtData {
    name: "meter_automute",
    width: 20,
    height: 20,
    offset: 1130064,
    count: 309,
    cells: 4,
    blob: BLOB,
};
/// `meter_bg_h.png` — 4x4, 1 rects, 1 sprite cell(s).
pub const METER_BG_H: ArtData = ArtData {
    name: "meter_bg_h",
    width: 4,
    height: 4,
    offset: 1133772,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `meter_bg_mcp.png` — 4x4, 1 rects, 1 sprite cell(s).
pub const METER_BG_MCP: ArtData = ArtData {
    name: "meter_bg_mcp",
    width: 4,
    height: 4,
    offset: 1133784,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `meter_bg_v.png` — 4x4, 1 rects, 1 sprite cell(s).
pub const METER_BG_V: ArtData = ArtData {
    name: "meter_bg_v",
    width: 4,
    height: 4,
    offset: 1133796,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `meter_clip_h.png` — 8x4, 2 rects, 2 sprite cell(s).
pub const METER_CLIP_H: ArtData = ArtData {
    name: "meter_clip_h",
    width: 8,
    height: 4,
    offset: 1133808,
    count: 2,
    cells: 2,
    blob: BLOB,
};
/// `meter_clip_v.png` — 4x8, 3 rects, 1 sprite cell(s).
pub const METER_CLIP_V: ArtData = ArtData {
    name: "meter_clip_v",
    width: 4,
    height: 8,
    offset: 1133832,
    count: 3,
    cells: 1,
    blob: BLOB,
};
/// `meter_clip_v_rms2.png` — 4x4, 2 rects, 1 sprite cell(s).
pub const METER_CLIP_V_RMS2: ArtData = ArtData {
    name: "meter_clip_v_rms2",
    width: 4,
    height: 4,
    offset: 1133868,
    count: 2,
    cells: 1,
    blob: BLOB,
};
/// `meter_foldermute.png` — 12x12, 16 rects, 1 sprite cell(s).
pub const METER_FOLDERMUTE: ArtData = ArtData {
    name: "meter_foldermute",
    width: 12,
    height: 12,
    offset: 1133892,
    count: 16,
    cells: 1,
    blob: BLOB,
};
/// `meter_mute.png` — 12x12, 13 rects, 1 sprite cell(s).
pub const METER_MUTE: ArtData = ArtData {
    name: "meter_mute",
    width: 12,
    height: 12,
    offset: 1134084,
    count: 13,
    cells: 1,
    blob: BLOB,
};
/// `meter_solodim.png` — 12x12, 11 rects, 1 sprite cell(s).
pub const METER_SOLODIM: ArtData = ArtData {
    name: "meter_solodim",
    width: 12,
    height: 12,
    offset: 1134240,
    count: 11,
    cells: 1,
    blob: BLOB,
};
/// `meter_strip_h.png` — 168x16, 142 rects, 4 sprite cell(s).
pub const METER_STRIP_H: ArtData = ArtData {
    name: "meter_strip_h",
    width: 168,
    height: 16,
    offset: 1134372,
    count: 142,
    cells: 4,
    blob: BLOB,
};
/// `meter_strip_h_rms.png` — 2x32, 8 rects, 1 sprite cell(s).
pub const METER_STRIP_H_RMS: ArtData = ArtData {
    name: "meter_strip_h_rms",
    width: 2,
    height: 32,
    offset: 1136076,
    count: 8,
    cells: 1,
    blob: BLOB,
};
/// `meter_strip_v.png` — 32x168, 238 rects, 4 sprite cell(s).
pub const METER_STRIP_V: ArtData = ArtData {
    name: "meter_strip_v",
    width: 32,
    height: 168,
    offset: 1136172,
    count: 238,
    cells: 4,
    blob: BLOB,
};
/// `meter_strip_v_rms.png` — 32x2, 8 rects, 4 sprite cell(s).
pub const METER_STRIP_V_RMS: ArtData = ArtData {
    name: "meter_strip_v_rms",
    width: 32,
    height: 2,
    offset: 1139028,
    count: 8,
    cells: 4,
    blob: BLOB,
};
/// `meter_unsolo.png` — 12x12, 18 rects, 1 sprite cell(s).
pub const METER_UNSOLO: ArtData = ArtData {
    name: "meter_unsolo",
    width: 12,
    height: 12,
    offset: 1139124,
    count: 18,
    cells: 1,
    blob: BLOB,
};
/// `midi_inline_ccwithitems_off.png` — 42x14, 120 rects, 3 sprite cell(s).
pub const MIDI_INLINE_CCWITHITEMS_OFF: ArtData = ArtData {
    name: "midi_inline_ccwithitems_off",
    width: 42,
    height: 14,
    offset: 1139340,
    count: 120,
    cells: 3,
    blob: BLOB,
};
/// `midi_inline_ccwithitems_on.png` — 42x14, 120 rects, 3 sprite cell(s).
pub const MIDI_INLINE_CCWITHITEMS_ON: ArtData = ArtData {
    name: "midi_inline_ccwithitems_on",
    width: 42,
    height: 14,
    offset: 1140780,
    count: 120,
    cells: 3,
    blob: BLOB,
};
/// `midi_inline_close.png` — 42x14, 183 rects, 3 sprite cell(s).
pub const MIDI_INLINE_CLOSE: ArtData = ArtData {
    name: "midi_inline_close",
    width: 42,
    height: 14,
    offset: 1142220,
    count: 183,
    cells: 3,
    blob: BLOB,
};
/// `midi_inline_fold_custom_view.png` — 42x14, 135 rects, 3 sprite cell(s).
pub const MIDI_INLINE_FOLD_CUSTOM_VIEW: ArtData = ArtData {
    name: "midi_inline_fold_custom_view",
    width: 42,
    height: 14,
    offset: 1144416,
    count: 135,
    cells: 3,
    blob: BLOB,
};
/// `midi_inline_fold_none.png` — 42x14, 114 rects, 3 sprite cell(s).
pub const MIDI_INLINE_FOLD_NONE: ArtData = ArtData {
    name: "midi_inline_fold_none",
    width: 42,
    height: 14,
    offset: 1146036,
    count: 114,
    cells: 3,
    blob: BLOB,
};
/// `midi_inline_fold_unnamed.png` — 42x14, 123 rects, 3 sprite cell(s).
pub const MIDI_INLINE_FOLD_UNNAMED: ArtData = ArtData {
    name: "midi_inline_fold_unnamed",
    width: 42,
    height: 14,
    offset: 1147404,
    count: 123,
    cells: 3,
    blob: BLOB,
};
/// `midi_inline_fold_unused_unnamed.png` — 42x14, 132 rects, 3 sprite cell(s).
pub const MIDI_INLINE_FOLD_UNUSED_UNNAMED: ArtData = ArtData {
    name: "midi_inline_fold_unused_unnamed",
    width: 42,
    height: 14,
    offset: 1148880,
    count: 132,
    cells: 3,
    blob: BLOB,
};
/// `midi_inline_noteview_diamond.png` — 42x14, 105 rects, 3 sprite cell(s).
pub const MIDI_INLINE_NOTEVIEW_DIAMOND: ArtData = ArtData {
    name: "midi_inline_noteview_diamond",
    width: 42,
    height: 14,
    offset: 1150464,
    count: 105,
    cells: 3,
    blob: BLOB,
};
/// `midi_inline_noteview_rect.png` — 42x14, 27 rects, 3 sprite cell(s).
pub const MIDI_INLINE_NOTEVIEW_RECT: ArtData = ArtData {
    name: "midi_inline_noteview_rect",
    width: 42,
    height: 14,
    offset: 1151724,
    count: 27,
    cells: 3,
    blob: BLOB,
};
/// `midi_inline_noteview_triangle.png` — 42x14, 51 rects, 3 sprite cell(s).
pub const MIDI_INLINE_NOTEVIEW_TRIANGLE: ArtData = ArtData {
    name: "midi_inline_noteview_triangle",
    width: 42,
    height: 14,
    offset: 1152048,
    count: 51,
    cells: 3,
    blob: BLOB,
};
/// `midi_inline_scroll.png` — 42x14, 135 rects, 3 sprite cell(s).
pub const MIDI_INLINE_SCROLL: ArtData = ArtData {
    name: "midi_inline_scroll",
    width: 42,
    height: 14,
    offset: 1152660,
    count: 135,
    cells: 3,
    blob: BLOB,
};
/// `midi_inline_scrollbar.png` — 12x19, 1 rects, 1 sprite cell(s).
pub const MIDI_INLINE_SCROLLBAR: ArtData = ArtData {
    name: "midi_inline_scrollbar",
    width: 12,
    height: 19,
    offset: 1154280,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `midi_inline_scrollthumb.png` — 45x16, 0 rects, 1 sprite cell(s).
pub const MIDI_INLINE_SCROLLTHUMB: ArtData = ArtData {
    name: "midi_inline_scrollthumb",
    width: 45,
    height: 16,
    offset: 1154292,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `midi_item_bounds.png` — 25x10, 88 rects, 1 sprite cell(s).
pub const MIDI_ITEM_BOUNDS: ArtData = ArtData {
    name: "midi_item_bounds",
    width: 25,
    height: 10,
    offset: 1154292,
    count: 88,
    cells: 1,
    blob: BLOB,
};
/// `midi_note_colormap.png` — 157x130, 16814 rects, 1 sprite cell(s).
pub const MIDI_NOTE_COLORMAP: ArtData = ArtData {
    name: "midi_note_colormap",
    width: 157,
    height: 130,
    offset: 1155348,
    count: 16814,
    cells: 1,
    blob: BLOB,
};
/// `midi_score_colormap.png` — 157x130, 471 rects, 1 sprite cell(s).
pub const MIDI_SCORE_COLORMAP: ArtData = ArtData {
    name: "midi_score_colormap",
    width: 157,
    height: 130,
    offset: 1357116,
    count: 471,
    cells: 1,
    blob: BLOB,
};
/// `mixer_menu.png` — 60x20, 314 rects, 1 sprite cell(s).
pub const MIXER_MENU: ArtData = ArtData {
    name: "mixer_menu",
    width: 60,
    height: 20,
    offset: 1362768,
    count: 314,
    cells: 1,
    blob: BLOB,
};
/// `monitor_fx_byp.png` — 156x16, 686 rects, 3 sprite cell(s).
pub const MONITOR_FX_BYP: ArtData = ArtData {
    name: "monitor_fx_byp",
    width: 156,
    height: 16,
    offset: 1366536,
    count: 686,
    cells: 3,
    blob: BLOB,
};
/// `monitor_fx_byp_byp.png` — 42x16, 180 rects, 3 sprite cell(s).
pub const MONITOR_FX_BYP_BYP: ArtData = ArtData {
    name: "monitor_fx_byp_byp",
    width: 42,
    height: 16,
    offset: 1374768,
    count: 180,
    cells: 3,
    blob: BLOB,
};
/// `monitor_fx_byp_off.png` — 42x16, 180 rects, 3 sprite cell(s).
pub const MONITOR_FX_BYP_OFF: ArtData = ArtData {
    name: "monitor_fx_byp_off",
    width: 42,
    height: 16,
    offset: 1376928,
    count: 180,
    cells: 3,
    blob: BLOB,
};
/// `monitor_fx_byp_on.png` — 42x16, 174 rects, 3 sprite cell(s).
pub const MONITOR_FX_BYP_ON: ArtData = ArtData {
    name: "monitor_fx_byp_on",
    width: 42,
    height: 16,
    offset: 1379088,
    count: 174,
    cells: 3,
    blob: BLOB,
};
/// `monitor_fx_off.png` — 156x16, 686 rects, 3 sprite cell(s).
pub const MONITOR_FX_OFF: ArtData = ArtData {
    name: "monitor_fx_off",
    width: 156,
    height: 16,
    offset: 1381176,
    count: 686,
    cells: 3,
    blob: BLOB,
};
/// `monitor_fx_on.png` — 156x16, 686 rects, 3 sprite cell(s).
pub const MONITOR_FX_ON: ArtData = ArtData {
    name: "monitor_fx_on",
    width: 156,
    height: 16,
    offset: 1389408,
    count: 686,
    cells: 3,
    blob: BLOB,
};
/// `piano_black_key.png` — 43x30, 152 rects, 1 sprite cell(s).
pub const PIANO_BLACK_KEY: ArtData = ArtData {
    name: "piano_black_key",
    width: 43,
    height: 30,
    offset: 1397640,
    count: 152,
    cells: 1,
    blob: BLOB,
};
/// `piano_black_key_sel.png` — 39x28, 160 rects, 1 sprite cell(s).
pub const PIANO_BLACK_KEY_SEL: ArtData = ArtData {
    name: "piano_black_key_sel",
    width: 39,
    height: 28,
    offset: 1399464,
    count: 160,
    cells: 1,
    blob: BLOB,
};
/// `piano_white_key.png` — 39x20, 55 rects, 1 sprite cell(s).
pub const PIANO_WHITE_KEY: ArtData = ArtData {
    name: "piano_white_key",
    width: 39,
    height: 20,
    offset: 1401384,
    count: 55,
    cells: 1,
    blob: BLOB,
};
/// `piano_white_key_sel.png` — 39x20, 104 rects, 3 sprite cell(s).
pub const PIANO_WHITE_KEY_SEL: ArtData = ArtData {
    name: "piano_white_key_sel",
    width: 39,
    height: 20,
    offset: 1402044,
    count: 104,
    cells: 3,
    blob: BLOB,
};
/// `scrollbar.png` — 204x238, 936 rects, 1 sprite cell(s).
pub const SCROLLBAR: ArtData = ArtData {
    name: "scrollbar",
    width: 204,
    height: 238,
    offset: 1403292,
    count: 936,
    cells: 1,
    blob: BLOB,
};
/// `tab_down.png` — 33x18, 291 rects, 1 sprite cell(s).
pub const TAB_DOWN: ArtData = ArtData {
    name: "tab_down",
    width: 33,
    height: 18,
    offset: 1414524,
    count: 291,
    cells: 1,
    blob: BLOB,
};
/// `tab_down_sel.png` — 33x18, 294 rects, 1 sprite cell(s).
pub const TAB_DOWN_SEL: ArtData = ArtData {
    name: "tab_down_sel",
    width: 33,
    height: 18,
    offset: 1418016,
    count: 294,
    cells: 1,
    blob: BLOB,
};
/// `tab_up.png` — 33x18, 277 rects, 1 sprite cell(s).
pub const TAB_UP: ArtData = ArtData {
    name: "tab_up",
    width: 33,
    height: 18,
    offset: 1421544,
    count: 277,
    cells: 1,
    blob: BLOB,
};
/// `tab_up_sel.png` — 33x18, 296 rects, 1 sprite cell(s).
pub const TAB_UP_SEL: ArtData = ArtData {
    name: "tab_up_sel",
    width: 33,
    height: 18,
    offset: 1424868,
    count: 296,
    cells: 1,
    blob: BLOB,
};
/// `table_expand_off.png` — 48x16, 21 rects, 3 sprite cell(s).
pub const TABLE_EXPAND_OFF: ArtData = ArtData {
    name: "table_expand_off",
    width: 48,
    height: 16,
    offset: 1428420,
    count: 21,
    cells: 3,
    blob: BLOB,
};
/// `table_expand_on.png` — 48x16, 27 rects, 3 sprite cell(s).
pub const TABLE_EXPAND_ON: ArtData = ArtData {
    name: "table_expand_on",
    width: 48,
    height: 16,
    offset: 1428672,
    count: 27,
    cells: 3,
    blob: BLOB,
};
/// `table_locked_off.png` — 48x17, 30 rects, 3 sprite cell(s).
pub const TABLE_LOCKED_OFF: ArtData = ArtData {
    name: "table_locked_off",
    width: 48,
    height: 17,
    offset: 1428996,
    count: 30,
    cells: 3,
    blob: BLOB,
};
/// `table_locked_on.png` — 48x17, 27 rects, 3 sprite cell(s).
pub const TABLE_LOCKED_ON: ArtData = ArtData {
    name: "table_locked_on",
    width: 48,
    height: 17,
    offset: 1429356,
    count: 27,
    cells: 3,
    blob: BLOB,
};
/// `table_locked_partial.png` — 48x17, 30 rects, 3 sprite cell(s).
pub const TABLE_LOCKED_PARTIAL: ArtData = ArtData {
    name: "table_locked_partial",
    width: 48,
    height: 17,
    offset: 1429680,
    count: 30,
    cells: 3,
    blob: BLOB,
};
/// `table_mute_off.png` — 48x17, 162 rects, 3 sprite cell(s).
pub const TABLE_MUTE_OFF: ArtData = ArtData {
    name: "table_mute_off",
    width: 48,
    height: 17,
    offset: 1430040,
    count: 162,
    cells: 3,
    blob: BLOB,
};
/// `table_mute_on.png` — 48x16, 524 rects, 4 sprite cell(s).
pub const TABLE_MUTE_ON: ArtData = ArtData {
    name: "table_mute_on",
    width: 48,
    height: 16,
    offset: 1431984,
    count: 524,
    cells: 4,
    blob: BLOB,
};
/// `table_recarm_off.png` — 48x17, 174 rects, 3 sprite cell(s).
pub const TABLE_RECARM_OFF: ArtData = ArtData {
    name: "table_recarm_off",
    width: 48,
    height: 17,
    offset: 1438272,
    count: 174,
    cells: 3,
    blob: BLOB,
};
/// `table_recarm_on.png` — 48x16, 223 rects, 4 sprite cell(s).
pub const TABLE_RECARM_ON: ArtData = ArtData {
    name: "table_recarm_on",
    width: 48,
    height: 16,
    offset: 1440360,
    count: 223,
    cells: 4,
    blob: BLOB,
};
/// `table_remove_off.png` — 48x17, 216 rects, 3 sprite cell(s).
pub const TABLE_REMOVE_OFF: ArtData = ArtData {
    name: "table_remove_off",
    width: 48,
    height: 17,
    offset: 1443036,
    count: 216,
    cells: 3,
    blob: BLOB,
};
/// `table_remove_on.png` — 48x17, 216 rects, 3 sprite cell(s).
pub const TABLE_REMOVE_ON: ArtData = ArtData {
    name: "table_remove_on",
    width: 48,
    height: 17,
    offset: 1445628,
    count: 216,
    cells: 3,
    blob: BLOB,
};
/// `table_solo_off.png` — 48x17, 156 rects, 3 sprite cell(s).
pub const TABLE_SOLO_OFF: ArtData = ArtData {
    name: "table_solo_off",
    width: 48,
    height: 17,
    offset: 1448220,
    count: 156,
    cells: 3,
    blob: BLOB,
};
/// `table_solo_on.png` — 48x17, 200 rects, 4 sprite cell(s).
pub const TABLE_SOLO_ON: ArtData = ArtData {
    name: "table_solo_on",
    width: 48,
    height: 17,
    offset: 1450092,
    count: 200,
    cells: 4,
    blob: BLOB,
};
/// `table_sub_expand_off.png` — 48x16, 69 rects, 3 sprite cell(s).
pub const TABLE_SUB_EXPAND_OFF: ArtData = ArtData {
    name: "table_sub_expand_off",
    width: 48,
    height: 16,
    offset: 1452492,
    count: 69,
    cells: 3,
    blob: BLOB,
};
/// `table_sub_expand_on.png` — 48x16, 37 rects, 3 sprite cell(s).
pub const TABLE_SUB_EXPAND_ON: ArtData = ArtData {
    name: "table_sub_expand_on",
    width: 48,
    height: 16,
    offset: 1453320,
    count: 37,
    cells: 3,
    blob: BLOB,
};
/// `table_target_invalid.png` — 48x16, 42 rects, 3 sprite cell(s).
pub const TABLE_TARGET_INVALID: ArtData = ArtData {
    name: "table_target_invalid",
    width: 48,
    height: 16,
    offset: 1453764,
    count: 42,
    cells: 3,
    blob: BLOB,
};
/// `table_target_off.png` — 48x16, 42 rects, 3 sprite cell(s).
pub const TABLE_TARGET_OFF: ArtData = ArtData {
    name: "table_target_off",
    width: 48,
    height: 16,
    offset: 1454268,
    count: 42,
    cells: 3,
    blob: BLOB,
};
/// `table_target_on.png` — 48x16, 42 rects, 3 sprite cell(s).
pub const TABLE_TARGET_ON: ArtData = ArtData {
    name: "table_target_on",
    width: 48,
    height: 16,
    offset: 1454772,
    count: 42,
    cells: 3,
    blob: BLOB,
};
/// `table_visible_off.png` — 48x16, 48 rects, 3 sprite cell(s).
pub const TABLE_VISIBLE_OFF: ArtData = ArtData {
    name: "table_visible_off",
    width: 48,
    height: 16,
    offset: 1455276,
    count: 48,
    cells: 3,
    blob: BLOB,
};
/// `table_visible_on.png` — 48x16, 75 rects, 3 sprite cell(s).
pub const TABLE_VISIBLE_ON: ArtData = ArtData {
    name: "table_visible_on",
    width: 48,
    height: 16,
    offset: 1455852,
    count: 75,
    cells: 3,
    blob: BLOB,
};
/// `table_visible_partial.png` — 48x17, 96 rects, 3 sprite cell(s).
pub const TABLE_VISIBLE_PARTIAL: ArtData = ArtData {
    name: "table_visible_partial",
    width: 48,
    height: 17,
    offset: 1456752,
    count: 96,
    cells: 3,
    blob: BLOB,
};
/// `tcp_fxparm_bg.png` — 10x10, 0 rects, 2 sprite cell(s).
pub const TCP_FXPARM_BG: ArtData = ArtData {
    name: "tcp_fxparm_bg",
    width: 10,
    height: 10,
    offset: 1457904,
    count: 0,
    cells: 2,
    blob: BLOB,
};
/// `tcp_fxparm_byp.png` — 38x56, 239 rects, 1 sprite cell(s).
pub const TCP_FXPARM_BYP: ArtData = ArtData {
    name: "tcp_fxparm_byp",
    width: 38,
    height: 56,
    offset: 1457904,
    count: 239,
    cells: 1,
    blob: BLOB,
};
/// `tcp_fxparm_empty.png` — 38x56, 96 rects, 1 sprite cell(s).
pub const TCP_FXPARM_EMPTY: ArtData = ArtData {
    name: "tcp_fxparm_empty",
    width: 38,
    height: 56,
    offset: 1460772,
    count: 96,
    cells: 1,
    blob: BLOB,
};
/// `tcp_fxparm_fx_byp.png` — 38x56, 161 rects, 1 sprite cell(s).
pub const TCP_FXPARM_FX_BYP: ArtData = ArtData {
    name: "tcp_fxparm_fx_byp",
    width: 38,
    height: 56,
    offset: 1461924,
    count: 161,
    cells: 1,
    blob: BLOB,
};
/// `tcp_fxparm_fx_norm.png` — 38x56, 182 rects, 1 sprite cell(s).
pub const TCP_FXPARM_FX_NORM: ArtData = ArtData {
    name: "tcp_fxparm_fx_norm",
    width: 38,
    height: 56,
    offset: 1463856,
    count: 182,
    cells: 1,
    blob: BLOB,
};
/// `tcp_fxparm_fx_off.png` — 38x56, 161 rects, 1 sprite cell(s).
pub const TCP_FXPARM_FX_OFF: ArtData = ArtData {
    name: "tcp_fxparm_fx_off",
    width: 38,
    height: 56,
    offset: 1466040,
    count: 161,
    cells: 1,
    blob: BLOB,
};
/// `tcp_fxparm_knob_stack.png` — 23x1150, 5036 rects, 1 sprite cell(s).
pub const TCP_FXPARM_KNOB_STACK: ArtData = ArtData {
    name: "tcp_fxparm_knob_stack",
    width: 23,
    height: 1150,
    offset: 1467972,
    count: 5036,
    cells: 1,
    blob: BLOB,
};
/// `tcp_fxparm_norm.png` — 38x56, 239 rects, 1 sprite cell(s).
pub const TCP_FXPARM_NORM: ArtData = ArtData {
    name: "tcp_fxparm_norm",
    width: 38,
    height: 56,
    offset: 1528404,
    count: 239,
    cells: 1,
    blob: BLOB,
};
/// `tcp_fxparm_off.png` — 38x56, 231 rects, 1 sprite cell(s).
pub const TCP_FXPARM_OFF: ArtData = ArtData {
    name: "tcp_fxparm_off",
    width: 38,
    height: 56,
    offset: 1531272,
    count: 231,
    cells: 1,
    blob: BLOB,
};
/// `tcp_iconbg.png` — 11x13, 2 rects, 1 sprite cell(s).
pub const TCP_ICONBG: ArtData = ArtData {
    name: "tcp_iconbg",
    width: 11,
    height: 13,
    offset: 1534044,
    count: 2,
    cells: 1,
    blob: BLOB,
};
/// `tcp_iconbgsel.png` — 11x13, 2 rects, 1 sprite cell(s).
pub const TCP_ICONBGSEL: ArtData = ArtData {
    name: "tcp_iconbgsel",
    width: 11,
    height: 13,
    offset: 1534068,
    count: 2,
    cells: 1,
    blob: BLOB,
};
/// `tcp_idxbg.png` — 23x6, 3 rects, 1 sprite cell(s).
pub const TCP_IDXBG: ArtData = ArtData {
    name: "tcp_idxbg",
    width: 23,
    height: 6,
    offset: 1534092,
    count: 3,
    cells: 1,
    blob: BLOB,
};
/// `tcp_idxbg_sel.png` — 25x6, 5 rects, 1 sprite cell(s).
pub const TCP_IDXBG_SEL: ArtData = ArtData {
    name: "tcp_idxbg_sel",
    width: 25,
    height: 6,
    offset: 1534128,
    count: 5,
    cells: 1,
    blob: BLOB,
};
/// `tcp_main_namebg_sel.png` — 3x3, 0 rects, 1 sprite cell(s).
pub const TCP_MAIN_NAMEBG_SEL: ArtData = ArtData {
    name: "tcp_main_namebg_sel",
    width: 3,
    height: 3,
    offset: 1534188,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `tcp_mainbg.png` — 22x9, 1 rects, 2 sprite cell(s).
pub const TCP_MAINBG: ArtData = ArtData {
    name: "tcp_mainbg",
    width: 22,
    height: 9,
    offset: 1534188,
    count: 1,
    cells: 2,
    blob: BLOB,
};
/// `tcp_mainbgsel.png` — 22x9, 1 rects, 2 sprite cell(s).
pub const TCP_MAINBGSEL: ArtData = ArtData {
    name: "tcp_mainbgsel",
    width: 22,
    height: 9,
    offset: 1534200,
    count: 1,
    cells: 2,
    blob: BLOB,
};
/// `tcp_namebg.png` — 3x3, 0 rects, 1 sprite cell(s).
pub const TCP_NAMEBG: ArtData = ArtData {
    name: "tcp_namebg",
    width: 3,
    height: 3,
    offset: 1534212,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `tcp_pan_knob_small.png` — 24x25, 245 rects, 1 sprite cell(s).
pub const TCP_PAN_KNOB_SMALL: ArtData = ArtData {
    name: "tcp_pan_knob_small",
    width: 24,
    height: 25,
    offset: 1534212,
    count: 245,
    cells: 1,
    blob: BLOB,
};
/// `tcp_pan_knob_stack.png` — 18x1314, 2254 rects, 1 sprite cell(s).
pub const TCP_PAN_KNOB_STACK: ArtData = ArtData {
    name: "tcp_pan_knob_stack",
    width: 18,
    height: 1314,
    offset: 1537152,
    count: 2254,
    cells: 1,
    blob: BLOB,
};
/// `tcp_panbg.png` — 43x11, 41 rects, 1 sprite cell(s).
pub const TCP_PANBG: ArtData = ArtData {
    name: "tcp_panbg",
    width: 43,
    height: 11,
    offset: 1564200,
    count: 41,
    cells: 1,
    blob: BLOB,
};
/// `tcp_panthumb.png` — 13x23, 81 rects, 1 sprite cell(s).
pub const TCP_PANTHUMB: ArtData = ArtData {
    name: "tcp_panthumb",
    width: 13,
    height: 23,
    offset: 1564692,
    count: 81,
    cells: 1,
    blob: BLOB,
};
/// `tcp_recinput.png` — 20x22, 43 rects, 1 sprite cell(s).
pub const TCP_RECINPUT: ArtData = ArtData {
    name: "tcp_recinput",
    width: 20,
    height: 22,
    offset: 1565664,
    count: 43,
    cells: 1,
    blob: BLOB,
};
/// `tcp_send_knob_stack.png` — 23x1150, 5036 rects, 1 sprite cell(s).
pub const TCP_SEND_KNOB_STACK: ArtData = ArtData {
    name: "tcp_send_knob_stack",
    width: 23,
    height: 1150,
    offset: 1566180,
    count: 5036,
    cells: 1,
    blob: BLOB,
};
/// `tcp_sendlist_bg.png` — 10x10, 0 rects, 2 sprite cell(s).
pub const TCP_SENDLIST_BG: ArtData = ArtData {
    name: "tcp_sendlist_bg",
    width: 10,
    height: 10,
    offset: 1626612,
    count: 0,
    cells: 2,
    blob: BLOB,
};
/// `tcp_sendlist_empty.png` — 38x56, 163 rects, 1 sprite cell(s).
pub const TCP_SENDLIST_EMPTY: ArtData = ArtData {
    name: "tcp_sendlist_empty",
    width: 38,
    height: 56,
    offset: 1626612,
    count: 163,
    cells: 1,
    blob: BLOB,
};
/// `tcp_sendlist_meter.png` — 38x26, 34 rects, 1 sprite cell(s).
pub const TCP_SENDLIST_METER: ArtData = ArtData {
    name: "tcp_sendlist_meter",
    width: 38,
    height: 26,
    offset: 1628568,
    count: 34,
    cells: 1,
    blob: BLOB,
};
/// `tcp_sendlist_midihw.png` — 34x56, 228 rects, 1 sprite cell(s).
pub const TCP_SENDLIST_MIDIHW: ArtData = ArtData {
    name: "tcp_sendlist_midihw",
    width: 34,
    height: 56,
    offset: 1628976,
    count: 228,
    cells: 1,
    blob: BLOB,
};
/// `tcp_sendlist_mute.png` — 34x56, 228 rects, 1 sprite cell(s).
pub const TCP_SENDLIST_MUTE: ArtData = ArtData {
    name: "tcp_sendlist_mute",
    width: 34,
    height: 56,
    offset: 1631712,
    count: 228,
    cells: 1,
    blob: BLOB,
};
/// `tcp_sendlist_norm.png` — 34x56, 212 rects, 1 sprite cell(s).
pub const TCP_SENDLIST_NORM: ArtData = ArtData {
    name: "tcp_sendlist_norm",
    width: 34,
    height: 56,
    offset: 1634448,
    count: 212,
    cells: 1,
    blob: BLOB,
};
/// `tcp_solodefeat_on.png` — 65x24, 525 rects, 1 sprite cell(s).
pub const TCP_SOLODEFEAT_ON: ArtData = ArtData {
    name: "tcp_solodefeat_on",
    width: 65,
    height: 24,
    offset: 1636992,
    count: 525,
    cells: 1,
    blob: BLOB,
};
/// `tcp_vol_knob_small.png` — 26x28, 240 rects, 1 sprite cell(s).
pub const TCP_VOL_KNOB_SMALL: ArtData = ArtData {
    name: "tcp_vol_knob_small",
    width: 26,
    height: 28,
    offset: 1643292,
    count: 240,
    cells: 1,
    blob: BLOB,
};
/// `tcp_vol_knob_stack.png` — 20x939, 7534 rects, 1 sprite cell(s).
pub const TCP_VOL_KNOB_STACK: ArtData = ArtData {
    name: "tcp_vol_knob_stack",
    width: 20,
    height: 939,
    offset: 1646172,
    count: 7534,
    cells: 1,
    blob: BLOB,
};
/// `tcp_volbg.png` — 19x24, 21 rects, 1 sprite cell(s).
pub const TCP_VOLBG: ArtData = ArtData {
    name: "tcp_volbg",
    width: 19,
    height: 24,
    offset: 1736580,
    count: 21,
    cells: 1,
    blob: BLOB,
};
/// `tcp_volthumb.png` — 27x29, 72 rects, 1 sprite cell(s).
pub const TCP_VOLTHUMB: ArtData = ArtData {
    name: "tcp_volthumb",
    width: 27,
    height: 29,
    offset: 1736832,
    count: 72,
    cells: 1,
    blob: BLOB,
};
/// `tcp_wid_knob_stack.png` — 18x1314, 3458 rects, 1 sprite cell(s).
pub const TCP_WID_KNOB_STACK: ArtData = ArtData {
    name: "tcp_wid_knob_stack",
    width: 18,
    height: 1314,
    offset: 1737696,
    count: 3458,
    cells: 1,
    blob: BLOB,
};
/// `tcp_width_knob_small.png` — 24x25, 245 rects, 1 sprite cell(s).
pub const TCP_WIDTH_KNOB_SMALL: ArtData = ArtData {
    name: "tcp_width_knob_small",
    width: 24,
    height: 25,
    offset: 1779192,
    count: 245,
    cells: 1,
    blob: BLOB,
};
/// `tcp_widthbg.png` — 43x11, 41 rects, 1 sprite cell(s).
pub const TCP_WIDTHBG: ArtData = ArtData {
    name: "tcp_widthbg",
    width: 43,
    height: 11,
    offset: 1782132,
    count: 41,
    cells: 1,
    blob: BLOB,
};
/// `tcp_widththumb.png` — 13x23, 81 rects, 1 sprite cell(s).
pub const TCP_WIDTHTHUMB: ArtData = ArtData {
    name: "tcp_widththumb",
    width: 13,
    height: 23,
    offset: 1782624,
    count: 81,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_add.png` — 90x30, 99 rects, 3 sprite cell(s).
pub const TOOLBAR_ADD: ArtData = ArtData {
    name: "toolbar_add",
    width: 90,
    height: 30,
    offset: 1783596,
    count: 99,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform.png` — 90x30, 255 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM: ArtData = ArtData {
    name: "toolbar_audio_waveform",
    width: 90,
    height: 30,
    offset: 1784784,
    count: 255,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_delete_remove.png` — 90x30, 390 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_audio_waveform_delete_remove",
    width: 90,
    height: 30,
    offset: 1787844,
    count: 390,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_delete_silence.png` — 90x30, 375 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_DELETE_SILENCE: ArtData = ArtData {
    name: "toolbar_audio_waveform_delete_silence",
    width: 90,
    height: 30,
    offset: 1792524,
    count: 375,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_digital_sample_rate.png` — 90x30, 108 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_DIGITAL_SAMPLE_RATE: ArtData = ArtData {
    name: "toolbar_audio_waveform_digital_sample_rate",
    width: 90,
    height: 30,
    offset: 1797024,
    count: 108,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_disk_load.png` — 90x30, 492 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_DISK_LOAD: ArtData = ArtData {
    name: "toolbar_audio_waveform_disk_load",
    width: 90,
    height: 30,
    offset: 1798320,
    count: 492,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_folder.png` — 90x30, 264 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_FOLDER: ArtData = ArtData {
    name: "toolbar_audio_waveform_folder",
    width: 90,
    height: 30,
    offset: 1804224,
    count: 264,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_metronome.png` — 90x30, 444 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_METRONOME: ArtData = ArtData {
    name: "toolbar_audio_waveform_metronome",
    width: 90,
    height: 30,
    offset: 1807392,
    count: 444,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_move_grid_quantize.png` — 90x30, 291 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_MOVE_GRID_QUANTIZE: ArtData = ArtData {
    name: "toolbar_audio_waveform_move_grid_quantize",
    width: 90,
    height: 30,
    offset: 1812720,
    count: 291,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_normalize_gain.png` — 90x30, 339 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_NORMALIZE_GAIN: ArtData = ArtData {
    name: "toolbar_audio_waveform_normalize_gain",
    width: 90,
    height: 30,
    offset: 1816212,
    count: 339,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_normalize_gain_common_locked.png` — 90x30, 300 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_NORMALIZE_GAIN_COMMON_LOCKED: ArtData = ArtData {
    name: "toolbar_audio_waveform_normalize_gain_common_locked",
    width: 90,
    height: 30,
    offset: 1820280,
    count: 300,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_primary_external_editor.png` — 90x30, 195 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_PRIMARY_EXTERNAL_EDITOR: ArtData = ArtData {
    name: "toolbar_audio_waveform_primary_external_editor",
    width: 90,
    height: 30,
    offset: 1823880,
    count: 195,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_properties.png` — 90x30, 339 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_PROPERTIES: ArtData = ArtData {
    name: "toolbar_audio_waveform_properties",
    width: 90,
    height: 30,
    offset: 1826220,
    count: 339,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_render_disk_mono.png` — 90x30, 399 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_RENDER_DISK_MONO: ArtData = ArtData {
    name: "toolbar_audio_waveform_render_disk_mono",
    width: 90,
    height: 30,
    offset: 1830288,
    count: 399,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_render_disk_stereo.png` — 90x30, 504 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_RENDER_DISK_STEREO: ArtData = ArtData {
    name: "toolbar_audio_waveform_render_disk_stereo",
    width: 90,
    height: 30,
    offset: 1835076,
    count: 504,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_render_effects_mono.png` — 90x30, 402 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_RENDER_EFFECTS_MONO: ArtData = ArtData {
    name: "toolbar_audio_waveform_render_effects_mono",
    width: 90,
    height: 30,
    offset: 1841124,
    count: 402,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_render_effects_stereo.png` — 90x30, 498 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_RENDER_EFFECTS_STEREO: ArtData = ArtData {
    name: "toolbar_audio_waveform_render_effects_stereo",
    width: 90,
    height: 30,
    offset: 1845948,
    count: 498,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_reverse.png` — 90x30, 345 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_REVERSE: ArtData = ArtData {
    name: "toolbar_audio_waveform_reverse",
    width: 90,
    height: 30,
    offset: 1851924,
    count: 345,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_secondary_external_editor.png` — 90x30, 204 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_SECONDARY_EXTERNAL_EDITOR: ArtData = ArtData {
    name: "toolbar_audio_waveform_secondary_external_editor",
    width: 90,
    height: 30,
    offset: 1856064,
    count: 204,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_selection.png` — 90x30, 321 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_SELECTION: ArtData = ArtData {
    name: "toolbar_audio_waveform_selection",
    width: 90,
    height: 30,
    offset: 1858512,
    count: 321,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_selection_trim.png` — 90x30, 285 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_SELECTION_TRIM: ArtData = ArtData {
    name: "toolbar_audio_waveform_selection_trim",
    width: 90,
    height: 30,
    offset: 1862364,
    count: 285,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_system.png` — 90x30, 582 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_SYSTEM: ArtData = ArtData {
    name: "toolbar_audio_waveform_system",
    width: 90,
    height: 30,
    offset: 1865784,
    count: 582,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_time_selection_render.png` — 90x30, 318 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_TIME_SELECTION_RENDER: ArtData = ArtData {
    name: "toolbar_audio_waveform_time_selection_render",
    width: 90,
    height: 30,
    offset: 1872768,
    count: 318,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_time_selection_render_stereo.png` — 90x30, 378 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_TIME_SELECTION_RENDER_STEREO: ArtData = ArtData {
    name: "toolbar_audio_waveform_time_selection_render_stereo",
    width: 90,
    height: 30,
    offset: 1876584,
    count: 378,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_transient_dynamic_split.png` — 90x30, 399 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_TRANSIENT_DYNAMIC_SPLIT: ArtData = ArtData {
    name: "toolbar_audio_waveform_transient_dynamic_split",
    width: 90,
    height: 30,
    offset: 1881120,
    count: 399,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_transient_dynamic_split_lines.png` — 90x30, 393 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_TRANSIENT_DYNAMIC_SPLIT_LINES: ArtData = ArtData {
    name: "toolbar_audio_waveform_transient_dynamic_split_lines",
    width: 90,
    height: 30,
    offset: 1885908,
    count: 393,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_audio_waveform_transient_dynamic_split_scissors.png` — 90x30, 543 rects, 3 sprite cell(s).
pub const TOOLBAR_AUDIO_WAVEFORM_TRANSIENT_DYNAMIC_SPLIT_SCISSORS: ArtData = ArtData {
    name: "toolbar_audio_waveform_transient_dynamic_split_scissors",
    width: 90,
    height: 30,
    offset: 1890624,
    count: 543,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_bass_clef_note.png` — 90x30, 285 rects, 3 sprite cell(s).
pub const TOOLBAR_BASS_CLEF_NOTE: ArtData = ArtData {
    name: "toolbar_bass_clef_note",
    width: 90,
    height: 30,
    offset: 1897140,
    count: 285,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_blank.png` — 90x30, 188 rects, 1 sprite cell(s).
pub const TOOLBAR_BLANK: ArtData = ArtData {
    name: "toolbar_blank",
    width: 90,
    height: 30,
    offset: 1900560,
    count: 188,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_blank_inverted.png` — 90x30, 37 rects, 3 sprite cell(s).
pub const TOOLBAR_BLANK_INVERTED: ArtData = ArtData {
    name: "toolbar_blank_inverted",
    width: 90,
    height: 30,
    offset: 1902816,
    count: 37,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_clip_properties.png` — 90x30, 549 rects, 3 sprite cell(s).
pub const TOOLBAR_CLIP_PROPERTIES: ArtData = ArtData {
    name: "toolbar_clip_properties",
    width: 90,
    height: 30,
    offset: 1903260,
    count: 549,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_clipboard_copy.png` — 90x30, 162 rects, 3 sprite cell(s).
pub const TOOLBAR_CLIPBOARD_COPY: ArtData = ArtData {
    name: "toolbar_clipboard_copy",
    width: 90,
    height: 30,
    offset: 1909848,
    count: 162,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_clipboard_cut.png` — 90x30, 132 rects, 3 sprite cell(s).
pub const TOOLBAR_CLIPBOARD_CUT: ArtData = ArtData {
    name: "toolbar_clipboard_cut",
    width: 90,
    height: 30,
    offset: 1911792,
    count: 132,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_clipboard_paste.png` — 90x30, 156 rects, 3 sprite cell(s).
pub const TOOLBAR_CLIPBOARD_PASTE: ArtData = ArtData {
    name: "toolbar_clipboard_paste",
    width: 90,
    height: 30,
    offset: 1913376,
    count: 156,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_dynamic_volume_ff.png` — 90x30, 369 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_DYNAMIC_VOLUME_FF: ArtData = ArtData {
    name: "toolbar_color_dynamic_volume_ff",
    width: 90,
    height: 30,
    offset: 1915248,
    count: 369,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_item.png` — 90x30, 303 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_ITEM: ArtData = ArtData {
    name: "toolbar_color_item",
    width: 90,
    height: 30,
    offset: 1919676,
    count: 303,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_item_selected.png` — 90x30, 296 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_ITEM_SELECTED: ArtData = ArtData {
    name: "toolbar_color_item_selected",
    width: 90,
    height: 30,
    offset: 1923312,
    count: 296,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_load_disk.png` — 90x30, 342 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_LOAD_DISK: ArtData = ArtData {
    name: "toolbar_color_load_disk",
    width: 90,
    height: 30,
    offset: 1926864,
    count: 342,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_midi_channel.png` — 90x30, 228 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_MIDI_CHANNEL: ArtData = ArtData {
    name: "toolbar_color_midi_channel",
    width: 90,
    height: 30,
    offset: 1930968,
    count: 228,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_none_delete_remove.png` — 90x30, 201 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_NONE_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_color_none_delete_remove",
    width: 90,
    height: 30,
    offset: 1933704,
    count: 201,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_note_pitch.png` — 90x30, 150 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_NOTE_PITCH: ArtData = ArtData {
    name: "toolbar_color_note_pitch",
    width: 90,
    height: 30,
    offset: 1936116,
    count: 150,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_properties.png` — 90x30, 162 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_PROPERTIES: ArtData = ArtData {
    name: "toolbar_color_properties",
    width: 90,
    height: 30,
    offset: 1937916,
    count: 162,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_random_question.png` — 90x30, 252 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_RANDOM_QUESTION: ArtData = ArtData {
    name: "toolbar_color_random_question",
    width: 90,
    height: 30,
    offset: 1939860,
    count: 252,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_region.png` — 90x30, 63 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_REGION: ArtData = ArtData {
    name: "toolbar_color_region",
    width: 90,
    height: 30,
    offset: 1942884,
    count: 63,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_selecte_delete_remove.png` — 90x30, 54 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_SELECTE_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_color_selecte_delete_remove",
    width: 90,
    height: 30,
    offset: 1943640,
    count: 54,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_selected.png` — 90x30, 54 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_SELECTED: ArtData = ArtData {
    name: "toolbar_color_selected",
    width: 90,
    height: 30,
    offset: 1944288,
    count: 54,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_source_input_channel.png` — 90x30, 144 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_SOURCE_INPUT_CHANNEL: ArtData = ArtData {
    name: "toolbar_color_source_input_channel",
    width: 90,
    height: 30,
    offset: 1944936,
    count: 144,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_sws_extension.png` — 90x30, 471 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_SWS_EXTENSION: ArtData = ArtData {
    name: "toolbar_color_sws_extension",
    width: 90,
    height: 30,
    offset: 1946664,
    count: 471,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_take_lane.png` — 90x30, 240 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_TAKE_LANE: ArtData = ArtData {
    name: "toolbar_color_take_lane",
    width: 90,
    height: 30,
    offset: 1952316,
    count: 240,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_color_track.png` — 90x30, 282 rects, 3 sprite cell(s).
pub const TOOLBAR_COLOR_TRACK: ArtData = ArtData {
    name: "toolbar_color_track",
    width: 90,
    height: 30,
    offset: 1955196,
    count: 282,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_cpu_offline.png` — 90x30, 204 rects, 3 sprite cell(s).
pub const TOOLBAR_CPU_OFFLINE: ArtData = ArtData {
    name: "toolbar_cpu_offline",
    width: 90,
    height: 30,
    offset: 1958580,
    count: 204,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_cpu_online.png` — 90x30, 225 rects, 3 sprite cell(s).
pub const TOOLBAR_CPU_ONLINE: ArtData = ArtData {
    name: "toolbar_cpu_online",
    width: 90,
    height: 30,
    offset: 1961028,
    count: 225,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_cpu_properties_performance.png` — 90x30, 177 rects, 3 sprite cell(s).
pub const TOOLBAR_CPU_PROPERTIES_PERFORMANCE: ArtData = ArtData {
    name: "toolbar_cpu_properties_performance",
    width: 90,
    height: 30,
    offset: 1963728,
    count: 177,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_delete.png` — 90x30, 231 rects, 3 sprite cell(s).
pub const TOOLBAR_DELETE: ArtData = ArtData {
    name: "toolbar_delete",
    width: 90,
    height: 30,
    offset: 1965852,
    count: 231,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_disk_properties_resource_path.png` — 90x30, 318 rects, 3 sprite cell(s).
pub const TOOLBAR_DISK_PROPERTIES_RESOURCE_PATH: ArtData = ArtData {
    name: "toolbar_disk_properties_resource_path",
    width: 90,
    height: 30,
    offset: 1968624,
    count: 318,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_dock.png` — 90x30, 234 rects, 3 sprite cell(s).
pub const TOOLBAR_DOCK: ArtData = ArtData {
    name: "toolbar_dock",
    width: 90,
    height: 30,
    offset: 1972440,
    count: 234,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_dock_off.png` — 90x30, 488 rects, 1 sprite cell(s).
pub const TOOLBAR_DOCK_OFF: ArtData = ArtData {
    name: "toolbar_dock_off",
    width: 90,
    height: 30,
    offset: 1975248,
    count: 488,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_dock_on.png` — 90x30, 488 rects, 1 sprite cell(s).
pub const TOOLBAR_DOCK_ON: ArtData = ArtData {
    name: "toolbar_dock_on",
    width: 90,
    height: 30,
    offset: 1981104,
    count: 488,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_dotted_note.png` — 90x30, 51 rects, 3 sprite cell(s).
pub const TOOLBAR_DOTTED_NOTE: ArtData = ArtData {
    name: "toolbar_dotted_note",
    width: 90,
    height: 30,
    offset: 1986960,
    count: 51,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_eighth_quaver_grid.png` — 90x30, 147 rects, 3 sprite cell(s).
pub const TOOLBAR_EIGHTH_QUAVER_GRID: ArtData = ArtData {
    name: "toolbar_eighth_quaver_grid",
    width: 90,
    height: 30,
    offset: 1987572,
    count: 147,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_eighth_quaver_note.png` — 90x30, 111 rects, 3 sprite cell(s).
pub const TOOLBAR_EIGHTH_QUAVER_NOTE: ArtData = ArtData {
    name: "toolbar_eighth_quaver_note",
    width: 90,
    height: 30,
    offset: 1989336,
    count: 111,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_env_auto_latch.png` — 90x30, 279 rects, 3 sprite cell(s).
pub const TOOLBAR_ENV_AUTO_LATCH: ArtData = ArtData {
    name: "toolbar_env_auto_latch",
    width: 90,
    height: 30,
    offset: 1990668,
    count: 279,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_env_auto_preview.png` — 90x30, 336 rects, 3 sprite cell(s).
pub const TOOLBAR_ENV_AUTO_PREVIEW: ArtData = ArtData {
    name: "toolbar_env_auto_preview",
    width: 90,
    height: 30,
    offset: 1994016,
    count: 336,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_env_auto_read.png` — 90x30, 282 rects, 3 sprite cell(s).
pub const TOOLBAR_ENV_AUTO_READ: ArtData = ArtData {
    name: "toolbar_env_auto_read",
    width: 90,
    height: 30,
    offset: 1998048,
    count: 282,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_env_auto_touch.png` — 90x30, 276 rects, 3 sprite cell(s).
pub const TOOLBAR_ENV_AUTO_TOUCH: ArtData = ArtData {
    name: "toolbar_env_auto_touch",
    width: 90,
    height: 30,
    offset: 2001432,
    count: 276,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_env_auto_trim.png` — 90x30, 249 rects, 3 sprite cell(s).
pub const TOOLBAR_ENV_AUTO_TRIM: ArtData = ArtData {
    name: "toolbar_env_auto_trim",
    width: 90,
    height: 30,
    offset: 2004744,
    count: 249,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_env_auto_write.png` — 90x30, 288 rects, 3 sprite cell(s).
pub const TOOLBAR_ENV_AUTO_WRITE: ArtData = ArtData {
    name: "toolbar_env_auto_write",
    width: 90,
    height: 30,
    offset: 2007732,
    count: 288,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_copy.png` — 90x30, 486 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_COPY: ArtData = ArtData {
    name: "toolbar_envelope_copy",
    width: 90,
    height: 30,
    offset: 2011188,
    count: 486,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_delete_remove.png` — 90x30, 315 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_envelope_delete_remove",
    width: 90,
    height: 30,
    offset: 2017020,
    count: 315,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_fade_shape_cycle.png` — 90x30, 339 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_FADE_SHAPE_CYCLE: ArtData = ArtData {
    name: "toolbar_envelope_fade_shape_cycle",
    width: 90,
    height: 30,
    offset: 2020800,
    count: 339,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_fade_shape_none_default_delete_remove.png` — 90x30, 375 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_FADE_SHAPE_NONE_DEFAULT_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_envelope_fade_shape_none_default_delete_remove",
    width: 90,
    height: 30,
    offset: 2024868,
    count: 375,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_insert_four.png` — 90x30, 390 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_INSERT_FOUR: ArtData = ArtData {
    name: "toolbar_envelope_insert_four",
    width: 90,
    height: 30,
    offset: 2029368,
    count: 390,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_item_selected.png` — 90x30, 240 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_ITEM_SELECTED: ArtData = ArtData {
    name: "toolbar_envelope_item_selected",
    width: 90,
    height: 30,
    offset: 2034048,
    count: 240,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_item_selected_replace.png` — 90x30, 240 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_ITEM_SELECTED_REPLACE: ArtData = ArtData {
    name: "toolbar_envelope_item_selected_replace",
    width: 90,
    height: 30,
    offset: 2036928,
    count: 240,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_knob_parameter_volume.png` — 90x30, 411 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_KNOB_PARAMETER_VOLUME: ArtData = ArtData {
    name: "toolbar_envelope_knob_parameter_volume",
    width: 90,
    height: 30,
    offset: 2039808,
    count: 411,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_lock.png` — 90x30, 198 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_LOCK: ArtData = ArtData {
    name: "toolbar_envelope_lock",
    width: 90,
    height: 30,
    offset: 2044740,
    count: 198,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_mute.png` — 90x30, 195 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_MUTE: ArtData = ArtData {
    name: "toolbar_envelope_mute",
    width: 90,
    height: 30,
    offset: 2047116,
    count: 195,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_new.png` — 90x30, 276 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_NEW: ArtData = ArtData {
    name: "toolbar_envelope_new",
    width: 90,
    height: 30,
    offset: 2049456,
    count: 276,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_pan.png` — 90x30, 204 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_PAN: ArtData = ArtData {
    name: "toolbar_envelope_pan",
    width: 90,
    height: 30,
    offset: 2052768,
    count: 204,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_pitch_note.png` — 90x30, 255 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_PITCH_NOTE: ArtData = ArtData {
    name: "toolbar_envelope_pitch_note",
    width: 90,
    height: 30,
    offset: 2055216,
    count: 255,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_point_delete_remove.png` — 90x30, 204 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_POINT_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_envelope_point_delete_remove",
    width: 90,
    height: 30,
    offset: 2058276,
    count: 204,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_point_insert.png` — 90x30, 297 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_POINT_INSERT: ArtData = ArtData {
    name: "toolbar_envelope_point_insert",
    width: 90,
    height: 30,
    offset: 2060724,
    count: 297,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_point_move_axis.png` — 90x30, 372 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_POINT_MOVE_AXIS: ArtData = ArtData {
    name: "toolbar_envelope_point_move_axis",
    width: 90,
    height: 30,
    offset: 2064288,
    count: 372,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_point_move_down.png` — 90x30, 231 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_POINT_MOVE_DOWN: ArtData = ArtData {
    name: "toolbar_envelope_point_move_down",
    width: 90,
    height: 30,
    offset: 2068752,
    count: 231,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_point_move_left.png` — 90x30, 243 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_POINT_MOVE_LEFT: ArtData = ArtData {
    name: "toolbar_envelope_point_move_left",
    width: 90,
    height: 30,
    offset: 2071524,
    count: 243,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_point_move_right.png` — 90x30, 240 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_POINT_MOVE_RIGHT: ArtData = ArtData {
    name: "toolbar_envelope_point_move_right",
    width: 90,
    height: 30,
    offset: 2074440,
    count: 240,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_point_move_up.png` — 90x30, 231 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_POINT_MOVE_UP: ArtData = ArtData {
    name: "toolbar_envelope_point_move_up",
    width: 90,
    height: 30,
    offset: 2077320,
    count: 231,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_point_new.png` — 90x30, 279 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_POINT_NEW: ArtData = ArtData {
    name: "toolbar_envelope_point_new",
    width: 90,
    height: 30,
    offset: 2080092,
    count: 279,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_point_time_selection_cut_scissors.png` — 90x30, 276 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_POINT_TIME_SELECTION_CUT_SCISSORS: ArtData = ArtData {
    name: "toolbar_envelope_point_time_selection_cut_scissors",
    width: 90,
    height: 30,
    offset: 2083440,
    count: 276,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_point_time_selection_delete_remove.png` — 90x30, 270 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_POINT_TIME_SELECTION_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_envelope_point_time_selection_delete_remove",
    width: 90,
    height: 30,
    offset: 2086752,
    count: 270,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_reduce_number_points_delete_remove.png` — 90x30, 270 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_REDUCE_NUMBER_POINTS_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_envelope_reduce_number_points_delete_remove",
    width: 90,
    height: 30,
    offset: 2089992,
    count: 270,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_show.png` — 90x30, 324 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_SHOW: ArtData = ArtData {
    name: "toolbar_envelope_show",
    width: 90,
    height: 30,
    offset: 2093232,
    count: 324,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_tempo_time_clock.png` — 90x30, 416 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_TEMPO_TIME_CLOCK: ArtData = ArtData {
    name: "toolbar_envelope_tempo_time_clock",
    width: 90,
    height: 30,
    offset: 2097120,
    count: 416,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_time_selection.png` — 90x30, 147 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_TIME_SELECTION: ArtData = ArtData {
    name: "toolbar_envelope_time_selection",
    width: 90,
    height: 30,
    offset: 2102112,
    count: 147,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envelope_vol.png` — 90x30, 177 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVELOPE_VOL: ArtData = ArtData {
    name: "toolbar_envelope_vol",
    width: 90,
    height: 30,
    offset: 2103876,
    count: 177,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envitem.png` — 90x30, 153 rects, 3 sprite cell(s).
pub const TOOLBAR_ENVITEM: ArtData = ArtData {
    name: "toolbar_envitem",
    width: 90,
    height: 30,
    offset: 2106000,
    count: 153,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_envitem_off.png` — 90x30, 430 rects, 1 sprite cell(s).
pub const TOOLBAR_ENVITEM_OFF: ArtData = ArtData {
    name: "toolbar_envitem_off",
    width: 90,
    height: 30,
    offset: 2107836,
    count: 430,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_envitem_on.png` — 90x30, 429 rects, 1 sprite cell(s).
pub const TOOLBAR_ENVITEM_ON: ArtData = ArtData {
    name: "toolbar_envitem_on",
    width: 90,
    height: 30,
    offset: 2112996,
    count: 429,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_autoplay.png` — 90x30, 408 rects, 3 sprite cell(s).
pub const TOOLBAR_EX_AUTOPLAY: ArtData = ArtData {
    name: "toolbar_ex_autoplay",
    width: 90,
    height: 30,
    offset: 2118144,
    count: 408,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_ex_autoplay_off.png` — 90x30, 737 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_AUTOPLAY_OFF: ArtData = ArtData {
    name: "toolbar_ex_autoplay_off",
    width: 90,
    height: 30,
    offset: 2123040,
    count: 737,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_autoplay_on.png` — 90x30, 737 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_AUTOPLAY_ON: ArtData = ArtData {
    name: "toolbar_ex_autoplay_on",
    width: 90,
    height: 30,
    offset: 2131884,
    count: 737,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_insert_open.png` — 90x30, 876 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_INSERT_OPEN: ArtData = ArtData {
    name: "toolbar_ex_insert_open",
    width: 90,
    height: 30,
    offset: 2140728,
    count: 876,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_pitch_detect.png` — 90x30, 558 rects, 3 sprite cell(s).
pub const TOOLBAR_EX_PITCH_DETECT: ArtData = ArtData {
    name: "toolbar_ex_pitch_detect",
    width: 90,
    height: 30,
    offset: 2151240,
    count: 558,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_ex_pitch_detect_off.png` — 90x30, 921 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_PITCH_DETECT_OFF: ArtData = ArtData {
    name: "toolbar_ex_pitch_detect_off",
    width: 90,
    height: 30,
    offset: 2157936,
    count: 921,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_pitch_detect_on.png` — 90x30, 921 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_PITCH_DETECT_ON: ArtData = ArtData {
    name: "toolbar_ex_pitch_detect_on",
    width: 90,
    height: 30,
    offset: 2168988,
    count: 921,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_preserve_pitch_tempo_matching.png` — 90x30, 510 rects, 3 sprite cell(s).
pub const TOOLBAR_EX_PRESERVE_PITCH_TEMPO_MATCHING: ArtData = ArtData {
    name: "toolbar_ex_preserve_pitch_tempo_matching",
    width: 90,
    height: 30,
    offset: 2180040,
    count: 510,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_ex_preserve_pitch_tempo_matching_off.png` — 90x30, 880 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_PRESERVE_PITCH_TEMPO_MATCHING_OFF: ArtData = ArtData {
    name: "toolbar_ex_preserve_pitch_tempo_matching_off",
    width: 90,
    height: 30,
    offset: 2186160,
    count: 880,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_preserve_pitch_tempo_matching_on.png` — 90x30, 880 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_PRESERVE_PITCH_TEMPO_MATCHING_ON: ArtData = ArtData {
    name: "toolbar_ex_preserve_pitch_tempo_matching_on",
    width: 90,
    height: 30,
    offset: 2196720,
    count: 880,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_properties_for_current_media.png` — 90x30, 339 rects, 3 sprite cell(s).
pub const TOOLBAR_EX_PROPERTIES_FOR_CURRENT_MEDIA: ArtData = ArtData {
    name: "toolbar_ex_properties_for_current_media",
    width: 90,
    height: 30,
    offset: 2207280,
    count: 339,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_ex_properties_for_current_media_off.png` — 90x30, 754 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_PROPERTIES_FOR_CURRENT_MEDIA_OFF: ArtData = ArtData {
    name: "toolbar_ex_properties_for_current_media_off",
    width: 90,
    height: 30,
    offset: 2211348,
    count: 754,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_properties_for_current_media_on.png` — 90x30, 754 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_PROPERTIES_FOR_CURRENT_MEDIA_ON: ArtData = ArtData {
    name: "toolbar_ex_properties_for_current_media_on",
    width: 90,
    height: 30,
    offset: 2220396,
    count: 754,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_start_on_bar.png` — 90x30, 147 rects, 3 sprite cell(s).
pub const TOOLBAR_EX_START_ON_BAR: ArtData = ArtData {
    name: "toolbar_ex_start_on_bar",
    width: 90,
    height: 30,
    offset: 2229444,
    count: 147,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_ex_start_on_bar_off.png` — 90x30, 455 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_START_ON_BAR_OFF: ArtData = ArtData {
    name: "toolbar_ex_start_on_bar_off",
    width: 90,
    height: 30,
    offset: 2231208,
    count: 455,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_start_on_bar_on.png` — 90x30, 455 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_START_ON_BAR_ON: ArtData = ArtData {
    name: "toolbar_ex_start_on_bar_on",
    width: 90,
    height: 30,
    offset: 2236668,
    count: 455,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_tempo_match.png` — 90x30, 126 rects, 3 sprite cell(s).
pub const TOOLBAR_EX_TEMPO_MATCH: ArtData = ArtData {
    name: "toolbar_ex_tempo_match",
    width: 90,
    height: 30,
    offset: 2242128,
    count: 126,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_ex_tempo_match_double.png` — 90x30, 228 rects, 3 sprite cell(s).
pub const TOOLBAR_EX_TEMPO_MATCH_DOUBLE: ArtData = ArtData {
    name: "toolbar_ex_tempo_match_double",
    width: 90,
    height: 30,
    offset: 2243640,
    count: 228,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_ex_tempo_match_double_off.png` — 90x30, 515 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_TEMPO_MATCH_DOUBLE_OFF: ArtData = ArtData {
    name: "toolbar_ex_tempo_match_double_off",
    width: 90,
    height: 30,
    offset: 2246376,
    count: 515,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_tempo_match_double_on.png` — 90x30, 515 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_TEMPO_MATCH_DOUBLE_ON: ArtData = ArtData {
    name: "toolbar_ex_tempo_match_double_on",
    width: 90,
    height: 30,
    offset: 2252556,
    count: 515,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_tempo_match_half.png` — 90x30, 219 rects, 3 sprite cell(s).
pub const TOOLBAR_EX_TEMPO_MATCH_HALF: ArtData = ArtData {
    name: "toolbar_ex_tempo_match_half",
    width: 90,
    height: 30,
    offset: 2258736,
    count: 219,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_ex_tempo_match_half_off.png` — 90x30, 528 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_TEMPO_MATCH_HALF_OFF: ArtData = ArtData {
    name: "toolbar_ex_tempo_match_half_off",
    width: 90,
    height: 30,
    offset: 2261364,
    count: 528,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_tempo_match_half_on.png` — 90x30, 528 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_TEMPO_MATCH_HALF_ON: ArtData = ArtData {
    name: "toolbar_ex_tempo_match_half_on",
    width: 90,
    height: 30,
    offset: 2267700,
    count: 528,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_tempo_match_off.png` — 90x30, 384 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_TEMPO_MATCH_OFF: ArtData = ArtData {
    name: "toolbar_ex_tempo_match_off",
    width: 90,
    height: 30,
    offset: 2274036,
    count: 384,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ex_tempo_match_on.png` — 90x30, 384 rects, 1 sprite cell(s).
pub const TOOLBAR_EX_TEMPO_MATCH_ON: ArtData = ArtData {
    name: "toolbar_ex_tempo_match_on",
    width: 90,
    height: 30,
    offset: 2278644,
    count: 384,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_filter.png` — 90x30, 87 rects, 3 sprite cell(s).
pub const TOOLBAR_FILTER: ArtData = ArtData {
    name: "toolbar_filter",
    width: 90,
    height: 30,
    offset: 2283252,
    count: 87,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_filter_off.png` — 90x30, 333 rects, 1 sprite cell(s).
pub const TOOLBAR_FILTER_OFF: ArtData = ArtData {
    name: "toolbar_filter_off",
    width: 90,
    height: 30,
    offset: 2284296,
    count: 333,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_filter_on.png` — 90x30, 333 rects, 1 sprite cell(s).
pub const TOOLBAR_FILTER_ON: ArtData = ArtData {
    name: "toolbar_filter_on",
    width: 90,
    height: 30,
    offset: 2288292,
    count: 333,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_folder_add_implode.png` — 90x30, 177 rects, 3 sprite cell(s).
pub const TOOLBAR_FOLDER_ADD_IMPLODE: ArtData = ArtData {
    name: "toolbar_folder_add_implode",
    width: 90,
    height: 30,
    offset: 2292288,
    count: 177,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_folder_add_new.png` — 90x30, 36 rects, 3 sprite cell(s).
pub const TOOLBAR_FOLDER_ADD_NEW: ArtData = ArtData {
    name: "toolbar_folder_add_new",
    width: 90,
    height: 30,
    offset: 2294412,
    count: 36,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_folder_combine.png` — 90x30, 240 rects, 3 sprite cell(s).
pub const TOOLBAR_FOLDER_COMBINE: ArtData = ArtData {
    name: "toolbar_folder_combine",
    width: 90,
    height: 30,
    offset: 2294844,
    count: 240,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_folder_delete_remove.png` — 90x30, 30 rects, 3 sprite cell(s).
pub const TOOLBAR_FOLDER_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_folder_delete_remove",
    width: 90,
    height: 30,
    offset: 2297724,
    count: 30,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_folder_hide.png` — 90x30, 210 rects, 3 sprite cell(s).
pub const TOOLBAR_FOLDER_HIDE: ArtData = ArtData {
    name: "toolbar_folder_hide",
    width: 90,
    height: 30,
    offset: 2298084,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_folder_item_delete_remove.png` — 90x30, 135 rects, 3 sprite cell(s).
pub const TOOLBAR_FOLDER_ITEM_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_folder_item_delete_remove",
    width: 90,
    height: 30,
    offset: 2300604,
    count: 135,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_folder_save_disk.png` — 90x30, 246 rects, 3 sprite cell(s).
pub const TOOLBAR_FOLDER_SAVE_DISK: ArtData = ArtData {
    name: "toolbar_folder_save_disk",
    width: 90,
    height: 30,
    offset: 2302224,
    count: 246,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_folder_seperate_explode.png` — 90x30, 177 rects, 3 sprite cell(s).
pub const TOOLBAR_FOLDER_SEPERATE_EXPLODE: ArtData = ArtData {
    name: "toolbar_folder_seperate_explode",
    width: 90,
    height: 30,
    offset: 2305176,
    count: 177,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_folder_show_visible.png` — 90x30, 210 rects, 3 sprite cell(s).
pub const TOOLBAR_FOLDER_SHOW_VISIBLE: ArtData = ArtData {
    name: "toolbar_folder_show_visible",
    width: 90,
    height: 30,
    offset: 2307300,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_freeze_render_apply_snowflake.png` — 90x30, 654 rects, 3 sprite cell(s).
pub const TOOLBAR_FREEZE_RENDER_APPLY_SNOWFLAKE: ArtData = ArtData {
    name: "toolbar_freeze_render_apply_snowflake",
    width: 90,
    height: 30,
    offset: 2309820,
    count: 654,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_glue.png` — 90x30, 276 rects, 3 sprite cell(s).
pub const TOOLBAR_GLUE: ArtData = ArtData {
    name: "toolbar_glue",
    width: 90,
    height: 30,
    offset: 2317668,
    count: 276,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_glue_time_selection.png` — 90x30, 324 rects, 3 sprite cell(s).
pub const TOOLBAR_GLUE_TIME_SELECTION: ArtData = ArtData {
    name: "toolbar_glue_time_selection",
    width: 90,
    height: 30,
    offset: 2320980,
    count: 324,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_grid.png` — 90x30, 36 rects, 3 sprite cell(s).
pub const TOOLBAR_GRID: ArtData = ArtData {
    name: "toolbar_grid",
    width: 90,
    height: 30,
    offset: 2324868,
    count: 36,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_grid_adjust_decrease.png` — 90x30, 114 rects, 3 sprite cell(s).
pub const TOOLBAR_GRID_ADJUST_DECREASE: ArtData = ArtData {
    name: "toolbar_grid_adjust_decrease",
    width: 90,
    height: 30,
    offset: 2325300,
    count: 114,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_grid_adjust_increase.png` — 90x30, 120 rects, 3 sprite cell(s).
pub const TOOLBAR_GRID_ADJUST_INCREASE: ArtData = ArtData {
    name: "toolbar_grid_adjust_increase",
    width: 90,
    height: 30,
    offset: 2326668,
    count: 120,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_grid_off.png` — 90x30, 290 rects, 1 sprite cell(s).
pub const TOOLBAR_GRID_OFF: ArtData = ArtData {
    name: "toolbar_grid_off",
    width: 90,
    height: 30,
    offset: 2328108,
    count: 290,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_grid_on.png` — 90x30, 290 rects, 1 sprite cell(s).
pub const TOOLBAR_GRID_ON: ArtData = ArtData {
    name: "toolbar_grid_on",
    width: 90,
    height: 30,
    offset: 2331588,
    count: 290,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_group.png` — 90x30, 153 rects, 3 sprite cell(s).
pub const TOOLBAR_GROUP: ArtData = ArtData {
    name: "toolbar_group",
    width: 90,
    height: 30,
    offset: 2335068,
    count: 153,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_group_add_item.png` — 90x30, 516 rects, 3 sprite cell(s).
pub const TOOLBAR_GROUP_ADD_ITEM: ArtData = ArtData {
    name: "toolbar_group_add_item",
    width: 90,
    height: 30,
    offset: 2336904,
    count: 516,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_group_add_item_selected.png` — 90x30, 546 rects, 3 sprite cell(s).
pub const TOOLBAR_GROUP_ADD_ITEM_SELECTED: ArtData = ArtData {
    name: "toolbar_group_add_item_selected",
    width: 90,
    height: 30,
    offset: 2343096,
    count: 546,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_group_explode.png` — 90x30, 783 rects, 3 sprite cell(s).
pub const TOOLBAR_GROUP_EXPLODE: ArtData = ArtData {
    name: "toolbar_group_explode",
    width: 90,
    height: 30,
    offset: 2349648,
    count: 783,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_group_off.png` — 90x30, 387 rects, 1 sprite cell(s).
pub const TOOLBAR_GROUP_OFF: ArtData = ArtData {
    name: "toolbar_group_off",
    width: 90,
    height: 30,
    offset: 2359044,
    count: 387,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_group_on.png` — 90x30, 387 rects, 1 sprite cell(s).
pub const TOOLBAR_GROUP_ON: ArtData = ArtData {
    name: "toolbar_group_on",
    width: 90,
    height: 30,
    offset: 2363688,
    count: 387,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_group_record.png` — 90x30, 789 rects, 3 sprite cell(s).
pub const TOOLBAR_GROUP_RECORD: ArtData = ArtData {
    name: "toolbar_group_record",
    width: 90,
    height: 30,
    offset: 2368332,
    count: 789,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_group_ungroup_remove_item.png` — 90x30, 510 rects, 3 sprite cell(s).
pub const TOOLBAR_GROUP_UNGROUP_REMOVE_ITEM: ArtData = ArtData {
    name: "toolbar_group_ungroup_remove_item",
    width: 90,
    height: 30,
    offset: 2377800,
    count: 510,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_group_ungroup_remove_item_selected.png` — 90x30, 546 rects, 3 sprite cell(s).
pub const TOOLBAR_GROUP_UNGROUP_REMOVE_ITEM_SELECTED: ArtData = ArtData {
    name: "toolbar_group_ungroup_remove_item_selected",
    width: 90,
    height: 30,
    offset: 2383920,
    count: 546,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_half_minim_grid.png` — 90x30, 105 rects, 3 sprite cell(s).
pub const TOOLBAR_HALF_MINIM_GRID: ArtData = ArtData {
    name: "toolbar_half_minim_grid",
    width: 90,
    height: 30,
    offset: 2390472,
    count: 105,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_half_minim_note.png` — 90x30, 69 rects, 3 sprite cell(s).
pub const TOOLBAR_HALF_MINIM_NOTE: ArtData = ArtData {
    name: "toolbar_half_minim_note",
    width: 90,
    height: 30,
    offset: 2391732,
    count: 69,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_hide_mixer.png` — 90x30, 210 rects, 3 sprite cell(s).
pub const TOOLBAR_HIDE_MIXER: ArtData = ArtData {
    name: "toolbar_hide_mixer",
    width: 90,
    height: 30,
    offset: 2392560,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_hide_selected.png` — 90x30, 192 rects, 3 sprite cell(s).
pub const TOOLBAR_HIDE_SELECTED: ArtData = ArtData {
    name: "toolbar_hide_selected",
    width: 90,
    height: 30,
    offset: 2395080,
    count: 192,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_hide_tcp.png` — 90x30, 210 rects, 3 sprite cell(s).
pub const TOOLBAR_HIDE_TCP: ArtData = ArtData {
    name: "toolbar_hide_tcp",
    width: 90,
    height: 30,
    offset: 2397384,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_input_fx_effect.png` — 90x30, 423 rects, 3 sprite cell(s).
pub const TOOLBAR_INPUT_FX_EFFECT: ArtData = ArtData {
    name: "toolbar_input_fx_effect",
    width: 90,
    height: 30,
    offset: 2399904,
    count: 423,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_arpeggiate.png` — 90x30, 144 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_ARPEGGIATE: ArtData = ArtData {
    name: "toolbar_item_arpeggiate",
    width: 90,
    height: 30,
    offset: 2404980,
    count: 144,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_duplicate_copy.png` — 90x30, 99 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_DUPLICATE_COPY: ArtData = ArtData {
    name: "toolbar_item_duplicate_copy",
    width: 90,
    height: 30,
    offset: 2406708,
    count: 99,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_effects_fx_delete_remove.png` — 90x30, 282 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_EFFECTS_FX_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_item_effects_fx_delete_remove",
    width: 90,
    height: 30,
    offset: 2407896,
    count: 282,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_effects_fx_show.png` — 90x30, 324 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_EFFECTS_FX_SHOW: ArtData = ArtData {
    name: "toolbar_item_effects_fx_show",
    width: 90,
    height: 30,
    offset: 2411280,
    count: 324,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_explode_lane_take.png` — 90x30, 81 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_EXPLODE_LANE_TAKE: ArtData = ArtData {
    name: "toolbar_item_explode_lane_take",
    width: 90,
    height: 30,
    offset: 2415168,
    count: 81,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_free_positioning.png` — 90x30, 24 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_FREE_POSITIONING: ArtData = ArtData {
    name: "toolbar_item_free_positioning",
    width: 90,
    height: 30,
    offset: 2416140,
    count: 24,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_green_arrow_selected.png` — 90x30, 66 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_GREEN_ARROW_SELECTED: ArtData = ArtData {
    name: "toolbar_item_green_arrow_selected",
    width: 90,
    height: 30,
    offset: 2416428,
    count: 66,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_green_arrow_selected_replace.png` — 90x30, 78 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_GREEN_ARROW_SELECTED_REPLACE: ArtData = ArtData {
    name: "toolbar_item_green_arrow_selected_replace",
    width: 90,
    height: 30,
    offset: 2417220,
    count: 78,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_implode_lane_take.png` — 90x30, 81 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_IMPLODE_LANE_TAKE: ArtData = ArtData {
    name: "toolbar_item_implode_lane_take",
    width: 90,
    height: 30,
    offset: 2418156,
    count: 81,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_insert_move_space.png` — 90x30, 123 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_INSERT_MOVE_SPACE: ArtData = ArtData {
    name: "toolbar_item_insert_move_space",
    width: 90,
    height: 30,
    offset: 2419128,
    count: 123,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_left_edge_grow.png` — 90x30, 60 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_LEFT_EDGE_GROW: ArtData = ArtData {
    name: "toolbar_item_left_edge_grow",
    width: 90,
    height: 30,
    offset: 2420604,
    count: 60,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_left_edge_position.png` — 90x30, 122 rects, 1 sprite cell(s).
pub const TOOLBAR_ITEM_LEFT_EDGE_POSITION: ArtData = ArtData {
    name: "toolbar_item_left_edge_position",
    width: 90,
    height: 30,
    offset: 2421324,
    count: 122,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_item_left_edge_shrink.png` — 90x30, 60 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_LEFT_EDGE_SHRINK: ArtData = ArtData {
    name: "toolbar_item_left_edge_shrink",
    width: 90,
    height: 30,
    offset: 2422788,
    count: 60,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_next.png` — 90x30, 63 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_NEXT: ArtData = ArtData {
    name: "toolbar_item_next",
    width: 90,
    height: 30,
    offset: 2423508,
    count: 63,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_previous.png` — 90x30, 63 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_PREVIOUS: ArtData = ArtData {
    name: "toolbar_item_previous",
    width: 90,
    height: 30,
    offset: 2424264,
    count: 63,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_properties.png` — 90x30, 159 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_PROPERTIES: ArtData = ArtData {
    name: "toolbar_item_properties",
    width: 90,
    height: 30,
    offset: 2425020,
    count: 159,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_red_arrow_selected.png` — 90x30, 66 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_RED_ARROW_SELECTED: ArtData = ArtData {
    name: "toolbar_item_red_arrow_selected",
    width: 90,
    height: 30,
    offset: 2426928,
    count: 66,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_remove_overlap.png` — 90x30, 81 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_REMOVE_OVERLAP: ArtData = ArtData {
    name: "toolbar_item_remove_overlap",
    width: 90,
    height: 30,
    offset: 2427720,
    count: 81,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_right_edge_grow.png` — 90x30, 60 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_RIGHT_EDGE_GROW: ArtData = ArtData {
    name: "toolbar_item_right_edge_grow",
    width: 90,
    height: 30,
    offset: 2428692,
    count: 60,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_right_edge_position.png` — 90x30, 120 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_RIGHT_EDGE_POSITION: ArtData = ArtData {
    name: "toolbar_item_right_edge_position",
    width: 90,
    height: 30,
    offset: 2429412,
    count: 120,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_right_edge_shrink.png` — 90x30, 60 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_RIGHT_EDGE_SHRINK: ArtData = ArtData {
    name: "toolbar_item_right_edge_shrink",
    width: 90,
    height: 30,
    offset: 2430852,
    count: 60,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_select.png` — 90x30, 213 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECT: ArtData = ArtData {
    name: "toolbar_item_select",
    width: 90,
    height: 30,
    offset: 2431572,
    count: 213,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_select_all.png` — 90x30, 96 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECT_ALL: ArtData = ArtData {
    name: "toolbar_item_select_all",
    width: 90,
    height: 30,
    offset: 2434128,
    count: 96,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_select_inverse.png` — 90x30, 96 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECT_INVERSE: ArtData = ArtData {
    name: "toolbar_item_select_inverse",
    width: 90,
    height: 30,
    offset: 2435280,
    count: 96,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_above_move.png` — 90x30, 312 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_ABOVE_MOVE: ArtData = ArtData {
    name: "toolbar_item_selected_above_move",
    width: 90,
    height: 30,
    offset: 2436432,
    count: 312,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_align.png` — 90x30, 207 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_ALIGN: ArtData = ArtData {
    name: "toolbar_item_selected_align",
    width: 90,
    height: 30,
    offset: 2440176,
    count: 207,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_area_select_move.png` — 90x30, 210 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_AREA_SELECT_MOVE: ArtData = ArtData {
    name: "toolbar_item_selected_area_select_move",
    width: 90,
    height: 30,
    offset: 2442660,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_cut_scissors.png` — 90x30, 231 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_CUT_SCISSORS: ArtData = ArtData {
    name: "toolbar_item_selected_cut_scissors",
    width: 90,
    height: 30,
    offset: 2445180,
    count: 231,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_grow_grid.png` — 90x30, 87 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_GROW_GRID: ArtData = ArtData {
    name: "toolbar_item_selected_grow_grid",
    width: 90,
    height: 30,
    offset: 2447952,
    count: 87,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_move.png` — 90x30, 204 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_MOVE: ArtData = ArtData {
    name: "toolbar_item_selected_move",
    width: 90,
    height: 30,
    offset: 2448996,
    count: 204,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_move_end.png` — 90x30, 45 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_MOVE_END: ArtData = ArtData {
    name: "toolbar_item_selected_move_end",
    width: 90,
    height: 30,
    offset: 2451444,
    count: 45,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_move_horizontal_position_time.png` — 90x30, 264 rects, 1 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_MOVE_HORIZONTAL_POSITION_TIME: ArtData = ArtData {
    name: "toolbar_item_selected_move_horizontal_position_time",
    width: 90,
    height: 30,
    offset: 2451984,
    count: 264,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_item_selected_move_nudge_left.png` — 90x30, 261 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_MOVE_NUDGE_LEFT: ArtData = ArtData {
    name: "toolbar_item_selected_move_nudge_left",
    width: 90,
    height: 30,
    offset: 2455152,
    count: 261,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_move_nudge_left_more.png` — 90x30, 435 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_MOVE_NUDGE_LEFT_MORE: ArtData = ArtData {
    name: "toolbar_item_selected_move_nudge_left_more",
    width: 90,
    height: 30,
    offset: 2458284,
    count: 435,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_move_nudge_right.png` — 90x30, 261 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_MOVE_NUDGE_RIGHT: ArtData = ArtData {
    name: "toolbar_item_selected_move_nudge_right",
    width: 90,
    height: 30,
    offset: 2463504,
    count: 261,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_move_nudge_right_more.png` — 90x30, 435 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_MOVE_NUDGE_RIGHT_MORE: ArtData = ArtData {
    name: "toolbar_item_selected_move_nudge_right_more",
    width: 90,
    height: 30,
    offset: 2466636,
    count: 435,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_move_vertical_track.png` — 90x30, 258 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_MOVE_VERTICAL_TRACK: ArtData = ArtData {
    name: "toolbar_item_selected_move_vertical_track",
    width: 90,
    height: 30,
    offset: 2471856,
    count: 258,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_snap.png` — 90x30, 99 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_SNAP: ArtData = ArtData {
    name: "toolbar_item_selected_snap",
    width: 90,
    height: 30,
    offset: 2474952,
    count: 99,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_swap.png` — 90x30, 138 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_SWAP: ArtData = ArtData {
    name: "toolbar_item_selected_swap",
    width: 90,
    height: 30,
    offset: 2476140,
    count: 138,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_take_delete.png` — 90x30, 237 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_TAKE_DELETE: ArtData = ArtData {
    name: "toolbar_item_selected_take_delete",
    width: 90,
    height: 30,
    offset: 2477796,
    count: 237,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_take_delete_invert.png` — 90x30, 237 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_TAKE_DELETE_INVERT: ArtData = ArtData {
    name: "toolbar_item_selected_take_delete_invert",
    width: 90,
    height: 30,
    offset: 2480640,
    count: 237,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_take_extract.png` — 90x30, 120 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_TAKE_EXTRACT: ArtData = ArtData {
    name: "toolbar_item_selected_take_extract",
    width: 90,
    height: 30,
    offset: 2483484,
    count: 120,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_take_insert.png` — 90x30, 120 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_TAKE_INSERT: ArtData = ArtData {
    name: "toolbar_item_selected_take_insert",
    width: 90,
    height: 30,
    offset: 2484924,
    count: 120,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_take_move_down.png` — 90x30, 126 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_TAKE_MOVE_DOWN: ArtData = ArtData {
    name: "toolbar_item_selected_take_move_down",
    width: 90,
    height: 30,
    offset: 2486364,
    count: 126,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_take_move_top.png` — 90x30, 186 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_TAKE_MOVE_TOP: ArtData = ArtData {
    name: "toolbar_item_selected_take_move_top",
    width: 90,
    height: 30,
    offset: 2487876,
    count: 186,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selected_take_move_up.png` — 90x30, 129 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTED_TAKE_MOVE_UP: ArtData = ArtData {
    name: "toolbar_item_selected_take_move_up",
    width: 90,
    height: 30,
    offset: 2490108,
    count: 129,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_selection_remove_contents_move_later.png` — 90x30, 192 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SELECTION_REMOVE_CONTENTS_MOVE_LATER: ArtData = ArtData {
    name: "toolbar_item_selection_remove_contents_move_later",
    width: 90,
    height: 30,
    offset: 2491656,
    count: 192,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_source_preferred_position_properties.png` — 90x30, 156 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_SOURCE_PREFERRED_POSITION_PROPERTIES: ArtData = ArtData {
    name: "toolbar_item_source_preferred_position_properties",
    width: 90,
    height: 30,
    offset: 2493960,
    count: 156,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_take.png` — 90x30, 39 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_TAKE: ArtData = ArtData {
    name: "toolbar_item_take",
    width: 90,
    height: 30,
    offset: 2495832,
    count: 39,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_take_explode.png` — 90x30, 225 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_TAKE_EXPLODE: ArtData = ArtData {
    name: "toolbar_item_take_explode",
    width: 90,
    height: 30,
    offset: 2496300,
    count: 225,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_take_selected_extract.png` — 90x30, 90 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_TAKE_SELECTED_EXTRACT: ArtData = ArtData {
    name: "toolbar_item_take_selected_extract",
    width: 90,
    height: 30,
    offset: 2499000,
    count: 90,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_item_take_selected_lock.png` — 90x30, 75 rects, 3 sprite cell(s).
pub const TOOLBAR_ITEM_TAKE_SELECTED_LOCK: ArtData = ArtData {
    name: "toolbar_item_take_selected_lock",
    width: 90,
    height: 30,
    offset: 2500080,
    count: 75,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_jog_back_rewind_little_bit.png` — 90x30, 465 rects, 3 sprite cell(s).
pub const TOOLBAR_JOG_BACK_REWIND_LITTLE_BIT: ArtData = ArtData {
    name: "toolbar_jog_back_rewind_little_bit",
    width: 90,
    height: 30,
    offset: 2500980,
    count: 465,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_jog_forward_little_bit.png` — 90x30, 471 rects, 3 sprite cell(s).
pub const TOOLBAR_JOG_FORWARD_LITTLE_BIT: ArtData = ArtData {
    name: "toolbar_jog_forward_little_bit",
    width: 90,
    height: 30,
    offset: 2506560,
    count: 471,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_knob_parameter_learn_lock.png` — 90x30, 504 rects, 3 sprite cell(s).
pub const TOOLBAR_KNOB_PARAMETER_LEARN_LOCK: ArtData = ArtData {
    name: "toolbar_knob_parameter_learn_lock",
    width: 90,
    height: 30,
    offset: 2512212,
    count: 504,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_knob_parameter_visible_show.png` — 90x30, 426 rects, 3 sprite cell(s).
pub const TOOLBAR_KNOB_PARAMETER_VISIBLE_SHOW: ArtData = ArtData {
    name: "toolbar_knob_parameter_visible_show",
    width: 90,
    height: 30,
    offset: 2518260,
    count: 426,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_load.png` — 90x30, 401 rects, 1 sprite cell(s).
pub const TOOLBAR_LOAD: ArtData = ArtData {
    name: "toolbar_load",
    width: 90,
    height: 30,
    offset: 2523372,
    count: 401,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_lock.png` — 90x30, 78 rects, 3 sprite cell(s).
pub const TOOLBAR_LOCK: ArtData = ArtData {
    name: "toolbar_lock",
    width: 90,
    height: 30,
    offset: 2528184,
    count: 78,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_lock_off.png` — 90x30, 304 rects, 1 sprite cell(s).
pub const TOOLBAR_LOCK_OFF: ArtData = ArtData {
    name: "toolbar_lock_off",
    width: 90,
    height: 30,
    offset: 2529120,
    count: 304,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_lock_on.png` — 90x30, 304 rects, 1 sprite cell(s).
pub const TOOLBAR_LOCK_ON: ArtData = ArtData {
    name: "toolbar_lock_on",
    width: 90,
    height: 30,
    offset: 2532768,
    count: 304,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_marker_delete_remove.png` — 90x30, 186 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_marker_delete_remove",
    width: 90,
    height: 30,
    offset: 2536416,
    count: 186,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_insert_new.png` — 90x30, 57 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_INSERT_NEW: ArtData = ArtData {
    name: "toolbar_marker_insert_new",
    width: 90,
    height: 30,
    offset: 2538648,
    count: 57,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_list.png` — 90x30, 186 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_LIST: ArtData = ArtData {
    name: "toolbar_marker_list",
    width: 90,
    height: 30,
    offset: 2539332,
    count: 186,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_load_disk.png` — 90x30, 393 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_LOAD_DISK: ArtData = ArtData {
    name: "toolbar_marker_load_disk",
    width: 90,
    height: 30,
    offset: 2541564,
    count: 393,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_lock.png` — 90x30, 114 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_LOCK: ArtData = ArtData {
    name: "toolbar_marker_lock",
    width: 90,
    height: 30,
    offset: 2546280,
    count: 114,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_next.png` — 90x30, 111 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_NEXT: ArtData = ArtData {
    name: "toolbar_marker_next",
    width: 90,
    height: 30,
    offset: 2547648,
    count: 111,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_previous.png` — 90x30, 111 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_PREVIOUS: ArtData = ArtData {
    name: "toolbar_marker_previous",
    width: 90,
    height: 30,
    offset: 2548980,
    count: 111,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_properties.png` — 90x30, 159 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_PROPERTIES: ArtData = ArtData {
    name: "toolbar_marker_properties",
    width: 90,
    height: 30,
    offset: 2550312,
    count: 159,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_renum.png` — 90x30, 298 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_RENUM: ArtData = ArtData {
    name: "toolbar_marker_renum",
    width: 90,
    height: 30,
    offset: 2552220,
    count: 298,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_time_selection_delete_remoe.png` — 90x30, 234 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_TIME_SELECTION_DELETE_REMOE: ArtData = ArtData {
    name: "toolbar_marker_time_selection_delete_remoe",
    width: 90,
    height: 30,
    offset: 2555796,
    count: 234,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_time_tempo_delete_remove.png` — 90x30, 291 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_TIME_TEMPO_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_marker_time_tempo_delete_remove",
    width: 90,
    height: 30,
    offset: 2558604,
    count: 291,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_time_tempo_insert_new.png` — 90x30, 162 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_TIME_TEMPO_INSERT_NEW: ArtData = ArtData {
    name: "toolbar_marker_time_tempo_insert_new",
    width: 90,
    height: 30,
    offset: 2562096,
    count: 162,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_time_tempo_next.png` — 90x30, 231 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_TIME_TEMPO_NEXT: ArtData = ArtData {
    name: "toolbar_marker_time_tempo_next",
    width: 90,
    height: 30,
    offset: 2564040,
    count: 231,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_time_tempo_previous.png` — 90x30, 231 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_TIME_TEMPO_PREVIOUS: ArtData = ArtData {
    name: "toolbar_marker_time_tempo_previous",
    width: 90,
    height: 30,
    offset: 2566812,
    count: 231,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marker_time_tempo_properties.png` — 90x30, 264 rects, 3 sprite cell(s).
pub const TOOLBAR_MARKER_TIME_TEMPO_PROPERTIES: ArtData = ArtData {
    name: "toolbar_marker_time_tempo_properties",
    width: 90,
    height: 30,
    offset: 2569584,
    count: 264,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marquee_cursor_selection.png` — 90x30, 69 rects, 3 sprite cell(s).
pub const TOOLBAR_MARQUEE_CURSOR_SELECTION: ArtData = ArtData {
    name: "toolbar_marquee_cursor_selection",
    width: 90,
    height: 30,
    offset: 2572752,
    count: 69,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_marquee_cursor_selection_off.png` — 90x30, 355 rects, 1 sprite cell(s).
pub const TOOLBAR_MARQUEE_CURSOR_SELECTION_OFF: ArtData = ArtData {
    name: "toolbar_marquee_cursor_selection_off",
    width: 90,
    height: 30,
    offset: 2573580,
    count: 355,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_marquee_cursor_selection_on.png` — 90x30, 355 rects, 1 sprite cell(s).
pub const TOOLBAR_MARQUEE_CURSOR_SELECTION_ON: ArtData = ArtData {
    name: "toolbar_marquee_cursor_selection_on",
    width: 90,
    height: 30,
    offset: 2577840,
    count: 355,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_metro.png` — 90x30, 234 rects, 3 sprite cell(s).
pub const TOOLBAR_METRO: ArtData = ArtData {
    name: "toolbar_metro",
    width: 90,
    height: 30,
    offset: 2582100,
    count: 234,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_metro_off.png` — 90x30, 549 rects, 1 sprite cell(s).
pub const TOOLBAR_METRO_OFF: ArtData = ArtData {
    name: "toolbar_metro_off",
    width: 90,
    height: 30,
    offset: 2584908,
    count: 549,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_metro_on.png` — 90x30, 549 rects, 1 sprite cell(s).
pub const TOOLBAR_METRO_ON: ArtData = ArtData {
    name: "toolbar_metro_on",
    width: 90,
    height: 30,
    offset: 2591496,
    count: 549,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_all.png` — 90x30, 21 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_ALL: ArtData = ArtData {
    name: "toolbar_midi_all",
    width: 90,
    height: 30,
    offset: 2598084,
    count: 21,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_cc_above.png` — 90x30, 24 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_CC_ABOVE: ArtData = ArtData {
    name: "toolbar_midi_cc_above",
    width: 90,
    height: 30,
    offset: 2598336,
    count: 24,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_cc_below.png` — 90x30, 24 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_CC_BELOW: ArtData = ArtData {
    name: "toolbar_midi_cc_below",
    width: 90,
    height: 30,
    offset: 2598624,
    count: 24,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_cc_explode.png` — 90x30, 507 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_CC_EXPLODE: ArtData = ArtData {
    name: "toolbar_midi_cc_explode",
    width: 90,
    height: 30,
    offset: 2598912,
    count: 507,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_cc_pitch_bend.png` — 90x30, 408 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_CC_PITCH_BEND: ArtData = ArtData {
    name: "toolbar_midi_cc_pitch_bend",
    width: 90,
    height: 30,
    offset: 2604996,
    count: 408,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_cc_scale_slope.png` — 90x30, 111 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_CC_SCALE_SLOPE: ArtData = ArtData {
    name: "toolbar_midi_cc_scale_slope",
    width: 90,
    height: 30,
    offset: 2609892,
    count: 111,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_cc_selected_decrease.png` — 90x30, 48 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_CC_SELECTED_DECREASE: ArtData = ArtData {
    name: "toolbar_midi_cc_selected_decrease",
    width: 90,
    height: 30,
    offset: 2611224,
    count: 48,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_cc_selected_increase.png` — 90x30, 48 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_CC_SELECTED_INCREASE: ArtData = ArtData {
    name: "toolbar_midi_cc_selected_increase",
    width: 90,
    height: 30,
    offset: 2611800,
    count: 48,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_cc_set_scale_fix.png` — 90x30, 93 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_CC_SET_SCALE_FIX: ArtData = ArtData {
    name: "toolbar_midi_cc_set_scale_fix",
    width: 90,
    height: 30,
    offset: 2612376,
    count: 93,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_delete_remove_none.png` — 90x30, 435 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_DELETE_REMOVE_NONE: ArtData = ArtData {
    name: "toolbar_midi_delete_remove_none",
    width: 90,
    height: 30,
    offset: 2613492,
    count: 435,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_envelope.png` — 90x30, 303 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_ENVELOPE: ArtData = ArtData {
    name: "toolbar_midi_envelope",
    width: 90,
    height: 30,
    offset: 2618712,
    count: 303,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_events_mode_cycle.png` — 90x30, 261 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_EVENTS_MODE_CYCLE: ArtData = ArtData {
    name: "toolbar_midi_events_mode_cycle",
    width: 90,
    height: 30,
    offset: 2622348,
    count: 261,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_events_mode_drum_diamond.png` — 90x30, 712 rects, 1 sprite cell(s).
pub const TOOLBAR_MIDI_EVENTS_MODE_DRUM_DIAMOND: ArtData = ArtData {
    name: "toolbar_midi_events_mode_drum_diamond",
    width: 90,
    height: 30,
    offset: 2625480,
    count: 712,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_events_mode_drum_triangle.png` — 90x30, 655 rects, 1 sprite cell(s).
pub const TOOLBAR_MIDI_EVENTS_MODE_DRUM_TRIANGLE: ArtData = ArtData {
    name: "toolbar_midi_events_mode_drum_triangle",
    width: 90,
    height: 30,
    offset: 2634024,
    count: 655,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_events_mode_normal_rectangle.png` — 90x30, 563 rects, 1 sprite cell(s).
pub const TOOLBAR_MIDI_EVENTS_MODE_NORMAL_RECTANGLE: ArtData = ArtData {
    name: "toolbar_midi_events_mode_normal_rectangle",
    width: 90,
    height: 30,
    offset: 2641884,
    count: 563,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_folder.png` — 90x30, 186 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_FOLDER: ArtData = ArtData {
    name: "toolbar_midi_folder",
    width: 90,
    height: 30,
    offset: 2648640,
    count: 186,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_hide_unused_note_rows.png` — 90x30, 384 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_HIDE_UNUSED_NOTE_ROWS: ArtData = ArtData {
    name: "toolbar_midi_hide_unused_note_rows",
    width: 90,
    height: 30,
    offset: 2650872,
    count: 384,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_hide_unused_unnamed_note_rows.png` — 90x30, 384 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_HIDE_UNUSED_UNNAMED_NOTE_ROWS: ArtData = ArtData {
    name: "toolbar_midi_hide_unused_unnamed_note_rows",
    width: 90,
    height: 30,
    offset: 2655480,
    count: 384,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_item.png` — 90x30, 219 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_ITEM: ArtData = ArtData {
    name: "toolbar_midi_item",
    width: 90,
    height: 30,
    offset: 2660088,
    count: 219,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_item_selected.png` — 90x30, 294 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_ITEM_SELECTED: ArtData = ArtData {
    name: "toolbar_midi_item_selected",
    width: 90,
    height: 30,
    offset: 2665272,
    count: 294,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_item_selected-262.png` — 90x30, 213 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_ITEM_SELECTED_262: ArtData = ArtData {
    name: "toolbar_midi_item_selected-262",
    width: 90,
    height: 30,
    offset: 2662716,
    count: 213,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_itemsel.png` — 90x30, 63 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_ITEMSEL: ArtData = ArtData {
    name: "toolbar_midi_itemsel",
    width: 90,
    height: 30,
    offset: 2668800,
    count: 63,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_itemsel_off.png` — 90x30, 325 rects, 1 sprite cell(s).
pub const TOOLBAR_MIDI_ITEMSEL_OFF: ArtData = ArtData {
    name: "toolbar_midi_itemsel_off",
    width: 90,
    height: 30,
    offset: 2669556,
    count: 325,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_itemsel_on.png` — 90x30, 325 rects, 1 sprite cell(s).
pub const TOOLBAR_MIDI_ITEMSEL_ON: ArtData = ArtData {
    name: "toolbar_midi_itemsel_on",
    width: 90,
    height: 30,
    offset: 2673456,
    count: 325,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_lengthen_note_grid_unit.png` — 90x30, 99 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_LENGTHEN_NOTE_GRID_UNIT: ArtData = ArtData {
    name: "toolbar_midi_lengthen_note_grid_unit",
    width: 90,
    height: 30,
    offset: 2677356,
    count: 99,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_lengthen_note_pixel.png` — 90x30, 141 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_LENGTHEN_NOTE_PIXEL: ArtData = ArtData {
    name: "toolbar_midi_lengthen_note_pixel",
    width: 90,
    height: 30,
    offset: 2678544,
    count: 141,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_list.png` — 90x30, 258 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_LIST: ArtData = ArtData {
    name: "toolbar_midi_list",
    width: 90,
    height: 30,
    offset: 2680236,
    count: 258,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_mode_event_list.png` — 90x30, 244 rects, 1 sprite cell(s).
pub const TOOLBAR_MIDI_MODE_EVENT_LIST: ArtData = ArtData {
    name: "toolbar_midi_mode_event_list",
    width: 90,
    height: 30,
    offset: 2683332,
    count: 244,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_mode_musical_notation.png` — 90x30, 625 rects, 1 sprite cell(s).
pub const TOOLBAR_MIDI_MODE_MUSICAL_NOTATION: ArtData = ArtData {
    name: "toolbar_midi_mode_musical_notation",
    width: 90,
    height: 30,
    offset: 2686260,
    count: 625,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_mode_named_notes.png` — 90x30, 217 rects, 1 sprite cell(s).
pub const TOOLBAR_MIDI_MODE_NAMED_NOTES: ArtData = ArtData {
    name: "toolbar_midi_mode_named_notes",
    width: 90,
    height: 30,
    offset: 2693760,
    count: 217,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_mode_piano_roll.png` — 90x30, 256 rects, 1 sprite cell(s).
pub const TOOLBAR_MIDI_MODE_PIANO_ROLL: ArtData = ArtData {
    name: "toolbar_midi_mode_piano_roll",
    width: 90,
    height: 30,
    offset: 2696364,
    count: 256,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_notes_force_snap_scale.png` — 90x30, 24 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_NOTES_FORCE_SNAP_SCALE: ArtData = ArtData {
    name: "toolbar_midi_notes_force_snap_scale",
    width: 90,
    height: 30,
    offset: 2699436,
    count: 24,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_panic_all_notes.png` — 90x30, 345 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_PANIC_ALL_NOTES: ArtData = ArtData {
    name: "toolbar_midi_panic_all_notes",
    width: 90,
    height: 30,
    offset: 2699724,
    count: 345,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_pitch_decrease_octave.png` — 90x30, 312 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_PITCH_DECREASE_OCTAVE: ArtData = ArtData {
    name: "toolbar_midi_pitch_decrease_octave",
    width: 90,
    height: 30,
    offset: 2703864,
    count: 312,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_pitch_decrease_semitone.png` — 90x30, 150 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_PITCH_DECREASE_SEMITONE: ArtData = ArtData {
    name: "toolbar_midi_pitch_decrease_semitone",
    width: 90,
    height: 30,
    offset: 2707608,
    count: 150,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_pitch_increase_octave.png` — 90x30, 303 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_PITCH_INCREASE_OCTAVE: ArtData = ArtData {
    name: "toolbar_midi_pitch_increase_octave",
    width: 90,
    height: 30,
    offset: 2713044,
    count: 303,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_pitch_increase_octave-173.png` — 90x30, 303 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_PITCH_INCREASE_OCTAVE_173: ArtData = ArtData {
    name: "toolbar_midi_pitch_increase_octave-173",
    width: 90,
    height: 30,
    offset: 2709408,
    count: 303,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_pitch_increase_semitone.png` — 90x30, 150 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_PITCH_INCREASE_SEMITONE: ArtData = ArtData {
    name: "toolbar_midi_pitch_increase_semitone",
    width: 90,
    height: 30,
    offset: 2716680,
    count: 150,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_pitch_transpose.png` — 90x30, 189 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_PITCH_TRANSPOSE: ArtData = ArtData {
    name: "toolbar_midi_pitch_transpose",
    width: 90,
    height: 30,
    offset: 2718480,
    count: 189,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_properties.png` — 90x30, 372 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_PROPERTIES: ArtData = ArtData {
    name: "toolbar_midi_properties",
    width: 90,
    height: 30,
    offset: 2720748,
    count: 372,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_render_apply_audio_waveform.png` — 90x30, 420 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_RENDER_APPLY_AUDIO_WAVEFORM: ArtData = ArtData {
    name: "toolbar_midi_render_apply_audio_waveform",
    width: 90,
    height: 30,
    offset: 2725212,
    count: 420,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_shorten_note_grid_unit.png` — 90x30, 99 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_SHORTEN_NOTE_GRID_UNIT: ArtData = ArtData {
    name: "toolbar_midi_shorten_note_grid_unit",
    width: 90,
    height: 30,
    offset: 2730252,
    count: 99,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_shorten_note_pixel.png` — 90x30, 141 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_SHORTEN_NOTE_PIXEL: ArtData = ArtData {
    name: "toolbar_midi_shorten_note_pixel",
    width: 90,
    height: 30,
    offset: 2731440,
    count: 141,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_show_all_note_rows.png` — 90x30, 384 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_SHOW_ALL_NOTE_ROWS: ArtData = ArtData {
    name: "toolbar_midi_show_all_note_rows",
    width: 90,
    height: 30,
    offset: 2733132,
    count: 384,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_size_full.png` — 90x30, 219 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_SIZE_FULL: ArtData = ArtData {
    name: "toolbar_midi_size_full",
    width: 90,
    height: 30,
    offset: 2737740,
    count: 219,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_step.png` — 90x30, 549 rects, 1 sprite cell(s).
pub const TOOLBAR_MIDI_STEP: ArtData = ArtData {
    name: "toolbar_midi_step",
    width: 90,
    height: 30,
    offset: 2740368,
    count: 549,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_tracksel.png` — 90x30, 36 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_TRACKSEL: ArtData = ArtData {
    name: "toolbar_midi_tracksel",
    width: 90,
    height: 30,
    offset: 2746956,
    count: 36,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_tracksel_off.png` — 90x30, 310 rects, 1 sprite cell(s).
pub const TOOLBAR_MIDI_TRACKSEL_OFF: ArtData = ArtData {
    name: "toolbar_midi_tracksel_off",
    width: 90,
    height: 30,
    offset: 2747388,
    count: 310,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_tracksel_on.png` — 90x30, 310 rects, 1 sprite cell(s).
pub const TOOLBAR_MIDI_TRACKSEL_ON: ArtData = ArtData {
    name: "toolbar_midi_tracksel_on",
    width: 90,
    height: 30,
    offset: 2751108,
    count: 310,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_midi_waveform_audio.png` — 90x30, 354 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_WAVEFORM_AUDIO: ArtData = ArtData {
    name: "toolbar_midi_waveform_audio",
    width: 90,
    height: 30,
    offset: 2754828,
    count: 354,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_midi_zoom.png` — 90x30, 342 rects, 3 sprite cell(s).
pub const TOOLBAR_MIDI_ZOOM: ArtData = ArtData {
    name: "toolbar_midi_zoom",
    width: 90,
    height: 30,
    offset: 2759076,
    count: 342,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_anarchy.png` — 90x30, 468 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_ANARCHY: ArtData = ArtData {
    name: "toolbar_misc_anarchy",
    width: 90,
    height: 30,
    offset: 2763180,
    count: 468,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_anchor.png` — 90x30, 240 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_ANCHOR: ArtData = ArtData {
    name: "toolbar_misc_anchor",
    width: 90,
    height: 30,
    offset: 2768796,
    count: 240,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_back_left_previous.png` — 90x30, 126 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_BACK_LEFT_PREVIOUS: ArtData = ArtData {
    name: "toolbar_misc_back_left_previous",
    width: 90,
    height: 30,
    offset: 2771676,
    count: 126,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_back_left_previous_more.png` — 90x30, 324 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_BACK_LEFT_PREVIOUS_MORE: ArtData = ArtData {
    name: "toolbar_misc_back_left_previous_more",
    width: 90,
    height: 30,
    offset: 2773188,
    count: 324,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_bash.png` — 90x30, 465 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_BASH: ArtData = ArtData {
    name: "toolbar_misc_bash",
    width: 90,
    height: 30,
    offset: 2777076,
    count: 465,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_bomb.png` — 90x30, 276 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_BOMB: ArtData = ArtData {
    name: "toolbar_misc_bomb",
    width: 90,
    height: 30,
    offset: 2782656,
    count: 276,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_brush_broom_clean.png` — 90x30, 159 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_BRUSH_BROOM_CLEAN: ArtData = ArtData {
    name: "toolbar_misc_brush_broom_clean",
    width: 90,
    height: 30,
    offset: 2785968,
    count: 159,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_bulb_idea.png` — 90x30, 288 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_BULB_IDEA: ArtData = ArtData {
    name: "toolbar_misc_bulb_idea",
    width: 90,
    height: 30,
    offset: 2787876,
    count: 288,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_calculate_numeric.png` — 90x30, 57 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_CALCULATE_NUMERIC: ArtData = ArtData {
    name: "toolbar_misc_calculate_numeric",
    width: 90,
    height: 30,
    offset: 2791332,
    count: 57,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_car.png` — 90x30, 171 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_CAR: ArtData = ArtData {
    name: "toolbar_misc_car",
    width: 90,
    height: 30,
    offset: 2792016,
    count: 171,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_coffee.png` — 90x30, 186 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_COFFEE: ArtData = ArtData {
    name: "toolbar_misc_coffee",
    width: 90,
    height: 30,
    offset: 2794068,
    count: 186,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_devil.png` — 90x30, 171 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_DEVIL: ArtData = ArtData {
    name: "toolbar_misc_devil",
    width: 90,
    height: 30,
    offset: 2796300,
    count: 171,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_down_next.png` — 90x30, 117 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_DOWN_NEXT: ArtData = ArtData {
    name: "toolbar_misc_down_next",
    width: 90,
    height: 30,
    offset: 2798352,
    count: 117,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_down_next_more.png` — 90x30, 297 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_DOWN_NEXT_MORE: ArtData = ArtData {
    name: "toolbar_misc_down_next_more",
    width: 90,
    height: 30,
    offset: 2799756,
    count: 297,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_drum.png` — 90x30, 309 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_DRUM: ArtData = ArtData {
    name: "toolbar_misc_drum",
    width: 90,
    height: 30,
    offset: 2803320,
    count: 309,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_duck.png` — 90x30, 228 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_DUCK: ArtData = ArtData {
    name: "toolbar_misc_duck",
    width: 90,
    height: 30,
    offset: 2807028,
    count: 228,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_explode.png` — 90x30, 324 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_EXPLODE: ArtData = ArtData {
    name: "toolbar_misc_explode",
    width: 90,
    height: 30,
    offset: 2809764,
    count: 324,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_filter.png` — 90x30, 201 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_FILTER: ArtData = ArtData {
    name: "toolbar_misc_filter",
    width: 90,
    height: 30,
    offset: 2813652,
    count: 201,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_finger.png` — 90x30, 63 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_FINGER: ArtData = ArtData {
    name: "toolbar_misc_finger",
    width: 90,
    height: 30,
    offset: 2816064,
    count: 63,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_firewire.png` — 90x30, 285 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_FIREWIRE: ArtData = ArtData {
    name: "toolbar_misc_firewire",
    width: 90,
    height: 30,
    offset: 2816820,
    count: 285,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_game.png` — 90x30, 57 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_GAME: ArtData = ArtData {
    name: "toolbar_misc_game",
    width: 90,
    height: 30,
    offset: 2820240,
    count: 57,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_guitar.png` — 90x30, 314 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_GUITAR: ArtData = ArtData {
    name: "toolbar_misc_guitar",
    width: 90,
    height: 30,
    offset: 2820924,
    count: 314,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_guitar_headstock.png` — 90x30, 345 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_GUITAR_HEADSTOCK: ArtData = ArtData {
    name: "toolbar_misc_guitar_headstock",
    width: 90,
    height: 30,
    offset: 2824692,
    count: 345,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_gun.png` — 90x30, 99 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_GUN: ArtData = ArtData {
    name: "toolbar_misc_gun",
    width: 90,
    height: 30,
    offset: 2828832,
    count: 99,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_heart.png` — 90x30, 165 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_HEART: ArtData = ArtData {
    name: "toolbar_misc_heart",
    width: 90,
    height: 30,
    offset: 2830020,
    count: 165,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_horns.png` — 90x30, 57 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_HORNS: ArtData = ArtData {
    name: "toolbar_misc_horns",
    width: 90,
    height: 30,
    offset: 2832000,
    count: 57,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_house_home.png` — 90x30, 90 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_HOUSE_HOME: ArtData = ArtData {
    name: "toolbar_misc_house_home",
    width: 90,
    height: 30,
    offset: 2832684,
    count: 90,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_ibeam_cursor_selection.png` — 90x30, 15 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_IBEAM_CURSOR_SELECTION: ArtData = ArtData {
    name: "toolbar_misc_ibeam_cursor_selection",
    width: 90,
    height: 30,
    offset: 2833764,
    count: 15,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_jack_input_output.png` — 90x30, 258 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_JACK_INPUT_OUTPUT: ArtData = ArtData {
    name: "toolbar_misc_jack_input_output",
    width: 90,
    height: 30,
    offset: 2833944,
    count: 258,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_key_lock.png` — 90x30, 78 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_KEY_LOCK: ArtData = ArtData {
    name: "toolbar_misc_key_lock",
    width: 90,
    height: 30,
    offset: 2837040,
    count: 78,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_keyboard.png` — 90x30, 24 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_KEYBOARD: ArtData = ArtData {
    name: "toolbar_misc_keyboard",
    width: 90,
    height: 30,
    offset: 2837976,
    count: 24,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_lips.png` — 90x30, 210 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_LIPS: ArtData = ArtData {
    name: "toolbar_misc_lips",
    width: 90,
    height: 30,
    offset: 2838264,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_mask.png` — 90x30, 249 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_MASK: ArtData = ArtData {
    name: "toolbar_misc_mask",
    width: 90,
    height: 30,
    offset: 2840784,
    count: 249,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_mic.png` — 90x30, 183 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_MIC: ArtData = ArtData {
    name: "toolbar_misc_mic",
    width: 90,
    height: 30,
    offset: 2843772,
    count: 183,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_mixer_control.png` — 90x30, 81 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_MIXER_CONTROL: ArtData = ArtData {
    name: "toolbar_misc_mixer_control",
    width: 90,
    height: 30,
    offset: 2845968,
    count: 81,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_monitor_speaker.png` — 90x30, 132 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_MONITOR_SPEAKER: ArtData = ArtData {
    name: "toolbar_misc_monitor_speaker",
    width: 90,
    height: 30,
    offset: 2846940,
    count: 132,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_mouse.png` — 90x30, 213 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_MOUSE: ArtData = ArtData {
    name: "toolbar_misc_mouse",
    width: 90,
    height: 30,
    offset: 2848524,
    count: 213,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_mouse_left_click.png` — 90x30, 213 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_MOUSE_LEFT_CLICK: ArtData = ArtData {
    name: "toolbar_misc_mouse_left_click",
    width: 90,
    height: 30,
    offset: 2851080,
    count: 213,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_mouse_right_click.png` — 90x30, 213 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_MOUSE_RIGHT_CLICK: ArtData = ArtData {
    name: "toolbar_misc_mouse_right_click",
    width: 90,
    height: 30,
    offset: 2853636,
    count: 213,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_network_stream.png` — 90x30, 48 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_NETWORK_STREAM: ArtData = ArtData {
    name: "toolbar_misc_network_stream",
    width: 90,
    height: 30,
    offset: 2856192,
    count: 48,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_phones.png` — 90x30, 153 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_PHONES: ArtData = ArtData {
    name: "toolbar_misc_phones",
    width: 90,
    height: 30,
    offset: 2856768,
    count: 153,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_pointer_cursor.png` — 90x30, 72 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_POINTER_CURSOR: ArtData = ArtData {
    name: "toolbar_misc_pointer_cursor",
    width: 90,
    height: 30,
    offset: 2858604,
    count: 72,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_pointer_cursor_white.png` — 90x30, 96 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_POINTER_CURSOR_WHITE: ArtData = ArtData {
    name: "toolbar_misc_pointer_cursor_white",
    width: 90,
    height: 30,
    offset: 2859468,
    count: 96,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_question_random.png` — 90x30, 189 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_QUESTION_RANDOM: ArtData = ArtData {
    name: "toolbar_misc_question_random",
    width: 90,
    height: 30,
    offset: 2860620,
    count: 189,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_radioactive.png` — 90x30, 513 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_RADIOACTIVE: ArtData = ArtData {
    name: "toolbar_misc_radioactive",
    width: 90,
    height: 30,
    offset: 2862888,
    count: 513,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_right_forward_next.png` — 90x30, 126 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_RIGHT_FORWARD_NEXT: ArtData = ArtData {
    name: "toolbar_misc_right_forward_next",
    width: 90,
    height: 30,
    offset: 2869044,
    count: 126,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_right_forward_next_more.png` — 90x30, 324 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_RIGHT_FORWARD_NEXT_MORE: ArtData = ArtData {
    name: "toolbar_misc_right_forward_next_more",
    width: 90,
    height: 30,
    offset: 2870556,
    count: 324,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_run_backward.png` — 90x30, 240 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_RUN_BACKWARD: ArtData = ArtData {
    name: "toolbar_misc_run_backward",
    width: 90,
    height: 30,
    offset: 2874444,
    count: 240,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_run_forward.png` — 90x30, 240 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_RUN_FORWARD: ArtData = ArtData {
    name: "toolbar_misc_run_forward",
    width: 90,
    height: 30,
    offset: 2877324,
    count: 240,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_saint.png` — 90x30, 225 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_SAINT: ArtData = ArtData {
    name: "toolbar_misc_saint",
    width: 90,
    height: 30,
    offset: 2880204,
    count: 225,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_skull_crossbones.png` — 90x30, 414 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_SKULL_CROSSBONES: ArtData = ArtData {
    name: "toolbar_misc_skull_crossbones",
    width: 90,
    height: 30,
    offset: 2882904,
    count: 414,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_speech_note.png` — 90x30, 177 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_SPEECH_NOTE: ArtData = ArtData {
    name: "toolbar_misc_speech_note",
    width: 90,
    height: 30,
    offset: 2887872,
    count: 177,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_star.png` — 90x30, 318 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_STAR: ArtData = ArtData {
    name: "toolbar_misc_star",
    width: 90,
    height: 30,
    offset: 2889996,
    count: 318,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_star_green.png` — 90x30, 192 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_STAR_GREEN: ArtData = ArtData {
    name: "toolbar_misc_star_green",
    width: 90,
    height: 30,
    offset: 2893812,
    count: 192,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_system_reaper.png` — 90x30, 438 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_SYSTEM_REAPER: ArtData = ArtData {
    name: "toolbar_misc_system_reaper",
    width: 90,
    height: 30,
    offset: 2896116,
    count: 438,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_tape.png` — 90x30, 315 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_TAPE: ArtData = ArtData {
    name: "toolbar_misc_tape",
    width: 90,
    height: 30,
    offset: 2901372,
    count: 315,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_tea_mmmmm_tea.png` — 90x30, 144 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_TEA_MMMMM_TEA: ArtData = ArtData {
    name: "toolbar_misc_tea_mmmmm_tea",
    width: 90,
    height: 30,
    offset: 2905152,
    count: 144,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_thought_idea.png` — 90x30, 219 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_THOUGHT_IDEA: ArtData = ArtData {
    name: "toolbar_misc_thought_idea",
    width: 90,
    height: 30,
    offset: 2906880,
    count: 219,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_toilet.png` — 90x30, 48 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_TOILET: ArtData = ArtData {
    name: "toolbar_misc_toilet",
    width: 90,
    height: 30,
    offset: 2909508,
    count: 48,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_trash_bin.png` — 90x30, 177 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_TRASH_BIN: ArtData = ArtData {
    name: "toolbar_misc_trash_bin",
    width: 90,
    height: 30,
    offset: 2910084,
    count: 177,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_up_previous.png` — 90x30, 117 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_UP_PREVIOUS: ArtData = ArtData {
    name: "toolbar_misc_up_previous",
    width: 90,
    height: 30,
    offset: 2912208,
    count: 117,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_up_previous_more.png` — 90x30, 297 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_UP_PREVIOUS_MORE: ArtData = ArtData {
    name: "toolbar_misc_up_previous_more",
    width: 90,
    height: 30,
    offset: 2913612,
    count: 297,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_usb.png` — 90x30, 195 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_USB: ArtData = ArtData {
    name: "toolbar_misc_usb",
    width: 90,
    height: 30,
    offset: 2917176,
    count: 195,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_walk_backward.png` — 90x30, 261 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_WALK_BACKWARD: ArtData = ArtData {
    name: "toolbar_misc_walk_backward",
    width: 90,
    height: 30,
    offset: 2919516,
    count: 261,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_misc_walk_forward.png` — 90x30, 261 rects, 3 sprite cell(s).
pub const TOOLBAR_MISC_WALK_FORWARD: ArtData = ArtData {
    name: "toolbar_misc_walk_forward",
    width: 90,
    height: 30,
    offset: 2922648,
    count: 261,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_mute_envelope.png` — 90x30, 309 rects, 3 sprite cell(s).
pub const TOOLBAR_MUTE_ENVELOPE: ArtData = ArtData {
    name: "toolbar_mute_envelope",
    width: 90,
    height: 30,
    offset: 2925780,
    count: 309,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_mute_none_unmute.png` — 90x30, 339 rects, 3 sprite cell(s).
pub const TOOLBAR_MUTE_NONE_UNMUTE: ArtData = ArtData {
    name: "toolbar_mute_none_unmute",
    width: 90,
    height: 30,
    offset: 2929488,
    count: 339,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_new.png` — 90x30, 262 rects, 1 sprite cell(s).
pub const TOOLBAR_NEW: ArtData = ArtData {
    name: "toolbar_new",
    width: 90,
    height: 30,
    offset: 2933556,
    count: 262,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_note_gliss_slide.png` — 90x30, 168 rects, 3 sprite cell(s).
pub const TOOLBAR_NOTE_GLISS_SLIDE: ArtData = ArtData {
    name: "toolbar_note_gliss_slide",
    width: 90,
    height: 30,
    offset: 2936700,
    count: 168,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_note_tie.png` — 90x30, 159 rects, 3 sprite cell(s).
pub const TOOLBAR_NOTE_TIE: ArtData = ArtData {
    name: "toolbar_note_tie",
    width: 90,
    height: 30,
    offset: 2938716,
    count: 159,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_parameter_scrub.png` — 90x30, 513 rects, 3 sprite cell(s).
pub const TOOLBAR_PARAMETER_SCRUB: ArtData = ArtData {
    name: "toolbar_parameter_scrub",
    width: 90,
    height: 30,
    offset: 2940624,
    count: 513,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_path_primary_disk.png` — 90x30, 462 rects, 3 sprite cell(s).
pub const TOOLBAR_PATH_PRIMARY_DISK: ArtData = ArtData {
    name: "toolbar_path_primary_disk",
    width: 90,
    height: 30,
    offset: 2946780,
    count: 462,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_path_primary_secondary_both_disk.png` — 90x30, 462 rects, 3 sprite cell(s).
pub const TOOLBAR_PATH_PRIMARY_SECONDARY_BOTH_DISK: ArtData = ArtData {
    name: "toolbar_path_primary_secondary_both_disk",
    width: 90,
    height: 30,
    offset: 2952324,
    count: 462,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_path_secondary_disk.png` — 90x30, 462 rects, 3 sprite cell(s).
pub const TOOLBAR_PATH_SECONDARY_DISK: ArtData = ArtData {
    name: "toolbar_path_secondary_disk",
    width: 90,
    height: 30,
    offset: 2957868,
    count: 462,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_pitch_preserve_lock.png` — 90x30, 336 rects, 3 sprite cell(s).
pub const TOOLBAR_PITCH_PRESERVE_LOCK: ArtData = ArtData {
    name: "toolbar_pitch_preserve_lock",
    width: 90,
    height: 30,
    offset: 2963412,
    count: 336,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_preroll_clock.png` — 90x30, 454 rects, 3 sprite cell(s).
pub const TOOLBAR_PREROLL_CLOCK: ArtData = ArtData {
    name: "toolbar_preroll_clock",
    width: 90,
    height: 30,
    offset: 2967444,
    count: 454,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_preroll_clock_record.png` — 90x30, 587 rects, 3 sprite cell(s).
pub const TOOLBAR_PREROLL_CLOCK_RECORD: ArtData = ArtData {
    name: "toolbar_preroll_clock_record",
    width: 90,
    height: 30,
    offset: 2972892,
    count: 587,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_project_save_as_new_disk.png` — 90x30, 418 rects, 3 sprite cell(s).
pub const TOOLBAR_PROJECT_SAVE_AS_NEW_DISK: ArtData = ArtData {
    name: "toolbar_project_save_as_new_disk",
    width: 90,
    height: 30,
    offset: 2979936,
    count: 418,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_project_unused_delete_remove_disk.png` — 90x30, 342 rects, 3 sprite cell(s).
pub const TOOLBAR_PROJECT_UNUSED_DELETE_REMOVE_DISK: ArtData = ArtData {
    name: "toolbar_project_unused_delete_remove_disk",
    width: 90,
    height: 30,
    offset: 2984952,
    count: 342,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_projprop.png` — 90x30, 663 rects, 1 sprite cell(s).
pub const TOOLBAR_PROJPROP: ArtData = ArtData {
    name: "toolbar_projprop",
    width: 90,
    height: 30,
    offset: 2989056,
    count: 663,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_quant.png` — 90x30, 396 rects, 3 sprite cell(s).
pub const TOOLBAR_QUANT: ArtData = ArtData {
    name: "toolbar_quant",
    width: 90,
    height: 30,
    offset: 2997012,
    count: 396,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_quant_off.png` — 90x30, 793 rects, 1 sprite cell(s).
pub const TOOLBAR_QUANT_OFF: ArtData = ArtData {
    name: "toolbar_quant_off",
    width: 90,
    height: 30,
    offset: 3001764,
    count: 793,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_quant_on.png` — 90x30, 793 rects, 1 sprite cell(s).
pub const TOOLBAR_QUANT_ON: ArtData = ArtData {
    name: "toolbar_quant_on",
    width: 90,
    height: 30,
    offset: 3011280,
    count: 793,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_quarter_crotchet_grid.png` — 90x30, 81 rects, 3 sprite cell(s).
pub const TOOLBAR_QUARTER_CROTCHET_GRID: ArtData = ArtData {
    name: "toolbar_quarter_crotchet_grid",
    width: 90,
    height: 30,
    offset: 3020796,
    count: 81,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_quarter_crotchet_note.png` — 90x30, 45 rects, 3 sprite cell(s).
pub const TOOLBAR_QUARTER_CROTCHET_NOTE: ArtData = ArtData {
    name: "toolbar_quarter_crotchet_note",
    width: 90,
    height: 30,
    offset: 3021768,
    count: 45,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_razor_off.png` — 90x30, 767 rects, 1 sprite cell(s).
pub const TOOLBAR_RAZOR_OFF: ArtData = ArtData {
    name: "toolbar_razor_off",
    width: 90,
    height: 30,
    offset: 3022308,
    count: 767,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_razor_on.png` — 90x30, 767 rects, 1 sprite cell(s).
pub const TOOLBAR_RAZOR_ON: ArtData = ArtData {
    name: "toolbar_razor_on",
    width: 90,
    height: 30,
    offset: 3031512,
    count: 767,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_record.png` — 90x30, 975 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD: ArtData = ArtData {
    name: "toolbar_record",
    width: 90,
    height: 30,
    offset: 3040716,
    count: 975,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_arm_all.png` — 90x30, 501 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_ARM_ALL: ArtData = ArtData {
    name: "toolbar_record_arm_all",
    width: 90,
    height: 30,
    offset: 3052416,
    count: 501,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_create_item_seperate_lane.png` — 90x30, 264 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_CREATE_ITEM_SEPERATE_LANE: ArtData = ArtData {
    name: "toolbar_record_create_item_seperate_lane",
    width: 90,
    height: 30,
    offset: 3058428,
    count: 264,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_loop_time_selection.png` — 90x30, 270 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_LOOP_TIME_SELECTION: ArtData = ArtData {
    name: "toolbar_record_loop_time_selection",
    width: 90,
    height: 30,
    offset: 3061596,
    count: 270,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_next_beat_measure.png` — 90x30, 198 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_NEXT_BEAT_MEASURE: ArtData = ArtData {
    name: "toolbar_record_next_beat_measure",
    width: 90,
    height: 30,
    offset: 3064836,
    count: 198,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_next_marker.png` — 90x30, 234 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_NEXT_MARKER: ArtData = ArtData {
    name: "toolbar_record_next_marker",
    width: 90,
    height: 30,
    offset: 3067212,
    count: 234,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_properties.png` — 90x30, 279 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_PROPERTIES: ArtData = ArtData {
    name: "toolbar_record_properties",
    width: 90,
    height: 30,
    offset: 3070020,
    count: 279,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_selected_item_auto_punch.png` — 90x30, 186 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_SELECTED_ITEM_AUTO_PUNCH: ArtData = ArtData {
    name: "toolbar_record_selected_item_auto_punch",
    width: 90,
    height: 30,
    offset: 3073368,
    count: 186,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_split_item_new_take.png` — 90x30, 234 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_SPLIT_ITEM_NEW_TAKE: ArtData = ArtData {
    name: "toolbar_record_split_item_new_take",
    width: 90,
    height: 30,
    offset: 3075600,
    count: 234,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_stop_delete.png` — 90x30, 342 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_STOP_DELETE: ArtData = ArtData {
    name: "toolbar_record_stop_delete",
    width: 90,
    height: 30,
    offset: 3078408,
    count: 342,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_stop_save.png` — 90x30, 360 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_STOP_SAVE: ArtData = ArtData {
    name: "toolbar_record_stop_save",
    width: 90,
    height: 30,
    offset: 3082512,
    count: 360,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_time_selection_auto_punch.png` — 90x30, 225 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_TIME_SELECTION_AUTO_PUNCH: ArtData = ArtData {
    name: "toolbar_record_time_selection_auto_punch",
    width: 90,
    height: 30,
    offset: 3086832,
    count: 225,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_time_selection_selected_item_auto_punch.png` — 90x30, 222 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_TIME_SELECTION_SELECTED_ITEM_AUTO_PUNCH: ArtData = ArtData {
    name: "toolbar_record_time_selection_selected_item_auto_punch",
    width: 90,
    height: 30,
    offset: 3089532,
    count: 222,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_record_trim_item_behind_tape.png` — 90x30, 528 rects, 3 sprite cell(s).
pub const TOOLBAR_RECORD_TRIM_ITEM_BEHIND_TAPE: ArtData = ArtData {
    name: "toolbar_record_trim_item_behind_tape",
    width: 90,
    height: 30,
    offset: 3092196,
    count: 528,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_redo.png` — 90x30, 392 rects, 1 sprite cell(s).
pub const TOOLBAR_REDO: ArtData = ArtData {
    name: "toolbar_redo",
    width: 90,
    height: 30,
    offset: 3098532,
    count: 392,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_region_delete_remove_none.png` — 90x30, 348 rects, 3 sprite cell(s).
pub const TOOLBAR_REGION_DELETE_REMOVE_NONE: ArtData = ArtData {
    name: "toolbar_region_delete_remove_none",
    width: 90,
    height: 30,
    offset: 3103236,
    count: 348,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_region_new.png` — 90x30, 186 rects, 3 sprite cell(s).
pub const TOOLBAR_REGION_NEW: ArtData = ArtData {
    name: "toolbar_region_new",
    width: 90,
    height: 30,
    offset: 3107412,
    count: 186,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_region_next.png` — 90x30, 213 rects, 3 sprite cell(s).
pub const TOOLBAR_REGION_NEXT: ArtData = ArtData {
    name: "toolbar_region_next",
    width: 90,
    height: 30,
    offset: 3109644,
    count: 213,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_region_play_loop.png` — 90x30, 351 rects, 3 sprite cell(s).
pub const TOOLBAR_REGION_PLAY_LOOP: ArtData = ArtData {
    name: "toolbar_region_play_loop",
    width: 90,
    height: 30,
    offset: 3112200,
    count: 351,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_region_previous.png` — 90x30, 213 rects, 3 sprite cell(s).
pub const TOOLBAR_REGION_PREVIOUS: ArtData = ArtData {
    name: "toolbar_region_previous",
    width: 90,
    height: 30,
    offset: 3116412,
    count: 213,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_region_properties.png` — 90x30, 237 rects, 3 sprite cell(s).
pub const TOOLBAR_REGION_PROPERTIES: ArtData = ArtData {
    name: "toolbar_region_properties",
    width: 90,
    height: 30,
    offset: 3118968,
    count: 237,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_region_time_selection.png` — 90x30, 273 rects, 3 sprite cell(s).
pub const TOOLBAR_REGION_TIME_SELECTION: ArtData = ArtData {
    name: "toolbar_region_time_selection",
    width: 90,
    height: 30,
    offset: 3121812,
    count: 273,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_relsnap.png` — 90x30, 150 rects, 3 sprite cell(s).
pub const TOOLBAR_RELSNAP: ArtData = ArtData {
    name: "toolbar_relsnap",
    width: 90,
    height: 30,
    offset: 3125088,
    count: 150,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_relsnap_off.png` — 90x30, 432 rects, 1 sprite cell(s).
pub const TOOLBAR_RELSNAP_OFF: ArtData = ArtData {
    name: "toolbar_relsnap_off",
    width: 90,
    height: 30,
    offset: 3126888,
    count: 432,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_relsnap_on.png` — 90x30, 432 rects, 1 sprite cell(s).
pub const TOOLBAR_RELSNAP_ON: ArtData = ArtData {
    name: "toolbar_relsnap_on",
    width: 90,
    height: 30,
    offset: 3132072,
    count: 432,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_remove_scissors_selection.png` — 90x30, 330 rects, 3 sprite cell(s).
pub const TOOLBAR_REMOVE_SCISSORS_SELECTION: ArtData = ArtData {
    name: "toolbar_remove_scissors_selection",
    width: 90,
    height: 30,
    offset: 3137256,
    count: 330,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_remove_selection_none.png` — 90x30, 150 rects, 3 sprite cell(s).
pub const TOOLBAR_REMOVE_SELECTION_NONE: ArtData = ArtData {
    name: "toolbar_remove_selection_none",
    width: 90,
    height: 30,
    offset: 3141216,
    count: 150,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_render_effects_midi.png` — 90x30, 366 rects, 3 sprite cell(s).
pub const TOOLBAR_RENDER_EFFECTS_MIDI: ArtData = ArtData {
    name: "toolbar_render_effects_midi",
    width: 90,
    height: 30,
    offset: 3143016,
    count: 366,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_replacemode.png` — 90x30, 253 rects, 1 sprite cell(s).
pub const TOOLBAR_REPLACEMODE: ArtData = ArtData {
    name: "toolbar_replacemode",
    width: 90,
    height: 30,
    offset: 3147408,
    count: 253,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_revert.png` — 90x30, 637 rects, 1 sprite cell(s).
pub const TOOLBAR_REVERT: ArtData = ArtData {
    name: "toolbar_revert",
    width: 90,
    height: 30,
    offset: 3150444,
    count: 637,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ripple.png` — 90x30, 99 rects, 3 sprite cell(s).
pub const TOOLBAR_RIPPLE: ArtData = ArtData {
    name: "toolbar_ripple",
    width: 90,
    height: 30,
    offset: 3158088,
    count: 99,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_ripple_all.png` — 90x30, 308 rects, 3 sprite cell(s).
pub const TOOLBAR_RIPPLE_ALL: ArtData = ArtData {
    name: "toolbar_ripple_all",
    width: 90,
    height: 30,
    offset: 3159276,
    count: 308,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_ripple_off.png` — 90x30, 373 rects, 1 sprite cell(s).
pub const TOOLBAR_RIPPLE_OFF: ArtData = ArtData {
    name: "toolbar_ripple_off",
    width: 90,
    height: 30,
    offset: 3162972,
    count: 373,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ripple_on.png` — 90x30, 373 rects, 1 sprite cell(s).
pub const TOOLBAR_RIPPLE_ON: ArtData = ArtData {
    name: "toolbar_ripple_on",
    width: 90,
    height: 30,
    offset: 3167448,
    count: 373,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_ripple_one.png` — 90x30, 210 rects, 3 sprite cell(s).
pub const TOOLBAR_RIPPLE_ONE: ArtData = ArtData {
    name: "toolbar_ripple_one",
    width: 90,
    height: 30,
    offset: 3171924,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_save.png` — 90x30, 369 rects, 1 sprite cell(s).
pub const TOOLBAR_SAVE: ArtData = ArtData {
    name: "toolbar_save",
    width: 90,
    height: 30,
    offset: 3174444,
    count: 369,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_screenset_camera_list.png` — 90x30, 207 rects, 3 sprite cell(s).
pub const TOOLBAR_SCREENSET_CAMERA_LIST: ArtData = ArtData {
    name: "toolbar_screenset_camera_list",
    width: 90,
    height: 30,
    offset: 3178872,
    count: 207,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_screenset_camera_new.png` — 90x30, 201 rects, 3 sprite cell(s).
pub const TOOLBAR_SCREENSET_CAMERA_NEW: ArtData = ArtData {
    name: "toolbar_screenset_camera_new",
    width: 90,
    height: 30,
    offset: 3181356,
    count: 201,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_screenset_camera_next.png` — 90x30, 333 rects, 3 sprite cell(s).
pub const TOOLBAR_SCREENSET_CAMERA_NEXT: ArtData = ArtData {
    name: "toolbar_screenset_camera_next",
    width: 90,
    height: 30,
    offset: 3183768,
    count: 333,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_screenset_camera_previous.png` — 90x30, 333 rects, 3 sprite cell(s).
pub const TOOLBAR_SCREENSET_CAMERA_PREVIOUS: ArtData = ArtData {
    name: "toolbar_screenset_camera_previous",
    width: 90,
    height: 30,
    offset: 3187764,
    count: 333,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_screenset_camera_save_disk.png` — 90x30, 402 rects, 3 sprite cell(s).
pub const TOOLBAR_SCREENSET_CAMERA_SAVE_DISK: ArtData = ArtData {
    name: "toolbar_screenset_camera_save_disk",
    width: 90,
    height: 30,
    offset: 3191760,
    count: 402,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_selection_delete_remove.png` — 90x30, 156 rects, 3 sprite cell(s).
pub const TOOLBAR_SELECTION_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_selection_delete_remove",
    width: 90,
    height: 30,
    offset: 3196584,
    count: 156,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_selection_inverse_delete_remove.png` — 90x30, 156 rects, 3 sprite cell(s).
pub const TOOLBAR_SELECTION_INVERSE_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_selection_inverse_delete_remove",
    width: 90,
    height: 30,
    offset: 3198456,
    count: 156,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_send_hide_mute.png` — 90x30, 354 rects, 3 sprite cell(s).
pub const TOOLBAR_SEND_HIDE_MUTE: ArtData = ArtData {
    name: "toolbar_send_hide_mute",
    width: 90,
    height: 30,
    offset: 3200328,
    count: 354,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_send_show_enable.png` — 90x30, 411 rects, 3 sprite cell(s).
pub const TOOLBAR_SEND_SHOW_ENABLE: ArtData = ArtData {
    name: "toolbar_send_show_enable",
    width: 90,
    height: 30,
    offset: 3204576,
    count: 411,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_shape_bezier.png` — 90x30, 135 rects, 3 sprite cell(s).
pub const TOOLBAR_SHAPE_BEZIER: ArtData = ArtData {
    name: "toolbar_shape_bezier",
    width: 90,
    height: 30,
    offset: 3209508,
    count: 135,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_shape_fast_end.png` — 90x30, 144 rects, 3 sprite cell(s).
pub const TOOLBAR_SHAPE_FAST_END: ArtData = ArtData {
    name: "toolbar_shape_fast_end",
    width: 90,
    height: 30,
    offset: 3211128,
    count: 144,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_shape_fast_start.png` — 90x30, 144 rects, 3 sprite cell(s).
pub const TOOLBAR_SHAPE_FAST_START: ArtData = ArtData {
    name: "toolbar_shape_fast_start",
    width: 90,
    height: 30,
    offset: 3212856,
    count: 144,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_shape_linear.png` — 90x30, 198 rects, 3 sprite cell(s).
pub const TOOLBAR_SHAPE_LINEAR: ArtData = ArtData {
    name: "toolbar_shape_linear",
    width: 90,
    height: 30,
    offset: 3214584,
    count: 198,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_shape_square.png` — 90x30, 9 rects, 3 sprite cell(s).
pub const TOOLBAR_SHAPE_SQUARE: ArtData = ArtData {
    name: "toolbar_shape_square",
    width: 90,
    height: 30,
    offset: 3216960,
    count: 9,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_show.png` — 90x30, 342 rects, 3 sprite cell(s).
pub const TOOLBAR_SHOW: ArtData = ArtData {
    name: "toolbar_show",
    width: 90,
    height: 30,
    offset: 3219372,
    count: 342,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_show selected.png` — 90x30, 192 rects, 3 sprite cell(s).
pub const TOOLBAR_SHOW_SELECTED: ArtData = ArtData {
    name: "toolbar_show selected",
    width: 90,
    height: 30,
    offset: 3217068,
    count: 192,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_show_insert.png` — 90x30, 162 rects, 3 sprite cell(s).
pub const TOOLBAR_SHOW_INSERT: ArtData = ArtData {
    name: "toolbar_show_insert",
    width: 90,
    height: 30,
    offset: 3223476,
    count: 162,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_show_parameter.png` — 90x30, 396 rects, 3 sprite cell(s).
pub const TOOLBAR_SHOW_PARAMETER: ArtData = ArtData {
    name: "toolbar_show_parameter",
    width: 90,
    height: 30,
    offset: 3225420,
    count: 396,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_show_send.png` — 90x30, 225 rects, 3 sprite cell(s).
pub const TOOLBAR_SHOW_SEND: ArtData = ArtData {
    name: "toolbar_show_send",
    width: 90,
    height: 30,
    offset: 3230172,
    count: 225,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_shuttle_back_rewind.png` — 90x30, 468 rects, 3 sprite cell(s).
pub const TOOLBAR_SHUTTLE_BACK_REWIND: ArtData = ArtData {
    name: "toolbar_shuttle_back_rewind",
    width: 90,
    height: 30,
    offset: 3232872,
    count: 468,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_shuttle_forward.png` — 90x30, 474 rects, 3 sprite cell(s).
pub const TOOLBAR_SHUTTLE_FORWARD: ArtData = ArtData {
    name: "toolbar_shuttle_forward",
    width: 90,
    height: 30,
    offset: 3238488,
    count: 474,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_sixteenth_semiquaver_grid.png` — 90x30, 177 rects, 3 sprite cell(s).
pub const TOOLBAR_SIXTEENTH_SEMIQUAVER_GRID: ArtData = ArtData {
    name: "toolbar_sixteenth_semiquaver_grid",
    width: 90,
    height: 30,
    offset: 3244176,
    count: 177,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_sixteenth_semiquaver_note.png` — 90x30, 141 rects, 3 sprite cell(s).
pub const TOOLBAR_SIXTEENTH_SEMIQUAVER_NOTE: ArtData = ArtData {
    name: "toolbar_sixteenth_semiquaver_note",
    width: 90,
    height: 30,
    offset: 3246300,
    count: 141,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_snap.png` — 90x30, 135 rects, 3 sprite cell(s).
pub const TOOLBAR_SNAP: ArtData = ArtData {
    name: "toolbar_snap",
    width: 90,
    height: 30,
    offset: 3247992,
    count: 135,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_snap_off.png` — 90x30, 399 rects, 1 sprite cell(s).
pub const TOOLBAR_SNAP_OFF: ArtData = ArtData {
    name: "toolbar_snap_off",
    width: 90,
    height: 30,
    offset: 3249612,
    count: 399,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_snap_offset_grid_move.png` — 90x30, 126 rects, 3 sprite cell(s).
pub const TOOLBAR_SNAP_OFFSET_GRID_MOVE: ArtData = ArtData {
    name: "toolbar_snap_offset_grid_move",
    width: 90,
    height: 30,
    offset: 3254400,
    count: 126,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_snap_on.png` — 90x30, 399 rects, 1 sprite cell(s).
pub const TOOLBAR_SNAP_ON: ArtData = ArtData {
    name: "toolbar_snap_on",
    width: 90,
    height: 30,
    offset: 3255912,
    count: 399,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_solo_in_front_dim.png` — 90x30, 336 rects, 3 sprite cell(s).
pub const TOOLBAR_SOLO_IN_FRONT_DIM: ArtData = ArtData {
    name: "toolbar_solo_in_front_dim",
    width: 90,
    height: 30,
    offset: 3260700,
    count: 336,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_solo_none_unsolo.png` — 90x30, 414 rects, 3 sprite cell(s).
pub const TOOLBAR_SOLO_NONE_UNSOLO: ArtData = ArtData {
    name: "toolbar_solo_none_unsolo",
    width: 90,
    height: 30,
    offset: 3264732,
    count: 414,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_split_scissors.png` — 90x30, 309 rects, 3 sprite cell(s).
pub const TOOLBAR_SPLIT_SCISSORS: ArtData = ArtData {
    name: "toolbar_split_scissors",
    width: 90,
    height: 30,
    offset: 3269700,
    count: 309,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_stretch_marker_delete_remove.png` — 90x30, 246 rects, 3 sprite cell(s).
pub const TOOLBAR_STRETCH_MARKER_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_stretch_marker_delete_remove",
    width: 90,
    height: 30,
    offset: 3273408,
    count: 246,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_stretch_marker_insert_new_add.png` — 90x30, 126 rects, 3 sprite cell(s).
pub const TOOLBAR_STRETCH_MARKER_INSERT_NEW_ADD: ArtData = ArtData {
    name: "toolbar_stretch_marker_insert_new_add",
    width: 90,
    height: 30,
    offset: 3276360,
    count: 126,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_stretch_marker_locking.png` — 90x30, 171 rects, 3 sprite cell(s).
pub const TOOLBAR_STRETCH_MARKER_LOCKING: ArtData = ArtData {
    name: "toolbar_stretch_marker_locking",
    width: 90,
    height: 30,
    offset: 3277872,
    count: 171,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_stretch_marker_next.png` — 90x30, 174 rects, 3 sprite cell(s).
pub const TOOLBAR_STRETCH_MARKER_NEXT: ArtData = ArtData {
    name: "toolbar_stretch_marker_next",
    width: 90,
    height: 30,
    offset: 3279924,
    count: 174,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_stretch_marker_previous.png` — 90x30, 174 rects, 3 sprite cell(s).
pub const TOOLBAR_STRETCH_MARKER_PREVIOUS: ArtData = ArtData {
    name: "toolbar_stretch_marker_previous",
    width: 90,
    height: 30,
    offset: 3282012,
    count: 174,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_stretch_marker_snap_grid.png` — 90x30, 240 rects, 3 sprite cell(s).
pub const TOOLBAR_STRETCH_MARKER_SNAP_GRID: ArtData = ArtData {
    name: "toolbar_stretch_marker_snap_grid",
    width: 90,
    height: 30,
    offset: 3284100,
    count: 240,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_stretch_marker_time_selection_delete_remove.png` — 90x30, 165 rects, 3 sprite cell(s).
pub const TOOLBAR_STRETCH_MARKER_TIME_SELECTION_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_stretch_marker_time_selection_delete_remove",
    width: 90,
    height: 30,
    offset: 3286980,
    count: 165,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_stretch_marker_time_selection_new_add.png` — 90x30, 165 rects, 3 sprite cell(s).
pub const TOOLBAR_STRETCH_MARKER_TIME_SELECTION_NEW_ADD: ArtData = ArtData {
    name: "toolbar_stretch_marker_time_selection_new_add",
    width: 90,
    height: 30,
    offset: 3288960,
    count: 165,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_stretch_marker_tonal.png` — 90x30, 228 rects, 3 sprite cell(s).
pub const TOOLBAR_STRETCH_MARKER_TONAL: ArtData = ArtData {
    name: "toolbar_stretch_marker_tonal",
    width: 90,
    height: 30,
    offset: 3290940,
    count: 228,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_sws_extension.png` — 90x30, 453 rects, 3 sprite cell(s).
pub const TOOLBAR_SWS_EXTENSION: ArtData = ArtData {
    name: "toolbar_sws_extension",
    width: 90,
    height: 30,
    offset: 3293676,
    count: 453,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_sws_extension_properties.png` — 90x30, 543 rects, 3 sprite cell(s).
pub const TOOLBAR_SWS_EXTENSION_PROPERTIES: ArtData = ArtData {
    name: "toolbar_sws_extension_properties",
    width: 90,
    height: 30,
    offset: 3299112,
    count: 543,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_sync_follow_play.png` — 90x30, 480 rects, 3 sprite cell(s).
pub const TOOLBAR_SYNC_FOLLOW_PLAY: ArtData = ArtData {
    name: "toolbar_sync_follow_play",
    width: 90,
    height: 30,
    offset: 3305628,
    count: 480,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_sync_follow_record.png` — 90x30, 621 rects, 3 sprite cell(s).
pub const TOOLBAR_SYNC_FOLLOW_RECORD: ArtData = ArtData {
    name: "toolbar_sync_follow_record",
    width: 90,
    height: 30,
    offset: 3311388,
    count: 621,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_system_external.png` — 90x30, 399 rects, 3 sprite cell(s).
pub const TOOLBAR_SYSTEM_EXTERNAL: ArtData = ArtData {
    name: "toolbar_system_external",
    width: 90,
    height: 30,
    offset: 3318840,
    count: 399,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_system_properties.png` — 90x30, 471 rects, 3 sprite cell(s).
pub const TOOLBAR_SYSTEM_PROPERTIES: ArtData = ArtData {
    name: "toolbar_system_properties",
    width: 90,
    height: 30,
    offset: 3323628,
    count: 471,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_system_set_save_default_disk.png` — 90x30, 576 rects, 3 sprite cell(s).
pub const TOOLBAR_SYSTEM_SET_SAVE_DEFAULT_DISK: ArtData = ArtData {
    name: "toolbar_system_set_save_default_disk",
    width: 90,
    height: 30,
    offset: 3329280,
    count: 576,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_theme_next.png` — 90x30, 270 rects, 3 sprite cell(s).
pub const TOOLBAR_THEME_NEXT: ArtData = ArtData {
    name: "toolbar_theme_next",
    width: 90,
    height: 30,
    offset: 3336192,
    count: 270,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_theme_previous.png` — 90x30, 270 rects, 3 sprite cell(s).
pub const TOOLBAR_THEME_PREVIOUS: ArtData = ArtData {
    name: "toolbar_theme_previous",
    width: 90,
    height: 30,
    offset: 3339432,
    count: 270,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_theme_refresh.png` — 90x30, 262 rects, 3 sprite cell(s).
pub const TOOLBAR_THEME_REFRESH: ArtData = ArtData {
    name: "toolbar_theme_refresh",
    width: 90,
    height: 30,
    offset: 3342672,
    count: 262,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_thirty_second_demisemiquaver_.png` — 90x30, 150 rects, 3 sprite cell(s).
pub const TOOLBAR_THIRTY_SECOND_DEMISEMIQUAVER_: ArtData = ArtData {
    name: "toolbar_thirty_second_demisemiquaver_",
    width: 90,
    height: 30,
    offset: 3345816,
    count: 150,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_thirty_second_demisemiquaver_grid.png` — 90x30, 186 rects, 3 sprite cell(s).
pub const TOOLBAR_THIRTY_SECOND_DEMISEMIQUAVER_GRID: ArtData = ArtData {
    name: "toolbar_thirty_second_demisemiquaver_grid",
    width: 90,
    height: 30,
    offset: 3347616,
    count: 186,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_beats.png` — 90x30, 276 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_BEATS: ArtData = ArtData {
    name: "toolbar_time_beats",
    width: 90,
    height: 30,
    offset: 3349848,
    count: 276,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_clock.png` — 90x30, 431 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_CLOCK: ArtData = ArtData {
    name: "toolbar_time_clock",
    width: 90,
    height: 30,
    offset: 3353160,
    count: 431,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_clock_properties.png` — 90x30, 456 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_CLOCK_PROPERTIES: ArtData = ArtData {
    name: "toolbar_time_clock_properties",
    width: 90,
    height: 30,
    offset: 3358332,
    count: 456,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_hourglass.png` — 90x30, 210 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_HOURGLASS: ArtData = ArtData {
    name: "toolbar_time_hourglass",
    width: 90,
    height: 30,
    offset: 3363804,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_hours.png` — 90x30, 672 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_HOURS: ArtData = ArtData {
    name: "toolbar_time_hours",
    width: 90,
    height: 30,
    offset: 3366324,
    count: 672,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_measures.png` — 90x30, 276 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_MEASURES: ArtData = ArtData {
    name: "toolbar_time_measures",
    width: 90,
    height: 30,
    offset: 3374388,
    count: 276,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_minutes.png` — 90x30, 636 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_MINUTES: ArtData = ArtData {
    name: "toolbar_time_minutes",
    width: 90,
    height: 30,
    offset: 3377700,
    count: 636,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_sample.png` — 90x30, 276 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SAMPLE: ArtData = ArtData {
    name: "toolbar_time_sample",
    width: 90,
    height: 30,
    offset: 3385332,
    count: 276,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_seconds.png` — 90x30, 648 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SECONDS: ArtData = ArtData {
    name: "toolbar_time_seconds",
    width: 90,
    height: 30,
    offset: 3388644,
    count: 648,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_delete_remove.png` — 90x30, 213 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_time_selection_delete_remove",
    width: 90,
    height: 30,
    offset: 3396420,
    count: 213,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_fit_item_selected.png` — 90x30, 93 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_FIT_ITEM_SELECTED: ArtData = ArtData {
    name: "toolbar_time_selection_fit_item_selected",
    width: 90,
    height: 30,
    offset: 3398976,
    count: 93,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_item_cut.png` — 90x30, 275 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_ITEM_CUT: ArtData = ArtData {
    name: "toolbar_time_selection_item_cut",
    width: 90,
    height: 30,
    offset: 3400092,
    count: 275,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_item_delete_remove.png` — 90x30, 207 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_ITEM_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_time_selection_item_delete_remove",
    width: 90,
    height: 30,
    offset: 3403392,
    count: 207,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_item_selected_grow_expand.png` — 90x30, 93 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_ITEM_SELECTED_GROW_EXPAND: ArtData = ArtData {
    name: "toolbar_time_selection_item_selected_grow_expand",
    width: 90,
    height: 30,
    offset: 3405876,
    count: 93,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_left.png` — 90x30, 48 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_LEFT: ArtData = ArtData {
    name: "toolbar_time_selection_left",
    width: 90,
    height: 30,
    offset: 3406992,
    count: 48,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_loop_lock.png` — 90x30, 204 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_LOOP_LOCK: ArtData = ArtData {
    name: "toolbar_time_selection_loop_lock",
    width: 90,
    height: 30,
    offset: 3407568,
    count: 204,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_loop_play.png` — 90x30, 225 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_LOOP_PLAY: ArtData = ArtData {
    name: "toolbar_time_selection_loop_play",
    width: 90,
    height: 30,
    offset: 3410016,
    count: 225,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_new.png` — 90x30, 135 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_NEW: ArtData = ArtData {
    name: "toolbar_time_selection_new",
    width: 90,
    height: 30,
    offset: 3412716,
    count: 135,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_play.png` — 90x30, 108 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_PLAY: ArtData = ArtData {
    name: "toolbar_time_selection_play",
    width: 90,
    height: 30,
    offset: 3414336,
    count: 108,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_properties.png` — 90x30, 168 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_PROPERTIES: ArtData = ArtData {
    name: "toolbar_time_selection_properties",
    width: 90,
    height: 30,
    offset: 3415632,
    count: 168,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_region.png` — 90x30, 198 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_REGION: ArtData = ArtData {
    name: "toolbar_time_selection_region",
    width: 90,
    height: 30,
    offset: 3417648,
    count: 198,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_selection_right.png` — 90x30, 48 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_SELECTION_RIGHT: ArtData = ArtData {
    name: "toolbar_time_selection_right",
    width: 90,
    height: 30,
    offset: 3420024,
    count: 48,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_time_stretch.png` — 90x30, 405 rects, 3 sprite cell(s).
pub const TOOLBAR_TIME_STRETCH: ArtData = ArtData {
    name: "toolbar_time_stretch",
    width: 90,
    height: 30,
    offset: 3420600,
    count: 405,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_timebase_beats_position.png` — 90x30, 87 rects, 3 sprite cell(s).
pub const TOOLBAR_TIMEBASE_BEATS_POSITION: ArtData = ArtData {
    name: "toolbar_timebase_beats_position",
    width: 90,
    height: 30,
    offset: 3425460,
    count: 87,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_timebase_beats_position_length_rate.png` — 90x30, 87 rects, 3 sprite cell(s).
pub const TOOLBAR_TIMEBASE_BEATS_POSITION_LENGTH_RATE: ArtData = ArtData {
    name: "toolbar_timebase_beats_position_length_rate",
    width: 90,
    height: 30,
    offset: 3426504,
    count: 87,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_timebase_time.png` — 90x30, 416 rects, 3 sprite cell(s).
pub const TOOLBAR_TIMEBASE_TIME: ArtData = ArtData {
    name: "toolbar_timebase_time",
    width: 90,
    height: 30,
    offset: 3427548,
    count: 416,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_tool_brush_paint.png` — 90x30, 174 rects, 3 sprite cell(s).
pub const TOOLBAR_TOOL_BRUSH_PAINT: ArtData = ArtData {
    name: "toolbar_tool_brush_paint",
    width: 90,
    height: 30,
    offset: 3432540,
    count: 174,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_tool_crop.png` — 90x30, 93 rects, 3 sprite cell(s).
pub const TOOLBAR_TOOL_CROP: ArtData = ArtData {
    name: "toolbar_tool_crop",
    width: 90,
    height: 30,
    offset: 3434628,
    count: 93,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_tool_erase_delete_remove.png` — 90x30, 228 rects, 3 sprite cell(s).
pub const TOOLBAR_TOOL_ERASE_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_tool_erase_delete_remove",
    width: 90,
    height: 30,
    offset: 3435744,
    count: 228,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_tool_hammer.png` — 90x30, 252 rects, 3 sprite cell(s).
pub const TOOLBAR_TOOL_HAMMER: ArtData = ArtData {
    name: "toolbar_tool_hammer",
    width: 90,
    height: 30,
    offset: 3438480,
    count: 252,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_tool_knife_trim.png` — 90x30, 204 rects, 3 sprite cell(s).
pub const TOOLBAR_TOOL_KNIFE_TRIM: ArtData = ArtData {
    name: "toolbar_tool_knife_trim",
    width: 90,
    height: 30,
    offset: 3441504,
    count: 204,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_tool_pencil_draw.png` — 90x30, 333 rects, 3 sprite cell(s).
pub const TOOLBAR_TOOL_PENCIL_DRAW: ArtData = ArtData {
    name: "toolbar_tool_pencil_draw",
    width: 90,
    height: 30,
    offset: 3443952,
    count: 333,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_tool_razor_blade.png` — 90x30, 245 rects, 3 sprite cell(s).
pub const TOOLBAR_TOOL_RAZOR_BLADE: ArtData = ArtData {
    name: "toolbar_tool_razor_blade",
    width: 90,
    height: 30,
    offset: 3447948,
    count: 245,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_tool_scissors_cut_trim.png` — 90x30, 288 rects, 3 sprite cell(s).
pub const TOOLBAR_TOOL_SCISSORS_CUT_TRIM: ArtData = ArtData {
    name: "toolbar_tool_scissors_cut_trim",
    width: 90,
    height: 30,
    offset: 3450888,
    count: 288,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_track_next.png` — 90x30, 84 rects, 3 sprite cell(s).
pub const TOOLBAR_TRACK_NEXT: ArtData = ArtData {
    name: "toolbar_track_next",
    width: 90,
    height: 30,
    offset: 3454344,
    count: 84,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_track_previous.png` — 90x30, 84 rects, 3 sprite cell(s).
pub const TOOLBAR_TRACK_PREVIOUS: ArtData = ArtData {
    name: "toolbar_track_previous",
    width: 90,
    height: 30,
    offset: 3455352,
    count: 84,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_transport_home_end.png` — 90x30, 309 rects, 3 sprite cell(s).
pub const TOOLBAR_TRANSPORT_HOME_END: ArtData = ArtData {
    name: "toolbar_transport_home_end",
    width: 90,
    height: 30,
    offset: 3456360,
    count: 309,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_treble_clef_note.png` — 90x30, 300 rects, 3 sprite cell(s).
pub const TOOLBAR_TREBLE_CLEF_NOTE: ArtData = ArtData {
    name: "toolbar_treble_clef_note",
    width: 90,
    height: 30,
    offset: 3460068,
    count: 300,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_trim_scissors_selection.png` — 90x30, 318 rects, 3 sprite cell(s).
pub const TOOLBAR_TRIM_SCISSORS_SELECTION: ArtData = ArtData {
    name: "toolbar_trim_scissors_selection",
    width: 90,
    height: 30,
    offset: 3463668,
    count: 318,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_triplet_note.png` — 90x30, 96 rects, 3 sprite cell(s).
pub const TOOLBAR_TRIPLET_NOTE: ArtData = ArtData {
    name: "toolbar_triplet_note",
    width: 90,
    height: 30,
    offset: 3467484,
    count: 96,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_undo.png` — 90x30, 392 rects, 1 sprite cell(s).
pub const TOOLBAR_UNDO: ArtData = ArtData {
    name: "toolbar_undo",
    width: 90,
    height: 30,
    offset: 3468636,
    count: 392,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_unfreeze_render_apply_snowflake.png` — 90x30, 690 rects, 3 sprite cell(s).
pub const TOOLBAR_UNFREEZE_RENDER_APPLY_SNOWFLAKE: ArtData = ArtData {
    name: "toolbar_unfreeze_render_apply_snowflake",
    width: 90,
    height: 30,
    offset: 3473340,
    count: 690,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_envitem.png` — 90x30, 102 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_ENVITEM: ArtData = ArtData {
    name: "toolbar_v3_envitem",
    width: 90,
    height: 30,
    offset: 3481620,
    count: 102,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_grid.png` — 90x30, 45 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_GRID: ArtData = ArtData {
    name: "toolbar_v3_grid",
    width: 90,
    height: 30,
    offset: 3482844,
    count: 45,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_group.png` — 90x30, 162 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_GROUP: ArtData = ArtData {
    name: "toolbar_v3_group",
    width: 90,
    height: 30,
    offset: 3483384,
    count: 162,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_load.png` — 90x30, 216 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_LOAD: ArtData = ArtData {
    name: "toolbar_v3_load",
    width: 90,
    height: 30,
    offset: 3485328,
    count: 216,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_lock.png` — 90x30, 78 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_LOCK: ArtData = ArtData {
    name: "toolbar_v3_lock",
    width: 90,
    height: 30,
    offset: 3487920,
    count: 78,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_metro.png` — 90x30, 153 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_METRO: ArtData = ArtData {
    name: "toolbar_v3_metro",
    width: 90,
    height: 30,
    offset: 3488856,
    count: 153,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_new.png` — 90x30, 198 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_NEW: ArtData = ArtData {
    name: "toolbar_v3_new",
    width: 90,
    height: 30,
    offset: 3490692,
    count: 198,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_projprop.png` — 90x30, 201 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_PROJPROP: ArtData = ArtData {
    name: "toolbar_v3_projprop",
    width: 90,
    height: 30,
    offset: 3493068,
    count: 201,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_redo.png` — 90x30, 273 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_REDO: ArtData = ArtData {
    name: "toolbar_v3_redo",
    width: 90,
    height: 30,
    offset: 3495480,
    count: 273,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_ripple.png` — 90x30, 27 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_RIPPLE: ArtData = ArtData {
    name: "toolbar_v3_ripple",
    width: 90,
    height: 30,
    offset: 3498756,
    count: 27,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_ripple_all.png` — 90x30, 27 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_RIPPLE_ALL: ArtData = ArtData {
    name: "toolbar_v3_ripple_all",
    width: 90,
    height: 30,
    offset: 3499080,
    count: 27,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_ripple_one.png` — 90x30, 27 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_RIPPLE_ONE: ArtData = ArtData {
    name: "toolbar_v3_ripple_one",
    width: 90,
    height: 30,
    offset: 3499404,
    count: 27,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_save.png` — 90x30, 246 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_SAVE: ArtData = ArtData {
    name: "toolbar_v3_save",
    width: 90,
    height: 30,
    offset: 3499728,
    count: 246,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_snap.png` — 90x30, 207 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_SNAP: ArtData = ArtData {
    name: "toolbar_v3_snap",
    width: 90,
    height: 30,
    offset: 3502680,
    count: 207,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_undo.png` — 90x30, 270 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_UNDO: ArtData = ArtData {
    name: "toolbar_v3_undo",
    width: 90,
    height: 30,
    offset: 3505164,
    count: 270,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_v3_xfade.png` — 90x30, 6 rects, 3 sprite cell(s).
pub const TOOLBAR_V3_XFADE: ArtData = ArtData {
    name: "toolbar_v3_xfade",
    width: 90,
    height: 30,
    offset: 3508404,
    count: 6,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_video_item_selected.png` — 90x30, 183 rects, 3 sprite cell(s).
pub const TOOLBAR_VIDEO_ITEM_SELECTED: ArtData = ArtData {
    name: "toolbar_video_item_selected",
    width: 90,
    height: 30,
    offset: 3508476,
    count: 183,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_video_properties.png` — 90x30, 219 rects, 3 sprite cell(s).
pub const TOOLBAR_VIDEO_PROPERTIES: ArtData = ArtData {
    name: "toolbar_video_properties",
    width: 90,
    height: 30,
    offset: 3510672,
    count: 219,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_video_screen.png` — 90x30, 102 rects, 3 sprite cell(s).
pub const TOOLBAR_VIDEO_SCREEN: ArtData = ArtData {
    name: "toolbar_video_screen",
    width: 90,
    height: 30,
    offset: 3513300,
    count: 102,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_video_sync_start.png` — 90x30, 126 rects, 3 sprite cell(s).
pub const TOOLBAR_VIDEO_SYNC_START: ArtData = ArtData {
    name: "toolbar_video_sync_start",
    width: 90,
    height: 30,
    offset: 3514524,
    count: 126,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_visible_mixer.png` — 90x30, 210 rects, 3 sprite cell(s).
pub const TOOLBAR_VISIBLE_MIXER: ArtData = ArtData {
    name: "toolbar_visible_mixer",
    width: 90,
    height: 30,
    offset: 3516036,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_visible_tcp.png` — 90x30, 210 rects, 3 sprite cell(s).
pub const TOOLBAR_VISIBLE_TCP: ArtData = ArtData {
    name: "toolbar_visible_tcp",
    width: 90,
    height: 30,
    offset: 3518556,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_whole_semibreve_grid.png` — 90x30, 96 rects, 3 sprite cell(s).
pub const TOOLBAR_WHOLE_SEMIBREVE_GRID: ArtData = ArtData {
    name: "toolbar_whole_semibreve_grid",
    width: 90,
    height: 30,
    offset: 3521076,
    count: 96,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_whole_semibreve_note.png` — 90x30, 60 rects, 3 sprite cell(s).
pub const TOOLBAR_WHOLE_SEMIBREVE_NOTE: ArtData = ArtData {
    name: "toolbar_whole_semibreve_note",
    width: 90,
    height: 30,
    offset: 3522228,
    count: 60,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_floating_toolbar.png` — 90x30, 315 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_FLOATING_TOOLBAR: ArtData = ArtData {
    name: "toolbar_window_floating_toolbar",
    width: 90,
    height: 30,
    offset: 3522948,
    count: 315,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_fullscreen.png` — 90x30, 264 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_FULLSCREEN: ArtData = ArtData {
    name: "toolbar_window_fullscreen",
    width: 90,
    height: 30,
    offset: 3526728,
    count: 264,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_background_synchronize.png` — 90x30, 165 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_BACKGROUND_SYNCHRONIZE: ArtData = ArtData {
    name: "toolbar_window_tab_background_synchronize",
    width: 90,
    height: 30,
    offset: 3529896,
    count: 165,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_clip.png` — 90x30, 492 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_CLIP: ArtData = ArtData {
    name: "toolbar_window_tab_clip",
    width: 90,
    height: 30,
    offset: 3531876,
    count: 492,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_clock.png` — 90x30, 468 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_CLOCK: ArtData = ArtData {
    name: "toolbar_window_tab_clock",
    width: 90,
    height: 30,
    offset: 3537780,
    count: 468,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_delete_remove.png` — 90x30, 354 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_DELETE_REMOVE: ArtData = ArtData {
    name: "toolbar_window_tab_delete_remove",
    width: 90,
    height: 30,
    offset: 3543396,
    count: 354,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_docker_bottom.png` — 90x30, 48 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_DOCKER_BOTTOM: ArtData = ArtData {
    name: "toolbar_window_tab_docker_bottom",
    width: 90,
    height: 30,
    offset: 3547644,
    count: 48,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_docker_left.png` — 90x30, 48 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_DOCKER_LEFT: ArtData = ArtData {
    name: "toolbar_window_tab_docker_left",
    width: 90,
    height: 30,
    offset: 3548220,
    count: 48,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_docker_right.png` — 90x30, 48 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_DOCKER_RIGHT: ArtData = ArtData {
    name: "toolbar_window_tab_docker_right",
    width: 90,
    height: 30,
    offset: 3548796,
    count: 48,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_docker_show.png` — 90x30, 375 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_DOCKER_SHOW: ArtData = ArtData {
    name: "toolbar_window_tab_docker_show",
    width: 90,
    height: 30,
    offset: 3549372,
    count: 375,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_docker_top.png` — 90x30, 48 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_DOCKER_TOP: ArtData = ArtData {
    name: "toolbar_window_tab_docker_top",
    width: 90,
    height: 30,
    offset: 3553872,
    count: 48,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_effects.png` — 90x30, 285 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_EFFECTS: ArtData = ArtData {
    name: "toolbar_window_tab_effects",
    width: 90,
    height: 30,
    offset: 3554448,
    count: 285,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_folder.png` — 90x30, 120 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_FOLDER: ArtData = ArtData {
    name: "toolbar_window_tab_folder",
    width: 90,
    height: 30,
    offset: 3557868,
    count: 120,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_list.png` — 90x30, 153 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_LIST: ArtData = ArtData {
    name: "toolbar_window_tab_list",
    width: 90,
    height: 30,
    offset: 3559308,
    count: 153,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_midi_editor.png` — 90x30, 336 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_MIDI_EDITOR: ArtData = ArtData {
    name: "toolbar_window_tab_midi_editor",
    width: 90,
    height: 30,
    offset: 3561144,
    count: 336,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_mixer_mcp.png` — 90x30, 108 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_MIXER_MCP: ArtData = ArtData {
    name: "toolbar_window_tab_mixer_mcp",
    width: 90,
    height: 30,
    offset: 3565176,
    count: 108,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_mixer_tcp.png` — 90x30, 120 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_MIXER_TCP: ArtData = ArtData {
    name: "toolbar_window_tab_mixer_tcp",
    width: 90,
    height: 30,
    offset: 3566472,
    count: 120,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_navigator.png` — 90x30, 393 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_NAVIGATOR: ArtData = ArtData {
    name: "toolbar_window_tab_navigator",
    width: 90,
    height: 30,
    offset: 3567912,
    count: 393,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_new.png` — 90x30, 348 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_NEW: ArtData = ArtData {
    name: "toolbar_window_tab_new",
    width: 90,
    height: 30,
    offset: 3572628,
    count: 348,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_new_background.png` — 90x30, 279 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_NEW_BACKGROUND: ArtData = ArtData {
    name: "toolbar_window_tab_new_background",
    width: 90,
    height: 30,
    offset: 3576804,
    count: 279,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_next_background.png` — 90x30, 81 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_NEXT_BACKGROUND: ArtData = ArtData {
    name: "toolbar_window_tab_next_background",
    width: 90,
    height: 30,
    offset: 3580152,
    count: 81,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_performance.png` — 90x30, 138 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_PERFORMANCE: ArtData = ArtData {
    name: "toolbar_window_tab_performance",
    width: 90,
    height: 30,
    offset: 3581124,
    count: 138,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_properties.png` — 90x30, 291 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_PROPERTIES: ArtData = ArtData {
    name: "toolbar_window_tab_properties",
    width: 90,
    height: 30,
    offset: 3582780,
    count: 291,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_region.png` — 90x30, 81 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_REGION: ArtData = ArtData {
    name: "toolbar_window_tab_region",
    width: 90,
    height: 30,
    offset: 3586272,
    count: 81,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_routing_matrix.png` — 90x30, 120 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_ROUTING_MATRIX: ArtData = ArtData {
    name: "toolbar_window_tab_routing_matrix",
    width: 90,
    height: 30,
    offset: 3587244,
    count: 120,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_screenset_layout.png` — 90x30, 204 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_SCREENSET_LAYOUT: ArtData = ArtData {
    name: "toolbar_window_tab_screenset_layout",
    width: 90,
    height: 30,
    offset: 3588684,
    count: 204,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_undo_history.png` — 90x30, 412 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_UNDO_HISTORY: ArtData = ArtData {
    name: "toolbar_window_tab_undo_history",
    width: 90,
    height: 30,
    offset: 3591132,
    count: 412,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_window_tab_video.png` — 90x30, 219 rects, 3 sprite cell(s).
pub const TOOLBAR_WINDOW_TAB_VIDEO: ArtData = ArtData {
    name: "toolbar_window_tab_video",
    width: 90,
    height: 30,
    offset: 3596076,
    count: 219,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_xfade.png` — 90x30, 210 rects, 3 sprite cell(s).
pub const TOOLBAR_XFADE: ArtData = ArtData {
    name: "toolbar_xfade",
    width: 90,
    height: 30,
    offset: 3598704,
    count: 210,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_xfade_off.png` — 90x30, 507 rects, 1 sprite cell(s).
pub const TOOLBAR_XFADE_OFF: ArtData = ArtData {
    name: "toolbar_xfade_off",
    width: 90,
    height: 30,
    offset: 3601224,
    count: 507,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_xfade_on.png` — 90x30, 507 rects, 1 sprite cell(s).
pub const TOOLBAR_XFADE_ON: ArtData = ArtData {
    name: "toolbar_xfade_on",
    width: 90,
    height: 30,
    offset: 3607308,
    count: 507,
    cells: 1,
    blob: BLOB,
};
/// `toolbar_zoom_all.png` — 90x30, 309 rects, 3 sprite cell(s).
pub const TOOLBAR_ZOOM_ALL: ArtData = ArtData {
    name: "toolbar_zoom_all",
    width: 90,
    height: 30,
    offset: 3613392,
    count: 309,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_zoom_in_audio_waveform.png` — 90x30, 387 rects, 3 sprite cell(s).
pub const TOOLBAR_ZOOM_IN_AUDIO_WAVEFORM: ArtData = ArtData {
    name: "toolbar_zoom_in_audio_waveform",
    width: 90,
    height: 30,
    offset: 3617100,
    count: 387,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_zoom_in_selected_item.png` — 90x30, 201 rects, 3 sprite cell(s).
pub const TOOLBAR_ZOOM_IN_SELECTED_ITEM: ArtData = ArtData {
    name: "toolbar_zoom_in_selected_item",
    width: 90,
    height: 30,
    offset: 3621744,
    count: 201,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_zoom_out_all.png` — 90x30, 447 rects, 3 sprite cell(s).
pub const TOOLBAR_ZOOM_OUT_ALL: ArtData = ArtData {
    name: "toolbar_zoom_out_all",
    width: 90,
    height: 30,
    offset: 3624156,
    count: 447,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_zoom_out_audio_waveform.png` — 90x30, 381 rects, 3 sprite cell(s).
pub const TOOLBAR_ZOOM_OUT_AUDIO_WAVEFORM: ArtData = ArtData {
    name: "toolbar_zoom_out_audio_waveform",
    width: 90,
    height: 30,
    offset: 3629520,
    count: 381,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_zoom_project.png` — 90x30, 219 rects, 3 sprite cell(s).
pub const TOOLBAR_ZOOM_PROJECT: ArtData = ArtData {
    name: "toolbar_zoom_project",
    width: 90,
    height: 30,
    offset: 3634092,
    count: 219,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_zoom_region.png` — 90x30, 279 rects, 3 sprite cell(s).
pub const TOOLBAR_ZOOM_REGION: ArtData = ArtData {
    name: "toolbar_zoom_region",
    width: 90,
    height: 30,
    offset: 3636720,
    count: 279,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_zoom_selected.png` — 90x30, 285 rects, 3 sprite cell(s).
pub const TOOLBAR_ZOOM_SELECTED: ArtData = ArtData {
    name: "toolbar_zoom_selected",
    width: 90,
    height: 30,
    offset: 3643524,
    count: 285,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_zoom_selected item.png` — 90x30, 288 rects, 3 sprite cell(s).
pub const TOOLBAR_ZOOM_SELECTED_ITEM: ArtData = ArtData {
    name: "toolbar_zoom_selected item",
    width: 90,
    height: 30,
    offset: 3640068,
    count: 288,
    cells: 3,
    blob: BLOB,
};
/// `toolbar_zoom_time_selection.png` — 90x30, 231 rects, 3 sprite cell(s).
pub const TOOLBAR_ZOOM_TIME_SELECTION: ArtData = ArtData {
    name: "toolbar_zoom_time_selection",
    width: 90,
    height: 30,
    offset: 3646944,
    count: 231,
    cells: 3,
    blob: BLOB,
};
/// `toosmall_b.png` — 16x16, 124 rects, 4 sprite cell(s).
pub const TOOSMALL_B: ArtData = ArtData {
    name: "toosmall_b",
    width: 16,
    height: 16,
    offset: 3649716,
    count: 124,
    cells: 4,
    blob: BLOB,
};
/// `toosmall_r.png` — 24x14, 23 rects, 1 sprite cell(s).
pub const TOOSMALL_R: ArtData = ArtData {
    name: "toosmall_r",
    width: 24,
    height: 14,
    offset: 3651204,
    count: 23,
    cells: 1,
    blob: BLOB,
};
/// `track_env.png` — 60x20, 276 rects, 3 sprite cell(s).
pub const TRACK_ENV: ArtData = ArtData {
    name: "track_env",
    width: 60,
    height: 20,
    offset: 3651480,
    count: 276,
    cells: 3,
    blob: BLOB,
};
/// `track_env_latch.png` — 60x20, 249 rects, 3 sprite cell(s).
pub const TRACK_ENV_LATCH: ArtData = ArtData {
    name: "track_env_latch",
    width: 60,
    height: 20,
    offset: 3654792,
    count: 249,
    cells: 3,
    blob: BLOB,
};
/// `track_env_preview.png` — 60x20, 288 rects, 3 sprite cell(s).
pub const TRACK_ENV_PREVIEW: ArtData = ArtData {
    name: "track_env_preview",
    width: 60,
    height: 20,
    offset: 3657780,
    count: 288,
    cells: 3,
    blob: BLOB,
};
/// `track_env_read.png` — 60x20, 306 rects, 3 sprite cell(s).
pub const TRACK_ENV_READ: ArtData = ArtData {
    name: "track_env_read",
    width: 60,
    height: 20,
    offset: 3661236,
    count: 306,
    cells: 3,
    blob: BLOB,
};
/// `track_env_touch.png` — 60x20, 270 rects, 3 sprite cell(s).
pub const TRACK_ENV_TOUCH: ArtData = ArtData {
    name: "track_env_touch",
    width: 60,
    height: 20,
    offset: 3664908,
    count: 270,
    cells: 3,
    blob: BLOB,
};
/// `track_env_write.png` — 60x20, 369 rects, 3 sprite cell(s).
pub const TRACK_ENV_WRITE: ArtData = ArtData {
    name: "track_env_write",
    width: 60,
    height: 20,
    offset: 3668148,
    count: 369,
    cells: 3,
    blob: BLOB,
};
/// `track_fcomp_off.png` — 51x13, 201 rects, 1 sprite cell(s).
pub const TRACK_FCOMP_OFF: ArtData = ArtData {
    name: "track_fcomp_off",
    width: 51,
    height: 13,
    offset: 3672576,
    count: 201,
    cells: 1,
    blob: BLOB,
};
/// `track_fcomp_small.png` — 51x13, 187 rects, 3 sprite cell(s).
pub const TRACK_FCOMP_SMALL: ArtData = ArtData {
    name: "track_fcomp_small",
    width: 51,
    height: 13,
    offset: 3674988,
    count: 187,
    cells: 3,
    blob: BLOB,
};
/// `track_fcomp_tiny.png` — 51x13, 203 rects, 3 sprite cell(s).
pub const TRACK_FCOMP_TINY: ArtData = ArtData {
    name: "track_fcomp_tiny",
    width: 51,
    height: 13,
    offset: 3677232,
    count: 203,
    cells: 3,
    blob: BLOB,
};
/// `track_folder_last.png` — 54x14, 92 rects, 1 sprite cell(s).
pub const TRACK_FOLDER_LAST: ArtData = ArtData {
    name: "track_folder_last",
    width: 54,
    height: 14,
    offset: 3679668,
    count: 92,
    cells: 1,
    blob: BLOB,
};
/// `track_folder_off.png` — 54x14, 8 rects, 1 sprite cell(s).
pub const TRACK_FOLDER_OFF: ArtData = ArtData {
    name: "track_folder_off",
    width: 54,
    height: 14,
    offset: 3680772,
    count: 8,
    cells: 1,
    blob: BLOB,
};
/// `track_folder_on.png` — 54x14, 176 rects, 1 sprite cell(s).
pub const TRACK_FOLDER_ON: ArtData = ArtData {
    name: "track_folder_on",
    width: 54,
    height: 14,
    offset: 3680868,
    count: 176,
    cells: 1,
    blob: BLOB,
};
/// `track_fx_dis.png` — 62x22, 411 rects, 3 sprite cell(s).
pub const TRACK_FX_DIS: ArtData = ArtData {
    name: "track_fx_dis",
    width: 62,
    height: 22,
    offset: 3682980,
    count: 411,
    cells: 3,
    blob: BLOB,
};
/// `track_fx_empty.png` — 62x22, 402 rects, 1 sprite cell(s).
pub const TRACK_FX_EMPTY: ArtData = ArtData {
    name: "track_fx_empty",
    width: 62,
    height: 22,
    offset: 3687912,
    count: 402,
    cells: 1,
    blob: BLOB,
};
/// `track_fx_in_empty.png` — 87x20, 226 rects, 3 sprite cell(s).
pub const TRACK_FX_IN_EMPTY: ArtData = ArtData {
    name: "track_fx_in_empty",
    width: 87,
    height: 20,
    offset: 3692736,
    count: 226,
    cells: 3,
    blob: BLOB,
};
/// `track_fx_in_norm.png` — 88x20, 300 rects, 3 sprite cell(s).
pub const TRACK_FX_IN_NORM: ArtData = ArtData {
    name: "track_fx_in_norm",
    width: 88,
    height: 20,
    offset: 3695448,
    count: 300,
    cells: 3,
    blob: BLOB,
};
/// `track_fx_norm.png` — 62x22, 405 rects, 1 sprite cell(s).
pub const TRACK_FX_NORM: ArtData = ArtData {
    name: "track_fx_norm",
    width: 62,
    height: 22,
    offset: 3699048,
    count: 405,
    cells: 1,
    blob: BLOB,
};
/// `track_fxempty_h.png` — 50x22, 251 rects, 3 sprite cell(s).
pub const TRACK_FXEMPTY_H: ArtData = ArtData {
    name: "track_fxempty_h",
    width: 50,
    height: 22,
    offset: 3703908,
    count: 251,
    cells: 3,
    blob: BLOB,
};
/// `track_fxempty_v.png` — 56x22, 225 rects, 3 sprite cell(s).
pub const TRACK_FXEMPTY_V: ArtData = ArtData {
    name: "track_fxempty_v",
    width: 56,
    height: 22,
    offset: 3706920,
    count: 225,
    cells: 3,
    blob: BLOB,
};
/// `track_fxoff_h.png` — 50x22, 261 rects, 3 sprite cell(s).
pub const TRACK_FXOFF_H: ArtData = ArtData {
    name: "track_fxoff_h",
    width: 50,
    height: 22,
    offset: 3709620,
    count: 261,
    cells: 3,
    blob: BLOB,
};
/// `track_fxoff_v.png` — 56x22, 258 rects, 1 sprite cell(s).
pub const TRACK_FXOFF_V: ArtData = ArtData {
    name: "track_fxoff_v",
    width: 56,
    height: 22,
    offset: 3712752,
    count: 258,
    cells: 1,
    blob: BLOB,
};
/// `track_fxon_h.png` — 50x22, 270 rects, 3 sprite cell(s).
pub const TRACK_FXON_H: ArtData = ArtData {
    name: "track_fxon_h",
    width: 50,
    height: 22,
    offset: 3715848,
    count: 270,
    cells: 3,
    blob: BLOB,
};
/// `track_fxon_v.png` — 56x22, 273 rects, 3 sprite cell(s).
pub const TRACK_FXON_V: ArtData = ArtData {
    name: "track_fxon_v",
    width: 56,
    height: 22,
    offset: 3719088,
    count: 273,
    cells: 3,
    blob: BLOB,
};
/// `track_io.png` — 86x22, 581 rects, 3 sprite cell(s).
pub const TRACK_IO: ArtData = ArtData {
    name: "track_io",
    width: 86,
    height: 22,
    offset: 3722364,
    count: 581,
    cells: 3,
    blob: BLOB,
};
/// `track_io_dis.png` — 86x22, 521 rects, 3 sprite cell(s).
pub const TRACK_IO_DIS: ArtData = ArtData {
    name: "track_io_dis",
    width: 86,
    height: 22,
    offset: 3729336,
    count: 521,
    cells: 3,
    blob: BLOB,
};
/// `track_io_r.png` — 86x22, 521 rects, 3 sprite cell(s).
pub const TRACK_IO_R: ArtData = ArtData {
    name: "track_io_r",
    width: 86,
    height: 22,
    offset: 3735588,
    count: 521,
    cells: 3,
    blob: BLOB,
};
/// `track_io_r_dis.png` — 86x22, 521 rects, 3 sprite cell(s).
pub const TRACK_IO_R_DIS: ArtData = ArtData {
    name: "track_io_r_dis",
    width: 86,
    height: 22,
    offset: 3741840,
    count: 521,
    cells: 3,
    blob: BLOB,
};
/// `track_io_s.png` — 86x22, 521 rects, 3 sprite cell(s).
pub const TRACK_IO_S: ArtData = ArtData {
    name: "track_io_s",
    width: 86,
    height: 22,
    offset: 3748092,
    count: 521,
    cells: 3,
    blob: BLOB,
};
/// `track_io_s_dis.png` — 86x22, 521 rects, 3 sprite cell(s).
pub const TRACK_IO_S_DIS: ArtData = ArtData {
    name: "track_io_s_dis",
    width: 86,
    height: 22,
    offset: 3754344,
    count: 521,
    cells: 3,
    blob: BLOB,
};
/// `track_io_s_r.png` — 86x22, 521 rects, 3 sprite cell(s).
pub const TRACK_IO_S_R: ArtData = ArtData {
    name: "track_io_s_r",
    width: 86,
    height: 22,
    offset: 3760596,
    count: 521,
    cells: 3,
    blob: BLOB,
};
/// `track_io_s_r_dis.png` — 86x22, 521 rects, 3 sprite cell(s).
pub const TRACK_IO_S_R_DIS: ArtData = ArtData {
    name: "track_io_s_r_dis",
    width: 86,
    height: 22,
    offset: 3766848,
    count: 521,
    cells: 3,
    blob: BLOB,
};
/// `track_monitor_auto.png` — 47x24, 248 rects, 1 sprite cell(s).
pub const TRACK_MONITOR_AUTO: ArtData = ArtData {
    name: "track_monitor_auto",
    width: 47,
    height: 24,
    offset: 3773100,
    count: 248,
    cells: 1,
    blob: BLOB,
};
/// `track_monitor_off.png` — 47x24, 306 rects, 3 sprite cell(s).
pub const TRACK_MONITOR_OFF: ArtData = ArtData {
    name: "track_monitor_off",
    width: 47,
    height: 24,
    offset: 3776076,
    count: 306,
    cells: 3,
    blob: BLOB,
};
/// `track_monitor_on.png` — 47x24, 225 rects, 1 sprite cell(s).
pub const TRACK_MONITOR_ON: ArtData = ArtData {
    name: "track_monitor_on",
    width: 47,
    height: 24,
    offset: 3779748,
    count: 225,
    cells: 1,
    blob: BLOB,
};
/// `track_mono.png` — 150x20, 1381 rects, 3 sprite cell(s).
pub const TRACK_MONO: ArtData = ArtData {
    name: "track_mono",
    width: 150,
    height: 20,
    offset: 3782448,
    count: 1381,
    cells: 3,
    blob: BLOB,
};
/// `track_mute_off.png` — 65x24, 394 rects, 1 sprite cell(s).
pub const TRACK_MUTE_OFF: ArtData = ArtData {
    name: "track_mute_off",
    width: 65,
    height: 24,
    offset: 3799020,
    count: 394,
    cells: 1,
    blob: BLOB,
};
/// `track_mute_on.png` — 65x24, 688 rects, 1 sprite cell(s).
pub const TRACK_MUTE_ON: ArtData = ArtData {
    name: "track_mute_on",
    width: 65,
    height: 24,
    offset: 3803748,
    count: 688,
    cells: 1,
    blob: BLOB,
};
/// `track_phase_inv.png` — 48x20, 518 rects, 3 sprite cell(s).
pub const TRACK_PHASE_INV: ArtData = ArtData {
    name: "track_phase_inv",
    width: 48,
    height: 20,
    offset: 3812004,
    count: 518,
    cells: 3,
    blob: BLOB,
};
/// `track_phase_norm.png` — 48x20, 495 rects, 3 sprite cell(s).
pub const TRACK_PHASE_NORM: ArtData = ArtData {
    name: "track_phase_norm",
    width: 48,
    height: 20,
    offset: 3818220,
    count: 495,
    cells: 3,
    blob: BLOB,
};
/// `track_recarm_auto.png` — 60x20, 342 rects, 3 sprite cell(s).
pub const TRACK_RECARM_AUTO: ArtData = ArtData {
    name: "track_recarm_auto",
    width: 60,
    height: 20,
    offset: 3824160,
    count: 342,
    cells: 3,
    blob: BLOB,
};
/// `track_recarm_auto_norec.png` — 60x20, 369 rects, 3 sprite cell(s).
pub const TRACK_RECARM_AUTO_NOREC: ArtData = ArtData {
    name: "track_recarm_auto_norec",
    width: 60,
    height: 20,
    offset: 3828264,
    count: 369,
    cells: 3,
    blob: BLOB,
};
/// `track_recarm_auto_on.png` — 60x20, 369 rects, 3 sprite cell(s).
pub const TRACK_RECARM_AUTO_ON: ArtData = ArtData {
    name: "track_recarm_auto_on",
    width: 60,
    height: 20,
    offset: 3832692,
    count: 369,
    cells: 3,
    blob: BLOB,
};
/// `track_recarm_norec.png` — 60x20, 364 rects, 3 sprite cell(s).
pub const TRACK_RECARM_NOREC: ArtData = ArtData {
    name: "track_recarm_norec",
    width: 60,
    height: 20,
    offset: 3837120,
    count: 364,
    cells: 3,
    blob: BLOB,
};
/// `track_recarm_off.png` — 60x20, 264 rects, 3 sprite cell(s).
pub const TRACK_RECARM_OFF: ArtData = ArtData {
    name: "track_recarm_off",
    width: 60,
    height: 20,
    offset: 3841488,
    count: 264,
    cells: 3,
    blob: BLOB,
};
/// `track_recarm_on.png` — 60x20, 318 rects, 3 sprite cell(s).
pub const TRACK_RECARM_ON: ArtData = ArtData {
    name: "track_recarm_on",
    width: 60,
    height: 20,
    offset: 3844656,
    count: 318,
    cells: 3,
    blob: BLOB,
};
/// `track_recmode_in.png` — 60x20, 185 rects, 3 sprite cell(s).
pub const TRACK_RECMODE_IN: ArtData = ArtData {
    name: "track_recmode_in",
    width: 60,
    height: 20,
    offset: 3848472,
    count: 185,
    cells: 3,
    blob: BLOB,
};
/// `track_recmode_off.png` — 60x20, 360 rects, 3 sprite cell(s).
pub const TRACK_RECMODE_OFF: ArtData = ArtData {
    name: "track_recmode_off",
    width: 60,
    height: 20,
    offset: 3850692,
    count: 360,
    cells: 3,
    blob: BLOB,
};
/// `track_recmode_out.png` — 60x20, 205 rects, 3 sprite cell(s).
pub const TRACK_RECMODE_OUT: ArtData = ArtData {
    name: "track_recmode_out",
    width: 60,
    height: 20,
    offset: 3855012,
    count: 205,
    cells: 3,
    blob: BLOB,
};
/// `track_solo_off.png` — 65x24, 454 rects, 1 sprite cell(s).
pub const TRACK_SOLO_OFF: ArtData = ArtData {
    name: "track_solo_off",
    width: 65,
    height: 24,
    offset: 3857472,
    count: 454,
    cells: 1,
    blob: BLOB,
};
/// `track_solo_on.png` — 65x24, 499 rects, 1 sprite cell(s).
pub const TRACK_SOLO_ON: ArtData = ArtData {
    name: "track_solo_on",
    width: 65,
    height: 24,
    offset: 3862920,
    count: 499,
    cells: 1,
    blob: BLOB,
};
/// `track_solodefeat_on.png` — 65x24, 525 rects, 1 sprite cell(s).
pub const TRACK_SOLODEFEAT_ON: ArtData = ArtData {
    name: "track_solodefeat_on",
    width: 65,
    height: 24,
    offset: 3868908,
    count: 525,
    cells: 1,
    blob: BLOB,
};
/// `track_stereo.png` — 150x20, 920 rects, 3 sprite cell(s).
pub const TRACK_STEREO: ArtData = ArtData {
    name: "track_stereo",
    width: 150,
    height: 20,
    offset: 3875208,
    count: 920,
    cells: 3,
    blob: BLOB,
};
/// `transport_bg.png` — 200x67, 66 rects, 1 sprite cell(s).
pub const TRANSPORT_BG: ArtData = ArtData {
    name: "transport_bg",
    width: 200,
    height: 67,
    offset: 3886248,
    count: 66,
    cells: 1,
    blob: BLOB,
};
/// `transport_bpm.png` — 92x26, 62 rects, 2 sprite cell(s).
pub const TRANSPORT_BPM: ArtData = ArtData {
    name: "transport_bpm",
    width: 92,
    height: 26,
    offset: 3887040,
    count: 62,
    cells: 2,
    blob: BLOB,
};
/// `transport_bpm_bg.png` — 6x10, 0 rects, 1 sprite cell(s).
pub const TRANSPORT_BPM_BG: ArtData = ArtData {
    name: "transport_bpm_bg",
    width: 6,
    height: 10,
    offset: 3887784,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `transport_edit_bg.png` — 11x11, 0 rects, 1 sprite cell(s).
pub const TRANSPORT_EDIT_BG: ArtData = ArtData {
    name: "transport_edit_bg",
    width: 11,
    height: 11,
    offset: 3887784,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `transport_end.png` — 108x26, 179 rects, 4 sprite cell(s).
pub const TRANSPORT_END: ArtData = ArtData {
    name: "transport_end",
    width: 108,
    height: 26,
    offset: 3887784,
    count: 179,
    cells: 4,
    blob: BLOB,
};
/// `transport_group_bg.png` — 11x11, 0 rects, 1 sprite cell(s).
pub const TRANSPORT_GROUP_BG: ArtData = ArtData {
    name: "transport_group_bg",
    width: 11,
    height: 11,
    offset: 3889932,
    count: 0,
    cells: 1,
    blob: BLOB,
};
/// `transport_home.png` — 108x26, 345 rects, 3 sprite cell(s).
pub const TRANSPORT_HOME: ArtData = ArtData {
    name: "transport_home",
    width: 108,
    height: 26,
    offset: 3889932,
    count: 345,
    cells: 3,
    blob: BLOB,
};
/// `transport_knob_bg_large.png` — 32x34, 261 rects, 1 sprite cell(s).
pub const TRANSPORT_KNOB_BG_LARGE: ArtData = ArtData {
    name: "transport_knob_bg_large",
    width: 32,
    height: 34,
    offset: 3894072,
    count: 261,
    cells: 1,
    blob: BLOB,
};
/// `transport_next.png` — 108x26, 215 rects, 4 sprite cell(s).
pub const TRANSPORT_NEXT: ArtData = ArtData {
    name: "transport_next",
    width: 108,
    height: 26,
    offset: 3897204,
    count: 215,
    cells: 4,
    blob: BLOB,
};
/// `transport_pause.png` — 108x26, 153 rects, 4 sprite cell(s).
pub const TRANSPORT_PAUSE: ArtData = ArtData {
    name: "transport_pause",
    width: 108,
    height: 26,
    offset: 3899784,
    count: 153,
    cells: 4,
    blob: BLOB,
};
/// `transport_pause_on.png` — 108x26, 1252 rects, 4 sprite cell(s).
pub const TRANSPORT_PAUSE_ON: ArtData = ArtData {
    name: "transport_pause_on",
    width: 108,
    height: 26,
    offset: 3901620,
    count: 1252,
    cells: 4,
    blob: BLOB,
};
/// `transport_play.png` — 108x26, 152 rects, 4 sprite cell(s).
pub const TRANSPORT_PLAY: ArtData = ArtData {
    name: "transport_play",
    width: 108,
    height: 26,
    offset: 3916644,
    count: 152,
    cells: 4,
    blob: BLOB,
};
/// `transport_play_on.png` — 108x26, 1276 rects, 4 sprite cell(s).
pub const TRANSPORT_PLAY_ON: ArtData = ArtData {
    name: "transport_play_on",
    width: 108,
    height: 26,
    offset: 3918468,
    count: 1276,
    cells: 4,
    blob: BLOB,
};
/// `transport_play_sync.png` — 108x26, 245 rects, 4 sprite cell(s).
pub const TRANSPORT_PLAY_SYNC: ArtData = ArtData {
    name: "transport_play_sync",
    width: 108,
    height: 26,
    offset: 3933780,
    count: 245,
    cells: 4,
    blob: BLOB,
};
/// `transport_play_sync_on.png` — 108x26, 1287 rects, 4 sprite cell(s).
pub const TRANSPORT_PLAY_SYNC_ON: ArtData = ArtData {
    name: "transport_play_sync_on",
    width: 108,
    height: 26,
    offset: 3936720,
    count: 1287,
    cells: 4,
    blob: BLOB,
};
/// `transport_playspeedbg.png` — 6x21, 1 rects, 1 sprite cell(s).
pub const TRANSPORT_PLAYSPEEDBG: ArtData = ArtData {
    name: "transport_playspeedbg",
    width: 6,
    height: 21,
    offset: 3952164,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `transport_playspeedthumb.png` — 22x28, 109 rects, 1 sprite cell(s).
pub const TRANSPORT_PLAYSPEEDTHUMB: ArtData = ArtData {
    name: "transport_playspeedthumb",
    width: 22,
    height: 28,
    offset: 3952176,
    count: 109,
    cells: 1,
    blob: BLOB,
};
/// `transport_previous.png` — 108x26, 329 rects, 3 sprite cell(s).
pub const TRANSPORT_PREVIOUS: ArtData = ArtData {
    name: "transport_previous",
    width: 108,
    height: 26,
    offset: 3953484,
    count: 329,
    cells: 3,
    blob: BLOB,
};
/// `transport_record.png` — 108x26, 406 rects, 1 sprite cell(s).
pub const TRANSPORT_RECORD: ArtData = ArtData {
    name: "transport_record",
    width: 108,
    height: 26,
    offset: 3957432,
    count: 406,
    cells: 1,
    blob: BLOB,
};
/// `transport_record_item.png` — 108x26, 544 rects, 3 sprite cell(s).
pub const TRANSPORT_RECORD_ITEM: ArtData = ArtData {
    name: "transport_record_item",
    width: 108,
    height: 26,
    offset: 3962304,
    count: 544,
    cells: 3,
    blob: BLOB,
};
/// `transport_record_item_on.png` — 108x26, 1772 rects, 3 sprite cell(s).
pub const TRANSPORT_RECORD_ITEM_ON: ArtData = ArtData {
    name: "transport_record_item_on",
    width: 108,
    height: 26,
    offset: 3968832,
    count: 1772,
    cells: 3,
    blob: BLOB,
};
/// `transport_record_loop.png` — 108x26, 538 rects, 3 sprite cell(s).
pub const TRANSPORT_RECORD_LOOP: ArtData = ArtData {
    name: "transport_record_loop",
    width: 108,
    height: 26,
    offset: 3990096,
    count: 538,
    cells: 3,
    blob: BLOB,
};
/// `transport_record_loop_on.png` — 108x26, 1630 rects, 3 sprite cell(s).
pub const TRANSPORT_RECORD_LOOP_ON: ArtData = ArtData {
    name: "transport_record_loop_on",
    width: 108,
    height: 26,
    offset: 3996552,
    count: 1630,
    cells: 3,
    blob: BLOB,
};
/// `transport_record_on.png` — 108x26, 1611 rects, 3 sprite cell(s).
pub const TRANSPORT_RECORD_ON: ArtData = ArtData {
    name: "transport_record_on",
    width: 108,
    height: 26,
    offset: 4016112,
    count: 1611,
    cells: 3,
    blob: BLOB,
};
/// `transport_repeat_off.png` — 96x24, 457 rects, 3 sprite cell(s).
pub const TRANSPORT_REPEAT_OFF: ArtData = ArtData {
    name: "transport_repeat_off",
    width: 96,
    height: 24,
    offset: 4035444,
    count: 457,
    cells: 3,
    blob: BLOB,
};
/// `transport_repeat_on.png` — 96x24, 459 rects, 3 sprite cell(s).
pub const TRANSPORT_REPEAT_ON: ArtData = ArtData {
    name: "transport_repeat_on",
    width: 96,
    height: 24,
    offset: 4040928,
    count: 459,
    cells: 3,
    blob: BLOB,
};
/// `transport_status_bg.png` — 32x28, 68 rects, 1 sprite cell(s).
pub const TRANSPORT_STATUS_BG: ArtData = ArtData {
    name: "transport_status_bg",
    width: 32,
    height: 28,
    offset: 4046436,
    count: 68,
    cells: 1,
    blob: BLOB,
};
/// `transport_status_bg_err.png` — 6x10, 1 rects, 1 sprite cell(s).
pub const TRANSPORT_STATUS_BG_ERR: ArtData = ArtData {
    name: "transport_status_bg_err",
    width: 6,
    height: 10,
    offset: 4047252,
    count: 1,
    cells: 1,
    blob: BLOB,
};
/// `transport_stop.png` — 108x26, 59 rects, 4 sprite cell(s).
pub const TRANSPORT_STOP: ArtData = ArtData {
    name: "transport_stop",
    width: 108,
    height: 26,
    offset: 4047264,
    count: 59,
    cells: 4,
    blob: BLOB,
};
/// `transport_timebase_beat.png` — 99x22, 263 rects, 1 sprite cell(s).
pub const TRANSPORT_TIMEBASE_BEAT: ArtData = ArtData {
    name: "transport_timebase_beat",
    width: 99,
    height: 22,
    offset: 4047972,
    count: 263,
    cells: 1,
    blob: BLOB,
};
/// `transport_timebase_time.png` — 99x22, 303 rects, 1 sprite cell(s).
pub const TRANSPORT_TIMEBASE_TIME: ArtData = ArtData {
    name: "transport_timebase_time",
    width: 99,
    height: 22,
    offset: 4051128,
    count: 303,
    cells: 1,
    blob: BLOB,
};

/// Every traced image, by REAPER file name.
pub static ALL: &[ArtData] = &[
    ANIMATION_TOOLBAR_ARMED,
    ANIMATION_TOOLBAR_HIGHLIGHT,
    CUSTOM_COMPING,
    CUSTOM_ENVCP_ARM_BG,
    CUSTOM_FIXED_LANES_OFF,
    CUSTOM_FIXED_LANES_ON,
    CUSTOM_MASTER_TRACK_PIN_OFF,
    CUSTOM_MASTER_TRACK_PIN_ON,
    CUSTOM_MCP_FOLDER_1_1,
    CUSTOM_MCP_FOLDER_1_2,
    CUSTOM_MCP_FOLDER_1_4,
    CUSTOM_MCP_FOLDER_1_8,
    CUSTOM_MCP_FOLDER_START,
    CUSTOM_MCP_SEL_GRADIENT,
    CUSTOM_TCP_NAMEBG,
    CUSTOM_TRACK_DIVIDER,
    CUSTOM_TRACK_FOLDER_1_1,
    CUSTOM_TRACK_FOLDER_1_2,
    CUSTOM_TRACK_FOLDER_1_4,
    CUSTOM_TRACK_FOLDER_1_8,
    CUSTOM_TRACK_FOLDER_HALF_1_1,
    CUSTOM_TRACK_FOLDER_HALF_1_2,
    CUSTOM_TRACK_FOLDER_HALF_1_4,
    CUSTOM_TRACK_FOLDER_HALF_1_8,
    CUSTOM_TRACK_FOLDER_RECARM,
    CUSTOM_TRACK_IO_DARKER,
    CUSTOM_TRACK_IO_TEXT_OFF,
    CUSTOM_TRACK_IO_TEXT_ON,
    CUSTOM_TRACK_PIN_OFF,
    CUSTOM_TRACK_PIN_ON,
    CUSTOM_TRACK_RECARM_BG,
    CUSTOM_TRANSPORT_EDIT_BG,
    CUSTOM_TRANSPORT_EDIT_DIV,
    CUSTOM_TRANSPORT_SEL_END,
    CUSTOM_TRANSPORT_SEL_START,
    ENVCP_ARM_OFF,
    ENVCP_ARM_ON,
    ENVCP_BG,
    ENVCP_BGSEL,
    ENVCP_BYPASS_OFF,
    ENVCP_BYPASS_ON,
    ENVCP_FADER,
    ENVCP_FADERBG,
    ENVCP_HIDE,
    ENVCP_KNOB_SMALL,
    ENVCP_LEARN,
    ENVCP_LEARN_ON,
    ENVCP_NAMEBG,
    ENVCP_PARAMMOD,
    ENVCP_PARAMMOD_ON,
    FIXED_LANES_BIG,
    FIXED_LANES_HIDDEN,
    FIXED_LANES_ONE,
    FIXED_LANES_SMALL,
    FOLDER_END,
    FOLDER_INDENT,
    FOLDER_START,
    GEN_BACK,
    GEN_BACK_ON,
    GEN_END,
    GEN_ENV,
    GEN_ENV_LATCH,
    GEN_ENV_PREVIEW,
    GEN_ENV_READ,
    GEN_ENV_TOUCH,
    GEN_ENV_WRITE,
    GEN_FORWARD,
    GEN_FORWARD_ON,
    GEN_HOME,
    GEN_IO,
    GEN_KNOB_BG_SMALL,
    GEN_MIDI_OFF,
    GEN_MIDI_ON,
    GEN_MONO,
    GEN_MUTE_OFF,
    GEN_MUTE_ON,
    GEN_PANBG_HORZ,
    GEN_PANBG_HORZ_DARK,
    GEN_PANTHUMB_HORZ,
    GEN_PAUSE,
    GEN_PAUSE_ON,
    GEN_PHASE_INV,
    GEN_PHASE_NORM,
    GEN_PLAY,
    GEN_PLAY_ON,
    GEN_REFRESH,
    GEN_REPEAT_OFF,
    GEN_REPEAT_ON,
    GEN_SOLO_OFF,
    GEN_SOLO_ON,
    GEN_STEREO,
    GEN_STOP,
    GEN_UP,
    GEN_VOLBG_HORZ,
    GEN_VOLBG_HORZ_DARK,
    GEN_VOLBG_VERT,
    GEN_VOLBG_VERT_DARK,
    GEN_VOLTHUMB_HORZ,
    GEN_VOLTHUMB_VERT,
    GLOBAL_BYPASS,
    GLOBAL_LATCH,
    GLOBAL_OFF,
    GLOBAL_PREVIEW,
    GLOBAL_READ,
    GLOBAL_TOUCH,
    GLOBAL_TRIM,
    GLOBAL_WRITE,
    ITEM_BG,
    ITEM_BG_SEL,
    ITEM_ENV_OFF,
    ITEM_ENV_OFF_HIDPI,
    ITEM_ENV_ON,
    ITEM_ENV_ON_HIDPI,
    ITEM_FX_OFF,
    ITEM_FX_OFF_HIDPI,
    ITEM_FX_ON,
    ITEM_FX_ON_HIDPI,
    ITEM_GROUP,
    ITEM_GROUP_HIDPI,
    ITEM_GROUP_SEL,
    ITEM_GROUP_SEL_HIDPI,
    ITEM_LOCK_OFF,
    ITEM_LOCK_OFF_HIDPI,
    ITEM_LOCK_ON,
    ITEM_LOCK_ON_HIDPI,
    ITEM_MUTE_OFF,
    ITEM_MUTE_OFF_HIDPI,
    ITEM_MUTE_ON,
    ITEM_MUTE_ON_HIDPI,
    ITEM_NOTE_OFF,
    ITEM_NOTE_OFF_HIDPI,
    ITEM_NOTE_ON,
    ITEM_NOTE_ON_HIDPI,
    ITEM_POOLED,
    ITEM_POOLED_HIDPI,
    ITEM_POOLED_ON,
    ITEM_POOLED_ON_HIDPI,
    ITEM_PROPS,
    ITEM_PROPS_HIDPI,
    ITEM_PROPS_ON,
    ITEM_PROPS_ON_HIDPI,
    ITEM_RANK,
    ITEM_RANK_DOWN,
    ITEM_RANK_DOWN_HIDPI,
    ITEM_RANK_HIDPI,
    ITEM_RANK_UP,
    ITEM_RANK_UP_HIDPI,
    ITEM_TIMEBASE_BEAT,
    ITEM_TIMEBASE_BEAT_HIDPI,
    ITEM_TIMEBASE_BEAT_ON,
    ITEM_TIMEBASE_BEAT_ON_HIDPI,
    ITEM_TIMEBASE_TIME,
    ITEM_TIMEBASE_TIME_HIDPI,
    ITEM_TIMEBASE_TIME_ON,
    ITEM_TIMEBASE_TIME_ON_HIDPI,
    ITEM_VOLKNOB,
    ITEM_VOLKNOB_HIDPI,
    LANE_SOLO_DOWN,
    LANE_SOLO_OFF,
    LANE_SOLO_OFF_INDICATOR,
    LANE_SOLO_ON,
    LANE_SOLO_ON_INDICATOR,
    LANE_SOLO_UP,
    MCP_BG,
    MCP_BGSEL,
    MCP_ENV,
    MCP_ENV_LATCH,
    MCP_ENV_PREVIEW,
    MCP_ENV_READ,
    MCP_ENV_TOUCH,
    MCP_ENV_WRITE,
    MCP_EXTMIXBG,
    MCP_EXTMIXBGSEL,
    MCP_FCOMP_OFF,
    MCP_FCOMP_TINY,
    MCP_FOLDER_LAST,
    MCP_FOLDER_ON,
    MCP_FX_DIS,
    MCP_FX_EMPTY,
    MCP_FX_IN_EMPTY,
    MCP_FX_IN_EMPTY_OL_2,
    MCP_FX_IN_NORM,
    MCP_FX_IN_NORM_OL_2,
    MCP_FX_NORM,
    MCP_FXLIST_BG,
    MCP_FXLIST_BYP,
    MCP_FXLIST_EMPTY,
    MCP_FXLIST_NORM,
    MCP_FXLIST_OFF,
    MCP_FXPARM_BG,
    MCP_FXPARM_BYP,
    MCP_FXPARM_EMPTY,
    MCP_FXPARM_KNOB_STACK,
    MCP_FXPARM_NORM,
    MCP_FXPARM_OFF,
    MCP_ICONBG,
    MCP_ICONBGSEL,
    MCP_IDXBG,
    MCP_IDXBG_SEL,
    MCP_IO,
    MCP_IO_DIS,
    MCP_IO_R,
    MCP_IO_R_DIS,
    MCP_IO_S,
    MCP_IO_S_DIS,
    MCP_IO_S_R,
    MCP_IO_S_R_DIS,
    MCP_MAIN_NAMEBG,
    MCP_MAIN_NAMEBG_SEL,
    MCP_MAINBG,
    MCP_MAINBGSEL,
    MCP_MAINEXTMIXBG,
    MCP_MAINEXTMIXBGSEL,
    MCP_MASTER_VOL_LABEL,
    MCP_MASTER_VOLBG,
    MCP_MASTER_VOLTHUMB,
    MCP_MONITOR_AUTO,
    MCP_MONITOR_OFF,
    MCP_MONITOR_ON,
    MCP_MONO,
    MCP_MUTE_OFF,
    MCP_MUTE_ON,
    MCP_NAMEBG,
    MCP_PAN_KNOB_LARGE,
    MCP_PAN_KNOB_SMALL,
    MCP_PAN_LABEL,
    MCP_PANBG,
    MCP_PANTHUMB,
    MCP_PHASE_INV,
    MCP_PHASE_NORM,
    MCP_RECARM_AUTO,
    MCP_RECARM_AUTO_NOREC,
    MCP_RECARM_AUTO_ON,
    MCP_RECARM_NOREC,
    MCP_RECARM_OFF,
    MCP_RECARM_ON,
    MCP_RECINPUT,
    MCP_RECMODE_IN,
    MCP_RECMODE_OFF,
    MCP_RECMODE_OUT,
    MCP_SEND_KNOB_STACK,
    MCP_SENDLIST_BG,
    MCP_SENDLIST_EMPTY,
    MCP_SENDLIST_METER,
    MCP_SENDLIST_MIDIHW,
    MCP_SENDLIST_MUTE,
    MCP_SENDLIST_NORM,
    MCP_SOLO_OFF,
    MCP_SOLO_ON,
    MCP_SOLODEFEAT_ON,
    MCP_STEREO,
    MCP_VOLBG,
    MCP_VOLBG_HORZ,
    MCP_VOLTHUMB,
    MCP_WID_LABEL,
    MCP_WIDTH_KNOB_LARGE,
    MCP_WIDTH_KNOB_SMALL,
    MCP_WIDTHBG,
    MCP_WIDTHTHUMB,
    METER_AUTOMUTE,
    METER_BG_H,
    METER_BG_MCP,
    METER_BG_V,
    METER_CLIP_H,
    METER_CLIP_V,
    METER_CLIP_V_RMS2,
    METER_FOLDERMUTE,
    METER_MUTE,
    METER_SOLODIM,
    METER_STRIP_H,
    METER_STRIP_H_RMS,
    METER_STRIP_V,
    METER_STRIP_V_RMS,
    METER_UNSOLO,
    MIDI_INLINE_CCWITHITEMS_OFF,
    MIDI_INLINE_CCWITHITEMS_ON,
    MIDI_INLINE_CLOSE,
    MIDI_INLINE_FOLD_CUSTOM_VIEW,
    MIDI_INLINE_FOLD_NONE,
    MIDI_INLINE_FOLD_UNNAMED,
    MIDI_INLINE_FOLD_UNUSED_UNNAMED,
    MIDI_INLINE_NOTEVIEW_DIAMOND,
    MIDI_INLINE_NOTEVIEW_RECT,
    MIDI_INLINE_NOTEVIEW_TRIANGLE,
    MIDI_INLINE_SCROLL,
    MIDI_INLINE_SCROLLBAR,
    MIDI_INLINE_SCROLLTHUMB,
    MIDI_ITEM_BOUNDS,
    MIDI_NOTE_COLORMAP,
    MIDI_SCORE_COLORMAP,
    MIXER_MENU,
    MONITOR_FX_BYP,
    MONITOR_FX_BYP_BYP,
    MONITOR_FX_BYP_OFF,
    MONITOR_FX_BYP_ON,
    MONITOR_FX_OFF,
    MONITOR_FX_ON,
    PIANO_BLACK_KEY,
    PIANO_BLACK_KEY_SEL,
    PIANO_WHITE_KEY,
    PIANO_WHITE_KEY_SEL,
    SCROLLBAR,
    TAB_DOWN,
    TAB_DOWN_SEL,
    TAB_UP,
    TAB_UP_SEL,
    TABLE_EXPAND_OFF,
    TABLE_EXPAND_ON,
    TABLE_LOCKED_OFF,
    TABLE_LOCKED_ON,
    TABLE_LOCKED_PARTIAL,
    TABLE_MUTE_OFF,
    TABLE_MUTE_ON,
    TABLE_RECARM_OFF,
    TABLE_RECARM_ON,
    TABLE_REMOVE_OFF,
    TABLE_REMOVE_ON,
    TABLE_SOLO_OFF,
    TABLE_SOLO_ON,
    TABLE_SUB_EXPAND_OFF,
    TABLE_SUB_EXPAND_ON,
    TABLE_TARGET_INVALID,
    TABLE_TARGET_OFF,
    TABLE_TARGET_ON,
    TABLE_VISIBLE_OFF,
    TABLE_VISIBLE_ON,
    TABLE_VISIBLE_PARTIAL,
    TCP_FXPARM_BG,
    TCP_FXPARM_BYP,
    TCP_FXPARM_EMPTY,
    TCP_FXPARM_FX_BYP,
    TCP_FXPARM_FX_NORM,
    TCP_FXPARM_FX_OFF,
    TCP_FXPARM_KNOB_STACK,
    TCP_FXPARM_NORM,
    TCP_FXPARM_OFF,
    TCP_ICONBG,
    TCP_ICONBGSEL,
    TCP_IDXBG,
    TCP_IDXBG_SEL,
    TCP_MAIN_NAMEBG_SEL,
    TCP_MAINBG,
    TCP_MAINBGSEL,
    TCP_NAMEBG,
    TCP_PAN_KNOB_SMALL,
    TCP_PAN_KNOB_STACK,
    TCP_PANBG,
    TCP_PANTHUMB,
    TCP_RECINPUT,
    TCP_SEND_KNOB_STACK,
    TCP_SENDLIST_BG,
    TCP_SENDLIST_EMPTY,
    TCP_SENDLIST_METER,
    TCP_SENDLIST_MIDIHW,
    TCP_SENDLIST_MUTE,
    TCP_SENDLIST_NORM,
    TCP_SOLODEFEAT_ON,
    TCP_VOL_KNOB_SMALL,
    TCP_VOL_KNOB_STACK,
    TCP_VOLBG,
    TCP_VOLTHUMB,
    TCP_WID_KNOB_STACK,
    TCP_WIDTH_KNOB_SMALL,
    TCP_WIDTHBG,
    TCP_WIDTHTHUMB,
    TOOLBAR_ADD,
    TOOLBAR_AUDIO_WAVEFORM,
    TOOLBAR_AUDIO_WAVEFORM_DELETE_REMOVE,
    TOOLBAR_AUDIO_WAVEFORM_DELETE_SILENCE,
    TOOLBAR_AUDIO_WAVEFORM_DIGITAL_SAMPLE_RATE,
    TOOLBAR_AUDIO_WAVEFORM_DISK_LOAD,
    TOOLBAR_AUDIO_WAVEFORM_FOLDER,
    TOOLBAR_AUDIO_WAVEFORM_METRONOME,
    TOOLBAR_AUDIO_WAVEFORM_MOVE_GRID_QUANTIZE,
    TOOLBAR_AUDIO_WAVEFORM_NORMALIZE_GAIN,
    TOOLBAR_AUDIO_WAVEFORM_NORMALIZE_GAIN_COMMON_LOCKED,
    TOOLBAR_AUDIO_WAVEFORM_PRIMARY_EXTERNAL_EDITOR,
    TOOLBAR_AUDIO_WAVEFORM_PROPERTIES,
    TOOLBAR_AUDIO_WAVEFORM_RENDER_DISK_MONO,
    TOOLBAR_AUDIO_WAVEFORM_RENDER_DISK_STEREO,
    TOOLBAR_AUDIO_WAVEFORM_RENDER_EFFECTS_MONO,
    TOOLBAR_AUDIO_WAVEFORM_RENDER_EFFECTS_STEREO,
    TOOLBAR_AUDIO_WAVEFORM_REVERSE,
    TOOLBAR_AUDIO_WAVEFORM_SECONDARY_EXTERNAL_EDITOR,
    TOOLBAR_AUDIO_WAVEFORM_SELECTION,
    TOOLBAR_AUDIO_WAVEFORM_SELECTION_TRIM,
    TOOLBAR_AUDIO_WAVEFORM_SYSTEM,
    TOOLBAR_AUDIO_WAVEFORM_TIME_SELECTION_RENDER,
    TOOLBAR_AUDIO_WAVEFORM_TIME_SELECTION_RENDER_STEREO,
    TOOLBAR_AUDIO_WAVEFORM_TRANSIENT_DYNAMIC_SPLIT,
    TOOLBAR_AUDIO_WAVEFORM_TRANSIENT_DYNAMIC_SPLIT_LINES,
    TOOLBAR_AUDIO_WAVEFORM_TRANSIENT_DYNAMIC_SPLIT_SCISSORS,
    TOOLBAR_BASS_CLEF_NOTE,
    TOOLBAR_BLANK,
    TOOLBAR_BLANK_INVERTED,
    TOOLBAR_CLIP_PROPERTIES,
    TOOLBAR_CLIPBOARD_COPY,
    TOOLBAR_CLIPBOARD_CUT,
    TOOLBAR_CLIPBOARD_PASTE,
    TOOLBAR_COLOR_DYNAMIC_VOLUME_FF,
    TOOLBAR_COLOR_ITEM,
    TOOLBAR_COLOR_ITEM_SELECTED,
    TOOLBAR_COLOR_LOAD_DISK,
    TOOLBAR_COLOR_MIDI_CHANNEL,
    TOOLBAR_COLOR_NONE_DELETE_REMOVE,
    TOOLBAR_COLOR_NOTE_PITCH,
    TOOLBAR_COLOR_PROPERTIES,
    TOOLBAR_COLOR_RANDOM_QUESTION,
    TOOLBAR_COLOR_REGION,
    TOOLBAR_COLOR_SELECTE_DELETE_REMOVE,
    TOOLBAR_COLOR_SELECTED,
    TOOLBAR_COLOR_SOURCE_INPUT_CHANNEL,
    TOOLBAR_COLOR_SWS_EXTENSION,
    TOOLBAR_COLOR_TAKE_LANE,
    TOOLBAR_COLOR_TRACK,
    TOOLBAR_CPU_OFFLINE,
    TOOLBAR_CPU_ONLINE,
    TOOLBAR_CPU_PROPERTIES_PERFORMANCE,
    TOOLBAR_DELETE,
    TOOLBAR_DISK_PROPERTIES_RESOURCE_PATH,
    TOOLBAR_DOCK,
    TOOLBAR_DOCK_OFF,
    TOOLBAR_DOCK_ON,
    TOOLBAR_DOTTED_NOTE,
    TOOLBAR_EIGHTH_QUAVER_GRID,
    TOOLBAR_EIGHTH_QUAVER_NOTE,
    TOOLBAR_ENV_AUTO_LATCH,
    TOOLBAR_ENV_AUTO_PREVIEW,
    TOOLBAR_ENV_AUTO_READ,
    TOOLBAR_ENV_AUTO_TOUCH,
    TOOLBAR_ENV_AUTO_TRIM,
    TOOLBAR_ENV_AUTO_WRITE,
    TOOLBAR_ENVELOPE_COPY,
    TOOLBAR_ENVELOPE_DELETE_REMOVE,
    TOOLBAR_ENVELOPE_FADE_SHAPE_CYCLE,
    TOOLBAR_ENVELOPE_FADE_SHAPE_NONE_DEFAULT_DELETE_REMOVE,
    TOOLBAR_ENVELOPE_INSERT_FOUR,
    TOOLBAR_ENVELOPE_ITEM_SELECTED,
    TOOLBAR_ENVELOPE_ITEM_SELECTED_REPLACE,
    TOOLBAR_ENVELOPE_KNOB_PARAMETER_VOLUME,
    TOOLBAR_ENVELOPE_LOCK,
    TOOLBAR_ENVELOPE_MUTE,
    TOOLBAR_ENVELOPE_NEW,
    TOOLBAR_ENVELOPE_PAN,
    TOOLBAR_ENVELOPE_PITCH_NOTE,
    TOOLBAR_ENVELOPE_POINT_DELETE_REMOVE,
    TOOLBAR_ENVELOPE_POINT_INSERT,
    TOOLBAR_ENVELOPE_POINT_MOVE_AXIS,
    TOOLBAR_ENVELOPE_POINT_MOVE_DOWN,
    TOOLBAR_ENVELOPE_POINT_MOVE_LEFT,
    TOOLBAR_ENVELOPE_POINT_MOVE_RIGHT,
    TOOLBAR_ENVELOPE_POINT_MOVE_UP,
    TOOLBAR_ENVELOPE_POINT_NEW,
    TOOLBAR_ENVELOPE_POINT_TIME_SELECTION_CUT_SCISSORS,
    TOOLBAR_ENVELOPE_POINT_TIME_SELECTION_DELETE_REMOVE,
    TOOLBAR_ENVELOPE_REDUCE_NUMBER_POINTS_DELETE_REMOVE,
    TOOLBAR_ENVELOPE_SHOW,
    TOOLBAR_ENVELOPE_TEMPO_TIME_CLOCK,
    TOOLBAR_ENVELOPE_TIME_SELECTION,
    TOOLBAR_ENVELOPE_VOL,
    TOOLBAR_ENVITEM,
    TOOLBAR_ENVITEM_OFF,
    TOOLBAR_ENVITEM_ON,
    TOOLBAR_EX_AUTOPLAY,
    TOOLBAR_EX_AUTOPLAY_OFF,
    TOOLBAR_EX_AUTOPLAY_ON,
    TOOLBAR_EX_INSERT_OPEN,
    TOOLBAR_EX_PITCH_DETECT,
    TOOLBAR_EX_PITCH_DETECT_OFF,
    TOOLBAR_EX_PITCH_DETECT_ON,
    TOOLBAR_EX_PRESERVE_PITCH_TEMPO_MATCHING,
    TOOLBAR_EX_PRESERVE_PITCH_TEMPO_MATCHING_OFF,
    TOOLBAR_EX_PRESERVE_PITCH_TEMPO_MATCHING_ON,
    TOOLBAR_EX_PROPERTIES_FOR_CURRENT_MEDIA,
    TOOLBAR_EX_PROPERTIES_FOR_CURRENT_MEDIA_OFF,
    TOOLBAR_EX_PROPERTIES_FOR_CURRENT_MEDIA_ON,
    TOOLBAR_EX_START_ON_BAR,
    TOOLBAR_EX_START_ON_BAR_OFF,
    TOOLBAR_EX_START_ON_BAR_ON,
    TOOLBAR_EX_TEMPO_MATCH,
    TOOLBAR_EX_TEMPO_MATCH_DOUBLE,
    TOOLBAR_EX_TEMPO_MATCH_DOUBLE_OFF,
    TOOLBAR_EX_TEMPO_MATCH_DOUBLE_ON,
    TOOLBAR_EX_TEMPO_MATCH_HALF,
    TOOLBAR_EX_TEMPO_MATCH_HALF_OFF,
    TOOLBAR_EX_TEMPO_MATCH_HALF_ON,
    TOOLBAR_EX_TEMPO_MATCH_OFF,
    TOOLBAR_EX_TEMPO_MATCH_ON,
    TOOLBAR_FILTER,
    TOOLBAR_FILTER_OFF,
    TOOLBAR_FILTER_ON,
    TOOLBAR_FOLDER_ADD_IMPLODE,
    TOOLBAR_FOLDER_ADD_NEW,
    TOOLBAR_FOLDER_COMBINE,
    TOOLBAR_FOLDER_DELETE_REMOVE,
    TOOLBAR_FOLDER_HIDE,
    TOOLBAR_FOLDER_ITEM_DELETE_REMOVE,
    TOOLBAR_FOLDER_SAVE_DISK,
    TOOLBAR_FOLDER_SEPERATE_EXPLODE,
    TOOLBAR_FOLDER_SHOW_VISIBLE,
    TOOLBAR_FREEZE_RENDER_APPLY_SNOWFLAKE,
    TOOLBAR_GLUE,
    TOOLBAR_GLUE_TIME_SELECTION,
    TOOLBAR_GRID,
    TOOLBAR_GRID_ADJUST_DECREASE,
    TOOLBAR_GRID_ADJUST_INCREASE,
    TOOLBAR_GRID_OFF,
    TOOLBAR_GRID_ON,
    TOOLBAR_GROUP,
    TOOLBAR_GROUP_ADD_ITEM,
    TOOLBAR_GROUP_ADD_ITEM_SELECTED,
    TOOLBAR_GROUP_EXPLODE,
    TOOLBAR_GROUP_OFF,
    TOOLBAR_GROUP_ON,
    TOOLBAR_GROUP_RECORD,
    TOOLBAR_GROUP_UNGROUP_REMOVE_ITEM,
    TOOLBAR_GROUP_UNGROUP_REMOVE_ITEM_SELECTED,
    TOOLBAR_HALF_MINIM_GRID,
    TOOLBAR_HALF_MINIM_NOTE,
    TOOLBAR_HIDE_MIXER,
    TOOLBAR_HIDE_SELECTED,
    TOOLBAR_HIDE_TCP,
    TOOLBAR_INPUT_FX_EFFECT,
    TOOLBAR_ITEM_ARPEGGIATE,
    TOOLBAR_ITEM_DUPLICATE_COPY,
    TOOLBAR_ITEM_EFFECTS_FX_DELETE_REMOVE,
    TOOLBAR_ITEM_EFFECTS_FX_SHOW,
    TOOLBAR_ITEM_EXPLODE_LANE_TAKE,
    TOOLBAR_ITEM_FREE_POSITIONING,
    TOOLBAR_ITEM_GREEN_ARROW_SELECTED,
    TOOLBAR_ITEM_GREEN_ARROW_SELECTED_REPLACE,
    TOOLBAR_ITEM_IMPLODE_LANE_TAKE,
    TOOLBAR_ITEM_INSERT_MOVE_SPACE,
    TOOLBAR_ITEM_LEFT_EDGE_GROW,
    TOOLBAR_ITEM_LEFT_EDGE_POSITION,
    TOOLBAR_ITEM_LEFT_EDGE_SHRINK,
    TOOLBAR_ITEM_NEXT,
    TOOLBAR_ITEM_PREVIOUS,
    TOOLBAR_ITEM_PROPERTIES,
    TOOLBAR_ITEM_RED_ARROW_SELECTED,
    TOOLBAR_ITEM_REMOVE_OVERLAP,
    TOOLBAR_ITEM_RIGHT_EDGE_GROW,
    TOOLBAR_ITEM_RIGHT_EDGE_POSITION,
    TOOLBAR_ITEM_RIGHT_EDGE_SHRINK,
    TOOLBAR_ITEM_SELECT,
    TOOLBAR_ITEM_SELECT_ALL,
    TOOLBAR_ITEM_SELECT_INVERSE,
    TOOLBAR_ITEM_SELECTED_ABOVE_MOVE,
    TOOLBAR_ITEM_SELECTED_ALIGN,
    TOOLBAR_ITEM_SELECTED_AREA_SELECT_MOVE,
    TOOLBAR_ITEM_SELECTED_CUT_SCISSORS,
    TOOLBAR_ITEM_SELECTED_GROW_GRID,
    TOOLBAR_ITEM_SELECTED_MOVE,
    TOOLBAR_ITEM_SELECTED_MOVE_END,
    TOOLBAR_ITEM_SELECTED_MOVE_HORIZONTAL_POSITION_TIME,
    TOOLBAR_ITEM_SELECTED_MOVE_NUDGE_LEFT,
    TOOLBAR_ITEM_SELECTED_MOVE_NUDGE_LEFT_MORE,
    TOOLBAR_ITEM_SELECTED_MOVE_NUDGE_RIGHT,
    TOOLBAR_ITEM_SELECTED_MOVE_NUDGE_RIGHT_MORE,
    TOOLBAR_ITEM_SELECTED_MOVE_VERTICAL_TRACK,
    TOOLBAR_ITEM_SELECTED_SNAP,
    TOOLBAR_ITEM_SELECTED_SWAP,
    TOOLBAR_ITEM_SELECTED_TAKE_DELETE,
    TOOLBAR_ITEM_SELECTED_TAKE_DELETE_INVERT,
    TOOLBAR_ITEM_SELECTED_TAKE_EXTRACT,
    TOOLBAR_ITEM_SELECTED_TAKE_INSERT,
    TOOLBAR_ITEM_SELECTED_TAKE_MOVE_DOWN,
    TOOLBAR_ITEM_SELECTED_TAKE_MOVE_TOP,
    TOOLBAR_ITEM_SELECTED_TAKE_MOVE_UP,
    TOOLBAR_ITEM_SELECTION_REMOVE_CONTENTS_MOVE_LATER,
    TOOLBAR_ITEM_SOURCE_PREFERRED_POSITION_PROPERTIES,
    TOOLBAR_ITEM_TAKE,
    TOOLBAR_ITEM_TAKE_EXPLODE,
    TOOLBAR_ITEM_TAKE_SELECTED_EXTRACT,
    TOOLBAR_ITEM_TAKE_SELECTED_LOCK,
    TOOLBAR_JOG_BACK_REWIND_LITTLE_BIT,
    TOOLBAR_JOG_FORWARD_LITTLE_BIT,
    TOOLBAR_KNOB_PARAMETER_LEARN_LOCK,
    TOOLBAR_KNOB_PARAMETER_VISIBLE_SHOW,
    TOOLBAR_LOAD,
    TOOLBAR_LOCK,
    TOOLBAR_LOCK_OFF,
    TOOLBAR_LOCK_ON,
    TOOLBAR_MARKER_DELETE_REMOVE,
    TOOLBAR_MARKER_INSERT_NEW,
    TOOLBAR_MARKER_LIST,
    TOOLBAR_MARKER_LOAD_DISK,
    TOOLBAR_MARKER_LOCK,
    TOOLBAR_MARKER_NEXT,
    TOOLBAR_MARKER_PREVIOUS,
    TOOLBAR_MARKER_PROPERTIES,
    TOOLBAR_MARKER_RENUM,
    TOOLBAR_MARKER_TIME_SELECTION_DELETE_REMOE,
    TOOLBAR_MARKER_TIME_TEMPO_DELETE_REMOVE,
    TOOLBAR_MARKER_TIME_TEMPO_INSERT_NEW,
    TOOLBAR_MARKER_TIME_TEMPO_NEXT,
    TOOLBAR_MARKER_TIME_TEMPO_PREVIOUS,
    TOOLBAR_MARKER_TIME_TEMPO_PROPERTIES,
    TOOLBAR_MARQUEE_CURSOR_SELECTION,
    TOOLBAR_MARQUEE_CURSOR_SELECTION_OFF,
    TOOLBAR_MARQUEE_CURSOR_SELECTION_ON,
    TOOLBAR_METRO,
    TOOLBAR_METRO_OFF,
    TOOLBAR_METRO_ON,
    TOOLBAR_MIDI_ALL,
    TOOLBAR_MIDI_CC_ABOVE,
    TOOLBAR_MIDI_CC_BELOW,
    TOOLBAR_MIDI_CC_EXPLODE,
    TOOLBAR_MIDI_CC_PITCH_BEND,
    TOOLBAR_MIDI_CC_SCALE_SLOPE,
    TOOLBAR_MIDI_CC_SELECTED_DECREASE,
    TOOLBAR_MIDI_CC_SELECTED_INCREASE,
    TOOLBAR_MIDI_CC_SET_SCALE_FIX,
    TOOLBAR_MIDI_DELETE_REMOVE_NONE,
    TOOLBAR_MIDI_ENVELOPE,
    TOOLBAR_MIDI_EVENTS_MODE_CYCLE,
    TOOLBAR_MIDI_EVENTS_MODE_DRUM_DIAMOND,
    TOOLBAR_MIDI_EVENTS_MODE_DRUM_TRIANGLE,
    TOOLBAR_MIDI_EVENTS_MODE_NORMAL_RECTANGLE,
    TOOLBAR_MIDI_FOLDER,
    TOOLBAR_MIDI_HIDE_UNUSED_NOTE_ROWS,
    TOOLBAR_MIDI_HIDE_UNUSED_UNNAMED_NOTE_ROWS,
    TOOLBAR_MIDI_ITEM,
    TOOLBAR_MIDI_ITEM_SELECTED,
    TOOLBAR_MIDI_ITEM_SELECTED_262,
    TOOLBAR_MIDI_ITEMSEL,
    TOOLBAR_MIDI_ITEMSEL_OFF,
    TOOLBAR_MIDI_ITEMSEL_ON,
    TOOLBAR_MIDI_LENGTHEN_NOTE_GRID_UNIT,
    TOOLBAR_MIDI_LENGTHEN_NOTE_PIXEL,
    TOOLBAR_MIDI_LIST,
    TOOLBAR_MIDI_MODE_EVENT_LIST,
    TOOLBAR_MIDI_MODE_MUSICAL_NOTATION,
    TOOLBAR_MIDI_MODE_NAMED_NOTES,
    TOOLBAR_MIDI_MODE_PIANO_ROLL,
    TOOLBAR_MIDI_NOTES_FORCE_SNAP_SCALE,
    TOOLBAR_MIDI_PANIC_ALL_NOTES,
    TOOLBAR_MIDI_PITCH_DECREASE_OCTAVE,
    TOOLBAR_MIDI_PITCH_DECREASE_SEMITONE,
    TOOLBAR_MIDI_PITCH_INCREASE_OCTAVE,
    TOOLBAR_MIDI_PITCH_INCREASE_OCTAVE_173,
    TOOLBAR_MIDI_PITCH_INCREASE_SEMITONE,
    TOOLBAR_MIDI_PITCH_TRANSPOSE,
    TOOLBAR_MIDI_PROPERTIES,
    TOOLBAR_MIDI_RENDER_APPLY_AUDIO_WAVEFORM,
    TOOLBAR_MIDI_SHORTEN_NOTE_GRID_UNIT,
    TOOLBAR_MIDI_SHORTEN_NOTE_PIXEL,
    TOOLBAR_MIDI_SHOW_ALL_NOTE_ROWS,
    TOOLBAR_MIDI_SIZE_FULL,
    TOOLBAR_MIDI_STEP,
    TOOLBAR_MIDI_TRACKSEL,
    TOOLBAR_MIDI_TRACKSEL_OFF,
    TOOLBAR_MIDI_TRACKSEL_ON,
    TOOLBAR_MIDI_WAVEFORM_AUDIO,
    TOOLBAR_MIDI_ZOOM,
    TOOLBAR_MISC_ANARCHY,
    TOOLBAR_MISC_ANCHOR,
    TOOLBAR_MISC_BACK_LEFT_PREVIOUS,
    TOOLBAR_MISC_BACK_LEFT_PREVIOUS_MORE,
    TOOLBAR_MISC_BASH,
    TOOLBAR_MISC_BOMB,
    TOOLBAR_MISC_BRUSH_BROOM_CLEAN,
    TOOLBAR_MISC_BULB_IDEA,
    TOOLBAR_MISC_CALCULATE_NUMERIC,
    TOOLBAR_MISC_CAR,
    TOOLBAR_MISC_COFFEE,
    TOOLBAR_MISC_DEVIL,
    TOOLBAR_MISC_DOWN_NEXT,
    TOOLBAR_MISC_DOWN_NEXT_MORE,
    TOOLBAR_MISC_DRUM,
    TOOLBAR_MISC_DUCK,
    TOOLBAR_MISC_EXPLODE,
    TOOLBAR_MISC_FILTER,
    TOOLBAR_MISC_FINGER,
    TOOLBAR_MISC_FIREWIRE,
    TOOLBAR_MISC_GAME,
    TOOLBAR_MISC_GUITAR,
    TOOLBAR_MISC_GUITAR_HEADSTOCK,
    TOOLBAR_MISC_GUN,
    TOOLBAR_MISC_HEART,
    TOOLBAR_MISC_HORNS,
    TOOLBAR_MISC_HOUSE_HOME,
    TOOLBAR_MISC_IBEAM_CURSOR_SELECTION,
    TOOLBAR_MISC_JACK_INPUT_OUTPUT,
    TOOLBAR_MISC_KEY_LOCK,
    TOOLBAR_MISC_KEYBOARD,
    TOOLBAR_MISC_LIPS,
    TOOLBAR_MISC_MASK,
    TOOLBAR_MISC_MIC,
    TOOLBAR_MISC_MIXER_CONTROL,
    TOOLBAR_MISC_MONITOR_SPEAKER,
    TOOLBAR_MISC_MOUSE,
    TOOLBAR_MISC_MOUSE_LEFT_CLICK,
    TOOLBAR_MISC_MOUSE_RIGHT_CLICK,
    TOOLBAR_MISC_NETWORK_STREAM,
    TOOLBAR_MISC_PHONES,
    TOOLBAR_MISC_POINTER_CURSOR,
    TOOLBAR_MISC_POINTER_CURSOR_WHITE,
    TOOLBAR_MISC_QUESTION_RANDOM,
    TOOLBAR_MISC_RADIOACTIVE,
    TOOLBAR_MISC_RIGHT_FORWARD_NEXT,
    TOOLBAR_MISC_RIGHT_FORWARD_NEXT_MORE,
    TOOLBAR_MISC_RUN_BACKWARD,
    TOOLBAR_MISC_RUN_FORWARD,
    TOOLBAR_MISC_SAINT,
    TOOLBAR_MISC_SKULL_CROSSBONES,
    TOOLBAR_MISC_SPEECH_NOTE,
    TOOLBAR_MISC_STAR,
    TOOLBAR_MISC_STAR_GREEN,
    TOOLBAR_MISC_SYSTEM_REAPER,
    TOOLBAR_MISC_TAPE,
    TOOLBAR_MISC_TEA_MMMMM_TEA,
    TOOLBAR_MISC_THOUGHT_IDEA,
    TOOLBAR_MISC_TOILET,
    TOOLBAR_MISC_TRASH_BIN,
    TOOLBAR_MISC_UP_PREVIOUS,
    TOOLBAR_MISC_UP_PREVIOUS_MORE,
    TOOLBAR_MISC_USB,
    TOOLBAR_MISC_WALK_BACKWARD,
    TOOLBAR_MISC_WALK_FORWARD,
    TOOLBAR_MUTE_ENVELOPE,
    TOOLBAR_MUTE_NONE_UNMUTE,
    TOOLBAR_NEW,
    TOOLBAR_NOTE_GLISS_SLIDE,
    TOOLBAR_NOTE_TIE,
    TOOLBAR_PARAMETER_SCRUB,
    TOOLBAR_PATH_PRIMARY_DISK,
    TOOLBAR_PATH_PRIMARY_SECONDARY_BOTH_DISK,
    TOOLBAR_PATH_SECONDARY_DISK,
    TOOLBAR_PITCH_PRESERVE_LOCK,
    TOOLBAR_PREROLL_CLOCK,
    TOOLBAR_PREROLL_CLOCK_RECORD,
    TOOLBAR_PROJECT_SAVE_AS_NEW_DISK,
    TOOLBAR_PROJECT_UNUSED_DELETE_REMOVE_DISK,
    TOOLBAR_PROJPROP,
    TOOLBAR_QUANT,
    TOOLBAR_QUANT_OFF,
    TOOLBAR_QUANT_ON,
    TOOLBAR_QUARTER_CROTCHET_GRID,
    TOOLBAR_QUARTER_CROTCHET_NOTE,
    TOOLBAR_RAZOR_OFF,
    TOOLBAR_RAZOR_ON,
    TOOLBAR_RECORD,
    TOOLBAR_RECORD_ARM_ALL,
    TOOLBAR_RECORD_CREATE_ITEM_SEPERATE_LANE,
    TOOLBAR_RECORD_LOOP_TIME_SELECTION,
    TOOLBAR_RECORD_NEXT_BEAT_MEASURE,
    TOOLBAR_RECORD_NEXT_MARKER,
    TOOLBAR_RECORD_PROPERTIES,
    TOOLBAR_RECORD_SELECTED_ITEM_AUTO_PUNCH,
    TOOLBAR_RECORD_SPLIT_ITEM_NEW_TAKE,
    TOOLBAR_RECORD_STOP_DELETE,
    TOOLBAR_RECORD_STOP_SAVE,
    TOOLBAR_RECORD_TIME_SELECTION_AUTO_PUNCH,
    TOOLBAR_RECORD_TIME_SELECTION_SELECTED_ITEM_AUTO_PUNCH,
    TOOLBAR_RECORD_TRIM_ITEM_BEHIND_TAPE,
    TOOLBAR_REDO,
    TOOLBAR_REGION_DELETE_REMOVE_NONE,
    TOOLBAR_REGION_NEW,
    TOOLBAR_REGION_NEXT,
    TOOLBAR_REGION_PLAY_LOOP,
    TOOLBAR_REGION_PREVIOUS,
    TOOLBAR_REGION_PROPERTIES,
    TOOLBAR_REGION_TIME_SELECTION,
    TOOLBAR_RELSNAP,
    TOOLBAR_RELSNAP_OFF,
    TOOLBAR_RELSNAP_ON,
    TOOLBAR_REMOVE_SCISSORS_SELECTION,
    TOOLBAR_REMOVE_SELECTION_NONE,
    TOOLBAR_RENDER_EFFECTS_MIDI,
    TOOLBAR_REPLACEMODE,
    TOOLBAR_REVERT,
    TOOLBAR_RIPPLE,
    TOOLBAR_RIPPLE_ALL,
    TOOLBAR_RIPPLE_OFF,
    TOOLBAR_RIPPLE_ON,
    TOOLBAR_RIPPLE_ONE,
    TOOLBAR_SAVE,
    TOOLBAR_SCREENSET_CAMERA_LIST,
    TOOLBAR_SCREENSET_CAMERA_NEW,
    TOOLBAR_SCREENSET_CAMERA_NEXT,
    TOOLBAR_SCREENSET_CAMERA_PREVIOUS,
    TOOLBAR_SCREENSET_CAMERA_SAVE_DISK,
    TOOLBAR_SELECTION_DELETE_REMOVE,
    TOOLBAR_SELECTION_INVERSE_DELETE_REMOVE,
    TOOLBAR_SEND_HIDE_MUTE,
    TOOLBAR_SEND_SHOW_ENABLE,
    TOOLBAR_SHAPE_BEZIER,
    TOOLBAR_SHAPE_FAST_END,
    TOOLBAR_SHAPE_FAST_START,
    TOOLBAR_SHAPE_LINEAR,
    TOOLBAR_SHAPE_SQUARE,
    TOOLBAR_SHOW,
    TOOLBAR_SHOW_SELECTED,
    TOOLBAR_SHOW_INSERT,
    TOOLBAR_SHOW_PARAMETER,
    TOOLBAR_SHOW_SEND,
    TOOLBAR_SHUTTLE_BACK_REWIND,
    TOOLBAR_SHUTTLE_FORWARD,
    TOOLBAR_SIXTEENTH_SEMIQUAVER_GRID,
    TOOLBAR_SIXTEENTH_SEMIQUAVER_NOTE,
    TOOLBAR_SNAP,
    TOOLBAR_SNAP_OFF,
    TOOLBAR_SNAP_OFFSET_GRID_MOVE,
    TOOLBAR_SNAP_ON,
    TOOLBAR_SOLO_IN_FRONT_DIM,
    TOOLBAR_SOLO_NONE_UNSOLO,
    TOOLBAR_SPLIT_SCISSORS,
    TOOLBAR_STRETCH_MARKER_DELETE_REMOVE,
    TOOLBAR_STRETCH_MARKER_INSERT_NEW_ADD,
    TOOLBAR_STRETCH_MARKER_LOCKING,
    TOOLBAR_STRETCH_MARKER_NEXT,
    TOOLBAR_STRETCH_MARKER_PREVIOUS,
    TOOLBAR_STRETCH_MARKER_SNAP_GRID,
    TOOLBAR_STRETCH_MARKER_TIME_SELECTION_DELETE_REMOVE,
    TOOLBAR_STRETCH_MARKER_TIME_SELECTION_NEW_ADD,
    TOOLBAR_STRETCH_MARKER_TONAL,
    TOOLBAR_SWS_EXTENSION,
    TOOLBAR_SWS_EXTENSION_PROPERTIES,
    TOOLBAR_SYNC_FOLLOW_PLAY,
    TOOLBAR_SYNC_FOLLOW_RECORD,
    TOOLBAR_SYSTEM_EXTERNAL,
    TOOLBAR_SYSTEM_PROPERTIES,
    TOOLBAR_SYSTEM_SET_SAVE_DEFAULT_DISK,
    TOOLBAR_THEME_NEXT,
    TOOLBAR_THEME_PREVIOUS,
    TOOLBAR_THEME_REFRESH,
    TOOLBAR_THIRTY_SECOND_DEMISEMIQUAVER_,
    TOOLBAR_THIRTY_SECOND_DEMISEMIQUAVER_GRID,
    TOOLBAR_TIME_BEATS,
    TOOLBAR_TIME_CLOCK,
    TOOLBAR_TIME_CLOCK_PROPERTIES,
    TOOLBAR_TIME_HOURGLASS,
    TOOLBAR_TIME_HOURS,
    TOOLBAR_TIME_MEASURES,
    TOOLBAR_TIME_MINUTES,
    TOOLBAR_TIME_SAMPLE,
    TOOLBAR_TIME_SECONDS,
    TOOLBAR_TIME_SELECTION_DELETE_REMOVE,
    TOOLBAR_TIME_SELECTION_FIT_ITEM_SELECTED,
    TOOLBAR_TIME_SELECTION_ITEM_CUT,
    TOOLBAR_TIME_SELECTION_ITEM_DELETE_REMOVE,
    TOOLBAR_TIME_SELECTION_ITEM_SELECTED_GROW_EXPAND,
    TOOLBAR_TIME_SELECTION_LEFT,
    TOOLBAR_TIME_SELECTION_LOOP_LOCK,
    TOOLBAR_TIME_SELECTION_LOOP_PLAY,
    TOOLBAR_TIME_SELECTION_NEW,
    TOOLBAR_TIME_SELECTION_PLAY,
    TOOLBAR_TIME_SELECTION_PROPERTIES,
    TOOLBAR_TIME_SELECTION_REGION,
    TOOLBAR_TIME_SELECTION_RIGHT,
    TOOLBAR_TIME_STRETCH,
    TOOLBAR_TIMEBASE_BEATS_POSITION,
    TOOLBAR_TIMEBASE_BEATS_POSITION_LENGTH_RATE,
    TOOLBAR_TIMEBASE_TIME,
    TOOLBAR_TOOL_BRUSH_PAINT,
    TOOLBAR_TOOL_CROP,
    TOOLBAR_TOOL_ERASE_DELETE_REMOVE,
    TOOLBAR_TOOL_HAMMER,
    TOOLBAR_TOOL_KNIFE_TRIM,
    TOOLBAR_TOOL_PENCIL_DRAW,
    TOOLBAR_TOOL_RAZOR_BLADE,
    TOOLBAR_TOOL_SCISSORS_CUT_TRIM,
    TOOLBAR_TRACK_NEXT,
    TOOLBAR_TRACK_PREVIOUS,
    TOOLBAR_TRANSPORT_HOME_END,
    TOOLBAR_TREBLE_CLEF_NOTE,
    TOOLBAR_TRIM_SCISSORS_SELECTION,
    TOOLBAR_TRIPLET_NOTE,
    TOOLBAR_UNDO,
    TOOLBAR_UNFREEZE_RENDER_APPLY_SNOWFLAKE,
    TOOLBAR_V3_ENVITEM,
    TOOLBAR_V3_GRID,
    TOOLBAR_V3_GROUP,
    TOOLBAR_V3_LOAD,
    TOOLBAR_V3_LOCK,
    TOOLBAR_V3_METRO,
    TOOLBAR_V3_NEW,
    TOOLBAR_V3_PROJPROP,
    TOOLBAR_V3_REDO,
    TOOLBAR_V3_RIPPLE,
    TOOLBAR_V3_RIPPLE_ALL,
    TOOLBAR_V3_RIPPLE_ONE,
    TOOLBAR_V3_SAVE,
    TOOLBAR_V3_SNAP,
    TOOLBAR_V3_UNDO,
    TOOLBAR_V3_XFADE,
    TOOLBAR_VIDEO_ITEM_SELECTED,
    TOOLBAR_VIDEO_PROPERTIES,
    TOOLBAR_VIDEO_SCREEN,
    TOOLBAR_VIDEO_SYNC_START,
    TOOLBAR_VISIBLE_MIXER,
    TOOLBAR_VISIBLE_TCP,
    TOOLBAR_WHOLE_SEMIBREVE_GRID,
    TOOLBAR_WHOLE_SEMIBREVE_NOTE,
    TOOLBAR_WINDOW_FLOATING_TOOLBAR,
    TOOLBAR_WINDOW_FULLSCREEN,
    TOOLBAR_WINDOW_TAB_BACKGROUND_SYNCHRONIZE,
    TOOLBAR_WINDOW_TAB_CLIP,
    TOOLBAR_WINDOW_TAB_CLOCK,
    TOOLBAR_WINDOW_TAB_DELETE_REMOVE,
    TOOLBAR_WINDOW_TAB_DOCKER_BOTTOM,
    TOOLBAR_WINDOW_TAB_DOCKER_LEFT,
    TOOLBAR_WINDOW_TAB_DOCKER_RIGHT,
    TOOLBAR_WINDOW_TAB_DOCKER_SHOW,
    TOOLBAR_WINDOW_TAB_DOCKER_TOP,
    TOOLBAR_WINDOW_TAB_EFFECTS,
    TOOLBAR_WINDOW_TAB_FOLDER,
    TOOLBAR_WINDOW_TAB_LIST,
    TOOLBAR_WINDOW_TAB_MIDI_EDITOR,
    TOOLBAR_WINDOW_TAB_MIXER_MCP,
    TOOLBAR_WINDOW_TAB_MIXER_TCP,
    TOOLBAR_WINDOW_TAB_NAVIGATOR,
    TOOLBAR_WINDOW_TAB_NEW,
    TOOLBAR_WINDOW_TAB_NEW_BACKGROUND,
    TOOLBAR_WINDOW_TAB_NEXT_BACKGROUND,
    TOOLBAR_WINDOW_TAB_PERFORMANCE,
    TOOLBAR_WINDOW_TAB_PROPERTIES,
    TOOLBAR_WINDOW_TAB_REGION,
    TOOLBAR_WINDOW_TAB_ROUTING_MATRIX,
    TOOLBAR_WINDOW_TAB_SCREENSET_LAYOUT,
    TOOLBAR_WINDOW_TAB_UNDO_HISTORY,
    TOOLBAR_WINDOW_TAB_VIDEO,
    TOOLBAR_XFADE,
    TOOLBAR_XFADE_OFF,
    TOOLBAR_XFADE_ON,
    TOOLBAR_ZOOM_ALL,
    TOOLBAR_ZOOM_IN_AUDIO_WAVEFORM,
    TOOLBAR_ZOOM_IN_SELECTED_ITEM,
    TOOLBAR_ZOOM_OUT_ALL,
    TOOLBAR_ZOOM_OUT_AUDIO_WAVEFORM,
    TOOLBAR_ZOOM_PROJECT,
    TOOLBAR_ZOOM_REGION,
    TOOLBAR_ZOOM_SELECTED,
    TOOLBAR_ZOOM_SELECTED_ITEM,
    TOOLBAR_ZOOM_TIME_SELECTION,
    TOOSMALL_B,
    TOOSMALL_R,
    TRACK_ENV,
    TRACK_ENV_LATCH,
    TRACK_ENV_PREVIEW,
    TRACK_ENV_READ,
    TRACK_ENV_TOUCH,
    TRACK_ENV_WRITE,
    TRACK_FCOMP_OFF,
    TRACK_FCOMP_SMALL,
    TRACK_FCOMP_TINY,
    TRACK_FOLDER_LAST,
    TRACK_FOLDER_OFF,
    TRACK_FOLDER_ON,
    TRACK_FX_DIS,
    TRACK_FX_EMPTY,
    TRACK_FX_IN_EMPTY,
    TRACK_FX_IN_NORM,
    TRACK_FX_NORM,
    TRACK_FXEMPTY_H,
    TRACK_FXEMPTY_V,
    TRACK_FXOFF_H,
    TRACK_FXOFF_V,
    TRACK_FXON_H,
    TRACK_FXON_V,
    TRACK_IO,
    TRACK_IO_DIS,
    TRACK_IO_R,
    TRACK_IO_R_DIS,
    TRACK_IO_S,
    TRACK_IO_S_DIS,
    TRACK_IO_S_R,
    TRACK_IO_S_R_DIS,
    TRACK_MONITOR_AUTO,
    TRACK_MONITOR_OFF,
    TRACK_MONITOR_ON,
    TRACK_MONO,
    TRACK_MUTE_OFF,
    TRACK_MUTE_ON,
    TRACK_PHASE_INV,
    TRACK_PHASE_NORM,
    TRACK_RECARM_AUTO,
    TRACK_RECARM_AUTO_NOREC,
    TRACK_RECARM_AUTO_ON,
    TRACK_RECARM_NOREC,
    TRACK_RECARM_OFF,
    TRACK_RECARM_ON,
    TRACK_RECMODE_IN,
    TRACK_RECMODE_OFF,
    TRACK_RECMODE_OUT,
    TRACK_SOLO_OFF,
    TRACK_SOLO_ON,
    TRACK_SOLODEFEAT_ON,
    TRACK_STEREO,
    TRANSPORT_BG,
    TRANSPORT_BPM,
    TRANSPORT_BPM_BG,
    TRANSPORT_EDIT_BG,
    TRANSPORT_END,
    TRANSPORT_GROUP_BG,
    TRANSPORT_HOME,
    TRANSPORT_KNOB_BG_LARGE,
    TRANSPORT_NEXT,
    TRANSPORT_PAUSE,
    TRANSPORT_PAUSE_ON,
    TRANSPORT_PLAY,
    TRANSPORT_PLAY_ON,
    TRANSPORT_PLAY_SYNC,
    TRANSPORT_PLAY_SYNC_ON,
    TRANSPORT_PLAYSPEEDBG,
    TRANSPORT_PLAYSPEEDTHUMB,
    TRANSPORT_PREVIOUS,
    TRANSPORT_RECORD,
    TRANSPORT_RECORD_ITEM,
    TRANSPORT_RECORD_ITEM_ON,
    TRANSPORT_RECORD_LOOP,
    TRANSPORT_RECORD_LOOP_ON,
    TRANSPORT_RECORD_ON,
    TRANSPORT_REPEAT_OFF,
    TRANSPORT_REPEAT_ON,
    TRANSPORT_STATUS_BG,
    TRANSPORT_STATUS_BG_ERR,
    TRANSPORT_STOP,
    TRANSPORT_TIMEBASE_BEAT,
    TRANSPORT_TIMEBASE_TIME,
];

/// Look one up by REAPER file name.
pub fn by_name(name: &str) -> Option<ArtData> {
    ALL.iter().find(|a| a.name == name).copied()
}
