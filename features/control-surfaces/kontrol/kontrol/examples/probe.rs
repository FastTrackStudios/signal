//! Light Guide feasibility probe for the Komplete Kontrol S88 MK3.
//!
//! The empirical test. It assumes nothing works. Two families of test:
//!
//! - **USB (the real path)** — bulk-write the MK3 Light Guide frame. This is
//!   the one that *should* light LEDs if the reverse-engineered MK3 protocol is
//!   right and USB permissions allow it.
//! - **MIDI (falsification)** — fire note-ons at the Main / DAW ALSA ports to
//!   confirm they do NOT light the guide (expected: nothing happens).
//!
//! Run (from the repo root):
//!
//! ```text
//! cargo run -p kontrol --example probe                 # list devices + guided run
//! cargo run -p kontrol --example probe -- usb          # REAL: bulk-write light frames over USB
//! cargo run -p kontrol --example probe -- usb-palette  # USB: sweep the palette on one key
//! cargo run -p kontrol --example probe -- list         # list MIDI out ports + NI USB devices
//! cargo run -p kontrol --example probe -- midi-main    # falsify: note-on sweep on Main port
//! cargo run -p kontrol --example probe -- midi-daw     # falsify: note-on sweep on DAW port
//! cargo run -p kontrol --example probe -- midi-ch16    # falsify: note-on ch16 on Main
//! ```
//!
//! WATCH THE KEYBOARD during each step. If a step lights LEDs, note which path.
//! If the USB path errors on permissions, add a udev rule for VID 0x17cc (see
//! the crate report) or run with sudo for a one-off test.

use std::{thread::sleep, time::Duration};

use kontrol::{
    output::{ports, MidiOut},
    usb::{self, KontrolUsb},
    Encoding, LightColor, S88_HIGHEST_NOTE, S88_LOWEST_NOTE,
};

fn banner(msg: &str) {
    println!("\n=== {msg} ===");
}

fn open_midi(role_needle: &str) -> Option<MidiOut> {
    let full = format!("{} {}", ports::DEVICE, role_needle);
    match MidiOut::open_contains(&full).or_else(|_| MidiOut::open_contains(role_needle)) {
        Ok(out) => {
            println!("opened MIDI output port: {:?}", out.name);
            Some(out)
        }
        Err(e) => {
            println!("could NOT open MIDI port matching {role_needle:?}: {e}");
            None
        }
    }
}

fn fire(out: &mut MidiOut, bytes: &[u8], what: &str) {
    let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02X}")).collect();
    println!("  -> {:<24} [{}]  to {:?}", what, hex.join(" "), out.name);
    if let Err(e) = out.send(bytes) {
        println!("     send error: {e}");
    }
}

fn midi_sweep(out: &mut MidiOut, enc: Encoding) {
    banner(&format!("MIDI note-on sweep on {:?} ({enc:?})", out.name));
    for note in S88_LOWEST_NOTE..=S88_HIGHEST_NOTE {
        fire(
            out,
            &enc.encode(note, LightColor::GREEN),
            &format!("light note {note}"),
        );
        sleep(Duration::from_millis(30));
    }
    sleep(Duration::from_millis(500));
    for note in S88_LOWEST_NOTE..=S88_HIGHEST_NOTE {
        let _ = out.send(&enc.encode(note, LightColor::OFF));
    }
}

fn list_ni_usb() {
    banner("Native Instruments USB devices (VID 0x17cc)");
    let devs = usb::list_ni_devices();
    if devs.is_empty() {
        println!("  (none found — is the keyboard plugged in? do you have USB access?)");
    }
    for (pid, model) in devs {
        println!("  PID {pid:#06x}  {model}");
    }
}

