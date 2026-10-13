//! Live MIDI events

use super::MidiEvent;
use facet::Facet;

/// A live MIDI event with timing information
#[derive(Clone, Debug, Facet)]
pub struct LiveMidiEvent {
    /// Device ID this event came from or is going to
    pub device_id: u32,
    /// Frame offset in 1/1024000 second units (REAPER's MIDI timing)
    pub frame_offset: u32,
    /// The MIDI message
    pub message: MidiEvent,
}

impl LiveMidiEvent {
    /// Create a new live MIDI event
    pub fn new(device_id: u32, frame_offset: u32, message: MidiEvent) -> Self {
        Self {
            device_id,
            frame_offset,
            message,
        }
    }

    /// Create an event to be sent immediately
    pub fn immediate(device_id: u32, message: MidiEvent) -> Self {
        Self::new(device_id, 0, message)
    }
}

/// Events related to live MIDI (for subscriptions)
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum LiveMidiServiceEvent {
    /// A MIDI input device was connected
    InputDeviceConnected { device_id: u32, name: String },
    /// A MIDI input device was disconnected
    InputDeviceDisconnected { device_id: u32 },
    /// A MIDI output device was connected
    OutputDeviceConnected { device_id: u32, name: String },
    /// A MIDI output device was disconnected
    OutputDeviceDisconnected { device_id: u32 },
    /// Received MIDI input (when subscribed)
    MidiReceived(LiveMidiEvent),
}
