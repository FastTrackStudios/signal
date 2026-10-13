//! Render a capsule-pair WAV through one modelled mic — the engine side of
//! a null test against the reference (`signal-analyzer`'s `sphere_capture
//! render-cold` renders the other side with the same settings).
//!
//! ```text
//! cargo run --release -p mic-dsp --example mic_render -- <model.micm> <in.wav> <out.wav> \
//!     pattern=4 axis=0 low_cut=0 proximity=0 output=0 phase=0 rear_trim=0 swap=0
//! ```
//!
//! The output is delayed by [`mic_dsp::LATENCY`] relative to the input,
//! like the reference before host latency compensation; this tool removes
//! it, so the two files line up sample for sample.

use std::path::Path;

use mic_dsp::{DualMic, DualSettings, MicChain, MicModel, Settings, Solo, latency};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [model, input, output, rest @ ..] = args.as_slice() else {
        return Err("usage: mic_render <model.micm> <in.wav> <out.wav> key=value...".into());
    };
    let kv = |k: &str| rest.iter().find_map(|a| a.strip_prefix(k).and_then(|v| v.strip_prefix('=')));
    let num = |k: &str, d: f64| kv(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d);
    let bytes = std::fs::read(model)?;
    let model = MicModel::from_bytes(&bytes).map_err(|e| format!("{e:?}"))?;
    let lat = latency(model.sample_rate);
    let index = |k: &str, d: usize| kv(k).and_then(|v| v.parse::<usize>().ok()).unwrap_or(d);
    let settings = Settings {
        pattern: index("pattern", 4),
        axis_deg: num("axis", 0.0),
        low_cut: index("low_cut", 0),
        proximity: num("proximity", 0.0),
        output_db: num("output", 0.0),
        phase_invert: index("phase", 0) != 0,
        rear_trim_db: num("rear_trim", 0.0),
        swap: index("swap", 0) != 0,
    };
    let mut chain = MicChain::new(model);
    chain.apply(settings, true);
    // Dual mode: `model2=<path>` plus pattern2/axis2/low_cut2, mix (0–100),
    // align (cm), solo (0/1/2). Mic 2 shares mic 1's other controls.
    let mut dual = match kv("model2") {
        Some(path) => {
            let m2 = MicModel::from_bytes(&std::fs::read(path)?).map_err(|e| format!("{e:?}"))?;
            let mut c2 = MicChain::new(m2);
            c2.apply(Settings { pattern: index("pattern2", 4), axis_deg: num("axis2", 0.0), low_cut: index("low_cut2", 0), ..settings }, true);
            let solo = match index("solo", 0) { 1 => Solo::Mic1, 2 => Solo::Mic2, _ => Solo::Off };
            let mut d = DualMic::new(chain, c2);
            d.set(DualSettings { mix: num("mix", 0.0) / 100.0, align_cm: num("align", 0.0), solo });
            Some(d)
        }
        None => None,
    };
    let mut single = if dual.is_none() { Some(MicChain::new(MicModel::from_bytes(&bytes).map_err(|e| format!("{e:?}"))?)) } else { None };
    if let Some(c) = single.as_mut() {
        c.apply(settings, true);
    }

    let mut reader = hound::WavReader::open(input)?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels);
    let samples: Vec<f32> = reader.samples::<f32>().collect::<Result<_, _>>()?;
    let frames = samples.chunks_exact(channels.max(1));
    let tail = core::iter::repeat_n([0.0f32, 0.0], lat);
    let pairs = frames.map(|c| [c.first().copied().unwrap_or(0.0), c.get(1).copied().unwrap_or(0.0)]).chain(tail);
    let out: Vec<f32> = pairs
        .map(|[f, r]| match (dual.as_mut(), single.as_mut()) {
            (Some(d), _) => d.process(f64::from(f), f64::from(r)),
            (None, Some(c)) => c.process(f64::from(f), f64::from(r)),
            (None, None) => 0.0,
        })
        .skip(lat)
        .map(|y| {
            #[expect(clippy::cast_possible_truncation, clippy::as_conversions, reason = "WAV output is f32")]
            let s = y as f32;
            s
        })
        .collect();
    let mut w = hound::WavWriter::create(
        Path::new(output),
        hound::WavSpec { channels: 1, sample_rate: spec.sample_rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float },
    )?;
    for s in out {
        w.write_sample(s)?;
    }
    w.finalize()?;
    Ok(())
}
