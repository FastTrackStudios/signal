//! Measure Pro-Q 4's spectral dynamics, band by band and knob by knob.
//!
//! The spectral engine's mappings were never measured — the region's width, what
//! Density does to selectivity, how the threshold reads, what Tilt weights. They
//! were reasoned from the published behaviour and left at that, and they are now
//! the largest remaining block of error in the translated library.
//!
//! A spectral band is a *shape in frequency*, not a number: it pulls a
//! resonance down and leaves the material around it alone, and how far "around
//! it" reaches is the whole question. So the stimulus is broadband noise with a
//! resonance planted in it, and the measurement is the transfer function either
//! side of that resonance — the notch's depth at the centre and how quickly it
//! recovers.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example eq_spectral_probe -- \
//!     --plugin ~/.vst3/yabridge/"FabFilter Pro-Q 4.vst3" \
//!     [--density 50] [--range -18] [--threshold -30] [--q 1] [--tilt] [--auto]
//! ```

use realfft::RealFftPlanner;
use signal_fx::NativeEq;
use signal_plugin_host::{HostedPlugin, PluginEvents, PluginInstance};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;
const FFT: usize = 8192;
const FRAMES: usize = 32;
/// Frames rendered and thrown away before anything is read.
///
/// Twelve hops is one second, and Pro-Q's auto-threshold bands take about
/// five to settle — every reading taken with the old figure was of a plugin
/// still moving. 95 hops is a little over eight seconds, the same as the
/// library harness uses, and for the same reason: settle, then measure.
const WARMUP: usize = 95;
const BAND_FLOATS: usize = 24 * 23;
/// Where the per-band Spectral Tilt block starts in a current Pro-Q state.
const SPECTRAL_TILT_BASE: usize = 576;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1).cloned())
}
fn num(name: &str, d: f64) -> f64 {
    arg(name).and_then(|v| v.parse().ok()).unwrap_or(d)
}
fn flag(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}

fn proq_q(q: f64) -> f32 {
    ((q / 0.025).ln() / (40.0f64 / 0.025).ln()) as f32
}
/// The inverse of the measured three-segment threshold curve.
fn proq_threshold(db: f64) -> f32 {
    (if db <= -72.0 {
        (db + 90.0) / 180.0
    } else if db <= -48.0 {
        0.1 + (db + 72.0) / 240.0
    } else {
        db / 60.0 + 1.0
    }) as f32
}

/// Noise with a resonance planted at `freq`.
///
/// The resonance is what a spectral band is for; the noise is what it must not
/// touch. Both are needed at once — a tone alone cannot show selectivity and
/// noise alone has nothing to select.
fn stimulus(freq: f64, frames: usize) -> Vec<f32> {
    let flat = flag("--flat");
    let mut s = 0xA5A5_1234u64;
    let inc = std::f64::consts::TAU * freq / SR;
    let mut phase = 0.0f64;
    (0..frames)
        .map(|_| {
            s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let n = ((s >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
            // `--flat` drops the planted resonance. A spectral band on Auto
            // engages on flat noise too — that is what its threshold learning
            // means — and the shape it applies is only visible without a
            // resonance dominating the picture.
            let v = if flat {
                0.20 * n
            } else {
                0.06 * n + 0.30 * phase.sin()
            };
            phase += inc;
            v as f32
        })
        .collect()
}

fn spectrum(buf: &[f32]) -> Vec<f64> {
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(FFT);
    let window: Vec<f64> = (0..FFT)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / FFT as f64).cos())
        .collect();
    let mut mag = vec![0.0f64; FFT / 2 + 1];
    let mut used = 0;
    let mut pos = WARMUP * FFT / 2;
    while pos + FFT <= buf.len() && used < FRAMES {
        let mut frame: Vec<f64> = (0..FFT).map(|i| buf[pos + i] as f64 * window[i]).collect();
        let mut out = fft.make_output_vec();
        fft.process(&mut frame, &mut out).expect("fft");
        for (m, c) in mag.iter_mut().zip(out.iter()) {
            *m += c.norm();
        }
        used += 1;
        pos += FFT / 2;
    }
    let n = used.max(1) as f64;
    for m in &mut mag {
        *m /= n;
    }
    mag
}

