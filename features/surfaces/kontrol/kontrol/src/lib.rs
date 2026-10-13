//! `kontrol` — a Linux driver for the **Native Instruments Komplete Kontrol**
//! keyboards, focused on the **Light Guide** (the per-key RGB LED strip above
//! the keybed). Native hardware I/O only — no wasm / no_std path.
//!
//! ## The key finding (read `docs/protocol.md`)
//!
//! The Light Guide is **not a MIDI feature**. On MK3 it is a raw USB **bulk**
//! transfer to a vendor interface (interface 4, endpoint 4); DrivenByMoss
//! implements *no* light-guide code for MkII/MK3 (only MkI, over USB HID). So
//! sending standard MIDI to the three ALSA ports (`KONTROL S88 MK3 Main / DAW /
//! Ext`) **cannot** light the keys.
//!
//! Accordingly this crate has two layers:
//! - [`usb`] — the **real** path: [`usb::KontrolUsb`] opens the MK3 over
//!   `rusb`/libusb and bulk-writes the Light Guide frame. This is what should
//!   actually light LEDs (confirmed format from tillt/KompleteSynthesia; the
//!   S88 MK3 *product id* and *init* are the unconfirmed parts — verify with
//!   the probe).
//! - [`output`] — a MIDI **output** layer used only by `examples/probe.rs` as a
//!   **falsification test** (prove that note-ons on Main/DAW do nothing), and as
//!   the seam for future NIHIA DAW-port integration.
//!
//! ```no_run
//! use kontrol::{usb::KontrolUsb, LightColor, note_to_key};
//! let mut kk = KontrolUsb::open()?;              // claim iface 4
//! kk.set_key(note_to_key(60), LightColor::GREEN); // middle C
//! kk.flush()?;                                    // bulk write
//! # Ok::<(), eyre::Report>(())
//! ```

pub mod output;
pub mod usb;

pub use output::{output_ports, Encoding, MidiOut};
pub use usb::KontrolUsb;

/// LED brightness. The palette byte is `hue_base + intensity`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Intensity {
    Low = 0,
    Medium = 1,
    High = 2,
    Bright = 3,
}

/// A Light Guide color: an index into the device's fixed palette (it is **not**
/// free 24-bit RGB — the strip has a fixed hue table; see `docs/protocol.md`).
///
/// The wire byte is `hue_base + intensity` where `intensity` is the low 2 bits.
/// The named constants are Native Instruments' hue bases (confirmed on MkII,
/// shared by MK3), at [`Intensity::High`]. `0x00` = off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightColor(u8);

impl LightColor {
    /// LED off (palette byte `0x00`).
    pub const OFF: LightColor = LightColor(0x00);

    // Hue bases (add an intensity 0..=3). From tillt `HIDController.h`,
    // corroborated by SynthesiaKontrol `color_scan.py`.
    /// Red hue base (`0x04`).
    pub const RED_BASE: u8 = 0x04;
    /// Orange hue base (`0x08`).
    pub const ORANGE_BASE: u8 = 0x08;
    /// Yellow hue base (`0x10`).
    pub const YELLOW_BASE: u8 = 0x10;
    /// Green hue base (`0x1C`).
    pub const GREEN_BASE: u8 = 0x1C;
    /// Blue hue base (`0x2C`).
    pub const BLUE_BASE: u8 = 0x2C;
    /// Purple hue base (`0x34`).
    pub const PURPLE_BASE: u8 = 0x34;
    /// Pink hue base (`0x38`).
    pub const PINK_BASE: u8 = 0x38;
    /// White hue base (`0x44`).
    pub const WHITE_BASE: u8 = 0x44;

    /// Common colors at [`Intensity::High`].
    pub const RED: LightColor = LightColor(Self::RED_BASE + 2);
    pub const ORANGE: LightColor = LightColor(Self::ORANGE_BASE + 2);
    pub const YELLOW: LightColor = LightColor(Self::YELLOW_BASE + 2);
    pub const GREEN: LightColor = LightColor(Self::GREEN_BASE + 2);
    pub const BLUE: LightColor = LightColor(Self::BLUE_BASE + 2);
    pub const PURPLE: LightColor = LightColor(Self::PURPLE_BASE + 2);
    pub const PINK: LightColor = LightColor(Self::PINK_BASE + 2);
    pub const WHITE: LightColor = LightColor(Self::WHITE_BASE + 2);

    /// Build from a hue base and an intensity.
    pub const fn new(hue_base: u8, intensity: Intensity) -> Self {
        LightColor((hue_base & 0xFC) | (intensity as u8 & 0x03))
    }

    /// Build from a raw palette byte (`0..=127`, clamped). `0` = off.
    pub const fn from_byte(raw: u8) -> Self {
        LightColor(if raw > 127 { 127 } else { raw })
    }

    /// The raw palette byte sent to the device.
    pub const fn byte(self) -> u8 {
        self.0
    }
}

/// Lowest MIDI note on an 88-key keybed (A0).
pub const S88_LOWEST_NOTE: u8 = 21;
/// Highest MIDI note on an 88-key keybed (C8).
pub const S88_HIGHEST_NOTE: u8 = 108;

/// Map a MIDI note number to the device Light Guide **key index**.
///
/// The MK3 frame addresses 128 keys and, per tillt, uses the MIDI note number
/// directly as the key index (unlike MkII's `note - 21` offset). We pass the
/// note through, clamped to the 0..127 frame. If empirical testing shows the
/// S88 MK3 needs the `-21` offset like MkII, adjust here — see `docs/protocol.md`.
pub const fn note_to_key(note: u8) -> u8 {
    note & 0x7f
}

/// Map a `midicore` key number to the device Light Guide key index.
pub fn key_from_midicore(key: midicore::KeyNumber) -> u8 {
    note_to_key(key.get())
}
