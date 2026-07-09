//! Render a MIDI file through the LIVE (reactive, exact-CSS) engine path → WAV,
//! for A/B against a real CSS-in-Kontakt export of the same MIDI.
//!
//! Unlike `render_css_test` (document/lookahead path), this drives the engine
//! the way a live plugin does: dispatch each CC / note-on / note-off in real
//! time and `render_offline` the gaps. Warms each note's articulation first
//! (offline cache). Channel 0 → the instrument.
//!
//! ```text
//! cargo run --release -p signal-sampler --example render_css_live -- css_test_full.mid css_test_full_live.wav
//! ```

use signal_sampler::SamplerRig;

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
    }
    out
}

fn write_wav(path: &str, samples: &[f32]) -> eyre::Result<()> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    w.finalize()?;
    Ok(())
}

fn main() -> eyre::Result<()> {
    let inp = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "css_test_full.mid".into());
    let outp = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "css_test_full_live.wav".into());
    let events = parse_smf(&std::fs::read(&inp)?);
    eprintln!("parsed {} events from {inp}", events.len());

    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    // The orchestra feature's strings *definition* — loads CSS 1st Violins with
    // the exact engine settings (solo mic, arco-attack bloom, release overlap)
    // that match a real CSS-in-Kontakt render.
    signal_orchestra::load_strings(
        &rig,
        ID,
        "1st Violins",
        "Mix",
        signal_orchestra::CSS_ROOT,
        signal_orchestra::CSS_CONFIG,
    )
    .map_err(|e| eyre::eyre!(e))?;

    let render = |rig: &SamplerRig, out: &mut Vec<f32>, secs: f64| -> eyre::Result<()> {
        let frames = (secs * SR as f64).round().max(0.0) as usize;
        if frames == 0 {
            return Ok(());
        }
        let mut buf = vec![0.0f32; frames * 2];
        rig.render_offline(&mut buf)?;
        out.extend_from_slice(&buf);
        Ok(())
    };

    let mut audio: Vec<f32> = Vec::new();
    let mut cursor = 0.0f64;
    for (sec, status, d1, d2) in &events {
        if *sec > cursor {
            render(&rig, &mut audio, sec - cursor)?;
            cursor = *sec;
        }
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
    render(&rig, &mut audio, 8.0)?; // tail

    write_wav(&outp, &audio)?;
    println!(
        "wrote {outp}  ({:.1}s, {} events)",
        audio.len() as f64 / 2.0 / SR as f64,
        events.len()
    );
    Ok(())
}
