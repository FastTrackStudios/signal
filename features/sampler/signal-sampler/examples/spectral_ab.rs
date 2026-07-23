//! Spectral A/B: compare our engine's output to a real CSS render per note,
//! using a log-band magnitude-spectrum distance instead of phase-cancellation.
//!
//! Phase-null fails here because CSS's default amp envelope (attack/release) and
//! sample-start handling reshape the dry samples in time — but the *spectrum* is
//! preserved, so a magnitude comparison is the right tool. For every short note
//! we sweep the round-robin slot (`set_forced_rr`), render the note in
//! isolation, and score each against the CSS window by spectral cosine
//! similarity. The best slot is the RR CSS most likely played; the mean
//! similarity tells us how close our default settings are to CSS's default.
//!
//! ```text
//! cargo run --release -p signal-sampler --example spectral_ab -- css_test.mid css_test_export.wav
//! ```

use std::f32::consts::PI;
use std::path::PathBuf;

use signal_sampler::SamplerRig;

const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
const CSS_CONFIG: &str =
    "features/rigs/orchestra/specs/cinematic-strings.styx";
const ID: &str = "strings_1v";
const SR: u32 = 48_000;
const MAX_RR: u32 = 6;
const FFT_N: usize = 8192; // ~0.17s analysis window
const BANDS: usize = 48; // log-spaced bands
const SHORT_KS: &[u8] = &[13, 18, 23, 28, 33, 38, 43, 68];

/// CC58 keyswitch → articulation name (from gen_css_test_midi's SHORTS table).
fn ks_name(ks: u8) -> &'static str {
    match ks {
        13 => "Spiccato",
        18 => "Staccatissimo",
        23 => "Staccato",
        28 => "Sfz",
        33 => "Pizzicato",
        38 => "Bartok snap",
        43 => "Col Legno",
        68 => "Marcato",
        _ => "?",
    }
}

fn read_vlq(d: &[u8], p: &mut usize) -> u32 {
    let mut v = 0u32;
    loop {
        let b = d[*p];
        *p += 1;
        v = (v << 7) | (b & 0x7f) as u32;
        if b & 0x80 == 0 {
            break;
        }
    }
    v
}

fn parse_smf(d: &[u8]) -> Vec<(f64, u8, u8, u8)> {
    let div = u16::from_be_bytes([d[12], d[13]]) as f64;
    let mut us_per_q = 500_000.0f64;
    let mut p = 14;
    while &d[p..p + 4] != b"MTrk" {
        let len = u32::from_be_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize;
        p += 8 + len;
    }
    let track_len = u32::from_be_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize;
    p += 8;
    let end = p + track_len;
    let mut sec = 0.0f64;
    let mut running = 0u8;
    let mut out = Vec::new();
    while p < end {
        let dt = read_vlq(d, &mut p) as u64;
        sec += dt as f64 * (us_per_q / 1_000_000.0) / div;
        let mut status = d[p];
        if status & 0x80 != 0 {
            p += 1;
            running = status;
        } else {
            status = running;
        }
        match status {
            0xFF => {
                let meta = d[p];
                p += 1;
                let len = read_vlq(d, &mut p) as usize;
                if meta == 0x51 {
                    us_per_q =
                        (d[p] as f64) * 65536.0 + (d[p + 1] as f64) * 256.0 + d[p + 2] as f64;
                }
                p += len;
            }
            0xF0 | 0xF7 => {
                let len = read_vlq(d, &mut p) as usize;
                p += len;
            }
            s if (0x80..=0xEF).contains(&s) => {
                let d1 = d[p];
                let two = !matches!(s & 0xF0, 0xC0 | 0xD0);
                let d2 = if two { d[p + 1] } else { 0 };
                p += if two { 2 } else { 1 };
                out.push((sec, s, d1, d2));
            }
            _ => break,
        }
    }
    out
}

