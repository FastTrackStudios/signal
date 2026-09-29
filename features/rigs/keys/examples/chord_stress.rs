//! Stress the keys profile the way a player does: five notes at full
//! velocity, struck again and again, rendered through the WHOLE program the
//! rig builds (every lane, seeded knobs and all) at the app's buffer size.
//! Per block it records the render time against the realtime deadline, the
//! output peak, and any non-finite sample — "it overloads, buffers, then
//! it's just full signal" is one of: blocks running past the deadline
//! (dropouts), or a stage blowing up (NaN / runaway).
//!
//! ```bash
//! FTS_PACK_LIBRARY=… FTS_SAMPLED_ROOT=… \
//!     cargo run --release -p signal-keys --example chord_stress -- [lane|all] [hits] [block]
//! ```
//! Exit code 1 when a block misses the deadline or a sample is non-finite.

use signal_plugin_host::{PluginEvents, PluginMidiEvent};
use signal_sampler::node_render::RenderNode;

fn ev(on: bool, key: u8, vel: u8, offset: u32) -> PluginMidiEvent {
    use daw::service::{Channel, KeyNumber, MidiEvent, Velocity};
    PluginMidiEvent {
        offset,
        message: if on {
            MidiEvent::NoteOn { channel: Channel::new(0), key: KeyNumber::new(key), velocity: Velocity::new(vel) }
        } else {
            MidiEvent::NoteOff { channel: Channel::new(0), key: KeyNumber::new(key), velocity: Velocity::new(0) }
        },
    }
}

/// This thread's page faults so far (major, minor).
fn faults() -> (i64, i64) {
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    // RUSAGE_THREAD is Linux-only; on macOS the process counters are close
    // enough while one thread renders.
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    (u.ru_majflt as i64, u.ru_minflt as i64)
}

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let which = args.first().cloned().unwrap_or_else(|| "all".into());
    let hits: usize = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(6);
    let block: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(128);
    let sr = 48_000u32;
    let backend = signal_keys::KeysRigBackend::new();
    let program = backend.debug_profile_program().expect("profile program");
    let tree = if which == "all" {
        program
    } else {
        program.find(&which).unwrap_or_else(|| panic!("no lane {which}")).clone()
    };
    let mut rn = RenderNode::compile(&tree, sr);
    rn.prepare(f64::from(sr), block as u32);
    rn.set_tempo(120.0);
    let t0 = std::time::Instant::now();
    while signal_sampler::rig::preloads_pending() > 0 && t0.elapsed().as_secs() < 180 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let (mut l, mut r) = (vec![0.0f32; block], vec![0.0f32; block]);
    let none = PluginEvents { params: &[], midi: &[], note_expressions: &[] };
    for _ in 0..200 {
        rn.render(&mut l, &mut r, &none);
    }
    let deadline = std::time::Duration::from_secs_f64(block as f64 / f64::from(sr));
    let chord = [48u8, 52, 55, 60, 64];
    let per_hit = (sr as usize * 3 / 2) / block; // 1.5 s per hit: 1 s held
    let held = per_hit * 2 / 3;
    let (mut times, mut peak, mut nonfinite, mut over) = (Vec::new(), 0.0f32, 0usize, 0usize);
    let mut peak_at = 0usize;
    for b in 0..hits * per_hit {
        let phase = b % per_hit;
        let midi: Vec<PluginMidiEvent> = if phase == 0 {
            chord.iter().map(|&k| ev(true, k, 127, 0)).collect()
        } else if phase == held {
            chord.iter().map(|&k| ev(false, k, 0, 0)).collect()
        } else {
            Vec::new()
        };
        l.fill(0.0);
        r.fill(0.0);
        let f0 = faults();
        let t = std::time::Instant::now();
        rn.render(&mut l, &mut r, &PluginEvents { params: &[], midi: &midi, note_expressions: &[] });
        let dt = t.elapsed();
        let f1 = faults();
        times.push(dt);
        if dt > deadline {
            over += 1;
            println!(
                "  late block {:.2} ms at hit {} +{} ms: {} major / {} minor page faults{}",
                dt.as_secs_f64() * 1e3,
                b / per_hit,
                phase * block * 1000 / sr as usize,
                f1.0 - f0.0,
                f1.1 - f0.1,
                if midi.is_empty() { "" } else { " (MIDI in this block)" }
            );
        }
        for v in l.iter().chain(r.iter()) {
            if !v.is_finite() {
                nonfinite += 1;
            } else if v.abs() > peak {
                peak = v.abs();
                peak_at = b;
            }
        }
        // Real time between blocks, so streaming / background work keeps up
        // the way it does live.
        if let Some(rest) = deadline.checked_sub(dt) {
            std::thread::sleep(rest);
        }
    }
    let mut sorted = times.clone();
    sorted.sort();
    let pct = |p: f64| sorted[((sorted.len() - 1) as f64 * p) as usize].as_secs_f64() * 1e3;
    println!(
        "{which}: {} blocks of {block} (deadline {:.2} ms) — median {:.3} ms, p99 {:.3} ms, max {:.3} ms; {over} over deadline",
        times.len(),
        deadline.as_secs_f64() * 1e3,
        pct(0.5),
        pct(0.99),
        pct(1.0)
    );
    println!(
        "  peak {:.3} ({:+.1} dBFS) at {:.2} s; {nonfinite} non-finite samples",
        peak,
        20.0 * peak.max(1e-9).log10(),
        peak_at as f64 * block as f64 / f64::from(sr)
    );
    // The worst blocks: when in the hit they fall.
    let mut worst: Vec<(usize, std::time::Duration)> = times.iter().copied().enumerate().collect();
    worst.sort_by(|a, b| b.1.cmp(&a.1));
    let w: Vec<String> = worst
        .iter()
        .take(5)
        .map(|(i, d)| format!("{:.2} ms @ hit {} +{} ms", d.as_secs_f64() * 1e3, i / per_hit, (i % per_hit) * block * 1000 / sr as usize))
        .collect();
    println!("  worst: {}", w.join(", "));
    if over > 0 || nonfinite > 0 {
        std::process::exit(1);
    }
}
