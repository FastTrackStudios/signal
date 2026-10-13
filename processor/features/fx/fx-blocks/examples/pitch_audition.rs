//! Audition renders for every pitch-shifting consumer: the Ice pitch delay
//! (the "Flute Octave Echo" preset: +12 ladder, feedback 0.55, Long slice),
//! the shimmer delay, the shimmer reverb ("Heaven Shimmer": two +12 voices),
//! the Flute patch's pitch path in series, and the pitch chain at ±12.
//!
//! `cargo run --release -p fx-blocks --example pitch_audition -- <out_dir> <tag> <input.wav>...`
//!
//! Each input is rendered through each case to `<out_dir>/<case>_<input>_<tag>.wav`
//! (stereo float32). Also prints per-case peak / RMS and the CPU cost per
//! sample of the whole block.
//!
//! Synthetic inputs: `sine:<hz>` (3 s, −26 dBFS) and `tone` — a steady
//! 187.5 Hz harmonic tone (period exactly 256 samples) for 3 s. For `tone`
//! every case also prints two artifact scores over 1.5–3 s: `nonharm` —
//! the energy that is not periodic in 1024 samples (synchronous average
//! over the 70 periods), i.e. everything off the harmonic series of
//! 46.875 Hz, which octave ladders of the tone (up, or down two passes)
//! never leave (not meaningful for fifths) — and `am`
//! — the std of the de-trended 10 ms log envelope (grain/splice warble).

use std::time::Instant;

use audiocore_dsp::{AudioConfig, Processor};
use daw::plugin::{PluginEvents, PluginInstance};
use fx_blocks::{NativeDelay, NativeReverb};
use pitch_dsp::chain::{Algorithm, PitchChain};

const SR: f64 = 48_000.0;
const BLOCK: usize = 64;

fn read_wav(path: &str) -> Option<Vec<f32>> {
    let d = std::fs::read(path).ok()?;
    if d.len() < 12 || &d[0..4] != b"RIFF" || &d[8..12] != b"WAVE" {
        return None;
    }
    let (mut fmt, mut ch, mut bits, mut pos) = (0u16, 1usize, 16u16, 12usize);
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= d.len() {
        let id = &d[pos..pos + 4];
        let sz = u32::from_le_bytes([d[pos + 4], d[pos + 5], d[pos + 6], d[pos + 7]]) as usize;
        let body = &d[pos + 8..(pos + 8 + sz).min(d.len())];
        if id == b"fmt " {
            fmt = u16::from_le_bytes([body[0], body[1]]);
            ch = u16::from_le_bytes([body[2], body[3]]) as usize;
            bits = u16::from_le_bytes([body[14], body[15]]);
            if fmt == 0xFFFE && body.len() >= 26 {
                fmt = u16::from_le_bytes([body[24], body[25]]);
            }
        } else if id == b"data" {
            data = Some(body);
        }
        pos += 8 + sz + (sz & 1);
    }
    let data = data?;
    let bps = (bits / 8) as usize;
    let frames = data.len() / (bps * ch);
    let mut out = Vec::with_capacity(frames);
    for f in 0..frames {
        let o = f * bps * ch;
        let b = &data[o..o + bps];
        let v = match (fmt, bits) {
            (3, 32) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            (1, 16) => f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0,
            (1, 24) => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            (1, 32) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
            _ => return None,
        };
        out.push(v);
    }
    Some(out)
}

