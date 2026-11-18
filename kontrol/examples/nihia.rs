//! NIHIA (DrivenByMoss) probe: put the MK3 into DAW-integration mode and try to
//! populate its screens over the DAW MIDI port (no USB bulk).
//!
//! Handshake (from DrivenByMoss KontrolProtocolControlSurface):
//!   HELLO = CC on channel 16 (0xBF), controller 0x01, value = protocol version.
//!   Display state via SysEx: header `F0 00 21 09 00 00 44 43 01 00` + stateID +
//!   value + index + ascii text + F7.
//!
//! Run: `cargo run -p kontrol --example nihia`

use std::thread::sleep;
use std::time::Duration;

use kontrol::output::MidiOut;

const HELLO: u8 = 0x01;
const GOODBYE: u8 = 0x02;
const SYSEX_HEADER: [u8; 10] = [0xF0, 0x00, 0x21, 0x09, 0x00, 0x00, 0x44, 0x43, 0x01, 0x00];
const SYSEX_TRACK_AVAILABLE: u8 = 0x40;
const SYSEX_TRACK_NAME: u8 = 0x48;

fn sysex(state_id: u8, value: u8, index: u8, text: &str) -> Vec<u8> {
    let mut m = SYSEX_HEADER.to_vec();
    m.push(state_id);
    m.push(value);
    m.push(index);
    for b in text.bytes() {
        m.push(b & 0x7f);
    }
    m.push(0xF7);
    m
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut daw = MidiOut::open_contains("DAW")?;
    println!("opened DAW port");

    // Try protocol versions 3 then 4 (MK3 is NIHIA v3/v4).
    for ver in [3u8, 4u8] {
        println!("HELLO version {ver} (BF 01 {ver:02X})");
        daw.send(&[0xBF, HELLO, ver])?;
        sleep(Duration::from_millis(400));
    }

    // Announce one available track + a name, and a couple of param labels, so
    // the DAW view has something to render.
    daw.send(&sysex(SYSEX_TRACK_AVAILABLE, 1, 0, ""))?; // track 0 available (type 1)
    daw.send(&sysex(SYSEX_TRACK_NAME, 0, 0, "FTS DRUMS"))?;
    daw.send(&sysex(0x46, 0, 0, "MM2 KIT"))?; // volume text slot
    daw.send(&sysex(0x48, 0, 1, "KICK"))?;
    daw.send(&sysex(0x48, 0, 2, "SNARE"))?;

    println!("MIXER view: FTS Drums / MM2 KIT — holding ~6s…");
    for _ in 0..12 {
        sleep(Duration::from_millis(500));
        daw.send(&[0xBF, HELLO, 3])?; // keepalive
    }

    // ── Instrument (params) view: 8 knob params + page + plugin name ──
    const SELECTED_PLUGIN: u8 = 0x70;
    const PARAM_NAME: u8 = 0x72;
    const PARAM_VALUE: u8 = 0x73;
    const SELECTED_PARAM_PAGE: u8 = 0x74;
    const PAGE_NAME: u8 = 0x75;
    daw.send(&sysex(SELECTED_PLUGIN, 0, 0, "MM2 KICK"))?;
    daw.send(&sysex(PAGE_NAME, 0, 0, "AMP"))?;
    daw.send(&[0xBF, SELECTED_PARAM_PAGE, 0])?;
    let params = [
        ("TUNE", "+0.0"),
        ("DECAY", "1.2s"),
        ("ATTACK", "0ms"),
        ("VOLUME", "-3dB"),
        ("PAN", "C"),
        ("PITCH", "0"),
        ("SEND", "20%"),
        ("COMP", "ON"),
    ];
    for (i, (name, val)) in params.iter().enumerate() {
        daw.send(&sysex(PARAM_NAME, 0, i as u8, name))?;
        daw.send(&sysex(PARAM_VALUE, 0, i as u8, val))?;
    }
    println!("INSTRUMENT view: MM2 KICK params — holding ~6s…");
    for _ in 0..12 {
        sleep(Duration::from_millis(500));
        daw.send(&[0xBF, HELLO, 3])?; // keepalive
    }

    daw.send(&[0xBF, GOODBYE, 0])?;
    println!("sent GOODBYE — done.");
    Ok(())
}
