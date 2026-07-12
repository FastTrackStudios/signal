//! Offline proof of the kit-designer swap: load a kit, replace the kick slot's
//! engine with a *different* kick engine from the library, reload via the
//! in-memory PresetSpec path, and confirm the kick still triggers.
//!   cargo run -p signal-drums --example swap_probe

use std::path::{Path, PathBuf};

use signal_sampler::{EngineSpec, PresetSpec, SamplerRig};

const KIT: &str = "kit";
const LIB: &str = "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let preset_path = PathBuf::from(LIB).join("Presets/Metal Monster.signalpreset");
    let dir = preset_path.parent().unwrap().to_path_buf();
    let spec = PresetSpec::from_file(&preset_path)?;
    let cur_kick = spec.engines.iter().find(|e| e.id == "kick").map(|e| e.engine.clone());
    println!("current kick engine: {:?}", cur_kick);

    // Find a *different* kick engine in the library (engine_type == "kick").
    let engines_dir = PathBuf::from(LIB).join("Engines");
    let mut alt_kick: Option<PathBuf> = None;
    for entry in std::fs::read_dir(&engines_dir)? {
        let p = entry?.path();
        if p.extension().and_then(|e| e.to_str()) != Some("signalengine") {
            continue;
        }
        if let Ok(es) = EngineSpec::from_file(&p) {
            if es.engine_type.eq_ignore_ascii_case("kick")
                && !p.file_name().unwrap().to_string_lossy().contains("Tama-Starclassic-Maple")
            {
                alt_kick = Some(p);
                break;
            }
        }
    }
    let alt_kick = alt_kick.ok_or("no alternate kick engine found")?;
    println!("swapping kick → {}", alt_kick.display());

    let rig = SamplerRig::new_offline(48_000);
    signal_drums::load_preset_kit(&rig, KIT, &preset_path)?;

    let hit_kick = |rig: &SamplerRig| -> f32 {
        let mut buf = vec![0.0f32; 512 * 2];
        // Warm up: let the kick's samples decode on the background thread.
        for _ in 0..60 {
            buf.iter_mut().for_each(|s| *s = 0.0);
            let _ = rig.render_offline(&mut buf);
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        // Kick = note 35 in Metal Monster.
        rig.midi_message(signal_drums::GM_DRUM_CHANNEL, 0x90, 35, 120);
        let mut pk = 0.0f32;
        for _ in 0..40 {
            buf.iter_mut().for_each(|s| *s = 0.0);
            let _ = rig.render_offline(&mut buf);
            std::thread::sleep(std::time::Duration::from_millis(6));
            pk = pk.max(buf.iter().fold(0.0, |a, s| a.max(s.abs())));
        }
        pk
    };

    let before = hit_kick(&rig);
    println!("kick peak (original): {before:.4}");

    // Build the swapped spec and reload — exactly what do_swap_piece does.
    let mut spec2 = PresetSpec::from_file(&preset_path)?;
    for e in spec2.engines.iter_mut() {
        if e.id == "kick" {
            e.engine = alt_kick.display().to_string(); // absolute path
        }
    }
    rig.load_preset_spec(KIT, &spec2, &dir)?;
    rig.set_midi_channel(KIT, signal_drums::GM_DRUM_CHANNEL);
    rig.set_default_instrument(KIT);

    let after = hit_kick(&rig);
    println!("kick peak (swapped):  {after:.4}");

    // Verify the swapped engine's name differs from the original.
    let orig_stem = cur_kick
        .as_deref()
        .map(|s| Path::new(s).file_stem().unwrap().to_string_lossy().to_string())
        .unwrap_or_default();
    let new_stem = alt_kick.file_stem().unwrap().to_string_lossy().to_string();
    println!("\n{} vs {}", orig_stem, new_stem);
    if after > 0.001 && orig_stem != new_stem {
        println!("PASS — swapped kick loads and plays");
    } else {
        println!("FAIL — swapped kick silent or unchanged");
    }
    Ok(())
}
