//! How long each reverb algorithm rings at each `decay` setting — measured,
//! not modelled: an impulse through the Reverb block, the tail's Schroeder
//! decay curve, RT60 from its −5…−25 dB slope (T20 × 3).
//!
//! The calibrated algorithms (Room, Hall, Plate) map `decay` to seconds by
//! formula; the rest (Spring, Cloud, Bloom, Shimmer, …) have their own
//! feedback laws, and a preset that wants "5 seconds" needs this table.
//!
//! `cargo run --release -p fx-blocks --example rt60_table > rt60.tsv`
//! Columns: algorithm index, variant, decay, measured RT60 (s).
//!
//! Arguments narrow it: `3:1 10:0` measures only those algorithm:variant
//! pairs; `--burst` excites with 100 ms of noise instead of an impulse and
//! measures the decay from the burst's end (for engines that build up, like
//! Swell, which an impulse never fills). `--verify` goes the other way:
//! asks each engine for times through the tables and prints what it rang.

use daw::plugin::{PluginEvents, PluginInstance};
use fx_blocks::NativeReverb;

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;
const SECS: f64 = 24.0;

fn rt60(algorithm: usize, variant: usize, decay: f64, size: f64, burst: bool) -> Option<f64> {
    let mut r = NativeReverb::new(SR);
    for (n, v) in [
        ("algorithm", algorithm as f64),
        ("variant", variant as f64),
        ("decay", decay),
        ("size", size),
        ("mix", 1.0),
        ("dry", 0.0),
        ("modulation", 0.2),
    ] {
        r.set_named(n, v);
    }
    r.prepare(SR, BLOCK as u32).ok()?;
    let blocks = (SECS * SR) as usize / BLOCK;
    let mut energy = Vec::with_capacity(blocks * BLOCK);
    let burst_len = if burst { (0.1 * SR) as usize } else { 1 };
    let mut seed = 1u32;
    for b in 0..blocks {
        let mut x = vec![0.0f32; BLOCK];
        for (i, s) in x.iter_mut().enumerate() {
            if b * BLOCK + i < burst_len {
                *s = if burst {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
                } else {
                    1.0
                };
            }
        }
        let (mut l, mut rr) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
        r.process_block(&x, &x, &mut l, &mut rr, &PluginEvents::EMPTY).ok()?;
        energy.extend(l.iter().zip(&rr).map(|(a, b)| f64::from(*a).powi(2) + f64::from(*b).powi(2)));
    }
    // The decay after the excitation ends.
    let energy = energy.split_off(burst_len.min(energy.len()));
    // Schroeder backward integration.
    let mut acc = 0.0;
    let mut edc = vec![0.0; energy.len()];
    for i in (0..energy.len()).rev() {
        acc += energy[i];
        edc[i] = acc;
    }
    let total = edc[0];
    if total <= 0.0 {
        return None;
    }
    let db = |i: usize| 10.0 * (edc[i] / total).log10();
    let t5 = (0..edc.len()).find(|&i| db(i) <= -5.0)?;
    let t25 = (t5..edc.len()).find(|&i| db(i) <= -25.0)?;
    Some((t25 - t5) as f64 / SR * 3.0)
}

fn main() {
    if std::env::args().any(|a| a == "--verify") {
        verify();
        return;
    }
    // Each engine's own law, not the tables built from this output.
    reverb::calibration::measure_raw(true);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let burst = args.iter().any(|a| a == "--burst");
    let only: Vec<(usize, usize)> = args
        .iter()
        .filter_map(|a| a.split_once(':'))
        .filter_map(|(a, v)| Some((a.parse().ok()?, v.parse().ok()?)))
        .collect();
    // (algorithm, variants) — Room 0, Hall 1, Plate 2, Spring 3, Cloud 4,
    // Bloom 5, Shimmer 6, Chorale 7, Magneto 8, Swell 10, Reflections 11.
    let algos: &[(usize, &[usize])] = &[
        (0, &[0, 1, 2]),
        (1, &[0, 1, 2]),
        (2, &[0, 1, 2]),
        (3, &[0, 1]),
        (4, &[0]),
        (5, &[0]),
        (6, &[0]),
        (7, &[0]),
        (8, &[0]),
        (10, &[0]),
        (11, &[0]),
    ];
    for &(a, vs) in algos {
        for &v in vs {
            if !only.is_empty() && !only.contains(&(a, v)) {
                continue;
            }
            for step in 0..=20 {
                let d = f64::from(step) / 20.0;
                let t = rt60(a, v, d, 0.7, burst).map_or("nan".to_string(), |t| format!("{t:.3}"));
                println!("{a}\t{v}\t{d:.2}\t{t}");
            }
        }
    }
}

/// Ask for times, through the tables, and measure what each engine rings.
/// Columns: algorithm, variant, requested (s), measured (s), error (%).
fn verify() {
    let pairs: &[(usize, usize, bool)] = &[
        (0, 0, false),
        (1, 0, false),
        (2, 0, false),
        (2, 1, false),
        (3, 0, false),
        (3, 1, false),
        (4, 0, false),
        (5, 0, false),
        (6, 0, false),
        (7, 0, false),
        (10, 0, true),
    ];
    for &(a, v, burst) in pairs {
        let alg = reverb::AlgorithmType::from_index(a);
        let Some((lo, hi)) = alg.t60_range(v) else { continue };
        for frac in [0.15, 0.5, 0.85] {
            let want = lo * (hi / lo).powf(frac);
            let d = reverb::algorithm::t60_to_decay(want, lo, hi);
            let got = rt60(a, v, d, 0.7, burst).unwrap_or(f64::NAN);
            println!("{a}\t{v}\t{want:.2}\t{got:.2}\t{:+.0}", (got / want - 1.0) * 100.0);
        }
    }
}
