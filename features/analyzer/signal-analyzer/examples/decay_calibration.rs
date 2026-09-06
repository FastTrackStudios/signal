//! Measure what the `decay` parameter actually means, per algorithm.
//!
//! `NativeReverb::decay` is a 0–1 control, but the reverberation time it
//! produces differs wildly between algorithms — Plate happens to be
//! calibrated, Hall runs about twice as long, Room's chamber variant an order
//! of magnitude short. This sweeps every algorithm and variant and reports the
//! RT60 each `decay` setting really gives, which is the data a calibration
//! curve is fitted to.
//!
//! ```text
//! cargo run -p signal-analyzer --example decay_calibration
//! cargo run -p signal-analyzer --example decay_calibration -- --tsv
//! ```

use signal_analyzer::{DecayFit, decay, generators};
use signal_fx::NativeReverb;
use signal_plugin_host::{PluginEvents, PluginInstance};

const SAMPLE_RATE: f64 = 48_000.0;
const BLOCK: usize = 512;
/// Long enough to fit a T20 on the longest tails the engines produce.
const TAIL_SECONDS: f64 = 20.0;
const WARMUP_BLOCKS: usize = 16;

/// `(algorithm index, name, variants to probe)`.
const ALGORITHMS: &[(u32, &str, &[usize])] = &[
    (0, "Room", &[0, 1, 2]),
    (1, "Hall", &[0, 1, 2]),
    (2, "Plate", &[0, 1, 2]),
    (3, "Spring", &[0, 1]),
    (4, "Cloud", &[0]),
    (5, "Bloom", &[0]),
    (6, "Shimmer", &[0]),
    (7, "Chorale", &[0]),
    (8, "Magneto", &[0]),
    (9, "NonLinear", &[0]),
    (10, "Swell", &[0]),
    (11, "Reflections", &[0]),
    (12, "Velvet", &[0]),
    (13, "FreeVerb", &[0]),
    (15, "Random", &[0]),
];

const VARIANT_NAMES: [[&str; 3]; 4] = [
    ["medium", "chamber", "studio"],
    ["concert", "cathedral", "arena"],
    ["dattorro", "lexicon", "progenitor"],
    ["mx", "vintage", ""],
];

fn variant_name(algo: u32, variant: usize) -> String {
    VARIANT_NAMES
        .get(algo as usize)
        .and_then(|v| v.get(variant))
        .filter(|s| !s.is_empty())
        .map_or_else(|| variant.to_string(), std::string::ToString::to_string)
}

/// Render an impulse through `NativeReverb` at one setting, return the RT60.
fn measure(algorithm: u32, variant: usize, decay_param: f64) -> Option<f64> {
    measure_with(algorithm, variant, decay_param, None, &[])
}

/// As `measure`, but with an explicit size and extra named overrides — used
/// to find which parameter is eating a requested decay time.
fn measure_with(
    algorithm: u32,
    variant: usize,
    decay_param: f64,
    decay_time: Option<f64>,
    extra: &[(&str, f64)],
) -> Option<f64> {
    let mut rev = NativeReverb::new(SAMPLE_RATE);
    // Algorithm and variant first — both rebuild the chain.
    rev.set_named("algorithm", f64::from(algorithm));
    rev.set_named("variant", variant as f64);
    rev.set_named("mix", 1.0);
    rev.set_named("size", 0.5);
    rev.set_named("decay", decay_param);
    for (name, v) in extra {
        rev.set_named(name, *v);
    }
    if let Some(t) = decay_time {
        rev.set_named("decay_time", t);
    }
    rev.prepare(SAMPLE_RATE, BLOCK as u32).ok()?;

    let events = PluginEvents::default();
    let (mut ol, mut or) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    let quiet = vec![0.0f32; BLOCK];
    for _ in 0..WARMUP_BLOCKS {
        rev.process_block(&quiet, &quiet, &mut ol, &mut or, &events)
            .ok()?;
    }

    let frames = (TAIL_SECONDS * SAMPLE_RATE) as usize;
    let stimulus = generators::impulse(frames);
    let mut out = Vec::with_capacity(frames);
    let mut pos = 0;
    while pos < frames {
        let n = BLOCK.min(frames - pos);
        let inb = &stimulus[pos..pos + n];
        rev.process_block(inb, inb, &mut ol[..n], &mut or[..n], &events)
            .ok()?;
        out.extend_from_slice(&ol[..n]);
        pos += n;
    }

    decay::reverb_time(&out, SAMPLE_RATE, DecayFit::T20)
}

