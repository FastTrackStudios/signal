//! Null-test CEILING check: null the *raw* CSS Mix sample files directly against
//! the CSS export window (bypassing our engine entirely). If a raw sample nulls
//! deeply, phase-cancellation is achievable and our engine is the variable to
//! fix. If even the raw file won't null, the export carries processing
//! (reverb/EQ/limiting) and a phase-null test is not viable — we'd switch to a
//! spectral / envelope match instead.
//!
//! ```text
//! cargo run --release -p signal-sampler --example null_ceiling -- css_test_export.wav
//! ```

use std::path::PathBuf;

const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";

fn read_wav(path: &str, start_s: f64, dur_s: f64) -> (Vec<f32>, u32) {
    let d = std::fs::read(path).expect("read wav");
    let mut p = 12;
    let mut fmt = (1u16, 2u16, 48000u32, 16u16);
    let mut data: &[u8] = &[];
    while p + 8 <= d.len() {
        let id = &d[p..p + 4];
        let sz = u32::from_le_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize;
        let body = &d[p + 8..(p + 8 + sz).min(d.len())];
        if id == b"fmt " {
            let mut format = u16::from_le_bytes([body[0], body[1]]);
            // WAVE_FORMAT_EXTENSIBLE → real format is the subformat GUID prefix.
            if format == 0xFFFE && body.len() >= 26 {
                format = u16::from_le_bytes([body[24], body[25]]);
            }
            fmt = (
                format,
                u16::from_le_bytes([body[2], body[3]]),
                u32::from_le_bytes([body[4], body[5], body[6], body[7]]),
                u16::from_le_bytes([body[14], body[15]]),
            );
        } else if id == b"data" {
            data = body;
        }
        p += 8 + sz + (sz & 1);
    }
    let (format, ch, sr, bits) = fmt;
    let ch = ch as usize;
    let bytes = (bits / 8) as usize;
    let total = data.len() / (bytes * ch);
    let start = ((start_s * sr as f64) as usize).min(total);
    let n = (((dur_s * sr as f64) as usize).min(total - start)).max(0);
    let mut mono = Vec::with_capacity(n);
    for f in start..start + n {
        let mut acc = 0.0f32;
        for c in 0..ch {
            let o = (f * ch + c) * bytes;
            let s = match (format, bits) {
                (3, 32) => f32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]),
                (1, 16) => i16::from_le_bytes([data[o], data[o + 1]]) as f32 / 32768.0,
                (1, 24) => {
                    let v = (data[o] as i32)
                        | ((data[o + 1] as i32) << 8)
                        | ((data[o + 2] as i32) << 16);
                    let v = if v & 0x80_0000 != 0 {
                        v | !0xFF_FFFF
                    } else {
                        v
                    };
                    v as f32 / 8_388_608.0
                }
                (1, 32) => {
                    i32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]) as f32
                        / 2_147_483_648.0
                }
                _ => 0.0,
            };
            acc += s;
        }
        mono.push(acc / ch as f32);
    }
    (mono, sr)
}

/// Best-gain, best-lag null depth in dB. `a`=reference, `b`=candidate.
fn null_depth_db(a: &[f32], b: &[f32], max_lag: i64) -> (f32, i64) {
    let n = a.len().min(b.len());
    let re: f32 = a[..n].iter().map(|x| x * x).sum();
    if re <= 0.0 || n == 0 {
        return (0.0, 0);
    }
    let mut best = f32::INFINITY;
    let mut best_lag = 0;
    let mut lag = -max_lag;
    while lag <= max_lag {
        let mut dot = 0.0f32;
        let mut bb = 0.0f32;
        for i in 0..n {
            let j = i as i64 + lag;
            if j >= 0 && (j as usize) < b.len() {
                dot += a[i] * b[j as usize];
                bb += b[j as usize] * b[j as usize];
            }
        }
        if bb > 0.0 {
            let g = dot / bb;
            let mut resid = 0.0f32;
            for i in 0..n {
                let j = i as i64 + lag;
                let bv = if j >= 0 && (j as usize) < b.len() {
                    b[j as usize]
                } else {
                    0.0
                };
                let e = a[i] - g * bv;
                resid += e * e;
            }
            if resid < best {
                best = resid;
                best_lag = lag;
            }
        }
        lag += 1;
    }
    (10.0 * (best / re).max(1e-12).log10(), best_lag)
}

fn main() {
    let refp = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "css_test_export.wav".into());
    // The vel127 spiccato hits in the shorts section of css_test.mid.
    let windows = [(98.0, 127), (101.6, 127), (105.2, 127), (108.8, 127)];
    let dir = PathBuf::from(CSS_ROOT).join("Mix/1st Violins/Short/Spiccato");
    let max_lag = (0.250 * 48000.0) as i64; // wide: samples have pre-roll before attack
    let dur = 0.30;

    // Candidate raw samples: every ff G4 round-robin.
    let mut cands: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("ff_G4_") && n.ends_with(".wav"))
                })
                .collect()
        })
        .unwrap_or_default();
    cands.sort();
    println!("candidates: {} raw Mix ff_G4 round-robins\n", cands.len());

    for (t, vel) in windows {
        let (a, _) = read_wav(&refp, t, dur);
        let css_peak = a.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        println!("-- export t={t}s (vel{vel})  cssPk={css_peak:.3} --");
        let mut best = (f32::INFINITY, String::new(), 0i64);
        for c in &cands {
            let (b, _) = read_wav(c.to_str().unwrap(), 0.0, dur + 0.30);
            let (db, lag) = null_depth_db(&a, &b, max_lag);
            let name = c.file_name().unwrap().to_str().unwrap().to_string();
            println!(
                "   {name:<14} null={db:6.1}dB  lag={:+}ms",
                lag * 1000 / 48000
            );
            if db < best.0 {
                best = (db, name, lag);
            }
        }
        println!(
            "   => best: {} at {:.1}dB (lag {:+}ms)\n",
            best.1,
            best.0,
            best.2 * 1000 / 48000
        );
    }
    println!("If best null ≈ -15dB or deeper → phase-null is viable; our engine is the variable.");
    println!("If best null ≈ 0..-3dB → export is processed; use spectral match instead.");
}
