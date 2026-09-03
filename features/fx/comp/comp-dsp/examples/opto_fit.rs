//! Fit the optical cell's constants to a measured release table.
//!
//! `OptoParams` has eight numbers and they interact: the tail mix and the
//! fast time constant both move the 63% point, in opposite directions, so
//! turning one by hand undoes the other. Hand-tuning got the deep end exact
//! and left the light end three and a half times too slow.
//!
//! So fit them. Coordinate descent over the eight, scored against the
//! release times and shapes measured off the real unit — the same two
//! numbers `comp_probe::fit_timing` reads out of a capture, so the model and
//! the plugin are being compared on identical terms.
//!
//! ```sh
//! cargo run --release -p comp-dsp --example opto_fit
//! ```
//!
//! The table below is the UADx LA-2A Gray, measured through a pulsing tone
//! with a four-second quiet half — long enough for an optical release, which
//! the default 240 ms is not.

use comp_dsp::opto::{OptoCell, OptoParams};

const SR: f64 = 48_000.0;

/// depth dB, t63 ms, t90/t63.
const MEASURED: &[(f64, f64, f64)] =
    &[(3.3, 162.0, 5.98), (11.2, 61.0, 2.28), (20.7, 36.0, 1.58), (27.9, 28.0, 1.75)];

fn release_times(params: OptoParams, depth: f64) -> (f64, f64) {
    let mut cell = OptoCell::with_params(SR, params);
    for _ in 0..(SR as usize) {
        cell.process(depth);
    }
    let from = cell.gain_reduction_db();
    let (mut t63, mut t90) = (None, None);
    for i in 0..(SR * 8.0) as usize {
        let g = cell.process(0.0);
        let travelled = (from - g) / from.max(1e-9);
        let ms = i as f64 * 1000.0 / SR;
        if t63.is_none() && travelled >= 0.63 {
            t63 = Some(ms);
        }
        if t90.is_none() && travelled >= 0.90 {
            t90 = Some(ms);
            break;
        }
    }
    let a = t63.unwrap_or(8000.0);
    let b = t90.unwrap_or(8000.0);
    (a, b / a.max(1e-9))
}

/// Error against the measured table.
///
/// Times are scored as log ratios so that being 30 ms out at 28 ms counts
/// like being 170 ms out at 162 ms — a relative error, which is how a time
/// constant is heard and how the measurements are spaced.
fn error(params: OptoParams) -> f64 {
    let mut e = 0.0;
    for &(depth, want_t63, want_ratio) in MEASURED {
        let (t63, ratio) = release_times(params, depth);
        e += (t63.max(1e-6) / want_t63).ln().powi(2);
        e += 0.5 * (ratio.max(1e-6) / want_ratio).ln().powi(2);
    }
    e
}

fn main() {
    let mut best = OptoParams::default();
    let mut best_err = error(best);
    println!("start  error {best_err:.4}");

    // Each field, with the range it is allowed to take.
    type Field = (&'static str, fn(&mut OptoParams, f64), fn(&OptoParams) -> f64, f64, f64);
    let fields: &[Field] = &[
        ("attack_light_ms", |p, v| p.attack_light_ms = v, |p| p.attack_light_ms, 1.0, 60.0),
        ("attack_deep_ms", |p, v| p.attack_deep_ms = v, |p| p.attack_deep_ms, 1.0, 60.0),
        ("release_fast_light_ms", |p, v| p.release_fast_light_ms = v, |p| p.release_fast_light_ms, 5.0, 600.0),
        ("release_fast_deep_ms", |p, v| p.release_fast_deep_ms = v, |p| p.release_fast_deep_ms, 5.0, 600.0),
        ("release_tail_ms", |p, v| p.release_tail_ms = v, |p| p.release_tail_ms, 100.0, 6000.0),
        ("tail_mix_light", |p, v| p.tail_mix_light = v, |p| p.tail_mix_light, 0.0, 0.98),
        ("tail_mix_deep", |p, v| p.tail_mix_deep = v, |p| p.tail_mix_deep, 0.0, 0.98),
        ("deep_db", |p, v| p.deep_db = v, |p| p.deep_db, 6.0, 40.0),
    ];

    // Coordinate descent with a shrinking step. Enough for eight smooth
    // parameters against eight numbers; nothing here justifies more.
    let mut scale = 0.5;
    for _round in 0..40 {
        let mut improved = false;
        for (_name, set, get, lo, hi) in fields {
            let current = get(&best);
            for dir in [1.0, -1.0] {
                let step = (current.abs().max(0.05)) * scale * dir;
                let candidate_value = (current + step).clamp(*lo, *hi);
                let mut candidate = best;
                set(&mut candidate, candidate_value);
                let e = error(candidate);
                if e < best_err - 1e-9 {
                    best = candidate;
                    best_err = e;
                    improved = true;
                }
            }
        }
        if !improved {
            scale *= 0.5;
            if scale < 1e-3 {
                break;
            }
        }
    }

    println!("fitted error {best_err:.4}\n");
    println!("  depth dB   t63 plugin   t63 model   ratio plugin   ratio model");
    for &(depth, want_t63, want_ratio) in MEASURED {
        let (t63, ratio) = release_times(best, depth);
        println!("  {depth:>8.1}   {want_t63:>10.0}   {t63:>9.0}   {want_ratio:>12.2}   {ratio:>11.2}");
    }

    println!("\nOptoParams {{");
    println!("    attack_light_ms: {:.1},", best.attack_light_ms);
    println!("    attack_deep_ms: {:.1},", best.attack_deep_ms);
    println!("    release_fast_light_ms: {:.1},", best.release_fast_light_ms);
    println!("    release_fast_deep_ms: {:.1},", best.release_fast_deep_ms);
    println!("    release_tail_ms: {:.1},", best.release_tail_ms);
    println!("    tail_mix_light: {:.3},", best.tail_mix_light);
    println!("    tail_mix_deep: {:.3},", best.tail_mix_deep);
    println!("    deep_db: {:.1},", best.deep_db);
    println!("}}");
}
