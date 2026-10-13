//! MIDI device types

use facet::Facet;

/// A MIDI input device
#[derive(Clone, Debug, Facet, Default)]
pub struct MidiInputDevice {
    /// Device ID
    pub id: u32,
    /// Device name
    pub name: String,
    /// Whether the device is available in the system
    pub is_available: bool,
    /// Whether the device is currently open
    pub is_open: bool,
    /// Whether the device is connected/responding
    pub is_connected: bool,
}

/// A MIDI output device
#[derive(Clone, Debug, Facet, Default)]
pub struct MidiOutputDevice {
    /// Device ID
    pub id: u32,
    /// Device name
    pub name: String,
    /// Whether the device is available in the system
    pub is_available: bool,
    /// Whether the device is currently open
    pub is_open: bool,
    /// Whether the device is connected/responding
    pub is_connected: bool,
}
