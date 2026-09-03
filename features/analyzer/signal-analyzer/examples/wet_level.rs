//! What level does each reverb algorithm put out for a unit impulse?
//!
//! A scratch probe for the wet-level calibration: renders the same impulse
//! through every algorithm at a fixed decay and prints peak / RMS / energy, so
//! a per-algorithm offset (or a runaway) is visible without the plugin host.

use std::fmt::Write;

use signal_analyzer::generators;
use signal_fx::NativeReverb;
use signal_plugin_host::PluginInstance;

const SR: f64 = 48_000.0;
const BLOCK: usize = 256;

fn render(alg: f64, decay_time: f64, frames: usize, extra: &[(&str, f64)]) -> Vec<f32> {
    let mut rev = NativeReverb::new(SR);
    rev.set_named("algorithm", alg);
    rev.set_named("decay_time", decay_time);
    rev.set_named("mix", 1.0);
    for (n, v) in extra {
        rev.set_named(n, *v);
    }
    rev.prepare(SR, BLOCK as u32).expect("prepare");
    let stim = generators::impulse(frames);
    let ev = signal_plugin_host::PluginEvents::default();
    let (mut wl, mut wr) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    let quiet = vec![0.0f32; BLOCK];
    for _ in 0..16 {
        let _ = rev.process_block(&quiet, &quiet, &mut wl, &mut wr, &ev);
    }
    let mut out = Vec::with_capacity(frames);
    let mut pos = 0;
    while pos < frames {
        let n = BLOCK.min(frames - pos);
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        rev.process_block(&stim[pos..pos + n], &stim[pos..pos + n], &mut l, &mut r, &ev)
            .expect("process");
        out.extend_from_slice(&l);
        pos += n;
    }
    out
}

fn report(label: &str, ir: &[f32]) {
    let peak = ir.iter().fold(0.0f32, |a, s| a.max(s.abs()));
    let energy: f64 = ir.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
    let rms = (energy / ir.len() as f64).sqrt();
    let nan = ir.iter().filter(|s| !s.is_finite()).count();
    println!(
        "{label:<28} peak {:>9.3}  rms {:>9.5}  energy_db {:>7.2}  nonfinite {nan}",
        peak,
        rms,
        10.0 * energy.max(1e-30).log10(),
    );
}

/// Render through `ReverbChain` directly, at the reference configuration the
/// calibration constants are defined against.
///
/// Deliberately not the `NativeReverb` path: the plugin applies its own
/// defaults for every parameter, the chain applies the DSP's, and the two
/// differ enough to move an engine's level by several dB. A calibration is
/// only exact at one operating point, so the point has to be named — it is a
/// default-constructed chain, fully wet, at the algorithm's own T60 where it
/// has one. `algorithms_share_one_output_level` in reverb-dsp asserts against
/// this same construction.
fn render_chain_t60(algo: reverb_dsp::AlgorithmType, t60: f64, frames: usize) -> Vec<f64> {
    use audiocore_dsp::{AudioConfig, Processor};
    let cfg = AudioConfig {
        sample_rate: SR,
        max_buffer_size: 512,
    };
    let mut c = reverb_dsp::chain::ReverbChain::new();
    c.set_algorithm(algo);
    c.mix = 1.0;
    c.update(cfg);
    if c.decay_seconds_range().is_some() {
        c.set_decay_seconds(t60);
        c.update(cfg);
    }
    let mut out = Vec::with_capacity(frames);
    let mut pos = 0;
    while pos < frames {
        let n = 512.min(frames - pos);
        let mut l = vec![0.0f64; n];
        let mut r = vec![0.0f64; n];
        if pos == 0 {
            l[0] = 1.0;
            r[0] = 1.0;
        }
        c.process(&mut l, &mut r);
        out.extend_from_slice(&l);
        pos += n;
    }
    out
}

/// Render at an explicit T60 rather than a knob position.
fn render_t60(alg: f64, t60: f64, frames: usize) -> Option<Vec<f32>> {
    let mut rev = NativeReverb::new(SR);
    rev.set_named("algorithm", alg);
    rev.set_named("mix", 1.0);
    rev.set_named("decay_time", t60);
    rev.prepare(SR, BLOCK as u32).expect("prepare");
    let stim = generators::impulse(frames);
    let ev = signal_plugin_host::PluginEvents::default();
    let (mut wl, mut wr) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    let quiet = vec![0.0f32; BLOCK];
    for _ in 0..16 {
        let _ = rev.process_block(&quiet, &quiet, &mut wl, &mut wr, &ev);
    }
    let mut out = Vec::with_capacity(frames);
    let mut pos = 0;
    while pos < frames {
        let n = BLOCK.min(frames - pos);
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        rev.process_block(&stim[pos..pos + n], &stim[pos..pos + n], &mut l, &mut r, &ev)
            .expect("process");
        out.extend_from_slice(&l);
        pos += n;
    }
    Some(out)
}

