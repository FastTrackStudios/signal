//! Shared constants for input module
//!
//! Centralized location for all magic numbers and thresholds used across
//! the input subsystem. Based on SWS BR_MouseUtil.cpp and BR_MidiUtil.cpp.

// region: --- Edge Detection Thresholds

/// Edge detection threshold in pixels (for item edges, envelope edges, etc.)
pub const EDGE_THRESHOLD_PX: i32 = 8;

/// Automation item edge detection threshold
pub const AI_EDGE_THRESHOLD_PX: i32 = 8;

/// Gap between envelope and track edge
pub const ENV_GAP: i32 = 4;

/// Envelope hit point detection radius
pub const ENV_HIT_POINT: i32 = 5;

/// Envelope hit point left offset
pub const ENV_HIT_POINT_LEFT: i32 = 6;

/// Envelope hit point down offset
pub const ENV_HIT_POINT_DOWN: i32 = 6;

/// Fade handle height in pixels
pub const FADE_HANDLE_HEIGHT_PX: i32 = 15;

/// Threshold for lower half of item (as fraction)
pub const LOWER_HALF_THRESHOLD: f64 = 0.5;

/// Stretch marker hit point detection radius
pub const STRETCH_M_HIT_POINT: i32 = 6;

/// Minimum take height for stretch marker detection
pub const STRETCH_M_MIN_TAKE_HEIGHT: i32 = 8;

/// Envelope line width
pub const ENV_LINE_WIDTH: i32 = 1;

// endregion: --- Edge Detection Thresholds

// region: --- MIDI Editor Dimensions

/// MIDI ruler height in pixels (from SWS BR_MidiUtil)
pub const MIDI_RULER_H: i32 = 64;

/// MIDI piano roll black keys width
pub const MIDI_BLACK_KEYS_W: i32 = 73;

/// MIDI CC lane divider height
pub const MIDI_LANE_DIVIDER_H: i32 = 6;

/// MIDI note edge hit threshold
pub const MIDI_NOTE_EDGE_HIT: i32 = 5;

// endregion: --- MIDI Editor Dimensions

// region: --- Window Constants

/// MIDI window type: Note view
pub const MIDI_WND_NOTEVIEW: i32 = 1;

/// MIDI window type: Keyboard/piano
pub const MIDI_WND_KEYBOARD: i32 = 2;

/// MIDI window type: Unknown
pub const MIDI_WND_UNKNOWN: i32 = 3;

/// Approximate scrollbar width
pub const SCROLLBAR_W: i32 = 17;

// endregion: --- Window Constants

// region: --- Track Constants

/// Gap above master track in TCP
pub const TCP_MASTER_GAP: i32 = 5;

/// Minimum height for item label
pub const ITEM_LABEL_MIN_HEIGHT: i32 = 28;

/// Threshold for minimum take height count
pub const TAKE_MIN_HEIGHT_COUNT: i32 = 10;

/// High minimum take height
pub const TAKE_MIN_HEIGHT_HIGH: i32 = 12;

/// Low minimum take height
pub const TAKE_MIN_HEIGHT_LOW: i32 = 6;

// endregion: --- Track Constants
