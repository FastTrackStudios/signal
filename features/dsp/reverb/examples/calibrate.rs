//! Measure every timed reverb's decay → RT60 at each size, and write the
//! tables the chain converts a time through (`src/calibration_tables.rs`).
//!
//! Each engine is handed its raw `decay` (`calibration::measure_raw`), an
//! impulse — 100 ms of noise for Swell, which an impulse never fills — goes
//! in once the setting has settled, and the tail's Schroeder decay curve
//! gives the RT60 from its −5…−25 dB slope (T20 × 3). At several sizes,
//! because size moves most engines' times: a table made at one size put a
//! spring asked for 2 s at 1.2 s in a small tank and 2.2 s in a large one.
//!
//! Only each curve's rising part is kept (a flat floor — the early cluster
//! dominating a short tail — and anything past where the engine stops
//! growing are dropped), so every row is strictly increasing and invertible.
//!
//! `cargo run --release -p reverb-dsp --example calibrate > src/calibration_tables.rs`
//! Then check it: `cargo run --release -p reverb-dsp --example time_accuracy`.

use audiocore_dsp::Processor as _;
use reverb::AlgorithmType;
use reverb::chain::ReverbChain;

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;
/// The sizes measured. 0.7 is the reference row: what a `decay` position
/// means as a time (`t60_range`), as the single-size tables had it.
const SIZES: [f64; 11] = [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
const NOMINAL: usize = 7;
/// The longest tail rendered, s: past it a time is not measured.
const LONGEST_S: f64 = 60.0;

/// Every engine whose decay is a time, its variant, and whether it wants a
/// burst to fill it.
const ENGINES: &[(AlgorithmType, usize, bool)] = &[
    (AlgorithmType::Room, 0, false),
    (AlgorithmType::Room, 1, false),
    (AlgorithmType::Room, 2, false),
    (AlgorithmType::Hall, 0, false),
    (AlgorithmType::Hall, 1, false),
    (AlgorithmType::Hall, 2, false),
    (AlgorithmType::Plate, 0, false),
    (AlgorithmType::Plate, 1, false),
    (AlgorithmType::Plate, 2, false),
    (AlgorithmType::Spring, 0, false),
    (AlgorithmType::Spring, 1, false),
    (AlgorithmType::Cloud, 0, false),
    (AlgorithmType::Bloom, 0, false),
    (AlgorithmType::Shimmer, 0, false),
    (AlgorithmType::Chorale, 0, false),
    (AlgorithmType::Swell, 0, true),
    (AlgorithmType::Random, 0, false),
];

/// The RT60 one raw setting rings, rendering `seconds` of tail.
fn rt60(algorithm: AlgorithmType, variant: usize, size: f64, decay: f64, burst: bool, seconds: f64) -> Option<f64> {
    let mut c = ReverbChain::new();
    c.set_algorithm_variant(algorithm, variant);
    c.mix = 1.0;
    c.predelay_ms = 0.0;
    c.params.size = size;
    c.params.modulation = 0.2;
    c.update(audiocore_dsp::AudioConfig {
        sample_rate: SR,
        max_buffer_size: BLOCK,
    });
    c.params.decay = decay;
    c.update_params();
    // Settled before the hit: a setting ramps in.
    for _ in 0..(SR * 1.0) as usize / BLOCK {
        let (mut l, mut r) = (vec![0.0_f64; BLOCK], vec![0.0_f64; BLOCK]);
        c.process(&mut l, &mut r);
    }
    let burst_len = if burst { (0.1 * SR) as usize } else { 1 };
    let blocks = (seconds * SR) as usize / BLOCK + 1;
    let mut energy = Vec::with_capacity(blocks * BLOCK);
    let mut seed = 1_u32;
    for b in 0..blocks {
        let mut l = vec![0.0_f64; BLOCK];
        for (i, s) in l.iter_mut().enumerate() {
            if b * BLOCK + i < burst_len {
                *s = if burst {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    f64::from(seed >> 8) / f64::from(1_u32 << 24) - 0.5
                } else {
                    1.0
                };
            }
        }
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        energy.extend(l.iter().zip(&r).map(|(a, b)| a * a + b * b));
    }
    let energy = energy.split_off(burst_len.min(energy.len()));
    let mut acc = 0.0;
    let mut edc = vec![0.0; energy.len()];
    for i in (0..energy.len()).rev() {
        acc += energy[i];
        edc[i] = acc;
    }
    let all = edc.first().copied().unwrap_or(0.0);
    if all <= 0.0 || !all.is_finite() {
        return None;
    }
    let db = |i: usize| 10.0 * (edc[i] / all).log10();
    let t5 = (0..edc.len()).find(|&i| db(i) <= -5.0)?;
    let t25 = (t5..edc.len()).find(|&i| db(i) <= -25.0)?;
    // The decay must have room to show itself: a −25 dB point in the
    // render's last fifth is the render running out, not the tail ending.
    if t25 > edc.len() * 4 / 5 {
        return None;
    }
    Some((t25 - t5) as f64 / SR * 3.0)
}

/// One engine at one size: `(decay, t60)` along its rising part.
fn row(algorithm: AlgorithmType, variant: usize, size: f64, burst: bool) -> Vec<(f64, f64)> {
    let mut points = Vec::new();
    let mut guess = 2.0_f64;
    // Every 0.025: a coarser grid strides over a knee (Cloud's tail goes
    // from its tapped line's floor to a real decay between 0.25 and 0.35).
    for step in 0..=40 {
        let decay = (f64::from(step) * 0.025).min(0.98);
        let mut seconds = (guess * 1.6 + 2.0).clamp(4.0, LONGEST_S);
        let t60 = loop {
            match rt60(algorithm, variant, size, decay, burst, seconds) {
                Some(t) => break Some(t),
                None if seconds < LONGEST_S => seconds = (seconds * 2.0).min(LONGEST_S),
                None => break None,
            }
        };
        let Some(t60) = t60 else { break };
        guess = t60;
        points.push((decay, t60));
    }
    // A point that does not ring longer than the last kept one is
    // skipped — a dip in a feedback law's curve, or the floor — and the
    // curve goes on from the next that does; on the floor itself, the later
    // setting stands for it.
    let mut kept: Vec<(f64, f64)> = Vec::new();
    for (decay, t60) in points {
        match kept.last() {
            Some(&(_, last)) if t60 <= last * 1.005 => {
                if kept.len() == 1 {
                    kept[0] = (decay, t60);
                }
            }
            _ => kept.push((decay, t60)),
        }
    }
    kept
}

fn name(algorithm: AlgorithmType, variant: usize) -> String {
    format!("{algorithm:?}_{variant}").to_uppercase()
}

fn main() {
    reverb::calibration::measure_raw(true);
    let jobs: Vec<(usize, usize)> = (0..ENGINES.len())
        .flat_map(|e| (0..SIZES.len()).map(move |s| (e, s)))
        .collect();
    let rows: Vec<((usize, usize), Vec<(f64, f64)>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|&(e, s)| {
                scope.spawn(move || {
                    let (algorithm, variant, burst) = ENGINES[e];
                    ((e, s), row(algorithm, variant, SIZES[s], burst))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("a measurement")).collect()
    });

    println!("// Generated by `cargo run --release -p reverb-dsp --example calibrate`.");
    println!("// Do not edit: re-measure after changing an engine's decay law.");
    println!();
    let fmt = |v: &[f64]| v.iter().map(|x| format!("{x:.3}")).collect::<Vec<_>>().join(", ");
    for (e, &(algorithm, variant, burst)) in ENGINES.iter().enumerate() {
        let n = name(algorithm, variant);
        println!(
            "/// {algorithm:?} (variant {variant}){}, at sizes {SIZES:?}.",
            if burst { ", measured from a burst" } else { "" }
        );
        println!("const {n}: SizedTable = SizedTable {{");
        println!("    sizes: &[{}],", fmt(&SIZES));
        println!("    nominal: {NOMINAL},");
        println!("    rows: &[");
        for s in 0..SIZES.len() {
            let points = &rows.iter().find(|(k, _)| *k == (e, s)).expect("measured").1;
            let decay: Vec<f64> = points.iter().map(|p| p.0).collect();
            let t60: Vec<f64> = points.iter().map(|p| p.1).collect();
            println!("        // size {:.2}: {:.2} s … {:.2} s", SIZES[s], t60.first().copied().unwrap_or(0.0), t60.last().copied().unwrap_or(0.0));
            println!("        DecayTable {{");
            println!("            decay: &[{}],", fmt(&decay));
            println!("            t60: &[{}],", fmt(&t60));
            println!("        }},");
        }
        println!("    ],");
        println!("}};");
        println!();
    }
    println!("/// The measured curves for `algorithm` / `variant`; `None` for an");
    println!("/// engine whose decay is not a tail length (a convolution's IR,");
    println!("/// Magneto's heads, Non-Linear's window, the feedback-only engines).");
    println!("#[must_use]");
    println!("pub const fn table(algorithm: AlgorithmType, variant: usize) -> Option<&'static SizedTable> {{");
    println!("    match (algorithm, variant) {{");
    for &(algorithm, variant, _) in ENGINES {
        // An engine with one variant answers for any.
        let only = ENGINES.iter().filter(|(a, _, _)| *a == algorithm).count() == 1;
        let v = if only { "_".to_owned() } else { variant.to_string() };
        println!("        (AlgorithmType::{algorithm:?}, {v}) => Some(&{}),", name(algorithm, variant));
    }
    println!("        _ => None,");
    println!("    }}");
    println!("}}");
    println!();
    println!("/// Every engine measured, for the tests.");
    println!("#[cfg(test)]");
    println!("pub const MEASURED: &[(AlgorithmType, usize)] = &[");
    for &(algorithm, variant, _) in ENGINES {
        println!("    (AlgorithmType::{algorithm:?}, {variant}),");
    }
    println!("];");
}
