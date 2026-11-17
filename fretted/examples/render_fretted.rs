//! Render a MIDI file through a fretted library, live (each event dispatched
//! as a player would send it) → a 48 kHz stereo WAV, for a null test
//! against the same MIDI rendered by Kontakt.
//!
//! ```text
//! render_fretted <in.mid> <out.wav> --library abyss --instrument Abyss [--mic <id>, default the spec's default mic] [--trace]
//! ```
//!
//! `--trace` prints every voice the engine spawned (file, root, rate, gain,
//! start), so a unit that does not null can be read note by note.

use sampler::SamplerRig;

const ID: &str = "fretted";
const SR: u32 = 48_000;
const BLOCK: usize = 128;

fn read_vlq(d: &[u8], p: &mut usize) -> u32 {
    let mut v = 0u32;
    loop {
        let b = d[*p];
        *p += 1;
        v = (v << 7) | u32::from(b & 0x7f);
        if b & 0x80 == 0 {
            break;
        }
    }
    v
}

/// The first track's channel events as `(seconds, status, d1, d2)`.
fn parse_smf(d: &[u8]) -> Vec<(f64, u8, u8, u8)> {
    let div = f64::from(u16::from_be_bytes([d[12], d[13]]));
    let mut us_per_q = 500_000.0f64;
    let mut p = 14;
    while &d[p..p + 4] != b"MTrk" {
        let len = u32::from_be_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize;
        p += 8 + len;
    }
    let track_len = u32::from_be_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize;
    p += 8;
    let end = p + track_len;
    let (mut sec, mut running, mut out) = (0.0f64, 0u8, Vec::new());
    while p < end {
        let dt = f64::from(read_vlq(d, &mut p));
        sec += dt * (us_per_q / 1_000_000.0) / div;
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
                    us_per_q = f64::from(
                        u32::from(d[p]) << 16 | u32::from(d[p + 1]) << 8 | u32::from(d[p + 2]),
                    );
                }
                p += len;
            }
            0xF0 | 0xF7 => {
                let len = read_vlq(d, &mut p) as usize;
                p += len;
            }
            s if (0x80..=0xEF).contains(&s) => {
                let two = !matches!(s & 0xF0, 0xC0 | 0xD0);
                out.push((sec, s, d[p], if two { d[p + 1] } else { 0 }));
                p += if two { 2 } else { 1 };
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
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        w.write_sample((f64::from(s.clamp(-1.0, 1.0)) * f64::from(i32::MAX)) as i32)?;
    }
    w.finalize()?;
    Ok(())
}

fn main() -> eyre::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let (Some(inp), Some(outp)) = (args.get(1), args.get(2)) else {
        eyre::bail!(
            "usage: render_fretted <in.mid> <out.wav> --library <name> --instrument <name> [--mic <m>] [--trace]"
        );
    };
    let lib_name = flag("--library").unwrap_or_else(|| "abyss".into());
    let library = signal_fretted::library(&lib_name)
        .ok_or_else(|| eyre::eyre!("unknown library {lib_name:?}"))?;
    let instrument = flag("--instrument").unwrap_or_else(|| "Abyss".into());
    let trace = args.iter().any(|a| a == "--trace");

    let patch = signal_fretted::load_patch(library, &instrument).map_err(|e| eyre::eyre!(e))?;
    // the spec's default mic (the pickup the shipped patch plays)
    let mic = flag("--mic").unwrap_or_else(|| {
        let mics = &patch.spec.mics;
        mics.iter()
            .find(|m| m.default)
            .or(mics.first())
            .map_or_else(|| "Blend".into(), |m| m.id.clone())
    });
    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    signal_fretted::install_patch(&rig, ID, patch, &instrument, &mic)
        .map_err(|e| eyre::eyre!(e))?;
    rig.set_trace_enabled(ID, trace);
    // Kontakt's KSP random() seed (the KSP Seed CLAP / KONTRA_KSP_SEED)
    let seed = flag("--ksp-seed").and_then(|s| s.parse().ok()).unwrap_or(1);
    rig.set_ksp_seed(ID, seed);

    let events = parse_smf(&std::fs::read(inp)?);
    let frame_of = |sec: f64| (sec * f64::from(SR)).round().max(0.0) as usize;
    let mut audio: Vec<f32> = Vec::new();
    let mut buf = vec![0.0f32; BLOCK * 2];
    let mut render_to = |audio: &mut Vec<f32>, frame: usize| -> eyre::Result<()> {
        while audio.len() / 2 < frame {
            let n = (frame - audio.len() / 2).min(BLOCK);
            buf[..n * 2].fill(0.0);
            rig.render_offline(&mut buf[..n * 2])?;
            audio.extend_from_slice(&buf[..n * 2]);
        }
        Ok(())
    };
    for &(sec, status, d1, d2) in &events {
        render_to(&mut audio, frame_of(sec))?;
        match status & 0xF0 {
            0xB0 => rig.cc(ID, d1, d2),
            0x90 if d2 > 0 => {
                // the neighbours too: anti-repetition plays a recording two
                // frets away (the borrow), which must be resident as well
                for k in d1.saturating_sub(2)..=d1.saturating_add(2).min(127) {
                    let _ = rig.warm_note(ID, k);
                }
                rig.note_on(ID, d1, d2);
            }
            0x90 | 0x80 => rig.note_off(ID, d1),
            _ => {}
        }
    }
    let end = audio.len() / 2 + frame_of(4.0);
    render_to(&mut audio, end)?;

    if trace {
        for e in &rig.render_trace(ID).events {
            let t = e.frame as f64 / f64::from(SR);
            match &e.kind {
                sampler::engine::trace::TraceKind::NoteOff { note } => {
                    println!("{t:9.4}s NOTEOFF {note}")
                }
                sampler::engine::trace::TraceKind::VoiceSpawn(v) => println!(
                    "{t:9.4}s [v{}] {:8} {:60} note {:3} root {:3} rate {:.6} gain {:.4} start {}",
                    v.voice_id,
                    v.voice_kind,
                    v.file,
                    v.note,
                    v.root_key,
                    v.rate,
                    v.gain,
                    v.start_frame
                ),
                sampler::engine::trace::TraceKind::SampleMiss {
                    note,
                    articulation,
                    dynamic,
                    rr,
                    reason,
                } => {
                    println!(
                        "{t:9.4}s MISS note {note} {articulation} {dynamic} rr {rr} {reason:?}"
                    );
                }
                _ => {}
            }
        }
    }
    write_wav(outp, &audio)?;
    println!(
        "wrote {outp} ({:.1}s, {} events)",
        audio.len() as f64 / 2.0 / f64::from(SR),
        events.len()
    );
    Ok(())
}
