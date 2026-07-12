//! End-to-end proof of the MM2 → signal-fx import: load our kit, parse an MM2
//! Cradle preset, build the kick strip's FX chain (comp + EQ) with our DSP,
//! install it on the kick channel, and confirm the rendered kick changes.
//!   cargo run -p signal-drums --example mm2_import_probe -- <MM2.preset>

use signal_drums::{cradle, mm2fx};
use signal_sampler::{FxTarget, SamplerRig};

const KIT: &str = "kit";
const PRESET: &str = "/run/media/AudioHaven/Signal/Libraries/Drum Kits/\
GGD Modern and Massive 2/Presets/Metal Monster.signalpreset";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mm2_path = std::env::args().nth(1).ok_or("usage: mm2_import_probe <MM2.preset>")?;
    let sr = 48_000;
    let rig = SamplerRig::new_offline(sr);
    signal_drums::load_preset_kit(&rig, KIT, PRESET)?;

    let mut buf = vec![0.0f32; 512 * 2];
    let warm = |rig: &SamplerRig, buf: &mut Vec<f32>| {
        for _ in 0..70 {
            buf.iter_mut().for_each(|s| *s = 0.0);
            let _ = rig.render_offline(buf);
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    };
    let hit_kick = |rig: &SamplerRig, buf: &mut Vec<f32>| -> (f32, f64) {
        rig.midi_message(signal_drums::GM_DRUM_CHANNEL, 0x90, 35, 120);
        let (mut pk, mut rms) = (0.0f32, 0.0f64);
        let mut n = 0u64;
        for _ in 0..40 {
            buf.iter_mut().for_each(|s| *s = 0.0);
            let _ = rig.render_offline(buf);
            for &s in buf.iter() {
                pk = pk.max(s.abs());
                rms += (s as f64) * (s as f64);
                n += 1;
            }
        }
        (pk, (rms / n as f64).sqrt())
    };

    warm(&rig, &mut buf);
    let (base_pk, base_rms) = hit_kick(&rig, &mut buf);
    println!("kick baseline:  peak {base_pk:.4}  rms {base_rms:.5}");

    // Find our kick channel index.
    let layout = rig.drum_mixer_layout(KIT).ok_or("no mixer layout")?;
    let kick_ch = layout
        .engines
        .iter()
        .find(|e| e.label.to_lowercase().contains("kick"))
        .and_then(|e| e.channels.first())
        .map(|c| c.channel_idx)
        .ok_or("no kick channel")?;
    println!("kick channel idx = {kick_ch}");

    // Parse MM2, take the "Kick In 1" strip's FX chain, install it.
    let text = std::fs::read_to_string(&mm2_path)?;
    let mixer = cradle::parse_mixer(&text)?;
    let kick_strip = mixer
        .strips
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case("Kick In 1"))
        .or_else(|| mixer.strips.iter().find(|s| s.name.to_lowercase().contains("kick")))
        .ok_or("no kick strip in MM2 preset")?;
    println!("MM2 kick strip '{}': {} fx slots", kick_strip.name, kick_strip.fx_slots().len());

    let mut installed = 0;
    for fx in kick_strip.fx_slots() {
        if let Some(plugin) = mm2fx::build_processor(&fx, sr as f64) {
            rig.install_mixer_plugin(KIT, FxTarget::Channel(kick_ch), plugin)?;
            println!("  installed {}", fx.fx_type);
            installed += 1;
        } else {
            println!("  skipped   {}", fx.fx_type);
        }
    }

    warm(&rig, &mut buf);
    let (fx_pk, fx_rms) = hit_kick(&rig, &mut buf);
    println!("kick w/ MM2 fx: peak {fx_pk:.4}  rms {fx_rms:.5}");

    // Verify gain plumbing: piece fader -20 dB should drop the kick ~10x.
    rig.set_mixer_piece_gain_db(KIT, 0, -20.0);
    warm(&rig, &mut buf);
    let (g_pk, _) = hit_kick(&rig, &mut buf);
    println!("kick w/ piece -20dB: peak {g_pk:.4} (was {fx_pk:.4})");
    let gain_works = g_pk < fx_pk * 0.3;
    println!("gain plumbing: {}", if gain_works { "OK" } else { "BROKEN" });

    let changed = (fx_pk - base_pk).abs() > 0.005 || (fx_rms - base_rms).abs() > 1e-4;
    println!(
        "\n{}",
        if installed > 0 && fx_pk > 0.0001 && changed {
            "PASS — MM2 kick chain built with our DSP and audibly reshapes the kick"
        } else {
            "FAIL — no audible change (mapping or install broken)"
        }
    );
    Ok(())
}