fn read_wav(path: &str) -> (Vec<f32>, u32) {
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
    let frames = data.len() / (bytes * ch);
    let mut mono = Vec::with_capacity(frames);
    for f in 0..frames {
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

/// In-place iterative radix-2 FFT.
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

/// Log-spaced band-energy vector (length BANDS) from a window, Hann-windowed.
/// Robust to level (caller normalises) and to time-shift (magnitude only).
fn band_spectrum(sig: &[f32]) -> [f32; BANDS] {
    let mut re = vec![0.0f32; FFT_N];
    let mut im = vec![0.0f32; FFT_N];
    let n = sig.len().min(FFT_N);
    for i in 0..n {
        let w = 0.5 - 0.5 * (2.0 * PI * i as f32 / (FFT_N as f32 - 1.0)).cos();
        re[i] = sig[i] * w;
    }
    fft(&mut re, &mut im);
    let mut bands = [0.0f32; BANDS];
    // Map bins [1, FFT_N/2) to log-spaced bands over ~40Hz..18kHz.
    let f_lo = 40.0f32;
    let f_hi = 18_000.0f32;
    let bin_hz = SR as f32 / FFT_N as f32;
    for bin in 1..FFT_N / 2 {
        let f = bin as f32 * bin_hz;
        if f < f_lo || f > f_hi {
            continue;
        }
        let frac = (f / f_lo).ln() / (f_hi / f_lo).ln();
        let band = ((frac * BANDS as f32) as usize).min(BANDS - 1);
        bands[band] += re[bin] * re[bin] + im[bin] * im[bin];
    }
    // Per-band dB relative to this window's peak band, FLOORED at -50 dB.
    // dB is perceptual; the floor clamps near-silent bands (e.g. a spiccato's
    // post-transient tail) to a constant instead of letting their noise floor
    // swing a mean-centered cosine — without over-weighting exact harmonic peaks
    // the way a raw-magnitude cosine does.
    let max = bands.iter().cloned().fold(0.0f32, f32::max).max(1e-12);
    for b in bands.iter_mut() {
        *b = (10.0 * (*b / max).max(1e-12).log10()).max(-50.0);
    }
    bands
}

/// Mean-centered cosine of two floored-dB band vectors (overall level/EQ-tilt
/// removed). 1.0 = identical spectral shape.
fn cosine(a: &[f32; BANDS], b: &[f32; BANDS]) -> f32 {
    let ma = a.iter().sum::<f32>() / BANDS as f32;
    let mb = b.iter().sum::<f32>() / BANDS as f32;
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..BANDS {
        let (x, y) = (a[i] - ma, b[i] - mb);
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= 0.0 || nb <= 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

fn render_mono(rig: &SamplerRig, frames: usize) -> Vec<f32> {
    let mut buf = vec![0.0f32; frames * 2];
    rig.render_offline(&mut buf).ok();
    buf.chunks_exact(2).map(|s| 0.5 * (s[0] + s[1])).collect()
}

fn main() -> eyre::Result<()> {
    let mid = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "css_test.mid".into());
    let refp = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "css_test_export.wav".into());
    let events = parse_smf(&std::fs::read(&mid)?);
    let (css, css_sr) = read_wav(&refp);

    let css_root = PathBuf::from(CSS_ROOT);
    let spec = css_root
        .join("_patches")
        .join("1st Violins")
        .join("library.styx");
    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    rig.load_instrument_with_config(
        ID,
        &PathBuf::from(CSS_CONFIG),
        &spec,
        &css_root,
        "1st Violins",
        "Mix",
    )?;
    rig.set_solo_mic(ID, Some("Mix".into()));
    // CSS-parity harness: reproduce Kontakt's expressive reactive latency.
    // The strict live policy (PlayMode::StrictLive) would otherwise force the
    // low_latency tables regardless of the MIDI's CC58 "expressive" request.
    rig.set_legato_mode(ID, true, true);

    let mut cur_cc58 = 0u8;
    println!("\n# Short-note spectral A/B (FFT {FFT_N}, {BANDS} log bands)");
    println!("# artic            vel  bestRR  cos-sim");
    let mut sims = Vec::new();
    let mut dumped = false;
    let mut by_artic: std::collections::BTreeMap<u8, Vec<f32>> = std::collections::BTreeMap::new();

    for (sec, status, d1, d2) in &events {
        if status & 0xF0 == 0xB0 && *d1 == 58 {
            cur_cc58 = *d2;
        }
        if !(status & 0xF0 == 0x90 && *d2 > 0 && SHORT_KS.contains(&cur_cc58)) {
            continue;
        }
        let start = (*sec * css_sr as f64) as usize;
        if start + FFT_N >= css.len() {
            continue;
        }
        let win = &css[start..start + FFT_N];
        // Energy gate: skip near-silent CSS windows (e.g. vel-1 hits) whose
        // spectrum is just noise and would pollute the similarity score.
        let rms = (win.iter().map(|x| x * x).sum::<f32>() / win.len() as f32).sqrt();
        if rms < 0.0015 {
            continue;
        }
        let css_spec = band_spectrum(win);

        let mut per = Vec::new();
        for rr in 0..MAX_RR {
            rig.panic(ID);
            rig.cc(ID, 58, cur_cc58);
            rig.set_forced_rr(ID, Some(rr));
            rig.warm_note(ID, *d1);
            rig.note_on(ID, *d1, *d2);
            let ours = render_mono(&rig, FFT_N);
            per.push(cosine(&css_spec, &band_spectrum(&ours)));
        }
        let (best_rr, best) = per
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, v)| (i, *v))
            .unwrap();
        // Debug: dump band spectra for the first note of the CSS_DUMP_KS artic.
        if std::env::var("CSS_DUMP_KS").ok().as_deref() == Some(&cur_cc58.to_string()) && !dumped {
            dumped = true;
            rig.panic(ID);
            rig.cc(ID, 58, cur_cc58);
            rig.set_forced_rr(ID, Some(best_rr as u32));
            rig.warm_note(ID, *d1);
            rig.note_on(ID, *d1, *d2);
            let ours_spec = band_spectrum(&render_mono(&rig, FFT_N));
            eprintln!(
                "\n# band dump {} vel{} (best RR{best_rr})",
                ks_name(cur_cc58),
                d2
            );
            eprintln!("# band  Hz~     CSS_log  OUR_log  diff");
            let f_lo = 40.0f32;
            let ratio = (18_000.0f32 / f_lo).powf(1.0 / BANDS as f32);
            for b in 0..BANDS {
                let hz = f_lo * ratio.powi(b as i32);
                eprintln!(
                    "  {b:>2}  {hz:>7.0}   {:7.2}  {:7.2}  {:+.2}",
                    css_spec[b],
                    ours_spec[b],
                    ours_spec[b] - css_spec[b]
                );
            }
        }
        sims.push(best);
        by_artic.entry(cur_cc58).or_default().push(best);
        println!(
            "  {:<16} {:>4}   RR{best_rr}   {best:6.3}",
            ks_name(cur_cc58),
            d2
        );
    }
    rig.set_forced_rr(ID, None);

    if !sims.is_empty() {
        println!("\n# Per-articulation mean spectral similarity (vs CSS default):");
        let mut ranked: Vec<(u8, f32)> = by_artic
            .iter()
            .map(|(k, v)| (*k, v.iter().sum::<f32>() / v.len() as f32))
            .collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        for (ks, m) in &ranked {
            let bar = "█".repeat((m * 30.0) as usize);
            println!("  {:<16} {:.3}  {bar}", ks_name(*ks), m);
        }
        let mean = sims.iter().sum::<f32>() / sims.len() as f32;
        let worst = sims.iter().cloned().fold(1.0f32, f32::min);
        println!(
            "\n{} short notes  ·  overall mean = {mean:.3}  ·  worst = {worst:.3}",
            sims.len()
        );
        println!("(1.000 = identical spectral shape; >0.95 = very close timbre)");
    }
    Ok(())
}
