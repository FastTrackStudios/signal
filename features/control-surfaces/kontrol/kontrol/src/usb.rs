//! The Light Guide transport for the Komplete Kontrol S-series **MK3**.
//!
//! CONFIRMED EMPIRICALLY on an S88 MK3 (`17cc:2120`) 2026-07-11: the Light
//! Guide is a **HID output report**, NOT the raw USB bulk transfer earlier
//! sources (tillt/KompleteSynthesia) used. The device's HID interface (USB
//! interface 2) takes numbered output reports:
//!   - report `0x80` → the 8 top knob LEDs
//!   - report `0x81` → the left-hand pitch/mod wheel LEDs
//!   - report **`0x82`** → the **keybed Light Guide** (one palette byte per key)
//!
//! A keybed frame is `[0x82] + 88 palette bytes`; key index = `note - 21`
//! (A0 = MIDI 21 = index 0). No init/handshake is needed — keys light on the
//! first report. Color is a palette byte (hue base + intensity); see
//! [`crate::LightColor`].
//!
//! We speak this over `hidapi` (hidraw on Linux), which coexists with the
//! kernel `usbhid`/`snd-usb-audio` drivers — so lighting keys does NOT disturb
//! the keyboard's MIDI. Accessing the hidraw node needs USB permissions (a
//! udev `uaccess` rule for VID `0x17cc`, or root).

use eyre::{eyre, Result};
use hidapi::{HidApi, HidDevice};

use crate::LightColor;

/// Native Instruments USB vendor id.
pub const NI_VID: u16 = 0x17cc;

/// USB product ids for the Komplete Kontrol MK3 keyboards.
pub const PID_S49_MK3: u16 = 0x2100; // UNCONFIRMED (placeholder)
pub const PID_S61_MK3: u16 = 0x2110; // from tillt
pub const PID_S88_MK3: u16 = 0x2120; // CONFIRMED by enumeration on this rig

/// Every MK3 product id we will try to open, most-likely first.
pub const MK3_PIDS: &[u16] = &[PID_S88_MK3, PID_S61_MK3, PID_S49_MK3];

/// The device's HID interface number (interface 2 = the LED/HID collection).
const HID_INTERFACE: i32 = 2;

/// HID output report id for the keybed Light Guide.
pub const REPORT_KEYBED: u8 = 0x82;
/// HID output report id for the top knob LEDs.
pub const REPORT_KNOBS: u8 = 0x80;
/// HID output report id for the pitch/mod wheel LEDs.
pub const REPORT_WHEELS: u8 = 0x81;

/// Number of keys on an 88-key keybed (also the report payload length).
const NUM_KEYS: usize = 88;

/// An open HID connection to an MK3 keyboard's Light Guide.
pub struct KontrolUsb {
    device: HidDevice,
    /// One palette byte per key (index 0 = lowest key = MIDI 21). Mutated by
    /// [`set_key`](Self::set_key), pushed by [`flush`](Self::flush).
    keys: [u8; NUM_KEYS],
    /// The product id we opened (which model).
    pub pid: u16,
}

impl KontrolUsb {
    /// Open the first attached Komplete Kontrol MK3 keyboard's Light Guide.
    ///
    /// Requires permission to access the hidraw node (a udev `uaccess` rule for
    /// VID `0x17cc`, or root).
    pub fn open() -> Result<Self> {
        let api = HidApi::new().map_err(|e| eyre!("hidapi init: {e}"))?;
        // Prefer the LED HID interface (2); fall back to any NI MK3 HID node.
        let mut chosen: Option<(u16, std::ffi::CString)> = None;
        for info in api.device_list() {
            if info.vendor_id() == NI_VID && MK3_PIDS.contains(&info.product_id()) {
                if info.interface_number() == HID_INTERFACE {
                    chosen = Some((info.product_id(), info.path().to_owned()));
                    break;
                }
                chosen.get_or_insert_with(|| (info.product_id(), info.path().to_owned()));
            }
        }
        let (pid, path) = chosen.ok_or_else(|| {
            eyre!(
                "no Komplete Kontrol MK3 HID device found (VID {NI_VID:#06x}, PIDs {MK3_PIDS:#06x?}) \
                 — connected? and do you have hidraw permission (udev rule for 0x17cc, or root)?"
            )
        })?;
        let device = api
            .open_path(&path)
            .map_err(|e| eyre!("open MK3 HID device: {e} (permission? udev rule for 0x17cc)"))?;
        tracing::info!(
            pid = format!("{pid:#06x}"),
            "kontrol: opened MK3 Light Guide (HID)"
        );
        Ok(Self {
            device,
            keys: [0; NUM_KEYS],
            pid,
        })
    }

