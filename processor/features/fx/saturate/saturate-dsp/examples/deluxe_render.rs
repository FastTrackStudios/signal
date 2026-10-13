//! Render raw little-endian f64 audio through the Deluxe engine — the
//! measurement tools null its output against the reference's captures.
//!
//! ```sh
//! cargo run --release -p saturate-dsp --example deluxe_render -- \
//!     in.f64 out.f64 crush crunch darkness_hz mix release(0|1) [stereo]
//! ```
//! `in.f64` is mono (fed to both channels) unless `stereo` is given, in
//! which case it is interleaved L/R. The output is the left channel, or
//! interleaved L/R for stereo.

use std::process::ExitCode;

use saturate_dsp::deluxe::{Deluxe, DeluxeParams};

fn run(args: &[String]) -> Result<(), String> {
    let [_, input, output, crush, crunch, dark, mix, release, rest @ ..] = args else {
        return Err("usage: deluxe_render in.f64 out.f64 crush crunch darkness_hz mix release [stereo]".into());
    };
    let num = |s: &String| s.parse::<f64>().map_err(|e| format!("{s}: {e}"));
    let mut fx = Deluxe::new(48_000.0);
    fx.set(DeluxeParams { crush: num(crush)?, crunch: num(crunch)?, darkness_hz: num(dark)?, mix: num(mix)?, release_on: num(release)? != 0.0 });
    let raw = std::fs::read(input).map_err(|e| format!("{input}: {e}"))?;
    let x: Vec<f64> = raw.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().unwrap_or([0; 8]))).collect();
    let mut out = Vec::with_capacity(raw.len());
    if rest.first().is_some_and(|s| s == "stereo") {
        for f in x.chunks_exact(2) {
            if let [l, r] = *f {
                let (yl, yr) = fx.process(l, r);
                out.extend_from_slice(&yl.to_le_bytes());
                out.extend_from_slice(&yr.to_le_bytes());
            }
        }
    } else {
        for &s in &x {
            out.extend_from_slice(&fx.process(s, s).0.to_le_bytes());
        }
    }
    std::fs::write(output, out).map_err(|e| format!("{output}: {e}"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("deluxe_render: {e}");
            ExitCode::FAILURE
        }
    }
}
