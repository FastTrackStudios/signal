//! Play a real drum groove through an MM2 `.signalpreset` and write a WAV —
//! the "MM2 sounds actually load and play in our sampler" proof. Exercises
//! velocity layers + round-robins (per-hit velocity jitter) and the full
//! multi-mic mix summed to the stereo master.
//!
//! ```sh
//! cargo run --release -p signal-sampler --example render_drum_groove
//! # or pick a preset / output / bars / bpm:
//! SIGNAL_PRESET="…/Metal Monster.signalpreset" SIGNAL_BARS=4 SIGNAL_BPM=140 \
//!   cargo run --release -p signal-sampler --example render_drum_groove -- out.wav
//! ```

use std::time::{Duration, Instant};

use signal_sampler::{PreloadProfile, SamplerRig};

const DEFAULT_PRESET: &str = "/run/media/AudioHaven/Signal/Libraries/Drum Kits/\
GGD Modern and Massive 2/Presets/Metal Monster.signalpreset";
const SR: u32 = 48_000;

// GM drum notes (match the MM2 preset note_routing).
const KICK: u8 = 36;
const SNARE: u8 = 38;
const HH_CLOSED: u8 = 42;
const HH_OPEN: u8 = 46;
const CRASH: u8 = 49;
const RTOM_HI: u8 = 47;
const RTOM_LO: u8 = 45;
const FTOM: u8 = 43;

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

/// One bar of a straight rock groove as (quarter-note offset, note, velocity).
/// 8th-note hats, kick on 1 & the "and" of 3, snare on 2 & 4.
fn bar(events: &mut Vec<(f64, u8, u8)>, base_qn: f64, first_bar: bool, jitter: &mut u64) {
    // deterministic per-hit velocity jitter so round-robins + velocity layers
    // both get exercised without needing rng in the example.
    let mut vel = |center: i32| -> u8 {
        *jitter = jitter.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = ((*jitter >> 40) & 0x1f) as i32 - 16; // ±16
        (center + j).clamp(1, 127) as u8
    };

    // hats on every 8th
    for i in 0..8 {
        events.push((base_qn + i as f64 * 0.5, HH_CLOSED, vel(96)));
    }
    // open hat accent on the "and" of 4
    events.push((base_qn + 3.5, HH_OPEN, vel(110)));
    // kick: beat 1, "and" of 3
    events.push((base_qn + 0.0, KICK, vel(118)));
    events.push((base_qn + 2.5, KICK, vel(112)));
    // extra kick pickup on beat 3 for drive
    events.push((base_qn + 2.0, KICK, vel(100)));
    // snare: beats 2 and 4
    events.push((base_qn + 1.0, SNARE, vel(120)));
    events.push((base_qn + 3.0, SNARE, vel(122)));
    // crash on the very first downbeat
    if first_bar {
        events.push((base_qn + 0.0, CRASH, vel(120)));
    }
}

