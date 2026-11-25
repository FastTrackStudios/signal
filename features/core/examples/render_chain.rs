//! Render a mono (or stereo) WAV through a Signal preset, offline, the way
//! an instance in Studio would run it: files the preset names (a NAM
//! capture, a cab IR) are read from disk by path.
//!
//! ```text
//! cargo run -p signal-core --release --example render_chain -- \
//!     <preset.json> <in.wav> <out.wav> [start seconds] [length seconds]
//! ```
//!
//! `preset.json` is a `Preset` or a whole `SignalState` (its `preset` is
//! used). The input's channels go to the preset's inputs in order (a mono
//! DI is input 0). Prints each block's build status (a capture that does
//! not load says why), the latency, and the output's peak and RMS.

use std::path::Path;

use signal_core::block::BlockFactory;
use signal_core::graph::{self, BuildOptions};
use signal_core::host::{AssetRef, HostError};
use signal_core::state::Preset;
use signal_core::{BuiltinFactory, Processor, SignalHost};

const BLOCK: usize = 512;
const RATE: u32 = 48_000;

/// Files by path, as the plugin's host reads them.
struct Files;

impl SignalHost for Files {
    fn load_file(&self, asset: &AssetRef) -> Result<Vec<u8>, HostError> {
        std::fs::read(&asset.path).map_err(|e| HostError::NotFound(format!("{}: {e}", asset.path)))
    }
}

/// A WAV's samples, de-interleaved, as f32; 16/24/32-bit PCM or 32-bit float.
fn read_wav(path: &Path) -> Result<(u32, Vec<Vec<f32>>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bad = || format!("{}: not a WAV this reads", path.display());
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(bad());
    }
    let (mut fmt, mut data) = (None, None);
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().map_err(|_| bad())?) as usize;
        let body = at + 8..(at + 8 + len).min(bytes.len());
        match id {
            b"fmt " => fmt = Some(body.clone()),
            b"data" => data = Some(body.clone()),
            _ => {}
        }
        at = body.end + (len & 1);
    }
    let (fmt, data) = (fmt.ok_or_else(bad)?, data.ok_or_else(bad)?);
    let f = &bytes[fmt];
    let u16_at = |i: usize| u16::from_le_bytes([f[i], f[i + 1]]);
    let mut format = u16_at(0);
    let channels = usize::from(u16_at(2));
    let rate = u32::from_le_bytes([f[4], f[5], f[6], f[7]]);
    let bits = usize::from(u16_at(14));
    if format == 0xFFFE && f.len() >= 26 {
        format = u16_at(24); // WAVE_FORMAT_EXTENSIBLE: the sub-format's tag
    }
    let width = bits / 8;
    let d = &bytes[data];
    let frames = d.len() / (width * channels);
    let mut out = vec![Vec::with_capacity(frames); channels];
    for frame in 0..frames {
        for (c, chan) in out.iter_mut().enumerate() {
            let s = &d[(frame * channels + c) * width..][..width];
            let v = match (format, bits) {
                (3, 32) => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
                (1, 16) => f32::from(i16::from_le_bytes([s[0], s[1]])) / 32_768.0,
                (1, 24) => (i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8) as f32 / 8_388_608.0,
                (1, 32) => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 / 2_147_483_648.0,
                _ => return Err(format!("{}: format {format}, {bits} bits", path.display())),
            };
            chan.push(v);
        }
    }
    Ok((rate, out))
}

