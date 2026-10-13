//! Render every chorus engine at a few settings on guitar-like material, for
//! listening: Karplus-Strong strummed chords, and (when given) a mono WAV of
//! real playing. Prints each render's level against its input.
//!
//! `cargo run --release -p chorus-dsp --example chorus_audition -- OUT_DIR [live.wav]`

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
use std::io::{Read, Write};

const SR: f64 = 48_000.0;

/// Strummed open chords through a plucked-string model.
fn ks_chords() -> Vec<f64> {
    // E, A, D, G, C, then a single-note line.
    let chords: [&[f64]; 5] = [
        &[82.41, 123.47, 164.81, 207.65, 246.94, 329.63],
        &[110.0, 164.81, 220.0, 277.18, 329.63],
        &[146.83, 220.0, 293.66, 369.99],
        &[98.0, 123.47, 146.83, 196.0, 246.94, 392.0],
        &[130.81, 164.81, 196.0, 261.63, 329.63],
    ];
    let line = [329.63, 392.0, 440.0, 493.88, 587.33, 493.88, 440.0, 392.0];
    let mut out = vec![0.0; (SR * 13.0) as usize];
    let mut seed = 7u32;
    let mut noise = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(seed >> 8) / f64::from(1u32 << 24) - 0.5
    };
    let mut pluck = |out: &mut Vec<f64>, start: usize, f: f64, amp: f64, len_s: f64| {
        let n = (SR / f).round() as usize;
        let mut buf: Vec<f64> = (0..n).map(|_| noise()).collect();
        // Soften the pick.
        for i in 1..n {
            buf[i] = 0.5 * (buf[i] + buf[i - 1]);
        }
        let decay = 0.996;
        let mut idx = 0;
        let mut prev = 0.0;
        for t in 0..(SR * len_s) as usize {
            let i = start + t;
            if i >= out.len() {
                break;
            }
            let cur = buf[idx];
            let next = buf[(idx + 1) % n];
            let y = decay * 0.5 * (cur + next);
            buf[idx] = y;
            idx = (idx + 1) % n;
            // Body: a gentle one-pole on the output.
            prev = 0.6 * prev + 0.4 * cur;
            out[i] += amp * prev;
        }
    };
    for (c, notes) in chords.iter().enumerate() {
        let start = (c as f64 * 1.8 * SR) as usize;
        for (s, f) in notes.iter().enumerate() {
            pluck(&mut out, start + s * 600, *f, 0.22, 2.2);
        }
    }
    for (k, f) in line.iter().enumerate() {
        pluck(
            &mut out,
            (SR * (9.2 + k as f64 * 0.42)) as usize,
            *f,
            0.35,
            1.2,
        );
    }
    let peak = out.iter().fold(0.0f64, |m, s| m.max(s.abs()));
    out.iter().map(|s| s * 0.5 / peak).collect()
}

/// Minimal WAV reader: PCM 16/24/32 or float 32, any channels (first taken).
fn read_wav(path: &str) -> Option<Vec<f64>> {
    let mut b = Vec::new();
    std::fs::File::open(path).ok()?.read_to_end(&mut b).ok()?;
    let u16le = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32le = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let (mut fmt, mut ch, mut bits) = (1u16, 1u16, 16u16);
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let len = u32le(i + 4) as usize;
        if id == b"fmt " {
            fmt = u16le(i + 8);
            ch = u16le(i + 10);
            bits = u16le(i + 22);
        } else if id == b"data" {
            let d = &b[i + 8..(i + 8 + len).min(b.len())];
            let step = usize::from(bits / 8) * usize::from(ch);
            return Some(
                d.chunks_exact(step)
                    .map(|f| match (fmt, bits) {
                        (3, 32) | (0xFFFE, 32) => {
                            f64::from(f32::from_le_bytes([f[0], f[1], f[2], f[3]]))
                        }
                        (_, 16) => f64::from(i16::from_le_bytes([f[0], f[1]])) / 32768.0,
                        (_, 24) => {
                            f64::from(i32::from_le_bytes([0, f[0], f[1], f[2]]) >> 8) / 8_388_608.0
                        }
                        _ => {
                            f64::from(i32::from_le_bytes([f[0], f[1], f[2], f[3]]))
                                / 2_147_483_648.0
                        }
                    })
                    .collect(),
            );
        }
        i += 8 + len + (len & 1);
    }
    None
}

