//! What every chorus engine actually does: delay range and peak pitch
//! deviation per depth and rate, the wet's frequency response, its level,
//! and CPU per sample.
//! `cargo run --release -p chorus-dsp --example chorus_measure [pitch|tone|level|cpu]`

// A measurement tool run by hand, not shipped code: the workspace's
// per-sample-DSP lints (casts, indexing, arithmetic) are noise here.
#![allow(
    clippy::allow_attributes,
    clippy::blanket_clippy_restriction_lints,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    reason = "measurement tool"
)]

use audiocore_dsp::{AudioConfig, Processor};
use chorus_dsp::chain::ChorusChain;
use chorus_dsp::engine::{EffectType, EngineType};
use std::f64::consts::PI;

const SR: f64 = 48_000.0;

fn chain(e: EngineType, effect: EffectType) -> ChorusChain {
    let mut c = ChorusChain::new();
    c.set_engine(e);
    c.effect_type = effect;
    c.update(AudioConfig {
        sample_rate: SR,
        max_buffer_size: 64,
    });
    c
}

fn pitch() {
    for effect in [EffectType::Chorus, EffectType::Flanger, EffectType::Vibrato] {
        println!("== {effect:?}: delay range ms / peak ±cents, depth .25 .5 .75 1 ==");
        for e in EngineType::ALL {
            for rate in [0.5, 1.0, 3.0, 10.0] {
                let mut line = format!("{:10} {rate:>4} Hz:", e.name());
                for depth in [0.25, 0.5, 0.75, 1.0] {
                    let mut c = chain(e, effect);
                    c.rate_hz = rate;
                    c.depth = depth;
                    let n = (SR * (2.0 / rate).max(1.0) * 1.5) as usize;
                    let mut d = Vec::with_capacity(n);
                    let (mut l, mut r) = ([0.0; 1], [0.0; 1]);
                    for _ in 0..n {
                        c.process(&mut l, &mut r);
                        d.push(c.delay_ms());
                    }
                    let d = &d[n / 3..];
                    let (lo, hi) = d
                        .iter()
                        .fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
                    let step = 48;
                    let cents = (step..d.len())
                        .step_by(step)
                        .map(|i| {
                            (1200.0
                                * (1.0 - (d[i] - d[i - step]) * 0.001 * SR / step as f64).log2())
                            .abs()
                        })
                        .fold(0.0, f64::max);
                    line += &format!("  {lo:5.2}-{hi:5.2} ±{cents:3.0}");
                }
                println!("{line}");
            }
        }
    }
}

fn tone() {
    println!("== wet response, dB re input, depth 0.3 rate 0.3, mono width; colour 0 / .5 / 1 ==");
    println!(
        "{:16} {:>6}{:>6}{:>6}{:>6}{:>6}{:>6}{:>6}{:>6}{:>6}",
        "", 50, 200, 1000, 2000, 4000, 6000, 8000, 12000, 16000
    );
    for e in EngineType::ALL {
        for color in [0.0, 0.5, 1.0] {
            let mut line = format!("{:10} c{color:<3}  ", e.name());
            for f in [
                50.0, 200.0, 1000.0, 2000.0, 4000.0, 6000.0, 8000.0, 12000.0, 16000.0,
            ] {
                let mut c = chain(e, EffectType::Chorus);
                c.mix = 1.0;
                c.depth = 0.3;
                c.rate_hz = 0.3;
                c.width = 0.0;
                c.color = color;
                let n = (SR * 0.6) as usize;
                let (mut ei, mut eo) = (0.0, 0.0);
                let mut i0 = 0;
                while i0 < n {
                    let mut l: Vec<f64> = (i0..i0 + 64)
                        .map(|i| (2.0 * PI * f * i as f64 / SR).sin() * 0.1)
                        .collect();
                    let x = l.clone();
                    let mut r = l.clone();
                    c.process(&mut l, &mut r);
                    if i0 > n / 3 {
                        ei += x.iter().map(|s| s * s).sum::<f64>();
                        eo += l.iter().map(|s| s * s).sum::<f64>();
                    }
                    i0 += 64;
                }
                line += &format!("{:6.1}", 10.0 * (eo / ei).log10());
            }
            println!("{line}");
        }
    }
}

