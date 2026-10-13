//! Ask every timed reverb for a tail length, and measure what it rings.
//!
//! The path a time-set reverb takes: `ReverbChain::set_decay_seconds` on the
//! active engine, an impulse in, the tail's Schroeder decay curve out, RT60
//! from its −5…−25 dB slope (T20 × 3) — the measure the calibration tables
//! were made with. At several sizes, because a decay that is a time should
//! not move when the room's size does.
//!
//! `cargo run --release -p reverb-dsp --example time_accuracy`
//! Columns: algorithm, variant, size, requested (s), measured (s), error (%)
//! against the time it is held to when the engine cannot reach it there.
//! Arguments: `--size 0.5` for one size only, `--only room` for one engine.

use audiocore_dsp::Processor as _;
use reverb_dsp::chain::ReverbChain;
use reverb_dsp::AlgorithmType;

const SR: f64 = 48_000.0;

/// What it rang, and the times it can reach at that size.
fn measured(algorithm: AlgorithmType, variant: usize, size: f64, seconds: f64) -> Option<(f64, (f64, f64))> {
    let mut c = ReverbChain::new();
    c.set_algorithm_variant(algorithm, variant);
    c.mix = 1.0;
    c.predelay_ms = 0.0;
    c.params.size = size;
    c.params.modulation = 0.2;
    // Diagnosis: `TA_SET=diffusion=0,modulation=0,extra_a=0` overrides.
    if let Ok(set) = std::env::var("TA_SET") {
        for kv in set.split(',') {
            let Some((k, v)) = kv.split_once('=') else { continue };
            let v: f64 = v.parse().unwrap_or(0.0);
            match k {
                "diffusion" => c.params.diffusion = v,
                "modulation" => c.params.modulation = v,
                "extra_a" => c.params.extra_a = v,
                "extra_b" => c.params.extra_b = v,
                "damping" => c.params.damping = v,
                _ => {}
            }
        }
    }
    c.update(audiocore_dsp::AudioConfig {
        sample_rate: SR,
        max_buffer_size: 512,
    });
    c.set_decay_seconds(seconds)?;
    let reach = c.decay_reach()?;
    // Let the decay's smoother arrive before the hit: a setting ramps in,
    // and a hit fired during the ramp rings the old time's tail.
    for _ in 0..(SR * 1.5) as usize / 512 {
        let (mut l, mut r) = (vec![0.0_f64; 512], vec![0.0_f64; 512]);
        c.process(&mut l, &mut r);
    }
    let total = (SR * (seconds * 3.0 + 2.0).min(40.0)) as usize;
    let mut energy = Vec::with_capacity(total);
    for block in 0..total / 512 {
        let mut l = vec![0.0_f64; 512];
        if block == 0 {
            l[0] = 1.0;
        }
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        energy.extend(l.iter().zip(&r).map(|(a, b)| a * a + b * b));
    }
    let mut acc = 0.0;
    let mut edc = vec![0.0; energy.len()];
    for i in (0..energy.len()).rev() {
        acc += energy[i];
        edc[i] = acc;
    }
    let all = edc[0];
    if all <= 0.0 {
        return None;
    }
    let db = |i: usize| 10.0 * (edc[i] / all).log10();
    let t5 = (0..edc.len()).find(|&i| db(i) <= -5.0)?;
    let t25 = (t5..edc.len()).find(|&i| db(i) <= -25.0)?;
    Some(((t25 - t5) as f64 / SR * 3.0, reach))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sizes: Vec<f64> = match args.iter().position(|a| a == "--size") {
        Some(i) => vec![args[i + 1].parse().expect("a size")],
        None => vec![0.15, 0.5, 0.85],
    };
    let only = args
        .iter()
        .position(|a| a == "--only")
        .map(|i| args[i + 1].to_lowercase());
    for &algorithm in AlgorithmType::ALL {
        if only
            .as_ref()
            .is_some_and(|o| !format!("{algorithm:?}").to_lowercase().starts_with(o))
        {
            continue;
        }
        for variant in 0..3 {
            let Some((lo, hi)) = algorithm.t60_range(variant) else {
                continue;
            };
            // Variants past the engine's own fall back to its first.
            if variant > 0 && algorithm.t60_range(variant) == algorithm.t60_range(0) {
                continue;
            }
            for &size in &sizes {
                for want in [0.25, 0.5, 1.0, 2.0, 4.0] {
                    if want < lo || want > hi {
                        continue;
                    }
                    let (got, (lo, hi)) =
                        measured(algorithm, variant, size, want).unwrap_or((f64::NAN, (0.0, 0.0)));
                    // Past the engine's reach at this size, the time it is
                    // held to is the one to have rung.
                    let held = want.clamp(lo, hi);
                    println!(
                        "{algorithm:?}\t{variant}\t{size:.2}\t{want:.2}\t{got:.2}\t{:+.0}\t{}",
                        (got / held - 1.0) * 100.0,
                        if (held - want).abs() > 1e-9 { format!("held to {held:.2}") } else { String::new() }
                    );
                }
            }
        }
    }
}