fn write_wav(path: &str, l: &[f32], r: &[f32]) {
    let n = l.len();
    let data_len = (n * 8) as u32;
    let mut b = Vec::with_capacity(44 + n * 8);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&48_000u32.to_le_bytes());
    b.extend_from_slice(&(48_000u32 * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..n {
        b.extend_from_slice(&l[i].to_le_bytes());
        b.extend_from_slice(&r[i].to_le_bytes());
    }
    std::fs::write(path, b).expect("write wav");
}

/// Steady 187.5 Hz harmonic tone (10 partials, 1/k), 3 s, 5 ms fade-in.
fn tone() -> Vec<f32> {
    (0..48_000 * 3)
        .map(|i| {
            let t = f64::from(i) / SR;
            let s: f64 = (1..=10)
                .map(|k| {
                    (std::f64::consts::TAU * 187.5 * f64::from(k) * t + f64::from(k * k)).sin()
                        / f64::from(k)
                })
                .sum();
            (0.05 * s * (t / 0.005).min(1.0)) as f32
        })
        .collect()
}

/// Energy not periodic in 256 samples, relative to the total (dB).
fn nonharmonic_db(x: &[f32]) -> f64 {
    // 1024 = four periods: octave-down ladders (two passes) stay periodic.
    const P: usize = 1024;
    let m = x.len() / P;
    let (mut tot, mut res) = (0.0f64, 0.0f64);
    for w in x.chunks_exact(P * m) {
        let mut avg = [0.0f64; P];
        for (i, v) in w.iter().enumerate() {
            avg[i % P] += f64::from(*v) / m as f64;
        }
        for (i, v) in w.iter().enumerate() {
            let v = f64::from(*v);
            tot += v * v;
            res += (v - avg[i % P]) * (v - avg[i % P]);
        }
    }
    10.0 * (res.max(1e-30) / tot.max(1e-30)).log10()
}

/// Std (dB) of the linearly de-trended 10 ms log envelope.
fn am_db(x: &[f32]) -> f64 {
    let y: Vec<f64> = x
        .chunks_exact(480)
        .map(|c| {
            let e = c.iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>() / 480.0;
            10.0 * (e + 1e-18).log10()
        })
        .collect();
    let n = y.len() as f64;
    let xm = (n - 1.0) / 2.0;
    let ym = y.iter().sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (i, v) in y.iter().enumerate() {
        sxy += (i as f64 - xm) * (v - ym);
        sxx += (i as f64 - xm) * (i as f64 - xm);
    }
    let b = sxy / sxx;
    (y.iter()
        .enumerate()
        .map(|(i, v)| (v - ym - b * (i as f64 - xm)).powi(2))
        .sum::<f64>()
        / n)
        .sqrt()
}

/// `name=value,…` parameter overrides from an environment variable
/// (`PA_REVERB` / `PA_DELAY`), applied after a case's preset.
fn overrides(var: &str) -> Vec<(String, f64)> {
    std::env::var(var)
        .unwrap_or_default()
        .split(',')
        .filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            Some((k.trim().to_string(), v.trim().parse().ok()?))
        })
        .collect()
}

fn delay(params: &[(&str, f64)]) -> Box<dyn PluginInstance> {
    let mut d = NativeDelay::new(SR);
    d.set_named("tempo_bpm", 120.0);
    for (n, v) in params {
        d.set_named(n, *v);
    }
    for (n, v) in overrides("PA_DELAY") {
        d.set_named(&n, v);
    }
    Box::new(d)
}

fn reverb(params: &[(&str, f64)]) -> Box<dyn PluginInstance> {
    let mut r = NativeReverb::new(SR);
    for (n, v) in params {
        r.set_named(n, *v);
    }
    for (n, v) in overrides("PA_REVERB") {
        r.set_named(&n, v);
    }
    Box::new(r)
}

const FLUTE_ECHO: &[(&str, f64)] = &[
    ("mix", 1.0),
    ("style", 6.0),
    ("tap_div_l", 1.0),
    ("tap_div_r", 1.0),
    ("time", 340.0),
    ("feedback", 0.55),
    ("level", -3.0),
    ("high_pass", 300.0),
    ("high_cut", 6000.0),
    ("duck_sens", 0.0),
    ("interval", 27.0),
    ("slice", 2.0),
    ("blend", 1.0),
    ("mod_depth", 0.2),
    ("dry", 0.25),
];
const FLUTE_WASH: &[(&str, f64)] = &[
    ("mix", 1.0),
    ("style", 0.0),
    ("tap_div_l", 8.0),
    ("tap_div_r", 8.0),
    ("time", 680.0),
    ("feedback", 0.7),
    ("level", -5.0),
    ("high_pass", 300.0),
    ("high_cut", 3500.0),
    ("duck_sens", 0.0),
    ("mod_depth", 0.35),
];
const HEAVEN: &[(&str, f64)] = &[
    ("mix", 1.0),
    ("algorithm", 6.0),
    ("variant", 0.0),
    ("decay", 0.9312),
    ("predelay", 80.0),
    ("size", 1.0),
    ("damping", 0.3),
    ("low_cut", 500.0),
    ("high_cut", 8000.0),
    ("modulation", 0.5),
    ("level", -9.0),
    ("shim_amount", 0.5),
    ("shim_shift1", 12.0),
    ("shim_shift2", 12.0),
    ("shim_voice2", 1.0),
];

