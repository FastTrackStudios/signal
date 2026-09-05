//! What slice of the spectrum does a dynamic band's detector actually hear?
//!
//! A dynamic band matches the plugin on a tone at every Q and misses on noise,
//! and the miss tracks the band's width — worst 0.44 dB at Q 4 against 3.56 at
//! Q 0.5. That is the signature of a detector integrating a different amount
//! of spectrum than Pro-Q's, and it is not something a curve fitted by hand
//! will find: the last two attempts adjusted the side filter's shape and moved
//! the error around without closing it.
//!
//! So measure the thing directly. A tone at the band's centre and broadband
//! noise at the same RMS are the same *level* and a different *spectrum*. Sweep
//! the input level with each and find where the band reaches half of its
//! range; the gap between those two levels is how much more (or less) of the
//! noise the detector collected than the tone. That gap **is** the detector's
//! effective noise bandwidth, in decibels, and it can be read off both engines
//! without either one's internals.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example eq_detector_probe -- \
//!     --plugin ~/.vst3/yabridge/"FabFilter Pro-Q 4.vst3" [--freq 1000]
//! ```

use signal_fx::NativeEq;
use signal_plugin_host::{HostedPlugin, PluginEvents, PluginInstance};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;
/// Long enough for the ballistics to settle at each level.
const SECONDS: f64 = 1.5;
const STRIDE: usize = 23;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1).cloned())
}
fn num(name: &str, d: f64) -> f64 {
    arg(name).and_then(|v| v.parse().ok()).unwrap_or(d)
}

fn proq_q(q: f64) -> f32 {
    (q / 0.025).log(40.0f64 / 0.025) as f32
}

/// Pro-Q's normalized threshold for a dB value.
fn proq_threshold(db: f64) -> f32 {
    (if db <= -72.0 {
        (db + 90.0) / 180.0
    } else if db <= -48.0 {
        0.1 + (db + 72.0) / 240.0
    } else {
        db / 60.0 + 1.0
    }) as f32
}