fn main() {
    let frames = (SR * 4.0) as usize;

    // What normalization would each algorithm need to sit at the level an
    // ideal exponential decay of the same T60 produces?
    //
    //   an IR of a(t) = e^(-6.908 t / T) has energy T / 13.82
    //
    // That target is vendor-independent: it is what a room with that decay
    // time does to a unit impulse, so it is the same number Valhalla's own
    // engines should be near.
    const T60: f64 = 2.0;
    let ideal_energy = T60 / 13.8155;
    println!("== wet normalization at T60 = {T60} s (ideal energy {ideal_energy:.5}) ==");
    let long = (SR * (T60 * 3.0)) as usize;
    println!("(unity energy is the anchor; `ideal` is shown only for reference)");
    for (i, a) in reverb_dsp::AlgorithmType::ALL.iter().enumerate() {
        let ir = render_chain_t60(*a, T60, long);
        let energy: f64 = ir.iter().map(|s| s * s).sum();
        if energy <= 1e-12 {
            println!("{i:>2} {a:<12?} silent");
            continue;
        }
        // The constant currently in force, plus whatever is still missing, is
        // what the constant should become.
        let residual_db = 10.0 * energy.log10();
        println!(
            "{i:>2} {a:<12?} energy {energy:>10.5}  off by {residual_db:>7.2} dB  \
             => wet_calibration_db {:>7.2}  (vs ideal {:>6.2} dB)",
            a.wet_calibration_db() - residual_db,
            10.0 * (energy / ideal_energy).log10(),
        );
    }

    println!("\n== does the correction hold across T60? ==");
    for (i, a) in reverb_dsp::AlgorithmType::ALL.iter().enumerate() {
        let mut line = format!("{i:>2} {a:?}");
        while line.len() < 20 { line.push(' '); }
        for t in [0.5f64, 1.0, 2.0, 4.0, 8.0] {
            let n = (SR * (t * 3.0).max(1.0)) as usize;
            let Some(ir) = render_t60(i as f64, t, n) else { continue };
            let e: f64 = ir.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
            let ideal = t / 13.8155;
            if e <= 1e-12 {
                let _ = write!(line, " {t:>4.1}s:  silent");
            } else {
                let _ = write!(line, " {t:>4.1}s:{:>7.2}", 10.0 * (e / ideal).log10());
            }
        }
        println!("{line}");
    }
    println!();
    println!("== every algorithm, decay_time = 2.0 s ==");
    for (i, a) in reverb_dsp::AlgorithmType::ALL.iter().enumerate() {
        let ir = render(i as f64, 2.0, frames, &[]);
        report(&format!("{i:>2} {a:?}"), &ir);
    }

    println!("\n== NonLinear, the shipped NL-Snare Gut Punch settings ==");
    let ir = render(
        9.0,
        0.407_386_410_148_817,
        frames,
        &[
            ("variant", 0.0),
            ("size", 0.56),
            ("predelay", 125.0),
            ("diffusion", 1.0),
            ("low_end", 0.623),
            ("tone", -0.464),
            ("modulation", 0.792),
        ],
    );
    report("NL-Snare Gut Punch", &ir);

    println!("\n== predelay sweep, every algorithm (decay 2.0 s) ==");
    for (i, a) in reverb_dsp::AlgorithmType::ALL.iter().enumerate() {
        let mut line = format!("{i:>2} {a:?}");
        while line.len() < 20 { line.push(' '); }
        for pd in [0.0, 1.0, 10.0, 50.0, 125.0, 250.0] {
            let ir = render(i as f64, 2.0, frames, &[("predelay", pd)]);
            let peak = ir.iter().fold(0.0f32, |a, s| a.max(s.abs()));
            let _ = write!(line, " {pd:>5.0}ms:{peak:>8.3}");
        }
        println!("{line}");
    }

    println!("\n== NonLinear, each parameter alone ==");
    for (n, v) in [
        ("variant", 0.0),
        ("size", 0.56),
        ("predelay", 125.0),
        ("diffusion", 1.0),
        ("low_end", 0.623),
        ("tone", -0.464),
        ("modulation", 0.792),
    ] {
        let ir = render(9.0, 0.407, frames, &[(n, v)]);
        report(&format!("  +{n}={v}"), &ir);
    }
}