/// Open USB, print status, or explain the failure.
fn open_usb() -> Option<KontrolUsb> {
    match KontrolUsb::open() {
        Ok(kk) => {
            println!("opened USB device: {} (PID {:#06x})", kk.model(), kk.pid);
            Some(kk)
        }
        Err(e) => {
            println!("could NOT open MK3 over USB: {e}");
            println!("  -> if this is a permissions error, add a udev rule for VID 0x17cc");
            println!("     or run once with sudo to confirm the LEDs respond.");
            None
        }
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "guided".into());

    match mode.as_str() {
        "list" => {
            banner("available MIDI OUTPUT ports");
            for p in kontrol::output_ports() {
                let mark = if p.contains(ports::DEVICE) {
                    "  <-- S88 MK3"
                } else {
                    ""
                };
                println!("  {p}{mark}");
            }
            list_ni_usb();
        }

        // THE REAL PATH: walk a lit key across the keybed over USB bulk.
        "usb" => {
            list_ni_usb();
            if let Some(mut kk) = open_usb() {
                banner("USB Light Guide: walk a green key up the keybed");
                for note in S88_LOWEST_NOTE..=S88_HIGHEST_NOTE {
                    kk.clear();
                    kk.set_key(kontrol::note_to_key(note), LightColor::GREEN);
                    match kk.flush() {
                        Ok(()) => println!("  lit key {note} (green)"),
                        Err(e) => {
                            println!("  flush failed: {e}");
                            break;
                        }
                    }
                    sleep(Duration::from_millis(80));
                }
                kk.clear();
                let _ = kk.flush();
                println!(
                    "\nIf keys lit up in sequence, the USB Light Guide WORKS. If not,\n\
                          check the reported PID vs. docs/protocol.md and the init sequence."
                );
            }
        }

        // USB palette discovery: hold one key, step the color byte.
        "usb-palette" => {
            if let Some(mut kk) = open_usb() {
                let key = kontrol::note_to_key(60); // middle C
                banner("USB palette sweep on middle C (color byte 0..=127)");
                for idx in 0u8..=127 {
                    kk.set_key(key, LightColor::from_byte(idx));
                    if let Err(e) = kk.flush() {
                        println!("  flush failed at index {idx}: {e}");
                        break;
                    }
                    println!("  color byte {idx:#04x} ({idx})");
                    sleep(Duration::from_millis(150));
                }
                kk.clear();
                let _ = kk.flush();
            }
        }

        "midi-main" => {
            if let Some(mut out) = open_midi(ports::MAIN) {
                midi_sweep(&mut out, Encoding::NoteOnCh1);
            }
        }
        "midi-daw" => {
            if let Some(mut out) = open_midi(ports::DAW) {
                midi_sweep(&mut out, Encoding::NoteOnCh1);
            }
        }
        "midi-ch16" => {
            if let Some(mut out) = open_midi(ports::MAIN) {
                midi_sweep(&mut out, Encoding::NoteOnCh16);
            }
        }

        // Guided: USB first (the real attempt), then the MIDI falsification.
        _ => {
            println!("Guided run. WATCH THE KEYBOARD.\n");
            list_ni_usb();

            banner("STEP 1 — USB bulk Light Guide (the real path)");
            if let Some(mut kk) = open_usb() {
                for _ in 0..2 {
                    for note in S88_LOWEST_NOTE..=S88_HIGHEST_NOTE {
                        kk.clear();
                        kk.set_key(kontrol::note_to_key(note), LightColor::GREEN);
                        if kk.flush().is_err() {
                            break;
                        }
                        sleep(Duration::from_millis(50));
                    }
                }
                kk.clear();
                let _ = kk.flush();
            }

            banner("STEP 2 — MIDI falsification (expected: nothing lights)");
            if let Some(mut out) = open_midi(ports::MAIN) {
                midi_sweep(&mut out, Encoding::NoteOnCh1);
            }
            if let Some(mut out) = open_midi(ports::DAW) {
                midi_sweep(&mut out, Encoding::NoteOnCh1);
            }

            println!(
                "\nInterpretation:\n\
                 - LEDs lit in STEP 1  -> USB Light Guide works; integrate the usb module.\n\
                 - LEDs lit in STEP 2  -> surprising! MK3 accepts MIDI light data; note the port.\n\
                 - Nothing lit anywhere -> see docs/protocol.md; likely needs a fuller MK3\n\
                   init handshake or a corrected key/PID mapping. USB is still the right path.\n"
            );
        }
    }
}
