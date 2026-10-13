//! Try the USB-bulk screen image WHILE the device is in NIHIA DAW mode — the KK
//! software drives both, so the display may only accept bitmaps once woken by
//! the hello. Also sweeps a few framings/endpoints.
//!
//! Run with sudo (bulk needs it):  sudo -E <bin>  (build first as your user)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use kontrol::output::MidiOut;
use rusb::{Context, UsbContext};

const VID: u16 = 0x17cc;
const PID: u16 = 0x2120;
const IFACE: u8 = 3;
const W: usize = 1280;
const H: usize = 480;

fn rgb565_be(r: u8, g: u8, b: u8) -> [u8; 2] {
    (((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | ((b as u16) >> 3)).to_be_bytes()
}
fn bars() -> Vec<u8> {
    let cols = [
        (255u8, 0, 0),
        (0, 255, 0),
        (0, 0, 255),
        (255, 255, 0),
        (0, 255, 255),
        (255, 0, 255),
        (255, 255, 255),
        (255, 128, 0),
    ];
    let mut p = Vec::with_capacity(W * H * 2);
    for _y in 0..H {
        for x in 0..W {
            let (r, g, b) = cols[(x * cols.len()) / W];
            p.extend_from_slice(&rgb565_be(r, g, b));
        }
    }
    p
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1) NIHIA hello + keepalive on the DAW MIDI port (puts device in DAW mode).
    let mut daw = MidiOut::open_contains("DAW")?;
    daw.send(&[0xBF, 0x01, 0x03])?;
    let stop = Arc::new(AtomicBool::new(false));
    let stopc = stop.clone();
    let ka = thread::spawn(move || {
        let mut d = MidiOut::open_contains("DAW").unwrap();
        while !stopc.load(Ordering::Relaxed) {
            let _ = d.send(&[0xBF, 0x01, 0x03]);
            thread::sleep(Duration::from_millis(400));
        }
    });
    thread::sleep(Duration::from_millis(800));
    println!("device in DAW mode; now trying USB-bulk image framings…");

    // 2) USB bulk to the vendor interface.
    let ctx = Context::new()?;
    let h = ctx
        .open_device_with_vid_pid(VID, PID)
        .ok_or("no device / sudo?")?;
    let _ = h.set_auto_detach_kernel_driver(true);
    h.claim_interface(IFACE)?;
    let px = bars();

    // framing A: tillt MK2 header 0x84
    let mut a = vec![0x84u8, 0x00, 0x00, 0x60, 0, 0, 0, 0];
    for v in [0u16, 0, W as u16, H as u16] {
        a.extend_from_slice(&v.to_be_bytes());
    }
    a.extend_from_slice(&[0x02, 0, 0, 0, 0, 0]);
    a.extend_from_slice(&((W * H / 2) as u16).to_be_bytes());
    a.extend_from_slice(&px);
    a.extend_from_slice(&[0x02, 0, 0, 0, 0x03, 0, 0, 0, 0x40, 0, 0, 0]);

    for (label, ep, buf) in [
        ("A 0x84 → EP04", 0x04u8, &a),
        ("A 0x84 → EP03", 0x03u8, &a),
        ("raw pixels → EP04", 0x04u8, &px),
        ("raw pixels → EP03", 0x03u8, &px),
    ] {
        match h.write_bulk(ep, buf, Duration::from_secs(3)) {
            Ok(n) => println!("  {label}: wrote {n} bytes — do the SCREENS show colour bars?"),
            Err(e) => println!("  {label}: ERR {e}"),
        }
        thread::sleep(Duration::from_secs(4));
    }

    let _ = h.release_interface(IFACE);
    stop.store(true, Ordering::Relaxed);
    let _ = ka.join();
    daw.send(&[0xBF, 0x02, 0x00])?; // goodbye
    println!("done.");
    Ok(())
}