fn write_wav(path: &Path, l: &[f32], r: &[f32]) -> Result<(), String> {
    let n = l.len().min(r.len());
    let data_len = (n * 8) as u32;
    let mut b = Vec::with_capacity(44 + n * 8);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..n {
        b.extend_from_slice(&l[i].to_le_bytes());
        b.extend_from_slice(&r[i].to_le_bytes());
    }
    std::fs::write(path, b).map_err(|e| format!("{}: {e}", path.display()))
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let [_, preset, input, out, rest @ ..] = args.as_slice() else {
        return Err(
            "usage: render_chain <preset.json> <in.wav> <out.wav> [start s] [length s]".into(),
        );
    };
    let text = std::fs::read_to_string(preset).map_err(|e| format!("{preset}: {e}"))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{preset}: {e}"))?;
    let preset: Preset = if json.get("version").is_some() {
        // A whole state: parsed as the plugin parses it.
        signal_core::SignalState::from_blob(text.as_bytes())
            .map_err(|e| format!("{preset}: {e}"))?
            .preset
    } else {
        serde_json::from_value(json.get("preset").cloned().unwrap_or(json))
            .map_err(|e| format!("{preset}: not a preset: {e}"))?
    };

    let (rate, mut chans) = read_wav(Path::new(input))?;
    if rate != RATE {
        return Err(format!("{input}: {rate} Hz; this renders at {RATE}"));
    }
    let start = rest
        .first()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let length = rest.get(1).and_then(|s| s.parse::<f64>().ok());
    let from = ((start * f64::from(RATE)) as usize).min(chans[0].len());
    let to = length.map_or(chans[0].len(), |l| {
        (from + (l * f64::from(RATE)) as usize).min(chans[0].len())
    });
    for c in &mut chans {
        *c = c[from..to].to_vec();
    }
    let frames = to - from;
    let inputs = usize::from(preset.inputs);
    while chans.len() < inputs.max(2) {
        chans.push(vec![0.0; frames]);
    }
    let in_peak = chans[0].iter().fold(0.0_f32, |m, s| m.max(s.abs()));

    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let opts = BuildOptions {
        sample_rate: RATE as f32,
        max_block: BLOCK,
        port: None,
        factories: &factories,
        host: &Files,
    };
    let started = std::time::Instant::now();
    let (graph, report) = graph::build(&preset, &opts);
    let mut failed = 0;
    for b in &report.blocks {
        match b.status.reason() {
            Some(why) => {
                failed += 1;
                println!("  {}/{}: NOT RUNNING: {why}", b.lane, b.block);
            }
            None => println!("  {}/{}: ok", b.lane, b.block),
        }
    }
    for w in &report.warnings {
        println!("  warning: {w}");
    }
    let latency = graph.latency() as usize;
    println!(
        "{}: built in {:.1?}, {} blocks ({failed} not running), latency {latency} frames",
        preset.name,
        started.elapsed(),
        report.blocks.len()
    );
    let mut proc = Processor::new();
    let _ = proc.install(graph);

    let width = chans.len().max(usize::from(preset.outputs));
    let mut bufs = vec![vec![0.0_f32; BLOCK]; width];
    let (mut l, mut r) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    let started = std::time::Instant::now();
    let mut done = 0;
    while done < frames + latency {
        let n = BLOCK.min(frames + latency - done);
        for (c, buf) in bufs.iter_mut().enumerate() {
            for (i, s) in buf[..n].iter_mut().enumerate() {
                *s = if c < inputs {
                    chans[c].get(done + i).copied().unwrap_or(0.0)
                } else {
                    0.0
                };
            }
        }
        let mut io: Vec<&mut [f32]> = bufs.iter_mut().map(|b| &mut b[..n]).collect();
        proc.process(&mut io, inputs, n, &[], &[]);
        for i in 0..n {
            if done + i >= latency {
                l.push(bufs[0][i]);
                r.push(bufs[1][i]);
            }
        }
        done += n;
    }
    let took = started.elapsed();
    let peak = l.iter().chain(&r).fold(0.0_f32, |m, s| m.max(s.abs()));
    let rms = (l.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / l.len().max(1) as f64).sqrt()
        as f32;
    write_wav(Path::new(out), &l, &r)?;
    println!(
        "wrote {out}: {:.1} s in {took:.1?} ({:.0}x real time); in peak {:.1} dBFS -> out peak {:.1} dBFS, RMS {:.1} dBFS",
        frames as f64 / f64::from(RATE),
        frames as f64 / f64::from(RATE) / took.as_secs_f64(),
        db(in_peak),
        db(peak),
        db(rms)
    );
    if failed > 0 {
        return Err(format!("{failed} block(s) did not build"));
    }
    Ok(())
}