fn render_plugin(p: &mut HostedPlugin, input: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while pos < input.len() {
        let n = BLOCK.min(input.len() - pos);
        let mut buf = vec![0.0f32; n * 2];
        for i in 0..n {
            buf[2 * i] = input[pos + i];
            buf[2 * i + 1] = input[pos + i];
        }
        if p.process_interleaved(&mut buf, &[], &[]).is_err() {
            return out;
        }
        out.extend((0..n).map(|i| buf[2 * i]));
        pos += n;
    }
    out
}

fn render_native(eq: &mut NativeEq, input: &[f32]) -> Vec<f32> {
    let ev = PluginEvents::default();
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while pos < input.len() {
        let n = BLOCK.min(input.len() - pos);
        let l = &input[pos..pos + n];
        let (mut ol, mut or) = (vec![0.0f32; n], vec![0.0f32; n]);
        eq.process_block(l, l, &mut ol, &mut or, &ev)
            .expect("process");
        out.extend_from_slice(&ol);
        pos += n;
    }
    out
}

/// dB transfer at `hz`, averaged over a narrow group of bins.
fn at(input: &[f64], output: &[f64], hz: f64) -> f64 {
    let bin_hz = SR / FFT as f64;
    let c = (hz / bin_hz).round() as usize;
    let (lo, hi) = (c.saturating_sub(1), (c + 1).min(input.len() - 1));
    let (mut num, mut den) = (0.0, 0.0);
    for i in lo..=hi {
        num += output[i] * output[i];
        den += input[i] * input[i];
    }
    if den <= 1e-30 {
        return 0.0;
    }
    10.0 * (num / den).log10()
}