/// A stereo processor stage.
enum Stage {
    Block(Box<dyn PluginInstance>),
    Chain(PitchChain),
}

fn stages(case: &str) -> Option<Vec<Stage>> {
    let pc = |algo: Algorithm, st: f64, live: bool| {
        let mut c = PitchChain::new();
        c.algorithm = algo;
        c.semitones = st;
        c.mix = 1.0;
        c.live = live;
        c.update(AudioConfig {
            sample_rate: SR,
            max_buffer_size: BLOCK,
        });
        Stage::Chain(c)
    };
    Some(match case {
        "ice_flute_echo" => vec![Stage::Block(delay(FLUTE_ECHO))],
        "ice_oct_down" => {
            let mut p: Vec<(&str, f64)> = FLUTE_ECHO.to_vec();
            p.push(("interval", 0.0));
            vec![Stage::Block(delay(&p))]
        }
        "ice_fifth_up" => {
            let mut p: Vec<(&str, f64)> = FLUTE_ECHO.to_vec();
            p.push(("interval", 22.0)); // +7
            vec![Stage::Block(delay(&p))]
        }
        "shimmer_delay" => vec![Stage::Block(delay(&[
            ("mix", 1.0),
            ("style", 4.0),
            ("time", 420.0),
            ("feedback", 0.6),
            ("dry", 0.3),
            ("high_cut", 7000.0),
        ]))],
        "heaven_shimmer" => vec![Stage::Block(reverb(HEAVEN))],
        "heaven_noshim" => {
            let mut p: Vec<(&str, f64)> = HEAVEN.to_vec();
            p.push(("shim_amount", 0.0));
            vec![Stage::Block(reverb(&p))]
        }
        "heaven_input" => {
            let mut p: Vec<(&str, f64)> = HEAVEN.to_vec();
            p.push(("shim_fb_mode", 0.0));
            vec![Stage::Block(reverb(&p))]
        }
        "flute_patch_pitch_path" => vec![
            Stage::Block(delay(FLUTE_ECHO)),
            Stage::Block(delay(FLUTE_WASH)),
            Stage::Block(reverb(HEAVEN)),
        ],
        "pitch_psola_+12" => vec![pc(Algorithm::Psola, 12.0, false)],
        "pitch_psola_-12" => vec![pc(Algorithm::Psola, -12.0, false)],
        "pitch_wsola_+12" => vec![pc(Algorithm::Wsola, 12.0, false)],
        "pitch_wsola_-12" => vec![pc(Algorithm::Wsola, -12.0, false)],
        "pitch_spectral_+12" => vec![pc(Algorithm::Spectral, 12.0, false)],
        "pitch_spectral_-12" => vec![pc(Algorithm::Spectral, -12.0, false)],
        "pitch_spectral_live_+12" => vec![pc(Algorithm::Spectral, 12.0, true)],
        "pitch_spectral_live_-12" => vec![pc(Algorithm::Spectral, -12.0, true)],
        _ => return None,
    })
}