fn write_wav(path: &str, l: &[f64], r: &[f64]) {
    let n = l.len();
    let mut b = Vec::with_capacity(44 + n * 8);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&((36 + n * 8) as u32).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&(SR as u32).to_le_bytes());
    b.extend_from_slice(&((SR as u32) * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&((n * 8) as u32).to_le_bytes());
    for (a, c) in l.iter().zip(r) {
        b.extend_from_slice(&(*a as f32).to_le_bytes());
        b.extend_from_slice(&(*c as f32).to_le_bytes());
    }
    std::fs::File::create(path)
        .and_then(|mut f| f.write_all(&b))
        .expect("write wav");
}

struct Setting {
    name: &'static str,
    effect: EffectType,
    rate: f64,
    depth: f64,
    mix: f64,
    color: f64,
    feedback: f64,
}

fn settings(e: EngineType) -> Vec<Setting> {
    // "classic": each engine where its unit is usually set.
    let (cr, cd, cm) = match e {
        EngineType::Juno => (0.5, 0.6, 0.6),
        EngineType::Ce2 => (1.5, 0.6, 0.5),
        EngineType::Dimension => (0.4, 0.5, 0.5),
        EngineType::Clone => (1.2, 0.85, 0.5),
        EngineType::TriChorus => (0.7, 0.5, 0.6),
        EngineType::Scf => (1.0, 0.5, 0.5),
        EngineType::Julia => (1.2, 0.55, 0.5),
        EngineType::Tape => (0.4, 0.5, 0.5),
        _ => (0.9, 0.45, 0.5),
    };
    let s = |name, effect, rate, depth, mix, color, feedback| Setting {
        name,
        effect,
        rate,
        depth,
        mix,
        color,
        feedback,
    };
    vec![
        s("subtle", EffectType::Chorus, 0.5, 0.25, 0.35, 0.5, 0.0),
        s("classic", EffectType::Chorus, cr, cd, cm, 0.5, 0.0),
        s("extreme", EffectType::Chorus, 7.0, 1.0, 0.5, 1.0, 0.0),
        s("flanger", EffectType::Flanger, 0.25, 0.8, 0.5, 0.5, 0.7),
        s("vibrato", EffectType::Vibrato, 5.0, 0.5, 1.0, 0.5, 0.0),
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&out).expect("out dir");
    let mut sources = vec![("ks", ks_chords())];
    if let Some(live) = args.get(2).and_then(|p| read_wav(p)) {
        // The loudest 12 s.
        let w = (SR * 12.0) as usize;
        let hop = (SR * 0.5) as usize;
        let mut best = (0usize, 0.0f64);
        let mut i = 0;
        while i + w <= live.len() {
            let e: f64 = live[i..i + w].iter().map(|s| s * s).sum();
            if e > best.1 {
                best = (i, e);
            }
            i += hop;
        }
        let take = &live[best.0..(best.0 + w).min(live.len())];
        sources.push(("live", take.to_vec()));
    }
    let rms = |v: &[f64]| (v.iter().map(|s| s * s).sum::<f64>() / v.len().max(1) as f64).sqrt();
    for (src, x) in &sources {
        write_wav(&format!("{out}/{src}__dry.wav"), x, x);
        let ri = rms(x);
        println!("== {src}: level re dry, dB (L / R / mono sum)");
        for e in EngineType::ALL {
            for st in settings(e) {
                if *src == "live" && matches!(st.name, "flanger" | "vibrato") {
                    continue;
                }
                let mut c = ChorusChain::new();
                c.set_engine(e);
                c.effect_type = st.effect;
                c.rate_hz = st.rate;
                c.depth = st.depth;
                c.mix = st.mix;
                c.color = st.color;
                c.feedback = st.feedback;
                c.update(AudioConfig {
                    sample_rate: SR,
                    max_buffer_size: 64,
                });
                let (mut l, mut r) = (x.clone(), x.clone());
                for (bl, br) in l.chunks_mut(64).zip(r.chunks_mut(64)) {
                    c.process(bl, br);
                }
                let m: Vec<f64> = l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect();
                let db = |v: f64| 20.0 * (v / ri).log10();
                // Clicks: the largest second difference (a discontinuity's
                // signature) against the dry's.
                let d2 = |v: &[f64]| {
                    (2..v.len())
                        .map(|i| (v[i] - 2.0 * v[i - 1] + v[i - 2]).abs())
                        .fold(0.0f64, f64::max)
                };
                let click = d2(&l).max(d2(&r)) / d2(x).max(1e-12);
                println!(
                    "{:10} {:8} {:+5.1} {:+5.1} {:+5.1}   d2 x{click:.2}",
                    e.name(),
                    st.name,
                    db(rms(&l)),
                    db(rms(&r)),
                    db(rms(&m))
                );
                let name = e.name().to_lowercase().replace('-', "");
                write_wav(
                    &format!("{out}/{src}__{:02}_{name}__{}.wav", e.index(), st.name),
                    &l,
                    &r,
                );
            }
        }
    }
}
