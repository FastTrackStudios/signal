//! How loud the chorus block plays against its input, per engine and mix —
//! a chorus should sit near unity (a pedal's level knob aside).
//! `cargo run --release -p fx-blocks --example chorus_gain`

use daw::plugin::{PluginEvents, PluginInstance};
use fx_blocks::NativeMod;

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

fn main() {
    // Pink-ish noise (a one-pole-filtered sum, −3 dB/oct-ish): broadband
    // like playing, so comb notches average out as they do on a guitar.
    let mut seed = 9u32;
    let (mut b0, mut b1, mut b2) = (0.0f32, 0.0f32, 0.0f32);
    let tone: Vec<f32> = (0..SR as usize * 4)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let w = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
            b0 = 0.997 * b0 + 0.029 * w;
            b1 = 0.985 * b1 + 0.032 * w;
            b2 = 0.95 * b2 + 0.048 * w;
            (b0 + b1 + b2 + 0.02 * w) * 2.0
        })
        .collect();
    let rms = |x: &[f32]| (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt();
    let kind = std::env::args().nth(1).unwrap_or_default();
    for engine in 0..11 {
        for mix in [0.25, 0.5, 0.75, 1.0] {
            let mut m = match kind.as_str() {
                "flanger" => NativeMod::flanger(SR),
                "vibrato" => NativeMod::vibrato(SR),
                _ => NativeMod::chorus(SR),
            };
            for (n, v) in [("engine", f64::from(engine)), ("mix", mix), ("depth", 0.35), ("rate", 0.8)] {
                m.set_named(n, v);
            }
            m.prepare(SR, BLOCK as u32).unwrap();
            let mut out = Vec::new();
            for c in tone.chunks(BLOCK) {
                let (mut l, mut r) = (vec![0.0f32; c.len()], vec![0.0f32; c.len()]);
                m.process_block(c, c, &mut l, &mut r, &PluginEvents::EMPTY).unwrap();
                out.extend(l.iter().zip(&r).map(|(a, b)| (a + b) * 0.5));
            }
            let half = tone.len() / 2;
            let db = 20.0 * (rms(&out[half..]) / rms(&tone[half..])).log10();
            println!("engine {engine} mix {mix}: {db:+.1} dB");
        }
    }
}
