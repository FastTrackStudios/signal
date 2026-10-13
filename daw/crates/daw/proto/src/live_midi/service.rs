//! Live MIDI service — device management + realtime I/O.
//!
//! For editing MIDI data in takes, see `Midi`.

use super::{MidiEvent, MidiInputDevice, MidiOutputDevice, SendMidiTiming, StuffMidiTarget};

#[architect::rpc]
pub trait LiveMidi {
    // ── Device enumeration ─────────────────────────────────────────

    fn input_devices(&self) -> Vec<MidiInputDevice>;
    fn output_devices(&self) -> Vec<MidiOutputDevice>;
    fn input_device(&self, id: u32) -> Option<MidiInputDevice>;
    fn output_device(&self, id: u32) -> Option<MidiOutputDevice>;

    // ── Device state ───────────────────────────────────────────────

    fn open_input_device(&self, id: u32) -> bool;
    fn close_input_device(&self, id: u32);
    fn open_output_device(&self, id: u32) -> bool;
    fn close_output_device(&self, id: u32);

    // ── Output ─────────────────────────────────────────────────────

    /// Send a MIDI message to a device.
    fn send_midi(&self, device_id: u32, message: MidiEvent, timing: SendMidiTiming);

    // ── Input subscription ─────────────────────────────────────────
    //
    // Subscription stream retires with the architect::rpc port; sibling
    // trait when revived. `subscribe_input` kept as a one-shot "arm"
    // operation; the actual event delivery uses the broadcaster.

    fn subscribe_input(&self, device_id: u32) -> bool;
    fn unsubscribe_input(&self, device_id: u32);

    // ── MIDI injection (StuffMIDIMessage) ──────────────────────────

    /// Inject into REAPER's internal MIDI message queue. Used for
    /// VKB simulation in integration tests and for sending CC to
    /// plugins that manage presets internally.
    fn stuff_midi_message(&self, target: StuffMidiTarget, message: MidiEvent);
}
