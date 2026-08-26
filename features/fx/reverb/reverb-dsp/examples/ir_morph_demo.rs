//! Dual-IR morph demo on real impulse responses.
//!
//! Loads IR A and IR B into the convolution algorithm's two slots and
//! renders a dry pluck pattern while the morph LFO sweeps between them,
//! with a touch of Motion on the tail. Output is a stereo WAV.
//!
//! Usage:
//!   cargo run -p reverb-dsp --release --example ir_morph_demo -- \
//!     <ir_a.wav> <ir_b.wav> <out.wav> [seconds=16]

use audiocore_dsp::{AudioConfig, Processor};
use reverb_dsp::algorithm::AlgorithmType;
use reverb_dsp::chain::ReverbChain;
use reverb_dsp::ir::{IrAsset, IrTransforms};

const SR: f64 = 48000.0;
const BLOCK: usize = 512;

fn load_ir(path: &str) -> (Vec<f64>, Vec<f64>) {
    let asset = IrAsset::load(path, SR).unwrap_or_else(|e| panic!("load {path}: {e}"));
    IrTransforms::default().apply(&asset)
}

/// Simple pluck: noise-excited decaying sine, staggered pentatonic pattern.
fn pluck_pattern(n: usize) -> Vec<f64> {
    let pitches = [220.0, 261.63, 293.66, 329.63, 392.0, 329.63, 293.66, 261.63];
    let interval = (SR * 0.5) as usize;
    let mut out = vec![0.0; n];
    let mut seed = 0x12345u32;
    let mut rng = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as i32) as f64 / i32::MAX as f64
    };
    let mut start = 0usize;
    let mut idx = 0usize;
    while start < n {
        let f = pitches[idx % pitches.len()];
        let dur = (SR * 0.35) as usize;
        for i in 0..dur.min(n - start) {
            let t = i as f64 / SR;
            let env = (-t * 9.0).exp();
            let tone = (2.0 * std::f64::consts::PI * f * t).sin();
            let attack_noise = if i < 96 {
                rng() * 0.2 * (1.0 - i as f64 / 96.0)
            } else {
                0.0
            };
            out[start + i] += (tone * 0.5 + attack_noise) * env * 0.6;
        }
        start += interval;
        idx += 1;
    }
    out
}

fn write_wav_stereo_16(path: &str, left: &[f64], right: &[f64]) {
    let n = left.len() as u32;
    let data_len = n * 4;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&2u16.to_le_bytes()); // stereo
    bytes.extend_from_slice(&(SR as u32).to_le_bytes());
    bytes.extend_from_slice(&((SR as u32) * 4).to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..left.len() {
        for s in [left[i], right[i]] {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    std::fs::write(path, bytes).expect("write wav");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: ir_morph_demo <ir_a.wav> <ir_b.wav> <out.wav> [seconds]");
        std::process::exit(1);
    }
    let seconds: f64 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(16.0);
    let n = (SR * seconds) as usize;

    let (a_l, a_r) = load_ir(&args[0]);
    let (b_l, b_r) = load_ir(&args[1]);

    let mut chain = ReverbChain::new();
    chain.set_algorithm(AlgorithmType::Convolution);
    chain.mix = 0.45;

    // Morph sweep: full-depth LFO, one full A->B->A cycle over the render.
    chain.conv_mod.morph = 0.5;
    chain.conv_mod.morph_lfo_depth = 1.0;
    chain.conv_mod.lfo_rate = 1.0 / seconds;
    // A touch of Motion so the tail breathes.
    chain.conv_mod.motion_depth = 0.25;
    chain.conv_mod.motion_rate = 0.4;

    chain.update(AudioConfig {
        sample_rate: SR,
        max_buffer_size: BLOCK,
    });

    use reverb_dsp::algorithm::IrSlot;
    assert!(chain.load_convolution_ir_slot(&a_l, &a_r, IrSlot::A));
    assert!(chain.load_convolution_ir_slot(&b_l, &b_r, IrSlot::B));

    let dry = pluck_pattern(n);
    let mut left = dry.clone();
    let mut right = dry;

    let mut pos = 0;
    while pos < n {
        let end = (pos + BLOCK).min(n);
        let (l, r) = (&mut left[pos..end], &mut right[pos..end]);
        chain.process(l, r);
        pos = end;
    }

    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f64, |m, s| m.max(s.abs()));
    assert!(peak.is_finite(), "non-finite output");

    // Normalize to -1 dBFS so the wet sum never clips the 16-bit output.
    if peak > 0.0 {
        let g = 0.89 / peak;
        left.iter_mut().for_each(|s| *s *= g);
        right.iter_mut().for_each(|s| *s *= g);
    }

    write_wav_stereo_16(&args[2], &left, &right);
    println!(
        "wrote {} ({:.0}s, peak {:.3})  A={}  B={}",
        args[2], seconds, peak, args[0], args[1]
    );
}