/// The un-band-filtered tail of a burst render.
fn render_burst_raw(
    algorithm: u32,
    variant: usize,
    decay_time: Option<f64>,
    extra: &[(&str, f64)],
    probe_hz: f64,
) -> Vec<f32> {
    render_burst_inner(algorithm, variant, decay_time, extra, probe_hz, false)
}

/// Render a short sine burst at `probe_hz`, then silence — the tail that
/// follows decays at that frequency alone, with no cross-band leakage to
/// confuse the fit.
fn render_burst(
    algorithm: u32,
    variant: usize,
    decay_time: Option<f64>,
    extra: &[(&str, f64)],
    probe_hz: f64,
) -> Vec<f32> {
    render_burst_inner(algorithm, variant, decay_time, extra, probe_hz, true)
}

fn render_burst_inner(
    algorithm: u32,
    variant: usize,
    decay_time: Option<f64>,
    extra: &[(&str, f64)],
    probe_hz: f64,
    band_filter: bool,
) -> Vec<f32> {
    let mut rev = NativeReverb::new(SAMPLE_RATE);
    rev.set_named("algorithm", f64::from(algorithm));
    rev.set_named("variant", variant as f64);
    rev.set_named("mix", 1.0);
    rev.set_named("size", 0.5);
    rev.set_named("decay", 0.5);
    for (name, v) in extra {
        rev.set_named(name, *v);
    }
    if let Some(t) = decay_time {
        rev.set_named("decay_time", t);
    }
    if rev.prepare(SAMPLE_RATE, BLOCK as u32).is_err() {
        return Vec::new();
    }
    let events = PluginEvents::default();
    let (mut ol, mut or) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    let quiet = vec![0.0f32; BLOCK];
    for _ in 0..WARMUP_BLOCKS {
        let _ = rev.process_block(&quiet, &quiet, &mut ol, &mut or, &events);
    }
    let frames = (TAIL_SECONDS * SAMPLE_RATE) as usize;
    let drive = (SAMPLE_RATE * 0.25) as usize;
    // Raised-cosine fades at both ends. A hard-edged burst is broadband
    // excitation: its onset and offset transients fill the whole spectrum,
    // the reverb's LF-dominated tail then dominates the fit, and the "probe
    // frequency" measures nothing of the sort.
    let fade = (SAMPLE_RATE * 0.02) as usize;
    let stimulus: Vec<f32> = (0..frames)
        .map(|i| {
            if i >= drive {
                return 0.0;
            }
            let env = if i < fade {
                0.5 * (1.0 - (std::f64::consts::PI * i as f64 / fade as f64).cos())
            } else if i + fade > drive {
                0.5 * (1.0 - (std::f64::consts::PI * (drive - i) as f64 / fade as f64).cos())
            } else {
                1.0
            };
            ((std::f64::consts::TAU * probe_hz * i as f64 / SAMPLE_RATE).sin() * env) as f32 * 0.5
        })
        .collect();
    let mut out = Vec::with_capacity(frames);
    let mut pos = 0;
    while pos < frames {
        let n = BLOCK.min(frames - pos);
        let inb = &stimulus[pos..pos + n];
        if rev
            .process_block(inb, inb, &mut ol[..n], &mut or[..n], &events)
            .is_err()
        {
            break;
        }
        out.extend_from_slice(&ol[..n]);
        pos += n;
    }
    let mut out = out;
    let tail = out.split_off(drive);
    if !band_filter {
        return tail;
    }
    let bp = signal_analyzer::filters::Biquad::bandpass(probe_hz, 1.414, SAMPLE_RATE);
    let mut band = bp.apply(&tail);
    for _ in 0..2 {
        band = bp.apply(&band);
    }
    band
}

/// The rendered impulse response itself, for per-band analysis.
fn render_ir(
    algorithm: u32,
    variant: usize,
    decay_param: f64,
    decay_time: Option<f64>,
    extra: &[(&str, f64)],
) -> Vec<f32> {
    let mut rev = NativeReverb::new(SAMPLE_RATE);
    rev.set_named("algorithm", f64::from(algorithm));
    rev.set_named("variant", variant as f64);
    rev.set_named("mix", 1.0);
    rev.set_named("size", 0.5);
    rev.set_named("decay", decay_param);
    for (name, v) in extra {
        rev.set_named(name, *v);
    }
    if let Some(t) = decay_time {
        rev.set_named("decay_time", t);
    }
    if rev.prepare(SAMPLE_RATE, BLOCK as u32).is_err() {
        return Vec::new();
    }
    let events = PluginEvents::default();
    let (mut ol, mut or) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    let quiet = vec![0.0f32; BLOCK];
    for _ in 0..WARMUP_BLOCKS {
        let _ = rev.process_block(&quiet, &quiet, &mut ol, &mut or, &events);
    }
    let frames = (TAIL_SECONDS * SAMPLE_RATE) as usize;
    let stimulus = generators::impulse(frames);
    let mut out = Vec::with_capacity(frames);
    let mut pos = 0;
    while pos < frames {
        let n = BLOCK.min(frames - pos);
        let inb = &stimulus[pos..pos + n];
        if rev
            .process_block(inb, inb, &mut ol[..n], &mut or[..n], &events)
            .is_err()
        {
            break;
        }
        out.extend_from_slice(&ol[..n]);
        pos += n;
    }
    out
}

