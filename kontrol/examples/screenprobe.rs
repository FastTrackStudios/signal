//! Exploratory DISPLAY probe for the S88 MK3, from the tillt/KompleteSynthesia
//! protocol: the two color screens are one 1280x480 RGB565 surface, bulk-written
//! to endpoint 0x04 (Linux: on vendor interface 3) with a 0x84 command frame.
//!
//! Sends vertical colour bars so we can see whether the framing/pixel format is
//! right. Run with sudo:  sudo ./target/debug/examples/screenprobe
//!
//! Frame (per tillt `encodeImage`):
//!   {0x84,0x00,screen,0x60,0,0,0,0}
//!   rect: x,y,w,h as big-endian u16
//!   {0x02,0,0,0,0,0}
//!   u16 be: (w*h/2)   // number of 32-bit words
//!   <RGB565 pixels, big-endian>
//!   {0x02,0,0,0,0x03,0,0,0,0x40,0,0,0}

use rusb::{Context, UsbContext};
use std::time::Duration;

const VID: u16 = 0x17cc;
const PID: u16 = 0x2120;
const IFACE: u8 = 3;
const EP: u8 = 0x04;
const W: usize = 1280;
const H: usize = 480;

fn rgb565_be(r: u8, g: u8, b: u8) -> [u8; 2] {
    let v: u16 = ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | ((b as u16) >> 3);
    v.to_be_bytes()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = Context::new()?;
    let h = ctx
        .open_device_with_vid_pid(VID, PID)
        .ok_or("S88 MK3 not found / no perm (sudo?)")?;
    let _ = h.set_auto_detach_kernel_driver(true);
    h.claim_interface(IFACE)?;
    println!("claimed interface {IFACE}; sending 1280x480 colour bars to the screens…");

    // 8 vertical colour bars + a vertical brightness gradient.
    let bars = [
        (255u8, 0, 0),
        (255, 128, 0),
        (255, 255, 0),
        (0, 255, 0),
        (0, 255, 255),
        (0, 0, 255),
        (160, 0, 255),
        (255, 255, 255),
    ];
    let mut pixels = Vec::with_capacity(W * H * 2);
    for y in 0..H {
        let bright = 40 + (215 * y / H) as u32; // top dim -> bottom bright
        for x in 0..W {
            let (r, g, b) = bars[(x * bars.len()) / W];
            let scale = |c: u8| ((c as u32 * bright) / 255) as u8;
            pixels.extend_from_slice(&rgb565_be(scale(r), scale(g), scale(b)));
        }
    }

    let mut frame: Vec<u8> = Vec::with_capacity(pixels.len() + 64);
    frame.extend_from_slice(&[0x84, 0x00, 0x00, 0x60, 0x00, 0x00, 0x00, 0x00]);
    for v in [0u16, 0, W as u16, H as u16] {
        frame.extend_from_slice(&v.to_be_bytes()); // rect x,y,w,h big-endian
    }
    frame.extend_from_slice(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x00]);
    let image_longs = (W * H / 2) as u16;
    frame.extend_from_slice(&image_longs.to_be_bytes());
    frame.extend_from_slice(&pixels);
    frame.extend_from_slice(&[
        0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00,
    ]);

    match h.write_bulk(EP, &frame, Duration::from_secs(3)) {
        Ok(n) => println!(
            "wrote {n}/{} bytes to EP {EP:#04x} — do the SCREENS show colour bars?",
            frame.len()
        ),
        Err(e) => println!("bulk write to EP {EP:#04x} FAILED: {e}"),
    }
    let _ = h.release_interface(IFACE);
    Ok(())
}
