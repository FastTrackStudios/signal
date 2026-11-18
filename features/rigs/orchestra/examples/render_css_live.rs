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
//! render_css_live --batch jobs.tsv --section "Solo Flute" --library woodwinds
//! ```
//!
//! `--batch FILE`: one render per line, `<in.mid>\t<out.wav>[\t<flags>]`
//! (flags as on the command line: `--rrs 0,1 --tunes 0,-3 --pitch-envs -1.1:400 --trace`,
//! `--window-frames A,B` to render up to frame B and keep A..B). The
//! instrument is parsed once and its samples decoded once; every line still
//! renders on a fresh rig, so each WAV equals a single run of that line.
//! The lines run side by side on `RENDER_THREADS` threads (default: every
//! core).

use sampler::SamplerRig;

const ID: &str = "strings_1v";
const SR: u32 = 48_000;

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

/// The SMF's first tempo, BPM (120 without one): the host tempo a
/// tempo-synced articulation reads.
fn smf_tempo_bpm(d: &[u8]) -> f64 {
    d.windows(6)
        .find(|w| w[..3] == [0xFF, 0x51, 0x03])
        .map(|w| f64::from(u32::from(w[3]) << 16 | u32::from(w[4]) << 8 | u32::from(w[5])))
        .filter(|&us| us > 0.0)
        .map_or(120.0, |us| 60_000_000.0 / us)
}