    /// Set one key's color in the local map (0 = lowest key = MIDI 21). Call
    /// [`flush`](Self::flush) to push. Out-of-range indices are ignored.
    pub fn set_key(&mut self, key: u8, color: LightColor) {
        if let Some(slot) = self.keys.get_mut(key as usize) {
            *slot = color.byte();
        }
    }

    /// All keys off in the local map. Call [`flush`](Self::flush) to push.
    pub fn clear(&mut self) {
        self.keys = [0; NUM_KEYS];
    }

    /// Push the current key map to the keybed Light Guide (one HID report).
    pub fn flush(&mut self) -> Result<()> {
        let mut buf = [0u8; 1 + NUM_KEYS];
        buf[0] = REPORT_KEYBED;
        buf[1..].copy_from_slice(&self.keys);
        let n = self
            .device
            .write(&buf)
            .map_err(|e| eyre!("HID write to keybed Light Guide: {e}"))?;
        tracing::debug!(
            "kontrol: wrote {n} bytes to keybed Light Guide (report {REPORT_KEYBED:#04x})"
        );
        Ok(())
    }

    /// Set one key and flush immediately.
    pub fn light(&mut self, key: u8, color: LightColor) -> Result<()> {
        self.set_key(key, color);
        self.flush()
    }

    /// Set every key to `color` and flush (a wash, or [`LightColor::OFF`] to
    /// blank the whole keybed).
    pub fn fill(&mut self, color: LightColor) -> Result<()> {
        self.keys = [color.byte(); NUM_KEYS];
        self.flush()
    }

    /// Which model string best matches the opened PID.
    pub fn model(&self) -> &'static str {
        match self.pid {
            PID_S88_MK3 => "Komplete Kontrol S88 MK3",
            PID_S61_MK3 => "Komplete Kontrol S61 MK3",
            PID_S49_MK3 => "Komplete Kontrol S49 MK3",
            _ => "Komplete Kontrol MK3",
        }
    }

    /// Write a raw HID output report (report id + payload). Escape hatch for the
    /// other LED zones — knobs ([`REPORT_KNOBS`]), wheels ([`REPORT_WHEELS`]) —
    /// and experiments.
    pub fn write_report(&self, report_id: u8, payload: &[u8]) -> Result<usize> {
        let mut buf = Vec::with_capacity(1 + payload.len());
        buf.push(report_id);
        buf.extend_from_slice(payload);
        self.device
            .write(&buf)
            .map_err(|e| eyre!("HID write report {report_id:#04x}: {e}"))
    }
}

/// List attached NI HID devices (VID `0x17cc`) as `(pid, "model")` for
/// diagnostics.
pub fn list_ni_devices() -> Vec<(u16, String)> {
    let Ok(api) = HidApi::new() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for info in api.device_list() {
        if info.vendor_id() != NI_VID {
            continue;
        }
        let pid = info.product_id();
        let model = match pid {
            PID_S88_MK3 => "S88 MK3",
            PID_S61_MK3 => "S61 MK3",
            PID_S49_MK3 => "S49 MK3?",
            _ => "other NI device",
        };
        out.push((pid, format!("{model} (iface {})", info.interface_number())));
    }
    out.dedup();
    out
}
