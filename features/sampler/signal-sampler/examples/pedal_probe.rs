//! `pedal_probe` — what the sustain pedal actually does to a played note.
//!
//! Three cases, the three that matter:
//!   1. pedal DOWN, then key up  → the note must keep sounding
//!   2. pedal UP after that      → it must release
//!   3. key up, THEN pedal down  → it must NOT come back (the note is over)
//!
//! ```bash
//! cargo run -p signal-sampler --release --example pedal_probe -- [pack]
//! ```

use std::path::Path;

use signal_sampler::SamplerRig;

const PACK: &str = "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/\
Packs/LA Custom C7 Grand.signalpack";
const ID: &str = "piano";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sr = 48_000usize;
    let pack = std::env::args().nth(1).unwrap_or_else(|| PACK.to_string());
    let rig = SamplerRig::new_offline(sr as u32);
    rig.load_pack(ID, Path::new(&pack))?;
    rig.set_midi_channel(ID, 0);
    rig.set_default_instrument(ID);

    let mut buf = vec![0.0f32; 512 * 2];
    let mut settle = |rig: &SamplerRig, buf: &mut Vec<f32>, blocks: usize| -> f32 {
        let mut peak = 0.0f32;
        for _ in 0..blocks {
            buf.fill(0.0);
            let _ = rig.render_offline(buf);
            peak = peak.max(buf.iter().fold(0.0f32, |a, s| a.max(s.abs())));
            std::thread::sleep(std::time::Duration::from_micros(10_667));
        }
        peak
    };
    // Warm the streamer.
    settle(&rig, &mut buf, 60);

    let mut fail = Vec::new();

    // ── 1 + 2: pedal down while the key is held, then key up ────────────
    rig.midi_message(0, 0xB0, 64, 127); // pedal down
    rig.midi_message(0, 0x90, 60, 100); // key down
    let sounding = settle(&rig, &mut buf, 40); // ~0.4 s
    rig.midi_message(0, 0x80, 60, 0); // key UP, pedal still down
    let held_by_pedal = settle(&rig, &mut buf, 90); // ~1 s later
    rig.midi_message(0, 0xB0, 64, 0); // pedal up
    let after_pedal_up = settle(&rig, &mut buf, 160); // ~1.7 s later

    println!("1. key down, pedal down       peak {sounding:.4}");
    println!("2. key UP, pedal still down   peak {held_by_pedal:.4}   (must keep sounding)");
    println!("   pedal up                   peak {after_pedal_up:.4}   (must fall away)");
    if sounding <= 1e-3 {
        fail.push("the note never sounded".to_string());
    }
    if held_by_pedal < sounding * 0.15 {
        fail.push("the pedal did not hold the note".to_string());
    }
    if after_pedal_up > held_by_pedal * 0.5 {
        fail.push("the note did not release when the pedal came up".to_string());
    }
    settle(&rig, &mut buf, 200);

    // ── 3: key up FIRST, pedal after — the note is already over ─────────
    rig.midi_message(0, 0x90, 64, 100);
    let struck = settle(&rig, &mut buf, 40);
    rig.midi_message(0, 0x80, 64, 0); // key up: the note is released
    let releasing = settle(&rig, &mut buf, 20); // let the release run
    rig.midi_message(0, 0xB0, 64, 127); // pedal down AFTER the fact
    let after_late_pedal = settle(&rig, &mut buf, 120);
    rig.midi_message(0, 0xB0, 64, 0);

    println!("3. key up, then pedal down    struck {struck:.4} → releasing {releasing:.4} → {after_late_pedal:.4}");
    if after_late_pedal > releasing * 0.6 {
        fail.push("a late pedal resurrected a note that was already released".to_string());
    }

    if fail.is_empty() {
        println!("\nPASS — the pedal extends held notes and never resurrects released ones");
        Ok(())
    } else {
        println!("\nFAIL — {}", fail.join("; "));
        std::process::exit(1);
    }
}

