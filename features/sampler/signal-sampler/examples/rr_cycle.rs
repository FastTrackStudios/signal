//! Round-robin cycle analysis. Section 3 of the full test plays, per short
//! articulation, a CC59 RR-reset then 12× the same note — exposing CSS's
//! deterministic RR cycle. For each of those 12 CSS hits we sweep our
//! `set_forced_rr` slots and score spectral similarity, building a matrix. Two
//! questions: (1) are our RR slots distinguishable at all (does the best slot
//! beat the others by a meaningful margin)? (2) does the best-slot sequence
//! reveal a stable mapping from CSS's cycle to our slots?
//!
//! ```text
//! cargo run --release -p signal-sampler --example rr_cycle -- css_test_full_export.wav 13
//! ```
//! arg2 = CC58 keyswitch of the articulation (13=Spiccato default).

use std::f32::consts::PI;
use std::path::PathBuf;

use signal_sampler::SamplerRig;

const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
const CSS_CONFIG: &str =
    "/run/media/Development/FastTrackStudio/sample-collector/specs/cinematic-strings.styx";
const ID: &str = "strings_1v";
const SR: u32 = 48_000;
const FFT_N: usize = 8192;
const BANDS: usize = 48;
const NOTE: u8 = 67; // G4
const SLOTS: u32 = 6;

// RR-exposure section (gen_css_test_full §3): first artic block starts at 146.8,
// each is 12 notes @0.8s then +1.0s gap = 10.6s per block. Order matches SHORTS.
const RR_SECTION_START: f64 = 146.8;
const SHORT_KS_ORDER: &[u8] = &[13, 18, 23, 28, 33, 38, 43, 68];

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
    let bin_hz = SR as f32 / FFT_N as f32;
    for bin in 1..FFT_N / 2 {
        let f = bin as f32 * bin_hz;
        if !(40.0..=18_000.0).contains(&f) {
            continue;
        }
        let frac = (f / 40.0).ln() / (18_000.0f32 / 40.0).ln();
        let band = ((frac * BANDS as f32) as usize).min(BANDS - 1);
        bands[band] += re[bin] * re[bin] + im[bin] * im[bin];
    }
    let max = bands.iter().cloned().fold(0.0f32, f32::max).max(1e-12);
    for b in bands.iter_mut() {
        *b = (10.0 * (*b / max).max(1e-12).log10()).max(-50.0);
    }
    bands
}

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

fn main() -> eyre::Result<()> {
    let refp = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "css_test_full_export.wav".into());
    let ks: u8 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(13);
    let block = SHORT_KS_ORDER.iter().position(|&k| k == ks).unwrap_or(0);
    let sec_start = RR_SECTION_START + block as f64 * 10.6 + 0.2; // first note time
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
    rig.cc(ID, 58, ks);

    // Our slot fingerprints (forced RR 0..SLOTS), isolated renders.
    let our: Vec<[f32; BANDS]> = (0..SLOTS)
        .map(|rr| {
            rig.panic(ID);
            rig.cc(ID, 58, ks);
            rig.set_forced_rr(ID, Some(rr));
            rig.warm_note(ID, NOTE);
            rig.note_on(ID, NOTE, 100);
            let mut buf = vec![0.0f32; FFT_N * 2];
            rig.render_offline(&mut buf).ok();
            let mono: Vec<f32> = buf.chunks_exact(2).map(|s| 0.5 * (s[0] + s[1])).collect();
            band_spectrum(&mono)
        })
        .collect();

    // Pairwise similarity among OUR slots → are they even distinguishable?
    let mut min_pair = 1.0f32;
    for a in 0..SLOTS as usize {
        for b in a + 1..SLOTS as usize {
            min_pair = min_pair.min(cosine(&our[a], &our[b]));
        }
    }
    println!("CC58={ks} RR-cycle analysis (G4 vel100), section @{sec_start:.1}s");
    println!(
        "our-slot distinguishability: min pairwise cos = {min_pair:.3}  (1.0 = identical → unresolvable)\n"
    );

    println!("CSS hit → best-matching our slot (and full row):");
    let mut seq = Vec::new();
    for hit in 0..12 {
        let t = sec_start + hit as f64 * 0.8;
        let start = (t * css_sr as f64) as usize;
        if start + FFT_N >= css.len() {
            break;
        }
        let cspec = band_spectrum(&css[start..start + FFT_N]);
        let sims: Vec<f32> = our.iter().map(|o| cosine(&cspec, o)).collect();
        let (best, bv) = sims
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, v)| (i, *v))
            .unwrap();
        let margin = bv
            - sims
                .iter()
                .cloned()
                .filter(|&v| v < bv)
                .fold(0.0f32, f32::max);
        seq.push(best);
        let row: String = sims
            .iter()
            .map(|v| format!("{v:.3}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!("  hit{hit:>2} → RR{best} (cos {bv:.3}, margin {margin:+.3})  [{row}]");
    }
    println!("\ncycle as our-slots: {seq:?}");
    println!("If margins are tiny (<0.01), RR is not reliably resolvable from audio —");
    println!("expected: RR variants are near-identical, and the manual says any RR sounds fine.");
    Ok(())
}
