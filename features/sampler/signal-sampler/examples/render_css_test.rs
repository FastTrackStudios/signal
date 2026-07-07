//! Render a test MIDI file through OUR engine offline → WAV, for A/B comparison
//! against the real CSS render of the same file.
//!
//! ```text
//! cargo run --release -p signal-sampler --example render_css_test -- css_test.mid our_render.wav
//! ```

use std::io::Write;
use std::path::PathBuf;

use signal_sampler::SamplerRig;

const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
const CSS_CONFIG: &str =
    "/run/media/Development/FastTrackStudio/sample-collector/specs/cinematic-strings.styx";
const ID: &str = "strings_1v";
const SR: u32 = 48_000;

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

/// Minimal SMF parse → `(seconds, status, d1, d2)` channel events, in order.
fn parse_smf(d: &[u8]) -> Vec<(f64, u8, u8, u8)> {
    let div = u16::from_be_bytes([d[12], d[13]]) as f64; // ticks/quarter
    let mut us_per_q = 500_000.0f64; // default 120 BPM
    // Locate MTrk.
    let mut p = 14;
    while &d[p..p + 4] != b"MTrk" {
        let len = u32::from_be_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize;
        p += 8 + len;
    }
    let track_len = u32::from_be_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize;
    p += 8;
    let end = p + track_len;
    let mut tick = 0u64;
    let mut sec = 0.0f64;
    let mut running = 0u8;
    let mut out = Vec::new();
    while p < end {
        let dt = read_vlq(d, &mut p) as u64;
        tick += dt;
        sec += dt as f64 * (us_per_q / 1_000_000.0) / div;
        let mut status = d[p];
        if status & 0x80 != 0 {
            p += 1;
            running = status;
        } else {
            status = running; // running status
        }
        match status {
            0xFF => {
                let meta = d[p];
                p += 1;
                let len = read_vlq(d, &mut p) as usize;
                if meta == 0x51 {
                    us_per_q =
                        ((d[p] as f64) * 65536.0) + (d[p + 1] as f64) * 256.0 + d[p + 2] as f64;
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
        let _ = tick;
    }
    out
}

fn write_wav(path: &str, samples: &[f32]) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    let n = samples.len() as u32;
    let data_bytes = n * 2; // 16-bit
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_bytes).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&2u16.to_le_bytes())?; // stereo
    f.write_all(&SR.to_le_bytes())?;
    f.write_all(&(SR * 4).to_le_bytes())?; // byte rate
    f.write_all(&4u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_bytes.to_le_bytes())?;
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        f.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}

const CHUNK: usize = 2048; // frames per render call

fn render_until(rig: &SamplerRig, out: &mut Vec<f32>, cur: &mut f64, t: f64) {
    while *cur < t {
        let frames = (((t - *cur) * SR as f64) as usize).clamp(1, CHUNK);
        let mut buf = vec![0.0f32; frames * 2];
        rig.render_offline(&mut buf).ok();
        out.extend_from_slice(&buf);
        *cur += frames as f64 / SR as f64;
    }
}

fn main() -> eyre::Result<()> {
    let inp = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "css_test.mid".into());
    let outp = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "our_render.wav".into());
    let bytes = std::fs::read(&inp)?;
    let events = parse_smf(&bytes);
    eprintln!("parsed {} events from {inp}", events.len());

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
    rig.set_articulation(ID, "Nonvib");
    // CSS-default envelope: fast attack (sustains start at the steady region) and
    // a ~0.4s release under the recorded NVrel/Vsusrel tail.
    rig.set_attack_ms(ID, 20);
    rig.set_release_ms(ID, 400);

    let mut out: Vec<f32> = Vec::new();
    let mut cur = 0.0f64;

    for (sec, status, d1, d2) in &events {
        render_until(&rig, &mut out, &mut cur, *sec);
        match status & 0xF0 {
            0xB0 => rig.cc(ID, *d1, *d2),
            0x90 if *d2 > 0 => {
                rig.warm_note(ID, *d1);
                rig.note_on(ID, *d1, *d2);
            }
            0x90 | 0x80 => rig.note_off(ID, *d1),
            _ => {}
        }
    }
    // Tail.
    let tail = cur + 3.0;
    render_until(&rig, &mut out, &mut cur, tail);

    write_wav(&outp, &out)?;
    println!("wrote {outp}  ({:.1}s)", out.len() as f64 / 2.0 / SR as f64);
    Ok(())
}