/// Minimal SMF parse → `(seconds, status, d1, d2)` channel events, in order.
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
    let mut sec = 0.0f64;
    let mut running = 0u8;
    let mut out = Vec::new();
    while p < end {
        let dt = u64::from(read_vlq(d, &mut p));
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
                    us_per_q = f64::from(d[p]).mul_add(65536.0, f64::from(d[p + 1]) * 256.0)
                        + f64::from(d[p + 2]);
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

/// 32-bit integer PCM: a −190 dB floor, so a null test against Kontakt is
/// limited by the engine, never by the file (16-bit capped it near −98 dB).
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

/// Host block size: the engine renders in plugin-sized blocks, as it would
/// under a DAW, so block-quantised behaviour shows up here too.
const BLOCK: usize = 128;

/// A forced-draw flag's values (`--rrs 0,1,2`), parsed as `T`.
fn draws<T: std::str::FromStr>(args: &[String], name: &str) -> Option<Vec<T>> {
    let i = args.iter().position(|a| a == name)?;
    Some(
        args.get(i + 1)
            .map(|s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect())
            .unwrap_or_default(),
    )
}

/// Render `inp` through a fresh rig holding `patch` into `outp`, with the
/// forced draws and `--trace` of `args`. Returns what a single run prints
/// (the trace, then the `wrote` line).
fn render_one(
    patch: sampler::PlayerPatch,
    section: &str,
    inp: &str,
    outp: &str,
    args: &[String],
) -> eyre::Result<String> {
    let events = parse_smf(&std::fs::read(inp)?);
    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    signal_orchestra::install_patch(&rig, ID, patch, section, "Mix").map_err(|e| eyre::eyre!(e))?;
    let trace = args.iter().any(|a| a == "--trace");
    rig.set_trace_enabled(ID, trace);
    // `--tunes a,b,…`: Kontakt's random short detune draws (cents), fitted
    // from its render, replayed so the rest of the null test is exact.
    if let Some(v) = draws::<i8>(args, "--tunes") {
        rig.force_random_tunes(ID, &v);
    }
    // the MIDI file's tempo is the host's (tempo-synced swells, crescendos)
    rig.set_host_tempo(ID, smf_tempo_bpm(&std::fs::read(inp)?));
    // `--ksp-seed N`: the script's KSP random() from seed N, as the KSP Seed
    // CLAP pins Kontakt and KONTRA_KSP_SEED pins KONTRA (MOJO 2).
    if let Some(v) = draws::<u32>(args, "--ksp-seed") {
        if let Some(&seed) = v.first() {
            rig.set_script_seed(ID, seed);
        }
    }
    // `--ksp-skip N`: N draws the reference's script made before its first
    // note, after the seed (a probe of where two draw streams part)
    if let Some(&n) = draws::<u32>(args, "--ksp-skip")
        .and_then(|v| v.first().copied())
        .as_ref()
    {
        rig.skip_script_draws(ID, n);
    }
    // `--humanize d1..d9,v1..v9,o1..o9`: an ensemble's humanize tables
    // (detune m-cents, volume mdB, offset ms of players 1-9), as KONTRA's
    // instrument drew them at load (MOJO 2).
    if let Some(v) = draws::<i32>(args, "--humanize") {
        rig.force_humanize(ID, &v);
    }
    // `--solo-notes a,b,…`: only voices of these notes sound (one voice's
    // part of a render, to take a residual apart)
    if let Some(v) = draws::<u8>(args, "--solo-notes") {
        rig.set_solo_notes(ID, Some(v.into_iter().collect()));
    }
    // `--rrs a,b,…`: random short RR draws (0-based; woodwinds / brass).
    if let Some(v) = draws::<u8>(args, "--rrs") {
        rig.force_random_rrs(ID, &v);
    }
    // `--attacks a,b,…`: loud first-note attack draws (index into the range).
    if let Some(v) = draws::<u8>(args, "--attacks") {
        rig.force_random_attacks(ID, &v);
    }
    // `--pitch-envs c:d,c:d,…`: random pitch-envelope draws (cents, decay
    // ms) of a `pitch_env_random` articulation (CSW marcato).
    if let Some(i) = args.iter().position(|a| a == "--pitch-envs") {
        let v: Vec<(f32, f32)> = args
            .get(i + 1)
            .map(|s| {
                s.split(',')
                    .filter_map(|p| {
                        let (c, d) = p.split_once(':')?;
                        Some((c.trim().parse().ok()?, d.trim().parse().ok()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        rig.force_random_pitch_envs(ID, &v);
    }
    // `--offsets a,b,…`: random marcato body-offset draws (ms; CSSS).
    if let Some(v) = draws::<u16>(args, "--offsets") {
        rig.force_random_offsets(ID, &v);
    }
    // `--variants a,b,…`: legato zone-2 body-variant draws (2..=4).
    if let Some(v) = draws::<u8>(args, "--variants") {
        rig.force_random_variants(ID, &v);
    }

    // Render up to an ABSOLUTE frame in host-sized blocks. Each event lands on
    // round(sec * SR) exactly: no per-gap rounding that drifts across a file.
    let render_to = |rig: &SamplerRig, out: &mut Vec<f32>, frame: usize| -> eyre::Result<()> {
        let mut buf = vec![0.0f32; BLOCK * 2];
        while out.len() / 2 < frame {
            let n = (frame - out.len() / 2).min(BLOCK);
            buf[..n * 2].fill(0.0);
            rig.render_offline(&mut buf[..n * 2])?;
            out.extend_from_slice(&buf[..n * 2]);
        }
        Ok(())
    };
    let frame_of = |sec: f64| (sec * f64::from(SR)).round().max(0.0) as usize;

    // `--window-frames A,B`: only frames A..B are wanted — render up to B,
    // write that slice (a fit scoring one note's window).
    let window = draws::<usize>(args, "--window-frames")
        .filter(|w| w.len() == 2 && w[0] < w[1])
        .map(|w| (w[0], w[1]));
    let mut audio: Vec<f32> = Vec::new();
    for (sec, status, d1, d2) in &events {
        if window.is_some_and(|(_, b)| frame_of(*sec) >= b) {
            break;
        }
        render_to(&rig, &mut audio, frame_of(*sec))?;
        match status & 0xF0 {
            0xB0 => rig.cc(ID, *d1, *d2),
            0x90 if *d2 > 0 => {
                let _ = rig.warm_note(ID, *d1);
                rig.note_on(ID, *d1, *d2);
            }
            0x90 | 0x80 => rig.note_off(ID, *d1),
            _ => {}
        }
    }
    let end = window.map_or(audio.len() / 2 + frame_of(8.0), |(_, b)| b); // tail
    render_to(&rig, &mut audio, end)?;
    if let Some((a, b)) = window {
        audio = audio[a * 2..b * 2].to_vec();
    }

    let mut report = String::new();
    if trace {
        use std::fmt::Write as _;
        // Every voice the engine started: what plays, from where, how loud.
        for e in &rig.render_trace(ID).events {
            let t = e.frame as f64 / f64::from(SR);
            match &e.kind {
                sampler::engine::trace::TraceKind::Transition {
                    from,
                    to,
                    portamento,
                } => {
                    let _ = writeln!(
                        report,
                        "{t:9.4}s TRANSITION {from} -> {to} portamento {portamento}"
                    );
                }
                sampler::engine::trace::TraceKind::NoteOff { note } => {
                    let _ = writeln!(report, "{t:9.4}s NOTEOFF    {note}");
                }
                sampler::engine::trace::TraceKind::VoiceEnd { voice_id } => {
                    let _ = writeln!(report, "{t:9.4}s END        voice {voice_id}");
                }
                sampler::engine::trace::TraceKind::VoiceSpawn(v) => {
                    let _ = writeln!(
                        report,
                        "[v{}] {t:9.4}s {:12} {:44} note {:3} root {:3} rate {:.6} gain {:.4} dyn {:4} start {:6} loop {}..{}",
                        v.voice_id,
                        v.voice_kind,
                        v.file,
                        v.note,
                        v.root_key,
                        v.rate,
                        v.gain,
                        v.dynamic,
                        v.start_frame,
                        v.loop_start,
                        v.loop_end
                    );
                }
                _ => {}
            }
        }
    }
    write_wav(outp, &audio)?;
    report.push_str(&format!(
        "wrote {outp}  ({:.1}s, {} events)",
        audio.len() as f64 / 2.0 / f64::from(SR),
        events.len()
    ));
    Ok(report)
}

fn main() -> eyre::Result<()> {
    let all: Vec<String> = std::env::args().collect();
    let flag = |name: &str| {
        all.iter()
            .position(|a| a == name)
            .and_then(|i| all.get(i + 1).cloned())
    };
    // `--library woodwinds` (default strings) and `--section "Solo Flute"`:
    // any instrument of any library, by its patch folder name (default 1st
    // Violins), with the engine settings that match its Kontakt render.
    let section = flag("--section").unwrap_or_else(|| "1st Violins".to_string());
    let lib_name = flag("--library").unwrap_or_else(|| "strings".to_string());
    let library = signal_orchestra::library(&lib_name)
        .ok_or_else(|| eyre::eyre!("unknown library {lib_name:?}"))?;
    let mut patch = signal_orchestra::load_patch(library, &section).map_err(|e| eyre::eyre!(e))?;
    // `--overlay FILE`: a further `section.styx` over the instrument's own
    // (a test of numbers not yet in the shared patch folder).
    if let Some(path) = flag("--overlay") {
        sampler::spec::SectionOverlay::from_file(std::path::Path::new(&path))
            .map_err(|e| eyre::eyre!("{path}: {e}"))?
            .apply(&mut patch.spec);
    }

    let Some(batch) = flag("--batch") else {
        let inp = all
            .get(1)
            .cloned()
            .unwrap_or_else(|| "css_test_full.mid".into());
        let outp = all
            .get(2)
            .cloned()
            .unwrap_or_else(|| "css_test_full_live.wav".into());
        println!("{}", render_one(patch, &section, &inp, &outp, &all)?);
        return Ok(());
    };

    let jobs: Vec<(String, String, Vec<String>)> = std::fs::read_to_string(&batch)?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut f = l.split('\t');
            let inp = f.next().unwrap_or_default().to_string();
            let outp = f.next().unwrap_or_default().to_string();
            let args = f
                .flat_map(str::split_whitespace)
                .map(String::from)
                .collect();
            (inp, outp, args)
        })
        .collect();
    let threads = std::env::var("RENDER_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, usize::from))
        .clamp(1, jobs.len().max(1));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let failed = std::sync::atomic::AtomicUsize::new(0);
    let out = std::sync::Mutex::new(std::io::stdout());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((inp, outp, args)) = jobs.get(i) else {
                        break;
                    };
                    let text = match render_one(patch.clone(), &section, inp, outp, args) {
                        Ok(t) => t,
                        Err(e) => {
                            failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            format!("FAILED {outp}: {e}")
                        }
                    };
                    if let Ok(mut o) = out.lock() {
                        use std::io::Write as _;
                        let _ = writeln!(o, "{text}");
                    }
                }
            });
        }
    });
    let failed = failed.into_inner();
    if failed > 0 {
        eyre::bail!("{failed} of {} renders failed", jobs.len());
    }
    Ok(())
}
