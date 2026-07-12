//! End-to-end live-chain proof: a groove authored in **Alesis Strata Prime**
//! notes (the physical e-kit's MIDI) runs through the `DrumMapConverter` and
//! plays the **MM2** sample kit — exactly the path a plugged-in Strata takes.
//! Renders to WAV.
//!
//! ```sh
//! cargo run --release -p signal-sampler --example strata_to_mm2 -- out.wav
//! ```

use std::time::{Duration, Instant};

use midicore::{Channel, ControllerNumber, ControllerValue, DrumMap, DrumMapConverter, KeyNumber,
    MidiEvent, Velocity};
use signal_sampler::{PreloadProfile, SamplerRig};

const PRESET: &str = "/run/media/AudioHaven/Signal/Libraries/Drum Kits/\
GGD Modern and Massive 2/Presets/Metal Monster.signalpreset";
const SR: u32 = 48_000;
const CH: u8 = 9; // GM percussion (MIDI channel 10)

// ── Strata Prime note numbers (PRIME Drum Module User Guide §6.2) ──
const S_KICK: u8 = 24;
const S_SNARE: u8 = 26;
const S_HAT_BOW: u8 = 18; // openness on CC4
const S_TOM1: u8 = 38;
const S_TOM2: u8 = 35;
const S_TOM3: u8 = 31;
const S_TOM4: u8 = 33;
const S_CRASH_L: u8 = 41;

/// A groove event authored on the Strata: (quarter-note, kind).
enum Hit {
    Note(u8, u8),     // note, velocity
    HatCc(u8),        // CC4 openness value (sets the next hat's openness)
}

fn write_wav(path: &std::path::Path, samples: &[f32]) -> Result<(), Box<dyn std::error::Error>> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    let amp = (1i32 << 23) as f32 - 1.0;
    for &s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * amp) as i32)?;
    }
    w.finalize()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
        .try_init();
    let out = std::env::args().skip(1).find(|a| a != "--")
        .unwrap_or_else(|| "target/strata_to_mm2.wav".to_string());
    let preset = std::path::PathBuf::from(
        std::env::var("SIGNAL_PRESET").unwrap_or_else(|_| PRESET.into()));
    if !preset.exists() {
        eprintln!("preset not found: {}", preset.display());
        std::process::exit(1);
    }

    let rig = SamplerRig::new_offline(SR);
    rig.set_preload_profile(PreloadProfile::DrumKit);
    let ids = rig.load_preset("kit", &preset)?;
    rig.set_midi_channel("kit", CH);
    println!("loaded {} engines", ids.len());

    // wait for preload
    let start = Instant::now();
    loop {
        let (mut l, mut t) = (0usize, 0usize);
        for id in &ids { let (a, b) = rig.preload_progress(id); l += a; t += b; }
        if t > 0 && l >= t { break; }
        if start.elapsed() > Duration::from_secs(240) { eprintln!("preload timeout"); break; }
        std::thread::sleep(Duration::from_millis(20));
    }

    // ── author a 2-bar groove in STRATA notes ──
    // hats: 8th notes, alternating tight/slightly-open via CC4; backbeat snare;
    // kick on 1 & the "and" of 3; a tom fill in bar 2.
    let mut ev: Vec<(f64, Hit)> = Vec::new();
    for bar in 0..2 {
        let base = bar as f64 * 4.0;
        // crash opens bar 1
        if bar == 0 { ev.push((base, Hit::Note(S_CRASH_L, 118))); }
        // hats every 8th (open the off-beats a touch)
        for i in 0..8 {
            let t = base + i as f64 * 0.5;
            ev.push((t - 0.001, Hit::HatCc(if i % 2 == 0 { 120 } else { 70 })));
            ev.push((t, Hit::Note(S_HAT_BOW, if i % 2 == 0 { 96 } else { 82 })));
        }
        ev.push((base + 0.0, Hit::Note(S_KICK, 118)));
        ev.push((base + 2.5, Hit::Note(S_KICK, 110)));
        if bar == 0 {
            ev.push((base + 1.0, Hit::Note(S_SNARE, 120)));
            ev.push((base + 3.0, Hit::Note(S_SNARE, 122)));
        } else {
            // bar 2: fill on beats 3-4
            ev.push((base + 1.0, Hit::Note(S_SNARE, 118)));
            for (i, &n) in [S_SNARE, S_TOM1, S_TOM2, S_TOM3, S_TOM4].iter().enumerate() {
                ev.push((base + 2.0 + i as f64 * 0.25, Hit::Note(n, 112)));
            }
        }
    }

    let bpm = 120.0;
    let spb = 60.0 / bpm * SR as f64;
    let mut sched: Vec<(usize, Hit)> = ev.into_iter().map(|(qn, h)| ((qn * spb) as usize, h)).collect();
    sched.sort_by_key(|e| e.0);

    // ── run each event through the converter, render at its sample position ──
    let mut conv = DrumMapConverter::new(DrumMap::StrataPrime, DrumMap::Mm2);
    let ch = Channel::new(CH);
    let dispatch = |rig: &SamplerRig, e: &MidiEvent| match e {
        MidiEvent::NoteOn { key, velocity, .. } => rig.midi_message(CH, 0x90, key.get(), velocity.get()),
        MidiEvent::NoteOff { key, velocity, .. } => rig.midi_message(CH, 0x80, key.get(), velocity.get()),
        MidiEvent::ControlChange { controller, value, .. } => rig.midi_message(CH, 0xB0, controller.get(), value.get()),
        _ => {}
    };

    let tail = SR as usize;
    let total = sched.last().map(|e| e.0).unwrap_or(0) + tail;
    const BLK: usize = 256;
    let mut out_buf: Vec<f32> = Vec::with_capacity(total * 2);
    let mut block = vec![0.0f32; BLK * 2];
    let (mut pos, mut i, mut peak) = (0usize, 0usize, 0.0f32);
    let mut emitted = 0usize;
    while pos < total {
        while i < sched.len() && sched[i].0 < pos + BLK {
            let raw = match &sched[i].1 {
                Hit::Note(n, v) => MidiEvent::NoteOn { channel: ch, key: KeyNumber::new(*n), velocity: Velocity::new(*v) },
                Hit::HatCc(v) => MidiEvent::ControlChange { channel: ch, controller: ControllerNumber::new(4), value: ControllerValue::new(*v) },
            };
            for out in conv.convert(raw) {
                dispatch(&rig, &out);
                if matches!(out, MidiEvent::NoteOn { .. }) { emitted += 1; }
            }
            i += 1;
        }
        for s in block.iter_mut() { *s = 0.0; }
        rig.render_offline(&mut block)?;
        for &s in &block { peak = peak.max(s.abs()); }
        out_buf.extend_from_slice(&block);
        pos += BLK;
    }

    println!("converter emitted {emitted} MM2 note-ons; master peak {peak:.4}");
    let outp = std::path::Path::new(&out);
    if let Some(d) = outp.parent() { std::fs::create_dir_all(d).ok(); }
    write_wav(outp, &out_buf)?;
    println!("wrote {}", outp.display());
    if peak < 1e-3 { eprintln!("WARNING: silent output"); std::process::exit(2); }
    Ok(())
}
