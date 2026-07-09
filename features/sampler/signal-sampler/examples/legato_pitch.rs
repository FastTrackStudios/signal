//! Detect the start vs end pitch of a CSS legato sample via FFT, to settle
//! whether "up_A4" means A4→(up) [source-labeled] or (up)→A4 [dest-labeled].
//!
//! ```text
//! cargo run --release -p signal-sampler --example legato_pitch -- "<path to ff_up_A4_1.wav>"
//! ```

use std::f32::consts::PI;

fn read_wav(path: &str) -> (Vec<f32>, u32) {
    let d = std::fs::read(path).expect("wav");
    let mut p = 12;
    let mut fmt = (1u16, 2u16, 48000u32, 16u16);
    let mut data: &[u8] = &[];
    while p + 8 <= d.len() {
        let id = &d[p..p + 4];
        let sz = u32::from_le_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize;
        let body = &d[p + 8..(p + 8 + sz).min(d.len())];
        if id == b"fmt " {
            let mut f = u16::from_le_bytes([body[0], body[1]]);
            if f == 0xFFFE && body.len() >= 26 {
                f = u16::from_le_bytes([body[24], body[25]]);
            }
            fmt = (
                f,
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
    let by = (bits / 8) as usize;
    let frames = data.len() / (by * ch);
    let mut mono = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut acc = 0.0f32;
        for c in 0..ch {
            let o = (f * ch + c) * by;
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

fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let a = i + k;
                let b = i + k + len / 2;
                let tr = cr * re[b] - ci * im[b];
                let ti = cr * im[b] + ci * re[b];
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
            i += len;
        }
        len <<= 1;
    }
}

fn note_name(hz: f32) -> String {
    if hz <= 0.0 {
        return "?".into();
    }
    let midi = (69.0 + 12.0 * (hz / 440.0).log2()).round() as i32;
    const N: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", N[(midi.rem_euclid(12)) as usize], midi / 12 - 1)
}

/// Autocorrelation pitch (robust to octave/harmonic errors) over [180,450]Hz.
fn pitch(sig: &[f32], sr: u32, start_s: f32) -> (f32, String) {
    let _ = fft; // FFT kept above for reference; ACF is more robust here.
    let n = (0.35 * sr as f32) as usize;
    let s = (start_s * sr as f32) as usize;
    if s + n >= sig.len() {
        return (0.0, "?".into());
    }
    let w = &sig[s..s + n];
    let lag_min = (sr as f32 / 450.0) as usize;
    let lag_max = (sr as f32 / 180.0) as usize;
    let energy: f32 = w.iter().map(|x| x * x).sum::<f32>().max(1e-9);
    let (mut best_lag, mut best_r) = (lag_min, 0.0f32);
    for lag in lag_min..=lag_max {
        let mut r = 0.0f32;
        for i in 0..n - lag {
            r += w[i] * w[i + lag];
        }
        let r = r / energy;
        if r > best_r {
            best_r = r;
            best_lag = lag;
        }
    }
    (
        sr as f32 / best_lag as f32,
        note_name(sr as f32 / best_lag as f32),
    )
}

fn main() {
    let path = std::env::args().nth(1).expect("path to legato wav");
    let (sig, sr) = read_wav(&path);
    let dur = sig.len() as f32 / sr as f32;
    println!("{}  ({:.2}s)", path.rsplit('/').next().unwrap(), dur);
    for t in [0.1f32, 0.3, 0.5, 0.8, 1.2, 1.6, 1.9] {
        if t + 0.35 < dur {
            let (hz, n) = pitch(&sig, sr, t);
            println!("  @{t:.1}s  {hz:6.1}Hz  {n}");
        }
    }
}
