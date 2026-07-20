//! Benchmark drum-preset load time, broken into phases so we know what to
//! optimize. Run (release, for real numbers):
//!
//! ```sh
//! cargo run --release -p signal-sampler --example bench_drum_load
//! # or a specific preset / profile:
//! SIGNAL_BENCH_PRESET="…/Metal Monster.signalpreset" \
//! SIGNAL_BENCH_PROFILE=drum-kit \
//!   cargo run --release -p signal-sampler --example bench_drum_load
//! ```
//!
//! Phases:
//!   1. parse+build  — `load_preset` (PresetSpec parse + open N packs, parse
//!                     their embedded LibrarySpecs, build N engines).
//!   2. preload      — background FLAC decode of every sample; we poll
//!                     per-engine `preload_progress` until complete, and also
//!                     report time-to-first-engine-ready (≈ time-to-playable).

use std::time::{Duration, Instant};

use signal_sampler::{PreloadProfile, SamplerRig};

const DEFAULT_PRESET: &str = "/run/media/AudioHaven/Signal/Libraries/Drum Kits/\
GGD Modern and Massive 2/Presets/Metal Monster.signalpreset";

fn main() {
    // Surface decode warnings (e.g. "failed to preload …").
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
        .try_init();
    let preset = std::env::var("SIGNAL_BENCH_PRESET").unwrap_or_else(|_| DEFAULT_PRESET.into());
    let preset = std::path::PathBuf::from(preset);
    if !preset.exists() {
        eprintln!(
            "preset not found: {} (set SIGNAL_BENCH_PRESET)",
            preset.display()
        );
        std::process::exit(1);
    }
    let profile = std::env::var("SIGNAL_BENCH_PROFILE")
        .ok()
        .and_then(|s| PreloadProfile::from_name(&s))
        .unwrap_or(PreloadProfile::DrumKit);

    println!("preset:  {}", preset.display());
    println!("profile: {profile:?}");

    let player = SamplerRig::new_offline(48_000);
    player.set_preload_profile(profile);

    // ── Phase 1: parse + build engines ──
    let t = Instant::now();
    let ids = player.load_preset("bench", &preset).expect("load_preset");
    let build_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "\n  parse+build : {build_ms:8.1} ms   ({} engines)",
        ids.len()
    );

    // ── Phase 2: preload (background FLAC decode) ──
    let start = Instant::now();
    let mut first_ready: Option<f64> = None;
    let mut last_loaded = 0usize;
    let timeout = Duration::from_secs(180);
    loop {
        let mut loaded = 0usize;
        let mut total = 0usize;
        let mut engines_ready = 0usize;
        for id in &ids {
            let (l, t) = player.preload_progress(id);
            loaded += l;
            total += t;
            if t > 0 && l >= t {
                engines_ready += 1;
            }
        }
        if first_ready.is_none() && engines_ready >= 1 {
            first_ready = Some(start.elapsed().as_secs_f64() * 1000.0);
        }
        if total > 0 && loaded >= total {
            last_loaded = loaded;
            break;
        }
        if start.elapsed() > timeout {
            eprintln!("  preload TIMED OUT at {loaded}/{total}");
            last_loaded = loaded;
            break;
        }
        last_loaded = loaded;
        std::thread::sleep(Duration::from_millis(20));
    }
    let preload_ms = start.elapsed().as_secs_f64() * 1000.0;
    let rate = if preload_ms > 0.0 {
        last_loaded as f64 / (preload_ms / 1000.0)
    } else {
        0.0
    };

    println!(
        "  first-ready : {:8.1} ms   (time-to-first-playable engine)",
        first_ready.unwrap_or(f64::NAN)
    );
    println!("  preload     : {preload_ms:8.1} ms   ({last_loaded} samples, {rate:.0} samples/s)");
    println!("  ─────────────────────────────");
    println!("  TOTAL       : {:8.1} ms", build_ms + preload_ms);

    // Per-engine ready order + sample counts.
    println!("\n  per-engine samples:");
    for id in &ids {
        let (l, t) = player.preload_progress(id);
        println!("    {l:5}/{t:<5}  {id}");
    }

    // ── Drum mixer: structure + that close mics AND OH/Room buses get signal ──
    if let Some(layout) = player.drum_mixer_layout("bench") {
        let n_ch: usize = layout.engines.iter().map(|e| e.channels.len()).sum();
        let n_sn: usize = layout.engines.iter().map(|e| e.sends.len()).sum();
        println!(
            "\n  mixer: {} engines, {} close channels, {} sends, {} buses",
            layout.engines.len(),
            n_ch,
            n_sn,
            layout.buses.len()
        );
        for b in &layout.buses {
            println!("    bus[{}] {}", b.bus_idx, b.label);
        }

        // Play a spread of drum notes and render ~0.5 s offline.
        // `note_on` dispatches on MIDI channel 0, so map the preset there
        // first — otherwise every note is dropped (`midi_channels` miss) and
        // the signal check reads silence.
        player.set_midi_channel("bench", 0);
        for note in 35u8..=59 {
            player.note_on("bench", note, 110);
        }
        let mut block = vec![0.0f32; 512 * 2];
        let mut master_peak = 0.0f32;
        for _ in 0..((48_000 / 512) / 2) {
            let _ = player.render_offline(&mut block);
            for &s in &block {
                master_peak = master_peak.max(s.abs());
            }
        }
        if let Some(meters) = player.drum_mixer_meters("bench") {
            let mut ch_hot = 0;
            for e in &layout.engines {
                for c in &e.channels {
                    if meters.channel_peak(c.channel_idx) > 1e-4 {
                        ch_hot += 1;
                    }
                }
            }
            println!("\n  signal check (after playing notes 35..59):");
            println!("    master peak       : {master_peak:.4}");
            println!("    close channels hot: {ch_hot}/{n_ch}");
            for b in &layout.buses {
                println!(
                    "    bus {:<12} peak: {:.4}",
                    b.label,
                    meters.bus_peak(b.bus_idx)
                );
            }
        }
    } else {
        println!("\n  mixer: (preset is not send-routed / no mixer)");
    }
}