fn pink(n: usize) -> Vec<f64> {
    let mut seed = 9u32;
    let (mut b0, mut b1, mut b2) = (0.0, 0.0, 0.0);
    (0..n)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let w = f64::from(seed >> 8) / f64::from(1u32 << 24) - 0.5;
            b0 = 0.997 * b0 + 0.029 * w;
            b1 = 0.985 * b1 + 0.032 * w;
            b2 = 0.95 * b2 + 0.048 * w;
            (b0 + b1 + b2 + 0.02 * w) * 2.0
        })
        .collect()
}

fn level() {
    let x = pink((SR * 4.0) as usize);
    let rms = |v: &[f64]| (v.iter().map(|s| s * s).sum::<f64>() / v.len() as f64).sqrt();
    let ri = rms(&x[x.len() / 2..]);
    println!(
        "== fully-wet level, dB (pink, depth .35, 0.8 Hz): L R mono-sum, width 0 / 1; and flanger fb .7 =="
    );
    for e in EngineType::ALL {
        let mut line = format!("{:10}", e.name());
        for (effect, width, fb) in [
            (EffectType::Chorus, 0.0, 0.0),
            (EffectType::Chorus, 1.0, 0.0),
            (EffectType::Flanger, 1.0, 0.7),
        ] {
            let mut c = chain(e, effect);
            c.mix = 1.0;
            c.depth = 0.35;
            c.rate_hz = 0.8;
            c.width = width;
            c.feedback = fb;
            let (mut l, mut r) = (x.clone(), x.clone());
            for (bl, br) in l.chunks_mut(64).zip(r.chunks_mut(64)) {
                c.process(bl, br);
            }
            let h = x.len() / 2;
            let m: Vec<f64> = l[h..]
                .iter()
                .zip(&r[h..])
                .map(|(a, b)| (a + b) * 0.5)
                .collect();
            let db = |v: f64| 20.0 * (v / ri).log10();
            line += &format!(
                "  {effect:?} w{width} fb{fb}: {:+5.1} {:+5.1} {:+5.1}",
                db(rms(&l[h..])),
                db(rms(&r[h..])),
                db(rms(&m))
            );
        }
        println!("{line}");
    }
}

fn cpu() {
    println!("== CPU, ns per stereo sample (64-frame blocks, 48 kHz; best of 7 × 1 s) ==");
    for e in EngineType::ALL {
        let mut line = format!("{:10}", e.name());
        for effect in [EffectType::Chorus, EffectType::Flanger] {
            for nv in [2, 4] {
                let mut c = chain(e, effect);
                c.num_voices = nv;
                c.feedback = 0.3;
                let mut l = vec![0.0; 64];
                let mut r = vec![0.0; 64];
                let blocks = SR as usize / 64;
                for _ in 0..200 {
                    c.process(&mut l, &mut r);
                }
                let mut best = f64::MAX;
                for _ in 0..7 {
                    let t = std::time::Instant::now();
                    for b in 0..blocks {
                        for (i, s) in l.iter_mut().enumerate() {
                            *s = ((b * 64 + i) as f64 * 0.013).sin() * 0.3;
                        }
                        r.copy_from_slice(&l);
                        c.process(&mut l, &mut r);
                    }
                    best = best.min(t.elapsed().as_nanos() as f64 / (blocks * 64) as f64);
                }
                line += &format!("  {effect:?}/{nv}v {best:6.1}");
            }
        }
        println!("{line}");
    }
    // The cost of the test signal itself, to subtract.
    let mut l = vec![0.0f64; 64];
    let t = std::time::Instant::now();
    for b in 0..(SR as usize / 64) {
        for (i, s) in l.iter_mut().enumerate() {
            *s = ((b * 64 + i) as f64 * 0.013).sin() * 0.3;
        }
        std::hint::black_box(&mut l);
    }
    println!(
        "(signal generation alone: {:.1} ns/sample)",
        t.elapsed().as_nanos() as f64 / SR
    );
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("pitch") => pitch(),
        Some("tone") => tone(),
        Some("level") => level(),
        Some("cpu") => cpu(),
        _ => {
            pitch();
            tone();
            level();
            cpu();
        }
    }
}
