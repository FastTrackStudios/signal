//! Round-robin null test against a real CSS render.
//!
//! Per the CSS v1.7 manual, round-robins are the *only* nondeterministic axis;
//! CC59 makes a render reproducible. This harness drives our engine through the
//! same test MIDI and, for every **short** note (non-overlapping, fully
//! decaying — the clean RR case), sweeps the round-robin slot via the new
//! `set_forced_rr` override, renders the note in isolation, and nulls it against
//! the CSS audio at that note's window. The slot with the deepest null is the
//! one CSS actually played; the mean null depth tells us how close we are.
//!
//! ```text
//! cargo run --release -p signal-sampler --example null_css_rr -- css_test.mid css_test_export.wav
//! ```

use std::path::PathBuf;

use signal_sampler::SamplerRig;

const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
const CSS_CONFIG: &str =
    "/run/media/Development/FastTrackStudio/sample-collector/specs/cinematic-strings.styx";
const ID: &str = "strings_1v";
const SR: u32 = 48_000;
const MAX_RR: u32 = 6; // sweep slots 0..MAX_RR (clamped per articulation)

/// CC58 keyswitch values the test MIDI uses for **short** articulations (from
/// gen_css_test_midi). These are the RR-bearing, non-overlapping triggers.
const SHORT_KS: &[u8] = &[13, 18, 23, 28, 33, 38, 43, 68];

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

/// Read a PCM WAV (16/24/32-bit int or 32-bit float) → mono f32 + sample rate.
fn read_wav(path: &str) -> (Vec<f32>, u32) {
    let d = std::fs::read(path).expect("read wav");
    let mut p = 12; // skip RIFF....WAVE
    let mut fmt = (1u16, 2u16, 48000u32, 16u16); // (format, channels, sr, bits)
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

/// Best-gain, best-lag normalised null residual between `a` (reference window)
/// and `b` (our render). Returns null depth in dB (more negative = deeper null).
/// Searches lag ±`max_lag` samples and the optimal scalar gain at each lag.
fn null_depth_db(a: &[f32], b: &[f32], max_lag: i64) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let ref_energy: f32 = a[..n].iter().map(|x| x * x).sum();
    if ref_energy <= 0.0 {
        return 0.0;
    }
    let mut best = f32::INFINITY;
    let mut lag = -max_lag;
    while lag <= max_lag {
        let mut dot = 0.0f32;
        let mut bb = 0.0f32;
        for i in 0..n {
            let j = i as i64 + lag;
            if j < 0 || j as usize >= b.len() {
                continue;
            }
            let bv = b[j as usize];
            dot += a[i] * bv;
            bb += bv * bv;
        }
        if bb > 0.0 {
            let g = dot / bb; // optimal gain
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
            }
        }
        lag += 1;
    }
    10.0 * (best / ref_energy).max(1e-12).log10()
}

fn render_window(rig: &SamplerRig, frames: usize) -> Vec<f32> {
    let mut buf = vec![0.0f32; frames * 2];
    rig.render_offline(&mut buf).ok();
    // Down-mix to mono.
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
    eprintln!(
        "css ref: {} samples @ {}Hz ({:.1}s);  {} midi events",
        css.len(),
        css_sr,
        css.len() as f32 / css_sr as f32,
        events.len()
    );

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

    let win_s = 0.45f64; // short-note window
    let win = (win_s * SR as f64) as usize;
    let max_lag = (0.060 * SR as f64) as i64; // ±60ms alignment search
    let mut cur_cc58 = 0u8;

    println!("\n# Short-note RR null sweep (window {win_s}s, lag ±10ms)");
    println!("# note  t(s)   vel  bestRR  nullDepth   per-slot dB");
    let mut depths = Vec::new();

    for (sec, status, d1, d2) in &events {
        if status & 0xF0 == 0xB0 && *d1 == 58 {
            cur_cc58 = *d2;
        }
        let is_short_note_on = status & 0xF0 == 0x90 && *d2 > 0 && SHORT_KS.contains(&cur_cc58);
        if !is_short_note_on {
            continue;
        }
        // CSS reference window for this note.
        let start = (*sec * css_sr as f64) as usize;
        let a = &css[start.min(css.len())..(start + win).min(css.len())];
        if a.len() < win / 2 {
            continue;
        }
        // Sweep RR slots; render this note isolated each time.
        let mut per_slot = Vec::new();
        let mut our_peak = 0.0f32;
        for rr in 0..MAX_RR {
            rig.panic(ID);
            rig.cc(ID, 58, cur_cc58);
            rig.set_forced_rr(ID, Some(rr));
            rig.warm_note(ID, *d1);
            rig.note_on(ID, *d1, *d2);
            let b = render_window(&rig, win);
            our_peak = our_peak.max(b.iter().fold(0.0, |m, &s| m.max(s.abs())));
            per_slot.push(null_depth_db(a, &b, max_lag));
        }
        let css_peak = a.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        let (best_rr, best_db) = per_slot
            .iter()
            .enumerate()
            .min_by(|x, y| x.1.partial_cmp(y.1).unwrap())
            .map(|(i, d)| (i, *d))
            .unwrap();
        depths.push(best_db);
        let slots: String = per_slot
            .iter()
            .map(|d| format!("{d:6.1}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "  {:<4} {:6.1} {:>4}   RR{best_rr}   {best_db:7.1}dB   ourPk={our_peak:.3} cssPk={css_peak:.3}  [{slots}]",
            d1, sec, d2
        );
    }
    rig.set_forced_rr(ID, None);

    if !depths.is_empty() {
        let mean = depths.iter().sum::<f32>() / depths.len() as f32;
        let best = depths.iter().cloned().fold(f32::INFINITY, f32::min);
        println!(
            "\n{} short notes  ·  mean best-RR null = {mean:.1}dB  ·  deepest = {best:.1}dB",
            depths.len()
        );
        println!("(−∞ = perfect cancellation; deeper = our RR matches CSS's sample)");
    }
    Ok(())
}
