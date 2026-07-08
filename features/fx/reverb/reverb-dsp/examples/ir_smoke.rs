//! Smoke-test the IR pipeline against a real impulse response file.
//!
//! Usage: cargo run -p reverb-dsp --example ir_smoke -- <path-to-ir.wav> [more.wav ...]
//!
//! Loads each file via IrAsset -> IrTransforms -> Convolution, renders a
//! unit impulse, and prints length / peak / RT60-style decay stats.

use reverb_dsp::algorithms::convolution::Convolution;
use reverb_dsp::ir::{IrAsset, IrTransforms};

const SR: f64 = 48000.0;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: ir_smoke <ir.wav> [more.wav ...]");
        std::process::exit(1);
    }

    for path in &args {
        match IrAsset::load(path, SR) {
            Ok(asset) => {
                let transforms = IrTransforms::default();
                let (ir_l, ir_r) = transforms.apply(&asset);

                let mut conv = Convolution::new(SR);
                conv.load_ir_stereo(&ir_l, &ir_r);

                // Render impulse through the convolver.
                use reverb_dsp::algorithm::ReverbAlgorithm;
                let n = (SR * asset.duration_seconds().min(10.0)) as usize + 4800;
                let mut energy = 0.0f64;
                let mut peak = 0.0f64;
                let mut last_above_60 = 0usize;
                for i in 0..n {
                    let x = if i == 0 { 1.0 } else { 0.0 };
                    let (l, r) = conv.tick(x, x);
                    let e = l * l + r * r;
                    energy += e;
                    peak = peak.max(l.abs()).max(r.abs());
                    if e > 1e-6 {
                        last_above_60 = i;
                    }
                    assert!(l.is_finite() && r.is_finite(), "NaN at sample {i} in {path}");
                }

                println!(
                    "OK  {:>7} frames  {:.1} s  ch={}  peak={:.3}  energy={:.3}  ~decay-to--60dB={:.2}s  {}",
                    asset.frames(),
                    asset.duration_seconds(),
                    asset.num_channels(),
                    peak,
                    energy,
                    last_above_60 as f64 / SR,
                    path
                );
            }
            Err(e) => println!("FAIL {path}: {e}"),
        }
    }
}
