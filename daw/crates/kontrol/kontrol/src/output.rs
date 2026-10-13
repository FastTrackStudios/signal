//! Thin MIDI **output** helpers for the S88 MK3, over midicore's MIDI output
//! primitive ([`midicore::midir::MidiOutput`]) — used only by the falsification
//! probe (proving note-ons on Main/DAW do NOT light the guide) and as the seam
//! for future NIHIA DAW-port integration.
//!
//! The domain here — the S88 port names ([`ports`]) and the Light-Guide
//! note-on [`Encoding`] candidates — stays; the raw `midir` connection plumbing
//! now lives in `midicore`, so this crate no longer re-rolls it.

use eyre::Result;
use midicore::{midir::MidiOutput, PortSelector};

use crate::LightColor;

/// The three MIDI ports the S88 MK3 exposes, matched by a case-insensitive
/// substring of the ALSA port name.
pub mod ports {
    /// The keybed / performance MIDI port.
    pub const MAIN: &str = "Main";
    /// The DAW-integration port (where NIHIA host-integration traffic flows).
    pub const DAW: &str = "DAW";
    /// The secondary / "external" MIDI port.
    pub const EXT: &str = "Ext";
    /// Substring identifying any S88 MK3 port (device name prefix).
    pub const DEVICE: &str = "KONTROL S88 MK3";
}

/// A candidate MIDI encoding for a Light-Guide note-on, used by the probe to
/// falsify the "maybe it's just MIDI" hypotheses. None of these is expected to
/// work on MK3 (the guide is USB, not MIDI) — see `docs/protocol.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// Note On, channel 1 (index 0), `key = note`, `velocity = palette byte`.
    NoteOnCh1,
    /// Note On, channel 16 (index 15) — NIHIA control channel.
    NoteOnCh16,
}

impl Encoding {
    /// Serialize one key's light state to raw MIDI bytes.
    pub fn encode(self, note: u8, color: LightColor) -> [u8; 3] {
        let ch = match self {
            Encoding::NoteOnCh1 => 0x00,
            Encoding::NoteOnCh16 => 0x0f,
        };
        [0x90 | ch, note & 0x7f, color.byte()]
    }
}

/// A single open MIDI output port (backed by [`midicore::midir::MidiOutput`]).
pub struct MidiOut {
    out: MidiOutput,
    /// The port name actually opened (for logs / the probe).
    pub name: String,
}

impl MidiOut {
    /// Open the first output port whose name contains `needle` (case-insensitive).
    pub fn open_contains(needle: &str) -> Result<Self> {
        let out = MidiOutput::open(PortSelector::NameContains(needle.to_string()))?;
        let name = out.opened.clone();
        tracing::info!(port = %name, "kontrol: opened MIDI output");
        Ok(Self { out, name })
    }

    /// Send raw MIDI bytes to the port.
    pub fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.out.send(bytes)
    }
}

/// List the names of all available MIDI **output** ports.
pub fn output_ports() -> Vec<String> {
    midicore::midir::output_ports()
}