fn main() {
    let Some(path) = arg("--plugin") else {
        eprintln!("usage: eq_spectral_probe --plugin <path> [--density D] [--range R] ...");
        std::process::exit(2);
    };
    let freq = num("--freq", 1000.0);
    let q = num("--q", 1.0);
    let range = num("--range", -18.0);
    let threshold = num("--threshold", -30.0);
    let density = num("--density", 50.0);
    let auto = flag("--auto");
    let tilt = flag("--tilt");

    let Ok(Some(mut plugin)) = HostedPlugin::load(&path) else {
        eprintln!("{path}: could not load");
        std::process::exit(1);
    };
    plugin.prepare(SR, BLOCK as u32).expect("prepare");

    let mut blob = plugin.save_state().expect("save_state");
    let count = u32::from_le_bytes(blob[16..20].try_into().unwrap()) as usize;
    let put = |blob: &mut Vec<u8>, i: usize, v: f32| {
        if i < count {
            let at = 20 + i * 4;
            blob[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
    };
    // Band 1, spectral.
    for (i, v) in [
        (0usize, 1.0f32),                     // Used
        (1, 1.0),                             // Enabled
        (2, freq.log2() as f32),              // Frequency
        (3, num("--gain", 0.0) as f32),       // Gain
        (4, proq_q(q)),                       // Q
        (5, num("--band-shape", 0.0) as f32), // Pro-Q shape: 0 Bell, 3 High Shelf
        (6, num("--slope", 2.0) as f32),      // slope
        (7, 2.0),                             // Stereo
        (8, 1.0),                             // All speakers except LFE
        (9, range as f32),                    // Dynamic Range
        (10, 1.0),                            // Dynamics Enabled
        (11, if auto { 1.0 } else { 0.0 }),   // Dynamics Auto
        (12, if auto { 1.0 } else { proq_threshold(threshold) }),
        (13, 50.0),           // Attack
        (14, 50.0),           // Release
        (20, 1.0),            // Spectral Enabled
        (21, density as f32), // Spectral Density
    ] {
        put(&mut blob, i, v);
    }
    for b in 1..24 {
        put(&mut blob, b * 23, 0.0);
    }
    put(&mut blob, 555, 1.0); // Gain Scale
    put(&mut blob, SPECTRAL_TILT_BASE, if tilt { 1.0 } else { 0.0 });
    let _ = BAND_FLOATS;
    plugin.load_state(&blob).expect("load_state");

    let mut ours = NativeEq::new(SR);
    for (n, v) in [
        ("b1_used", 1.0),
        ("b1_on", 1.0),
        ("b1_freq", freq),
        ("b1_gain", num("--gain", 0.0)),
        ("b1_q", q),
        (
            "b1_shape",
            match num("--band-shape", 0.0) as i32 {
                2 => 3.0,
                3 => 2.0,
                other => other as f64,
            },
        ),
        ("b1_slope", num("--slope", 2.0)),
        ("b1_dyn_range", range),
        ("b1_dyn_thr", threshold),
        ("b1_dyn_atk", 50.0),
        ("b1_dyn_rel", 50.0),
        ("b1_dyn_auto", if auto { 1.0 } else { 0.0 }),
        ("b1_spectral", 1.0),
        ("b1_spectral_density", density),
        ("b1_spectral_tilt", if tilt { 1.0 } else { 0.0 }),
    ] {
        ours.set_named(n, v);
    }
    ours.prepare(SR, BLOCK as u32).expect("prepare");

    let frames = (WARMUP + FRAMES + 2) * FFT / 2 + FFT;
    let input = stimulus(freq, frames);
    let a = render_plugin(&mut plugin, &input);
    let b = render_native(&mut ours, &input);

    let si = spectrum(&input);
    let sa = spectrum(&a);
    let sb = spectrum(&b);

    println!(
        "spectral bell {freq:.0} Hz Q {q}, range {range:+} dB, \
         threshold {}, density {density}{}\n",
        if auto {
            "auto".into()
        } else {
            format!("{threshold:+}")
        },
        if tilt { ", tilt" } else { "" }
    );
    println!("  off Hz      Hz     Pro-Q       ours       diff");
    // Octave offsets either side of the resonance: the notch's profile.
    let mut worst = 0.0f64;
    // Offsets in Hz as well as octaves: the spread the plugin applies is a
    // width in hertz, not a constant-Q neighbourhood, so a purely octave grid
    // steps straight over it at high frequencies and crowds it at low ones.
    if flag("--shape") {
        // A pure log sweep, for reading the *shape* of the reduction rather
        // than its profile around a resonance.
        println!("  off Hz      Hz     Pro-Q       ours       diff");
        let mut hz = freq / 8.0;
        let mut worst = 0.0f64;
        while hz <= (freq * 8.0).min(20_000.0) {
            if hz >= 20.0 {
                let ra = at(&si, &sa, hz);
                let rb = at(&si, &sb, hz);
                worst = worst.max((rb - ra).abs());
                println!(
                    "  {:>+6.0}  {hz:>7.0}  {ra:>9.2}  {rb:>9.2}  {:>9.2}",
                    hz - freq,
                    rb - ra
                );
            }
            hz *= 2.0f64.powf(1.0 / 4.0);
        }
        println!("\nworst difference {worst:.2} dB");
        return;
    }
    let mut offsets: Vec<f64> = vec![-2.0, -1.0, -0.5, -0.25, -1.0 / 6.0];
    for hz_off in [
        -160.0f64, -109.0, -60.0, -30.0, 0.0, 30.0, 60.0, 109.0, 160.0,
    ] {
        let hz = freq + hz_off;
        if hz > 20.0 {
            offsets.push((hz / freq).log2());
        }
    }
    offsets.extend([1.0 / 6.0, 0.25, 0.5, 1.0, 2.0]);
    offsets.sort_by(f64::total_cmp);
    for oct in offsets {
        let hz = freq * 2.0f64.powf(oct);
        if !(20.0..20_000.0).contains(&hz) {
            continue;
        }
        let ra = at(&si, &sa, hz);
        let rb = at(&si, &sb, hz);
        let d = rb - ra;
        if d.abs() > worst {
            worst = d.abs();
        }
        println!(
            "  {:>+6.0}  {hz:>7.0}  {ra:>9.2}  {rb:>9.2}  {d:>9.2}",
            hz - freq
        );
    }
    println!("\nworst difference {worst:.2} dB");
}
