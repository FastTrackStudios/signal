//! `impl LiveMidi for Standalone`.
//!
//! Standalone has no host MIDI devices, so enumeration returns empty,
//! open/subscribe operations report failure, and fire-and-forget sends are
//! no-ops.

use daw_proto::live_midi::{
    LiveMidi, MidiEvent, MidiInputDevice, MidiOutputDevice, SendMidiTiming, StuffMidiTarget,
};

use crate::sync::Standalone;

impl LiveMidi for Standalone {
    fn input_devices(&self) -> Vec<MidiInputDevice> {
        Vec::new()
    }
    fn output_devices(&self) -> Vec<MidiOutputDevice> {
        Vec::new()
    }
    fn input_device(&self, _id: u32) -> Option<MidiInputDevice> {
        None
    }
    fn output_device(&self, _id: u32) -> Option<MidiOutputDevice> {
        None
    }
    fn open_input_device(&self, _id: u32) -> bool {
        false
    }
    fn close_input_device(&self, _id: u32) {}
    fn open_output_device(&self, _id: u32) -> bool {
        false
    }
    fn close_output_device(&self, _id: u32) {}
    fn send_midi(&self, _device_id: u32, _message: MidiEvent, _timing: SendMidiTiming) {}
    fn subscribe_input(&self, _device_id: u32) -> bool {
        false
    }
    fn unsubscribe_input(&self, _device_id: u32) {}
    fn stuff_midi_message(&self, _target: StuffMidiTarget, _message: MidiEvent) {}
}
