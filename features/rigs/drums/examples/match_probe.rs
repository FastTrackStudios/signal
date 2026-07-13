//! Verify the mix-import matcher: for a kit + an MM2 preset, show which of our
//! channels/buses match an MM2 strip (so its level + FX get applied). Catches
//! the id-scheme mismatch (Metal Monster `snare` vs Pound `snare-a`).
//!   cargo run -p signal-drums --example match_probe -- <kit.signalpreset> <mm2.preset>

use signal_drums::{cradle, library, mm2fx};
use signal_sampler::SamplerRig;

const KIT: &str = "kit";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let kit = std::env::args().nth(1).ok_or("usage: match_probe <kit> <mm2.preset>")?;
    let mm2 = std::env::args().nth(2).ok_or("usage: match_probe <kit> <mm2.preset>")?;

    let rig = SamplerRig::new_offline(48_000);
    signal_drums::load_preset_kit(&rig, KIT, &kit)?;
    let layout = rig.drum_mixer_layout(KIT).ok_or("no layout")?;
    let mixer = cradle::parse_mixer(&std::fs::read_to_string(&mm2)?)?;

    let (mut matched, mut total) = (0u32, 0u32);
    for eng in &layout.engines {
        for ch in &eng.channels {
            total += 1;
            let piece = library::slot_label(&eng.label);
            let target = if ch.mic_label.is_empty() { piece } else { format!("{} {}", piece, ch.mic_label) };
            match mm2fx::match_strip(&mixer, &target) {
                Some(s) => {
                    matched += 1;
                    println!("  ✓ {:<16} → '{}'  ({} fx)", target, s.name, s.fx_slots().len());
                }
                None => println!("  ✗ {:<16} (no MM2 strip)", target),
            }
        }
    }
    for bus in &layout.buses {
        match mm2fx::match_strip(&mixer, &bus.label) {
            Some(s) => println!("  ✓ BUS {:<12} → '{}'  ({} fx)", bus.label, s.name, s.fx_slots().len()),
            None => println!("  ✗ BUS {:<12} (no MM2 strip)", bus.label),
        }
    }
    println!("\nchannel match coverage: {matched}/{total}");
    Ok(())
}
