//! Dual-IR morph demo on real impulse responses.
//!
//! Loads IR A and IR B into the convolution algorithm's two slots and
//! renders a dry pluck pattern while the morph LFO sweeps between them,
//! with a touch of Motion on the tail. Output is a stereo WAV.
//!
//! Usage:
//!   cargo run -p reverb-dsp --release --example `ir_morph_demo` -- \
//!     <`ir_a.wav`> <`ir_b.wav`> <out.wav> [seconds=16]

use std::process::ExitCode;

use audiocore_dsp::{AudioConfig, Processor};
use dsp_core::num;
use reverb_dsp::algorithm::{AlgorithmType, IrSlot};
use reverb_dsp::chain::ReverbChain;
use reverb_dsp::ir::{IrAsset, IrTransforms};

const SR: f64 = 48000.0;
const BLOCK: usize = 512;

fn load_ir(path: &str) -> Result<(Vec<f64>, Vec<f64>), String> {
    let asset = IrAsset::load(path, SR).map_err(|e| format!("load {path}: {e}"))?;
    Ok(IrTransforms::default().apply(&asset))
}

/// Simple pluck: noise-excited decaying sine, staggered pentatonic pattern.
fn pluck_pattern(n: usize) -> Vec<f64> {
    let pitches = [220.0, 261.63, 293.66, 329.63, 392.0, 329.63, 293.66, 261.63];
    let interval = num::f64_to_index(SR * 0.5);
    let mut out = vec![0.0; n];
    let mut seed = 0x12345u32;
    let mut rng = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        f64::from(seed.cast_signed()) / f64::from(i32::MAX)
    };
    let mut start = 0usize;
    let mut idx = 0usize;
    while start < n {
        let f = pitches
            .get(idx.checked_rem(pitches.len()).unwrap_or(0))
            .copied()
            .unwrap_or(440.0);
        let dur = num::f64_to_index(SR * 0.35);
        for i in 0..dur.min(n.saturating_sub(start)) {
            let t = num::count_to_f64(i) / SR;
            let env = (-t * 9.0).exp();
            let tone = (2.0 * std::f64::consts::PI * f * t).sin();
            let attack_noise = if i < 96 {
                rng() * 0.2 * (1.0 - num::count_to_f64(i) / 96.0)
            } else {
                0.0
            };
            if let Some(slot) = out.get_mut(start.saturating_add(i)) {
                *slot += (tone * 0.5 + attack_noise) * env * 0.6;
            }
        }
        start = start.saturating_add(interval);
        idx = idx.saturating_add(1);
    }
    out
}

fn write_wav_stereo_16(path: &str, left: &[f64], right: &[f64]) -> Result<(), String> {
    let n = u32::try_from(left.len()).unwrap_or(u32::MAX);
    let data_len = n.saturating_mul(4);
    let mut bytes = Vec::with_capacity(
        44_usize.saturating_add(usize::try_from(data_len).unwrap_or(usize::MAX)),
    );
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36_u32.saturating_add(data_len)).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&2u16.to_le_bytes()); // stereo
    let rate = num::f64_to_u32(SR);
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&rate.saturating_mul(4).to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for (l, r) in left.iter().zip(right.iter()) {
        for sample in [*l, *r] {
            let scaled = num::trunc_to_i64(sample.clamp(-1.0, 1.0) * 32767.0);
            let pcm = i16::try_from(scaled).unwrap_or(i16::MAX);
            bytes.extend_from_slice(&pcm.to_le_bytes());
        }
    }
    std::fs::write(path, bytes).map_err(|e| format!("write wav: {e}"))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [ir_a, ir_b, out_path, rest @ ..] = args.as_slice() else {
        return Err("usage: ir_morph_demo <ir_a.wav> <ir_b.wav> <out.wav> [seconds]".to_owned());
    };
    let seconds: f64 = rest.first().and_then(|s| s.parse().ok()).unwrap_or(16.0);
    let n = num::f64_to_index(SR * seconds);

    let (a_l, a_r) = load_ir(ir_a)?;
    let (b_l, b_r) = load_ir(ir_b)?;

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

    if !chain.load_convolution_ir_slot(&a_l, &a_r, IrSlot::A) {
        return Err(format!("slot A rejected {ir_a}"));
    }
    if !chain.load_convolution_ir_slot(&b_l, &b_r, IrSlot::B) {
        return Err(format!("slot B rejected {ir_b}"));
    }

    let dry = pluck_pattern(n);
    let mut left = dry.clone();
    let mut right = dry;

    let mut pos = 0;
    while pos < n {
        let end = (pos.saturating_add(BLOCK)).min(n);
        let (Some(block_l), Some(block_r)) = (left.get_mut(pos..end), right.get_mut(pos..end))
        else {
            return Err("invalid slice bounds".to_owned());
        };
        chain.process(block_l, block_r);
        pos = end;
    }

    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f64, |m, s| m.max(s.abs()));
    if !peak.is_finite() {
        return Err("non-finite output".to_owned());
    }

    // Normalize to -1 dBFS so the wet sum never clips the 16-bit output.
    if peak > 0.0 {
        let g = 0.89 / peak;
        for s in &mut left {
            *s *= g;
        }
        for s in &mut right {
            *s *= g;
        }
    }

    write_wav_stereo_16(out_path, &left, &right)?;
    println!("wrote {out_path} ({seconds:.0}s, peak {peak:.3})  A={ir_a}  B={ir_b}");
    Ok(())
}
