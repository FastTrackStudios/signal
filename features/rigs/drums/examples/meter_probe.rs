//! Offline probe: load an MM2 kit, hit the kick, render blocks, and read the
//! drum-mixer meters — proves the per-channel/bus/master peak plumbing.
//!   cargo run -p signal-drums --example meter_probe

use signal_sampler::{PresetSpec, SamplerRig};

const KIT: &str = "kit";
const PRESET: &str = "/run/media/AudioHaven/Signal/Libraries/Drum Kits/\
GGD Modern and Massive 2/Presets/Metal Monster.signalpreset";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = PresetSpec::from_file(std::path::Path::new(PRESET))?;
    // Kick note = first routed note whose engine id contains "kick".
    let kick = spec
        .note_routing
        .iter()
        .find(|nr| nr.targets.iter().any(|t| t.to_ascii_lowercase().contains("kick")))
        .map(|nr| nr.note)
        .or_else(|| spec.note_routing.first().map(|nr| nr.note))
        .unwrap_or(36);

    let rig = SamplerRig::new_offline(48_000);
    let ids = signal_drums::load_preset_kit(&rig, KIT, PRESET)?;
    println!("loaded {} engines; kick note = {kick}", ids.len());

    // Let preload settle (samples may decode on a background thread).
    let mut buf = vec![0.0f32; 512 * 2];
    for _ in 0..40 {
        let _ = rig.render_offline(&mut buf);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    for id in &ids {
        let (l, t) = rig.preload_progress(id);
        println!("  preload {id}: {l}/{t}");
    }

    // Hit the kick on GM ch10 and render ~0.2 s, tracking peak meters.
    rig.midi_message(signal_drums::GM_DRUM_CHANNEL, 0x90, kick, 120);
    let (mut mx_ch, mut mx_bus, mut mx_master) = (0.0f32, 0.0f32, 0.0f32);
    for _ in 0..20 {
        buf.iter_mut().for_each(|s| *s = 0.0);
        rig.render_offline(&mut buf)?;
        if let Some(m) = rig.drum_mixer_meters(KIT) {
            for i in 0..8 {
                mx_ch = mx_ch.max(m.channel_peak(i));
                mx_bus = mx_bus.max(m.bus_peak(i));
            }
            mx_master = mx_master.max(m.master_peak());
        }
    }
    println!("voices after hit: {}", rig.active_voices(KIT));
    println!("max channel peak: {mx_ch:.4}");
    println!("max bus peak:     {mx_bus:.4}");
    println!("master peak:      {mx_master:.4}");
    if mx_ch == 0.0 && mx_bus == 0.0 && mx_master == 0.0 {
        println!("\n!! all meters ZERO — plumbing gap");
        return Ok(());
    }
    println!("ok — meters are live\n");

    // ── per-piece controls ──
    // Helper: hit every piece's note once, render ~0.3 s, return per-piece max.
    let layout = rig.drum_mixer_layout(KIT).ok_or("no mixer layout")?;
    let notes: Vec<(usize, u8)> = layout
        .engines
        .iter()
        .map(|e| {
            let note = spec
                .note_routing
                .iter()
                .find(|nr| nr.targets.iter().any(|t| t == &e.label))
                .map(|nr| nr.note)
                .unwrap_or(0);
            (e.engine_idx, note)
        })
        .collect();
    let hit_all = |rig: &SamplerRig, buf: &mut Vec<f32>| -> Vec<f32> {
        for (_, note) in &notes {
            if *note > 0 {
                rig.midi_message(signal_drums::GM_DRUM_CHANNEL, 0x90, *note, 120);
            }
        }
        let mut peaks = vec![0.0f32; notes.len()];
        for _ in 0..30 {
            buf.iter_mut().for_each(|s| *s = 0.0);
            let _ = rig.render_offline(buf);
            if let Some(m) = rig.drum_mixer_meters(KIT) {
                for (i, (eidx, _)) in notes.iter().enumerate() {
                    peaks[i] = peaks[i].max(m.piece_peak(*eidx));
                }
            }
        }
        peaks
    };

    let label = |i: usize| layout.engines[i].label.clone();
    let (kick_i, snare_i) = (0usize, 1usize.min(notes.len() - 1));

    // Baseline: all pieces audible.
    let base = hit_all(&rig, &mut buf);
    println!("baseline piece peaks:");
    for (i, p) in base.iter().enumerate() {
        println!("  [{}] {:<8} {p:.4}", notes[i].0, label(i));
    }

    // Mute the kick piece → its meter should drop to ~0, others unchanged.
    rig.set_mixer_piece_mute(KIT, notes[kick_i].0, true);
    let muted = hit_all(&rig, &mut buf);
    println!("\nafter MUTE {}: kick={:.4} (was {:.4}), snare={:.4}",
        label(kick_i), muted[kick_i], base[kick_i], muted[snare_i]);
    rig.set_mixer_piece_mute(KIT, notes[kick_i].0, false);

    // Solo the snare piece → only snare audible.
    rig.set_mixer_piece_solo(KIT, notes[snare_i].0, true);
    let soloed = hit_all(&rig, &mut buf);
    println!("after SOLO {}: snare={:.4}, kick={:.4} (should be ~0)",
        label(snare_i), soloed[snare_i], soloed[kick_i]);
    rig.set_mixer_piece_solo(KIT, notes[snare_i].0, false);

    // Fader: pull kick to -18 dB → its meter should shrink ~8x.
    rig.set_mixer_piece_gain_db(KIT, notes[kick_i].0, -18.0);
    let faded = hit_all(&rig, &mut buf);
    println!("after FADER {} -18dB: kick={:.4} (was {:.4})",
        label(kick_i), faded[kick_i], base[kick_i]);
    rig.set_mixer_piece_gain_db(KIT, notes[kick_i].0, 0.0);

    let ok = muted[kick_i] < base[kick_i] * 0.1
        && soloed[kick_i] < soloed[snare_i] * 0.1
        && faded[kick_i] < base[kick_i] * 0.5;
    println!("\n{}", if ok { "PASS — piece mute/solo/fader all effective" } else { "FAIL — a piece control had no effect" });
    Ok(())
}
