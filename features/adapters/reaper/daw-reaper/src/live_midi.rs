//! `impl LiveMidi for Reaper` — device management + StuffMIDIMessage injection.
//!
//! Device enumeration / open / close / send are not wired to a real
//! MIDI driver yet; they return stubs. `stuff_midi_message` is real
//! and routes through the REAPER C API.
//!
//! For editing MIDI data in takes, see `Midi`.

use daw_proto::live_midi::{
    LiveMidi, MidiEvent, MidiEventExt, MidiInputDevice, MidiOutputDevice, SendMidiTiming,
    StuffMidiTarget,
};
use tracing::{debug, warn};

impl LiveMidi for crate::Reaper {
    // ── Device enumeration (stubs — not yet implemented) ───────────

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

    // ── Device state (stubs) ───────────────────────────────────────

    fn open_input_device(&self, _id: u32) -> bool {
        false
    }

    fn close_input_device(&self, _id: u32) {}

    fn open_output_device(&self, _id: u32) -> bool {
        false
    }

    fn close_output_device(&self, _id: u32) {}

    // ── Output (stub) ──────────────────────────────────────────────

    fn send_midi(&self, _device_id: u32, _message: MidiEvent, _timing: SendMidiTiming) {}

    // ── Input subscription (one-shot arm; no streaming) ────────────

    fn subscribe_input(&self, _device_id: u32) -> bool {
        false
    }

    fn unsubscribe_input(&self, _device_id: u32) {}

    // ── MIDI injection ─────────────────────────────────────────────

    /// Inject into REAPER's internal MIDI queue. Used for VKB
    /// simulation in integration tests and for sending CC to
    /// plugins that manage presets internally.
    fn stuff_midi_message(&self, target: StuffMidiTarget, message: MidiEvent) {
        let Some((status, data1, data2)) = message.to_raw_bytes() else {
            warn!(
                "stuff_midi_message: cannot convert {:?} to short message (SysEx/Raw not supported)",
                message
            );
            return;
        };

        let mode = match target {
            StuffMidiTarget::VirtualMidiKeyboard => 0,
            StuffMidiTarget::ControlInput => 1,
            StuffMidiTarget::VirtualMidiKeyboardCurrentChannel => 2,
        };

        debug!(
            "stuff_midi_message: mode={} status=0x{:02X} data1={} data2={}",
            mode, status, data1, data2
        );

        let reaper = reaper_high::Reaper::get();
        reaper.medium_reaper().low().StuffMIDIMessage(
            mode,
            status as i32,
            data1 as i32,
            data2 as i32,
        );
    }
}