fn power_at(buf: &[f32], freq: f64) -> f64 {
    let w = std::f64::consts::TAU * freq / SR;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &x in buf {
        let s0 = f64::from(x) + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (coeff * s1).mul_add(-s2, s1.mul_add(s1, s2 * s2)) / (buf.len() as f64).powi(2)
}

fn tone(freq: f64, amplitude: f64, frames: usize) -> Vec<f32> {
    let inc = std::f64::consts::TAU * freq / SR;
    (0..frames)
        .map(|i| (amplitude * (inc * i as f64).sin()) as f32)
        .collect()
}

fn noise(rms: f64, frames: usize) -> Vec<f32> {
    let mut s = 0xD1CE_0007u64;
    (0..frames)
        .map(|_| {
            s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let u = ((s >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
            (rms * u * 3.0f64.sqrt()) as f32
        })
        .collect()
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

/// One band, static gain 0, dynamic with a fixed threshold.
fn proq_state(freq: f64, q: f64, range_db: f64, threshold_db: f64) -> Vec<f32> {
    let mut p = vec![0.0f32; 600];
    p[0] = 1.0;
    p[1] = 1.0;
    p[2] = freq.log2() as f32;
    p[3] = 0.0;
    p[4] = proq_q(q);
    p[5] = 0.0; // Bell
    p[6] = 2.0;
    p[7] = 2.0; // Stereo
    p[8] = 1.0;
    p[9] = range_db as f32;
    p[10] = 1.0;
    p[11] = 0.0; // not auto — a fixed threshold is the whole point
    p[12] = proq_threshold(threshold_db);
    p[13] = 50.0;
    p[14] = 50.0;
    for b in 1..24 {
        p[b * STRIDE] = 0.0;
    }
    if p.len() > 555 {
        p[555] = 1.0; // Gain Scale
    }
    p
}

fn native(freq: f64, q: f64, range_db: f64, threshold_db: f64) -> NativeEq {
    let mut eq = NativeEq::new(SR);
    for (n, v) in [
        ("b1_used", 1.0),
        ("b1_on", 1.0),
        ("b1_freq", freq),
        ("b1_gain", 0.0),
        ("b1_q", q),
        ("b1_shape", 0.0),
        ("b1_slope", 2.0),
        ("b1_dyn_range", range_db),
        ("b1_dyn_thr", threshold_db),
        ("b1_dyn_atk", 50.0),
        ("b1_dyn_rel", 50.0),
        ("b1_dyn_auto", 0.0),
    ] {
        eq.set_named(n, v);
    }
    eq.prepare(SR, BLOCK as u32).expect("prepare");
    eq
}

/// The input level, in dBFS, at which the applied gain first passes `target`.
///
/// Linear interpolation between the two sweep points that straddle it, so the
/// answer is not quantised to the step size. `None` if the curve never gets
/// there — which is itself a finding, not an error.
fn crossing(points: &[(f64, f64)], target: f64) -> Option<f64> {
    for pair in points.windows(2) {
        let (l0, g0) = pair[0];
        let (l1, g1) = pair[1];
        if (g0 - target) * (g1 - target) <= 0.0 && (g1 - g0).abs() > 1.0e-9 {
            return Some(l0 + (l1 - l0) * (target - g0) / (g1 - g0));
        }
    }
    None
}

fn main() {
    let Some(path) = arg("--plugin") else {
        eprintln!("usage: eq_detector_probe --plugin <path> [--freq Hz] [--range dB]");
        std::process::exit(2);
    };
    let freq = num("--freq", 1000.0);
    let range = num("--range", -12.0);
    let threshold = num("--threshold", -30.0);

    // `--ours-only` skips the plugin entirely and prints our column against
    // the reference row below. Fitting the side filter's width is a search over
    // one constant, and each pass through the plugin costs several minutes of
    // wine; ours costs seconds.
    let ours_only = std::env::args().any(|a| a == "--ours-only");
    let mut plugin = if ours_only {
        None
    } else { match HostedPlugin::load(&path) { Ok(Some(mut p)) => {
        p.prepare(SR, BLOCK as u32).expect("prepare");
        Some(p)
    } _ => {
        eprintln!("{path}: could not load");
        std::process::exit(1);
    }}};

    /// Pro-Q's effective noise bandwidth, in dB, at the Q values swept below.
    ///
    /// Measured with this probe: a -12 dB bell at 1 kHz with a -30 dBFS
    /// threshold, the level that takes it to half range read off a tone and
    /// off broadband noise, and the gap between them.
    const REFERENCE: [(f64, f64); 6] = [
        (0.2, -4.5),
        (0.5, -6.9),
        (1.0, -9.4),
        (2.0, -12.2),
        (4.0, -15.3),
        (8.0, -17.3),
    ];

    println!("bell at {freq:.0} Hz, range {range:+} dB, threshold {threshold:+} dBFS");
    println!(
        "half range is {:+.1} dB of gain; the level that reaches it is read off a\n\
         level sweep, once with a tone at the band and once with broadband noise.\n",
        range * 0.5
    );
    println!(
        "  {:>5} {:>9} {:>9} {:>9} | {:>9} {:>9} {:>9} | {:>7}",
        "Q", "tone", "noise", "Pro-Q BW", "tone", "noise", "ours BW", "diff"
    );

    let levels: Vec<f64> = (0..=20)
        .map(|i| 3.0f64.mul_add(f64::from(i), -60.0))
        .collect();
    let frames = (SR * SECONDS) as usize;
    let mut worst = 0.0f64;

    for (q, reference) in REFERENCE {
        if let Some(plugin) = plugin.as_mut() {
            let mut blob = plugin.save_state().expect("save_state");
            let count = u32::from_le_bytes(blob[16..20].try_into().unwrap()) as usize;
            let floats = proq_state(freq, q, range, threshold);
            for (i, v) in floats.iter().take(count).enumerate() {
                let at = 20 + i * 4;
                blob[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
            plugin.load_state(&blob).expect("load_state");
        }
        let mut ours = native(freq, q, range, threshold);

        // (level, applied gain) for each stimulus, on each engine.
        let mut curves: [[Vec<(f64, f64)>; 2]; 2] = Default::default();
        for (s, use_noise) in [false, true].into_iter().enumerate() {
            for &level_db in &levels {
                let amp = 10.0f64.powf(level_db / 20.0);
                let input = if use_noise {
                    noise(amp, frames)
                } else {
                    tone(freq, amp, frames)
                };
                let cut = input.len() / 2;
                let b = render_native(&mut ours, &input);
                if b.len() < input.len() {
                    break;
                }
                let dry = power_at(&input[cut..], freq);
                if let Some(plugin) = plugin.as_mut() {
                    let a = render_plugin(plugin, &input);
                    if a.len() < input.len() {
                        break;
                    }
                    curves[0][s].push((level_db, 10.0 * (power_at(&a[cut..], freq) / dry).log10()));
                }
                curves[1][s].push((level_db, 10.0 * (power_at(&b[cut..], freq) / dry).log10()));
            }
        }

        let half = range * 0.5;
        let fmt = |v: Option<f64>| v.map_or_else(|| "—".into(), |x| format!("{x:.1}"));
        let bw = |c: &[Vec<(f64, f64)>; 2]| match (crossing(&c[0], half), crossing(&c[1], half)) {
            (Some(t), Some(n)) => Some(t - n),
            _ => None,
        };
        let (rb, ob) = (
            if ours_only {
                Some(reference)
            } else {
                bw(&curves[0])
            },
            bw(&curves[1]),
        );
        let diff = match (rb, ob) {
            (Some(a), Some(b)) => {
                worst = worst.max((b - a).abs());
                format!("{:.2}", b - a)
            }
            _ => "—".into(),
        };
        println!(
            "  {q:>5} {:>9} {:>9} {:>9} | {:>9} {:>9} {:>9} | {diff:>7}",
            fmt(crossing(&curves[0][0], half)),
            fmt(crossing(&curves[0][1], half)),
            fmt(rb),
            fmt(crossing(&curves[1][0], half)),
            fmt(crossing(&curves[1][1], half)),
            fmt(ob),
        );
    }
    println!("\nworst bandwidth difference {worst:.2} dB");
    println!(
        "\nA positive Pro-Q column means the detector needs MORE level from a tone\n\
         than from noise to reach the same reduction — it is collecting energy\n\
         from a band wider than the tone. The two engines agreeing on that\n\
         number is what makes them agree on programme material."
    );
}
