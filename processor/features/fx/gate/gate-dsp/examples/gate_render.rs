//! Render a WAV through the drum gate — the engine side of a null test
//! against the reference.
//!
//! ```text
//! cargo run --release -p gate-dsp --example gate_render -- <in.wav> <out.wav> \
//!     mode=kick threshold=-30 reduction=-80 length=300 debleed=0 ghost=0 output=0 [key=<sidechain.wav>]
//! ```
//!
//! The output has the look-ahead latency removed (as a host would), so it
//! lines up sample for sample with the input and with the reference's
//! latency-compensated render. Written as 32-bit float.

#![expect(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "offline render tool: per-sample loops over buffers sized by construction"
)]

use gate_dsp::{DrumGate, Mode, Settings};

/// Channels of samples, and the sample rate.
type Wav = (Vec<Vec<f64>>, u32);

fn read(path: &str) -> Result<Wav, Box<dyn std::error::Error>> {
    let mut r = hound::WavReader::open(path)?;
    let spec = r.spec();
    let ch = usize::from(spec.channels);
    let samples: Vec<f64> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.map(f64::from)).collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = f64::from(1u32 << (spec.bits_per_sample - 1));
            r.samples::<i32>().map(|s| s.map(|v| f64::from(v) / scale)).collect::<Result<_, _>>()?
        }
    };
    let mut chans = vec![Vec::with_capacity(samples.len() / ch); ch];
    for (i, s) in samples.into_iter().enumerate() {
        chans[i % ch].push(s);
    }
    Ok((chans, spec.sample_rate))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, rest @ ..] = args.as_slice() else {
        return Err("usage: gate_render <in.wav> <out.wav> key=value...".into());
    };
    let kv = |k: &str| rest.iter().find_map(|a| a.strip_prefix(k).and_then(|v| v.strip_prefix('=')));
    let num = |k: &str, d: f64| kv(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d);
    let mode = match kv("mode").unwrap_or("kick").to_ascii_lowercase().replace([' ', '_', '-'], "").as_str() {
        "snaretop" => Mode::SnareTop,
        "snarebottom" => Mode::SnareBottom,
        "toms" => Mode::Toms,
        _ => Mode::Kick,
    };
    let settings = Settings {
        mode,
        threshold_db: num("threshold", -30.0),
        reduction_db: num("reduction", -80.0),
        length_ms: num("length", 300.0),
        debleed: num("debleed", 0.0),
        ghost: num("ghost", 0.0) != 0.0,
        output_db: num("output", 0.0),
    };
    let (mut chans, sr) = read(input)?;
    let key = kv("key").map(read).transpose()?.map(|(k, _)| k);
    let nch = chans.len();
    let len = chans.first().map_or(0, Vec::len);
    let mut gate = DrumGate::new(f64::from(sr), nch, settings);
    let lat = gate.latency();
    let mut frame = vec![0.0; nch];
    let mut kframe = vec![0.0; nch];
    let mut out = vec![Vec::with_capacity(len); nch];
    for n in 0..len + lat {
        for c in 0..nch {
            frame[c] = chans[c].get(n).copied().unwrap_or(0.0);
            if let Some(k) = &key {
                kframe[c] = k.get(c).or_else(|| k.first()).and_then(|v| v.get(n)).copied().unwrap_or(0.0);
            }
        }
        gate.process_frame(&mut frame, key.as_ref().map(|_| kframe.as_slice()));
        if n >= lat {
            for c in 0..nch {
                out[c].push(frame[c]);
            }
        }
    }
    chans.clear();
    let spec = hound::WavSpec { channels: u16::try_from(nch)?, sample_rate: sr, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let mut w = hound::WavWriter::create(output, spec)?;
    for i in 0..len {
        for c in &out {
            #[expect(clippy::cast_possible_truncation, clippy::as_conversions, reason = "f32 WAV output")]
            w.write_sample(c[i] as f32)?;
        }
    }
    w.finalize()?;
    eprintln!("wrote {output} ({nch} ch, {len} frames, latency {lat})");
    Ok(())
}