fn main() {
    // `--probe` isolates what eats a requested decay time: ask Room/chamber
    // for 2.5 s, then re-ask with one preset parameter changed at a time.
    if std::env::args().any(|a| a == "--probe") {
        let target = 2.466;
        let cases: &[(&str, &[(&str, f64)])] = &[
            ("baseline (size 0.5)", &[]),
            ("size 1.0", &[("size", 1.0)]),
            ("size 0.0", &[("size", 0.0)]),
            ("low_end 0.0", &[("low_end", 0.0)]),
            ("low_end 0.5", &[("low_end", 0.5)]),
            ("damping 0.0", &[("damping", 0.0)]),
            ("diffusion 1.0", &[("diffusion", 1.0)]),
            ("modulation 0.5", &[("modulation", 0.5)]),
        ];
        // Does taming the lows still shorten the LOW-BAND decay? That is the
        // physical claim; terminal tail energy is only a proxy for it.
        // Decay Rate EQ: a low shelf at 0.5x should halve the LOW band's
        // decay and leave the top alone; a high shelf likewise at the top.
        println!("decay_time honoured per engine?");
        for (algo, variant, name) in [
            (0u32, 0usize, "Room"),
            (0, 1, "Chamber"),
            (1, 0, "Hall"),
            (1, 1, "Cathedral"),
            (2, 0, "Plate"),
        ] {
            print!("  {name:<10}");
            for want in [1.0f64, 2.5, 5.0] {
                let ir = render_ir(algo, variant, 0.5, Some(want), &[]);
                match signal_analyzer::decay::reverb_time(&ir, SAMPLE_RATE, DecayFit::T20) {
                    Some(v) => print!("  ask {want:4.1} -> {v:5.2}"),
                    None => print!("  ask {want:4.1} ->     ·"),
                }
            }
            println!();
        }
        println!();

        println!("Room/chamber: per-band RT60 vs Decay Rate EQ");
        let dband = |shape: f64, freq: f64, rate: f64| -> Vec<(&'static str, f64)> {
            vec![
                ("dband1_shape", shape),
                ("dband1_freq", freq),
                ("dband1_rate", rate),
                ("dband1_q", 0.707),
            ]
        };
        for (label, extra) in [
            ("flat", vec![]),
            ("low shelf 80Hz 0.5x", dband(1.0, 80.0, 0.5)),
            ("low shelf 300Hz 0.5x", dband(1.0, 300.0, 0.5)),
            ("low shelf 2kHz 0.5x", dband(1.0, 2000.0, 0.5)),
            (
                "bell 1kHz 0.5x q4",
                vec![
                    ("dband1_shape", 0.0),
                    ("dband1_freq", 1000.0),
                    ("dband1_rate", 0.5),
                    ("dband1_q", 4.0),
                ],
            ),
            ("high shelf 3kHz 0.5x", dband(2.0, 3000.0, 0.5)),
        ] {
            let ir = render_ir(0, 1, 0.5, Some(2.5), &extra);
            let bands =
                signal_analyzer::decay::reverb_time_per_band(&ir, SAMPLE_RATE, DecayFit::T20);
            print!("  {label:<22}");
            for (hz, rt) in bands.iter().skip(1).take(6) {
                match rt {
                    Some(v) => print!("  {hz:.0}Hz {v:5.2}"),
                    None => print!("  {hz:.0}Hz    ·"),
                }
            }
            println!();
        }
        println!();

        // Decisive check: excite a narrow burst at one frequency and measure
        // the decay of the raw output. No band filtering, so no leakage.
        println!("Room/chamber: burst decay at one frequency (no band filter)");
        for (algo, variant, aname) in [(0u32, 1usize, "chamber")] {
            for (label, extra) in [
                ("flat", vec![]),
                ("lowshelf 20Hz 0.5x", dband(1.0, 20.0, 0.5)),
                ("lowshelf 300Hz 0.5x", dband(1.0, 300.0, 0.5)),
                ("highshelf 18kHz 0.5x", dband(2.0, 18000.0, 0.5)),
                ("highshelf 3kHz 0.5x", dband(2.0, 3000.0, 0.5)),
                (
                    "bell 100Hz 0.5x q0.5",
                    vec![
                        ("dband1_shape", 0.0),
                        ("dband1_freq", 100.0),
                        ("dband1_rate", 0.5),
                        ("dband1_q", 0.5),
                    ],
                ),
            ] {
                print!("  {aname:<8} {label:<22}");
                for probe_hz in [125.0, 1000.0, 4000.0] {
                    let ir = render_burst(algo, variant, Some(2.5), &extra, probe_hz);
                    match signal_analyzer::decay::reverb_time(&ir, SAMPLE_RATE, DecayFit::T20) {
                        Some(v) => print!("  {probe_hz:.0}Hz {v:5.2}"),
                        None => print!("  {probe_hz:.0}Hz    ·"),
                    }
                }
                println!();
            }
        }
        println!();

        // Where does a 4 kHz burst's tail energy actually live? A linear
        // system cannot create 125 Hz from a 4 kHz input; a time-varying one
        // (modulated allpass, rotated feedback) can smear it.
        println!("Room/chamber: octave energy of the tail from a 4 kHz burst");
        for (label, extra) in [
            ("flat", vec![]),
            ("lowshelf 300Hz 0.5x", dband(1.0, 300.0, 0.5)),
        ] {
            let tail = render_burst_raw(0, 1, Some(2.5), &extra, 4000.0);
            let bands = signal_analyzer::filters::octave_bands(&tail, SAMPLE_RATE);
            let e: Vec<f64> = bands
                .iter()
                .map(|b| b.iter().map(|&s| f64::from(s) * f64::from(s)).sum::<f64>())
                .collect();
            let total: f64 = e.iter().sum::<f64>().max(1e-30);
            print!("  {label:<22}");
            for (i, hz) in signal_analyzer::filters::OCTAVE_CENTRES_HZ
                .iter()
                .enumerate()
            {
                print!("  {hz:.0}Hz {:5.1}dB", 10.0 * (e[i] / total).log10());
            }
            println!();
        }
        println!();

        println!("Hall: per-band RT60 vs low_end");
        for (label, low_end) in [("low_end 0.5 (neutral)", 0.5), ("low_end 0.0 (tamed)", 0.0)] {
            let ir = render_ir(1, 0, 0.9, None, &[("low_end", low_end)]);
            let bands =
                signal_analyzer::decay::reverb_time_per_band(&ir, SAMPLE_RATE, DecayFit::T20);
            print!("  {label:<22}");
            for (hz, rt) in bands.iter().take(5) {
                match rt {
                    Some(v) => print!("  {hz:.0}Hz {v:5.2}"),
                    None => print!("  {hz:.0}Hz    ·"),
                }
            }
            println!();
        }
        println!();
        println!("Room/chamber, decay_time := {target} s");
        for (label, extra) in cases {
            let got = measure_with(0, 1, 0.5, Some(target), extra);
            match got {
                Some(v) => println!("  {label:<22} RT60 {v:6.3} s   ratio {:.2}", v / target),
                None => println!("  {label:<22} not measurable"),
            }
        }
        return;
    }

    let tsv = std::env::args().any(|a| a == "--tsv");
    let points: Vec<f64> = vec![0.05, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.95, 1.0];

    if tsv {
        println!("algorithm\tvariant\tdecay\trt60_s");
    }

    for &(algo, name, variants) in ALGORITHMS {
        for &variant in variants {
            let vname = variant_name(algo, variant);
            let measured: Vec<(f64, Option<f64>)> = points
                .iter()
                .map(|&d| (d, measure(algo, variant, d)))
                .collect();

            if tsv {
                for (d, rt) in &measured {
                    match rt {
                        Some(v) => println!("{name}\t{vname}\t{d}\t{v:.4}"),
                        None => println!("{name}\t{vname}\t{d}\t"),
                    }
                }
                continue;
            }

            print!("{name:<11} {vname:<11}");
            for (_, rt) in &measured {
                match rt {
                    Some(v) => print!(" {v:6.2}"),
                    None => print!("      ·"),
                }
            }
            println!();
        }
    }

    if !tsv {
        print!("{:<23}", "decay →");
        for d in &points {
            print!(" {d:6.2}");
        }
        println!("\n(RT60 in seconds; · = not measurable)");
    }
}
