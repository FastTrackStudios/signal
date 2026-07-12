//! Demo: paint an MM2 kit's mapping onto the S88 Light Guide (kick=red,
//! snare=yellow, hats=green, toms=orange, cymbals=blue/purple), then play a
//! groove so the mapped keys flash. Run with sudo (hidraw needs it):
//!   cargo build -p signal-drums --example kit_lights
//!   sudo ./target/debug/examples/kit_lights

use std::thread::sleep;
use std::time::Duration;

use signal_drums::DrumLightGuide;
use signal_sampler::PresetSpec;

const PRESET: &str = "/run/media/AudioHaven/Signal/Libraries/Drum Kits/\
GGD Modern and Massive 2/Presets/Metal Monster.signalpreset";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = PresetSpec::from_file(std::path::Path::new(PRESET))?;
    // (note, engine-id) per piece, from the preset's note routing.
    let pieces: Vec<(u8, String)> = spec
        .engines
        .iter()
        .map(|e| {
            let note = spec
                .note_routing
                .iter()
                .find(|nr| nr.targets.iter().any(|t| t == &e.id))
                .map(|nr| nr.note)
                .unwrap_or(0);
            (note, e.id.clone())
        })
        .collect();
    println!("kit '{}' pieces:", spec.name);
    for (n, id) in &pieces {
        println!("  note {n:>3}  {id}");
    }

    let mut lg = DrumLightGuide::open().ok_or("no keyboard / hidraw permission (run with sudo?)")?;
    lg.set_kit(&pieces);
    println!("\npainted the kit onto the keybed. Playing a groove — watch the keys flash…");

    // A simple rock groove in GM drum notes.
    let hats = 42;
    let steps: [&[u8]; 8] = [
        &[36, 42],       // 1  kick + hat
        &[42],           // &
        &[38, 42],       // 2  snare + hat
        &[42],           // &
        &[36, 42],       // 3
        &[36, 42],       // &  double kick
        &[38, 42],       // 4  snare
        &[46],           // &  open hat
    ];
    let _ = hats;
    for _bar in 0..8 {
        for step in &steps {
            for &n in *step {
                lg.note_on(n);
            }
            // decay ticks across the step
            for _ in 0..4 {
                sleep(Duration::from_millis(60));
                lg.tick();
            }
        }
    }
    lg.clear();
    println!("done.");
    Ok(())
}