const CASES: &[&str] = &[
    "ice_flute_echo",
    "ice_oct_down",
    "ice_fifth_up",
    "shimmer_delay",
    "heaven_shimmer",
    "heaven_input",
    "heaven_noshim",
    "flute_patch_pitch_path",
    "pitch_psola_+12",
    "pitch_psola_-12",
    "pitch_wsola_+12",
    "pitch_wsola_-12",
    "pitch_spectral_+12",
    "pitch_spectral_-12",
    "pitch_spectral_live_+12",
    "pitch_spectral_live_-12",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: pitch_audition <out_dir> <tag> <input.wav>...");
        return;
    }
    let (out_dir, tag) = (&args[1], &args[2]);
    std::fs::create_dir_all(out_dir).ok();
    let only = std::env::var("PA_ONLY").ok();
    for input_path in &args[3..] {
        let synth = input_path
            .strip_prefix("sine:")
            .and_then(|f| f.parse::<f64>().ok())
            .map(|f| {
                (0..48_000 * 3)
                    .map(|i| ((std::f64::consts::TAU * f * f64::from(i) / SR).sin() * 0.05) as f32)
                    .collect::<Vec<f32>>()
            });
        let synth = synth.or_else(|| (input_path == "tone").then(tone));
        let synth = synth.or_else(|| {
            (input_path == "impulse").then(|| {
                let mut v = vec![0.0f32; 48_000];
                v[100] = 0.5;
                v
            })
        });
        let Some(mut x) = synth.or_else(|| read_wav(input_path)) else {
            eprintln!("cannot read {input_path}");
            continue;
        };
        x.truncate(30 * 48_000);
        // + 8 s of tail
        x.extend(std::iter::repeat_n(0.0, 8 * 48_000));
        let stem = std::path::Path::new(input_path)
            .file_stem()
            .map(|s| s.to_string_lossy().replace(':', "_"))
            .unwrap_or_default();
        for case in CASES {
            if let Some(o) = &only {
                if !o.split(',').any(|p| case.starts_with(p)) {
                    continue;
                }
            }
            let Some(mut st) = stages(case) else { continue };
            for s in &mut st {
                if let Stage::Block(b) = s {
                    b.prepare(SR, BLOCK as u32).expect("prepare");
                }
            }
            let (mut l, mut r) = (x.clone(), x.clone());
            let t = Instant::now();
            let (mut ol, mut or) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
            let (mut dl, mut dr) = (vec![0.0f64; BLOCK], vec![0.0f64; BLOCK]);
            for s in &mut st {
                let mut i = 0;
                while i < l.len() {
                    let n = BLOCK.min(l.len() - i);
                    match s {
                        Stage::Block(b) => {
                            b.process_block(
                                &l[i..i + n],
                                &r[i..i + n],
                                &mut ol[..n],
                                &mut or[..n],
                                &PluginEvents::EMPTY,
                            )
                            .expect("process");
                            l[i..i + n].copy_from_slice(&ol[..n]);
                            r[i..i + n].copy_from_slice(&or[..n]);
                        }
                        Stage::Chain(c) => {
                            for k in 0..n {
                                dl[k] = f64::from(l[i + k]);
                                dr[k] = f64::from(r[i + k]);
                            }
                            c.process(&mut dl[..n], &mut dr[..n]);
                            for k in 0..n {
                                l[i + k] = dl[k] as f32;
                                r[i + k] = dr[k] as f32;
                            }
                        }
                    }
                    i += n;
                }
            }
            let ns = t.elapsed().as_nanos() as f64 / l.len() as f64;
            let peak = l.iter().chain(&r).fold(0.0f32, |m, v| m.max(v.abs()));
            let rms = (l
                .iter()
                .chain(&r)
                .map(|v| f64::from(*v) * f64::from(*v))
                .sum::<f64>()
                / (2 * l.len()) as f64)
                .sqrt();
            let scores = if input_path == "tone" {
                let seg = &l[72_000..144_000];
                format!(
                    "  nonharm {:>6.1} dB  am {:>5.2} dB",
                    nonharmonic_db(seg),
                    am_db(seg)
                )
            } else {
                String::new()
            };
            println!(
                "{case:<26} {stem:<20} peak {peak:>7.3}  rms {:>7.1} dBFS  {ns:>6.0} ns/smp{scores}",
                20.0 * rms.log10()
            );
            write_wav(&format!("{out_dir}/{case}_{stem}_{tag}.wav"), &l, &r);
        }
    }
}
