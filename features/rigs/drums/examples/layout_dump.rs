//! Dump our drum-mixer layout (engines → channels/mics + sends, buses) for a
//! given kit, so the MM2 mix matcher can be aligned to the real structure.
//!   cargo run -p signal-drums --example layout_dump -- <kit.signalpreset>

use signal_sampler::SamplerRig;

const KIT: &str = "kit";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: layout_dump <kit.signalpreset>")?;
    let rig = SamplerRig::new_offline(48_000);
    signal_drums::load_preset_kit(&rig, KIT, &path)?;
    let layout = rig.drum_mixer_layout(KIT).ok_or("no layout")?;
    println!("{} engines, {} buses", layout.engines.len(), layout.buses.len());
    for e in &layout.engines {
        let ch: Vec<String> = e.channels.iter().map(|c| format!("ch{}='{}'", c.channel_idx, c.mic_label)).collect();
        println!("  engine '{}' -> [{}]", e.label, ch.join(", "));
    }
    for b in &layout.buses {
        println!("  BUS {} '{}'", b.bus_idx, b.label);
    }
    Ok(())
}
