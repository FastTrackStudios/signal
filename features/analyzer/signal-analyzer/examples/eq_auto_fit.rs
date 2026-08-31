//! Fit our auto-threshold trajectory to a recorded Pro-Q one, offline.
//!
//! `eq_auto_probe --traj --json <file>` writes what the plugin does from a
//! cold start: one band, unchanging noise, read once a second for twelve, at
//! three ranges and three levels. That file is the reference here, so the fit
//! runs without wine, without the plugin, and in a second rather than twenty
//! minutes — which is what makes sweeping a parameter over a grid practical
//! at all.
//!
//! The rule the plan sets is "fit across the range, never to one point": the
//! score reported is the **worst** error over every trajectory in the file,
//! not the mean, so a constant that nails one range and misses another loses
//! to one that is mediocre everywhere.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example eq_auto_fit -- \
//!     --ref traj_baseline.json
//! ```

use signal_fx::NativeEq;
use signal_plugin_host::{PluginEvents, PluginInstance};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

/// The probe's stimulus, verbatim — the fit is meaningless against different
/// noise.
fn stimulus(rms: f64, frames: usize) -> Vec<f32> {
    let mut s = 0x51DE_0042u64;
    (0..frames)
        .map(|_| {
            s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let u = ((s >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
            (rms * u * 3.0f64.sqrt()) as f32
        })
        .collect()
}

fn power_at(buf: &[f32], hz: f64) -> f64 {
    let w = std::f64::consts::TAU * hz / SR;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &x in buf {
        let s0 = x as f64 + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2) / (buf.len() as f64).powi(2)
}

fn render(eq: &mut NativeEq, input: &[f32]) -> Vec<f32> {
    let ev = PluginEvents::default();
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while pos < input.len() {
        let n = BLOCK.min(input.len() - pos);
        let l = &input[pos..pos + n];
        let (mut ol, mut or) = (vec![0.0f32; n], vec![0.0f32; n]);
        eq.process_block(l, l, &mut ol, &mut or, &ev).expect("process");
        out.extend_from_slice(&ol);
        pos += n;
    }
    out
}

/// Our trajectory for one recorded run: gain applied at the band's frequency,
/// once a second, from a cold start.
fn ours(freq: f64, q: f64, range: f64, level_db: f64, seconds: usize) -> Vec<f64> {
    let mut eq = NativeEq::new(SR);
    for (name, v) in [
        ("used", 1.0),
        ("on", 1.0),
        ("freq", freq),
        ("gain", -range),
        ("q", q),
        ("shape", 0.0),
        ("slope", 2.0),
        ("dyn_range", range),
        ("dyn_atk", 50.0),
        ("dyn_rel", 50.0),
        ("dyn_auto", 1.0),
    ] {
        eq.set_named(&format!("b1_{name}"), v);
    }
    eq.prepare(SR, BLOCK as u32).expect("prepare");

    let chunk = stimulus(10.0f64.powf(level_db / 20.0), SR as usize);
    let cut = chunk.len() * 3 / 4;
    let dry = power_at(&chunk[cut..], freq);
    (0..seconds)
        .map(|_| {
            let o = render(&mut eq, &chunk);
            if std::env::args().any(|a| a == "--live") {
                eprintln!("    live gain {:?}", eq.live_dyn_gain_db(0));
            }
            10.0 * (power_at(&o[cut..], freq) / dry).log10()
        })
        .collect()
}

fn num(name: &str, d: f64) -> f64 {
    arg(name).and_then(|v| v.parse().ok()).unwrap_or(d)
}

fn main() {
    // `--single` renders one band with parameters given on the command line
    // and prints where its gain settles, second by second. The reference file
    // covers the shape of the fit; this covers "what does THIS band do", which
    // is the question every preset diagnosis ends up asking.
    if std::env::args().any(|a| a == "--single") {
        let (freq, q) = (num("--freq", 1000.0), num("--q", 1.0));
        let range = num("--range", -12.0);
        let base = num("--base", 0.0);
        let level = num("--level", -18.8);
        let seconds = num("--seconds", 12.0) as usize;
        println!(
            "bell {freq:.0} Hz Q {q}, base {base:+} dB, range {range:+} dB, \
             auto, noise at {level:.1} dBFS"
        );
        println!("  {:>8} {:>10} {:>10}", "elapsed", "measured", "live gain");
        let mut eq = NativeEq::new(SR);
        for (name, v) in [
            ("used", 1.0),
            ("on", 1.0),
            ("freq", freq),
            ("gain", base),
            ("q", q),
            ("shape", 0.0),
            ("slope", 2.0),
            ("dyn_range", range),
            ("dyn_atk", 50.0),
            ("dyn_rel", 50.0),
            ("dyn_auto", 1.0),
        ] {
            eq.set_named(&format!("b1_{name}"), v);
        }
        eq.prepare(SR, BLOCK as u32).expect("prepare");
        let chunk = stimulus(10.0f64.powf(level / 20.0), SR as usize);
        let cut = chunk.len() * 3 / 4;
        let dry = power_at(&chunk[cut..], freq);
        for second in 1..=seconds {
            let o = render(&mut eq, &chunk);
            let measured = 10.0 * (power_at(&o[cut..], freq) / dry).log10();
            println!(
                "  {:>8} {measured:>10.2} {:>10.2}",
                format!("{second} s"),
                eq.live_dyn_gain_db(0).unwrap_or(f64::NAN)
            );
        }
        return;
    }

    let Some(path) = arg("--ref") else {
        eprintln!("usage: eq_auto_fit --ref <traj.json>");
        std::process::exit(2);
    };
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read reference")).expect("parse");
    let runs = doc["trajectories"].as_array().expect("trajectories").clone();

    println!(
        "  {:>7} {:>8} {:>4} {:>9} {:>9} {:>9}",
        "range", "level", "s", "Pro-Q", "ours", "diff"
    );
    let mut worst = 0.0f64;
    let mut worst_settled = 0.0f64;
    for run in &runs {
        let range = run["range_db"].as_f64().unwrap();
        let level = run["level_dbfs"].as_f64().unwrap();
        let freq = run["freq_hz"].as_f64().unwrap();
        let q = run["q"].as_f64().unwrap();
        let proq: Vec<f64> = run["proq_db"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let mine = ours(freq, q, range, level, proq.len());
        for (i, (r, o)) in proq.iter().zip(mine.iter()).enumerate() {
            let d = (o - r).abs();
            worst = worst.max(d);
            // "Settled" is past the eight seconds the library harness warms
            // up for — the only part of the trajectory that reaches a preset
            // measurement.
            if i + 1 >= 8 {
                worst_settled = worst_settled.max(d);
            }
            println!(
                "  {range:>7.1} {level:>8.1} {:>4} {r:>9.2} {o:>9.2} {:>9.2}",
                i + 1,
                o - r
            );
        }
    }
    println!("\nworst over the whole trajectory: {worst:.2} dB");
    println!("worst from 8 s on (what a preset measurement sees): {worst_settled:.2} dB");
}