/// A tom fill for the last bar (replaces the backbeat second half).
fn fill(events: &mut Vec<(f64, u8, u8)>, base_qn: f64, jitter: &mut u64) {
    let mut vel = |center: i32| -> u8 {
        *jitter = jitter.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = ((*jitter >> 40) & 0x1f) as i32 - 16;
        (center + j).clamp(1, 127) as u8
    };
    for i in 0..8 {
        events.push((base_qn + i as f64 * 0.5, HH_CLOSED, vel(88)));
    }
    events.push((base_qn + 0.0, KICK, vel(115)));
    events.push((base_qn + 1.0, SNARE, vel(118)));
    // 16th-note tom roll across beats 3-4
    let toms = [SNARE, SNARE, RTOM_HI, RTOM_HI, RTOM_LO, RTOM_LO, FTOM, FTOM];
    for (i, &n) in toms.iter().enumerate() {
        events.push((base_qn + 2.0 + i as f64 * 0.25, n, vel(112 + (i as i32))));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
        .try_init();

    let preset = std::env::var("SIGNAL_PRESET").unwrap_or_else(|_| DEFAULT_PRESET.into());
    let preset = std::path::PathBuf::from(preset);
    // First positional arg that isn't the "--" separator = output path.
    let out = std::env::args()
        .skip(1)
        .find(|a| a != "--")
        .unwrap_or_else(|| "target/mm2_groove.wav".to_string());
    let bars: usize = std::env::var("SIGNAL_BARS").ok().and_then(|s| s.parse().ok()).unwrap_or(4);
    let bpm: f64 = std::env::var("SIGNAL_BPM").ok().and_then(|s| s.parse().ok()).unwrap_or(120.0);

    if !preset.exists() {
        eprintln!("preset not found: {} (set SIGNAL_PRESET)", preset.display());
        std::process::exit(1);
    }
    println!("preset: {}", preset.display());
    println!("groove: {bars} bars @ {bpm} BPM  ->  {out}");

    let rig = SamplerRig::new_offline(SR);
    rig.set_preload_profile(PreloadProfile::DrumKit);

    // ── load ──
    let t = Instant::now();
    let ids = rig.load_preset("kit", &preset)?;
    rig.set_midi_channel("kit", 0); // rig.note_on dispatches on MIDI channel 0
    println!("loaded {} engines in {:.0} ms", ids.len(), t.elapsed().as_secs_f64() * 1000.0);

    // ── wait for FLAC preload so the offline walk never misses into silence ──
    let start = Instant::now();
    let timeout = Duration::from_secs(240);
    loop {
        let (mut loaded, mut total) = (0usize, 0usize);
        for id in &ids {
            let (l, tt) = rig.preload_progress(id);
            loaded += l;
            total += tt;
        }
        if total > 0 && loaded >= total {
            println!("preloaded {loaded} samples in {:.0} ms", start.elapsed().as_secs_f64() * 1000.0);
            break;
        }
        if start.elapsed() > timeout {
            eprintln!("preload timed out at {loaded}/{total} — rendering anyway");
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    // ── build the groove event list ──
    let mut events: Vec<(f64, u8, u8)> = Vec::new();
    let mut jitter: u64 = 0x1234_5678_9abc_def0;
    for b in 0..bars {
        let base = b as f64 * 4.0;
        if b + 1 == bars && bars > 1 {
            fill(&mut events, base, &mut jitter);
        } else {
            bar(&mut events, base, b == 0, &mut jitter);
        }
    }
    // qn -> sample position
    let spb = 60.0 / bpm * SR as f64; // samples per quarter note
    let mut sched: Vec<(usize, u8, u8)> = events
        .iter()
        .map(|&(qn, n, v)| ((qn * spb) as usize, n, v))
        .collect();
    sched.sort_by_key(|e| e.0);
    println!("scheduled {} hits", sched.len());

    // ── render, firing notes at their sample positions ──
    let tail = SR as usize; // 1 s tail so cymbals ring out
    let total_samples = sched.last().map(|e| e.0).unwrap_or(0) + tail;
    const BLK: usize = 256;
    let mut out_buf: Vec<f32> = Vec::with_capacity(total_samples * 2);
    let mut block = vec![0.0f32; BLK * 2];
    let mut pos = 0usize;
    let mut ev = 0usize;
    let mut peak = 0.0f32;
    while pos < total_samples {
        // fire every event landing in [pos, pos+BLK)
        while ev < sched.len() && sched[ev].0 < pos + BLK {
            rig.note_on("kit", sched[ev].1, sched[ev].2);
            ev += 1;
        }
        for s in block.iter_mut() {
            *s = 0.0;
        }
        rig.render_offline(&mut block)?;
        for &s in &block {
            peak = peak.max(s.abs());
        }
        out_buf.extend_from_slice(&block);
        pos += BLK;
    }

    println!("rendered {:.2} s, master peak {peak:.4}", out_buf.len() as f64 / 2.0 / SR as f64);
    let outp = std::path::Path::new(&out);
    if let Some(dir) = outp.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    write_wav(outp, &out_buf)?;
    println!("wrote {}", outp.display());
    if peak < 1e-3 {
        eprintln!("WARNING: near-silent output — samples may not have loaded");
        std::process::exit(2);
    }
    Ok(())
}
