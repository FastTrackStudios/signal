//! What freezing a Core saves at run time: each frozen Core snapshot's
//! Core rendered live and frozen on the offline rig, timed.
//!
//!     cargo run --release -p signal-guitar --example freeze_bench [seconds]
//!
//! Each render plays `seconds` of the DI reference (default 20) through the
//! Core alone; a short render is timed too and taken off, so the figure is
//! the processing, not opening the rig and loading models. Best of 3, the
//! two interleaved. Prints the real-time cost as a share of one core.

use std::time::Instant;

use signal_guitar::freeze::{RATE, core_only, snapshot_chain};

fn main() {
    let secs: f64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20.0);
    signal_guitar::levelling::apply_nam_calibration();
    let lib = signal_guitar::library::RigLibrary::load_or_bootstrap();
    let comp = signal_guitar::library::RigLibrary::load_compositions();
    let di = signal_sampler::nam_calibrate::DiReference::load_or_synthetic(f64::from(RATE));
    let di: Vec<f32> = di.samples.iter().map(|&s| s as f32).collect();
    let long: Vec<f32> = (0..(secs * f64::from(RATE)) as usize).map(|i| di[i % di.len()]).collect();
    let short: Vec<f32> = long[..(RATE as usize / 2)].to_vec();
    let time = |patch: &signal_sampler::rig_profile::RigPatch, input: &[f32]| -> f64 {
        let t = Instant::now();
        let _ = signal_guitar::measure::render_through(patch, RATE, input).expect("renders");
        t.elapsed().as_secs_f64()
    };
    println!("{:<22} {:>10} {:>10} {:>8}", "snapshot", "live", "frozen", "saving");
    for p in &comp.presets {
        for s in p.snapshots.iter().filter(|s| !s.frozen_nam.is_empty()) {
            let r = if s.frozen_nam2.is_empty() { s.frozen_nam.as_str() } else { s.frozen_nam2.as_str() };
            let live = core_only(&snapshot_chain(&comp, &lib.profile, &lib.drive_presets, &p.name, &s.name, None).expect("live builds"));
            let frozen = core_only(&snapshot_chain(&comp, &lib.profile, &lib.drive_presets, &p.name, &s.name, Some((&s.frozen_nam, r))).expect("frozen builds"));
            let (mut l, mut f) = (f64::MAX, f64::MAX);
            for _ in 0..3 {
                l = l.min(time(&live, &long) - time(&live, &short));
                f = f.min(time(&frozen, &long) - time(&frozen, &short));
            }
            // Seconds of processing per second of audio, as % of one core.
            let pct = |x: f64| 100.0 * x / (secs - 0.5);
            println!("{:<22} {:>9.2}% {:>9.2}% {:>7.0}%", s.name, pct(l), pct(f), 100.0 * (1.0 - f / l));
        }
    }
}
