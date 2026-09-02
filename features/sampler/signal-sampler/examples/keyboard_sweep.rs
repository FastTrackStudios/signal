//! Play every key and check what the ENGINE actually did.
//!
//! `check_pack_resolve` asks the sample map a static question: does some entry
//! exist for the default articulation at each note and dynamic. A pack can
//! answer yes and still be unplayable, because at trigger time the engine picks
//! a specific (articulation, dynamic, round-robin, mic, direction) tuple and
//! looks up THAT. Those are different questions, and the difference is exactly
//! the class of bug this exists to catch: "note-on triggered NO body voice",
//! and notes that sound at the wrong pitch.
//!
//! So this drives the real path — `note_on` into a real `SampleEngine`, render
//! a block, read the structured trace — and reports, per note and velocity:
//!
//! - **NO BODY**: nothing but a release/click spawned. What the player hears as
//!   a dead key.
//! NOTE on measuring pitch from the rendered audio: an autocorrelation over
//! the engine's output was tried and REMOVED. It reported the same frequency
//! for every key — a measurement artefact that reads exactly like an
//! instrument transposed six octaves — and a check that cries wolf is worse
//! than no check. Systematic tuning is read from the spec instead, where it
//! is exact; that is what caught the NI pianos' +100 cents.
//!
//! - **PITCH**: a body spawned, but from a zone whose `root_key` is further
//!   from the played note than `--max-stretch` semitones. A sample stretched
//!   that far is the "wrong pitch" complaint; the trace knows the root key, so
//!   this is measured, not guessed.
//! - **NOT LOADED**: the sample matched but was not resident, so the audio
//!   thread skipped it. A preload/streaming problem, NOT a mapping problem —
//!   distinguished because the two have completely different fixes.
//!
//! ```text
//! cargo run --release -p signal-sampler --example keyboard_sweep -- <pack> [--vel 40,80,110] [--max-stretch 3]
//! ```
//!
//! Exit 0 = every key sounded a body at a sane pitch. 1 = it did not.

use std::collections::BTreeMap;
use std::path::Path;

use signal_sampler::engine::trace::{MissReason, TraceKind};
use signal_sampler::{PlayerPatch, SampleEngine};

const SR: u32 = 48_000;
/// Long enough for a note-on to spawn and render, short enough to sweep 88
/// keys quickly.
const BLOCK: usize = 512;

const NN: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
fn nm(p: u8) -> String {
    format!("{}{}", NN[(p % 12) as usize], (p / 12) as i32 - 1)
}

#[derive(Default)]
struct Failures {
    no_body: Vec<(u8, u8)>,
    not_loaded: Vec<(u8, u8)>,
    /// (note, velocity, root_key, semitones stretched)
    pitch: Vec<(u8, u8, u8, i16)>,
    /// What the engine actually asked the map for when it missed —
    /// (note, velocity, articulation, dynamic, rr). Without this a miss says
    /// "this key is dead" but not which lookup failed, which is the only part
    /// that tells you whether to fix the pack or the engine.
    asked: Vec<(u8, u8, String, String, usize)>,
}

/// Minimal 16-bit stereo WAV writer — enough to listen to and measure.
fn write_wav(path: &str, interleaved: &[f32], sr: u32) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let n = interleaved.len();
    let data_len = (n * 2) as u32;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&2u16.to_le_bytes())?; // stereo
    f.write_all(&sr.to_le_bytes())?;
    f.write_all(&(sr * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in interleaved {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        f.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}

fn arg(name: &str) -> Option<String> {
    let mut it = std::env::args().skip_while(|a| a != name);
    it.next()?;
    it.next()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pack = std::env::args()
        .nth(1)
        .ok_or("usage: keyboard_sweep <pack> [--vel 40,80,110] [--max-stretch 3]")?;
    let vels: Vec<u8> = arg("--vel")
        .unwrap_or_else(|| "40,80,110".into())
        .split(',')
        .filter_map(|v| v.trim().parse().ok())
        .collect();
    let max_stretch: i16 = arg("--max-stretch")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    // `--note N` dumps every trace event for one key instead of sweeping —
    // the "why is THIS key dead" view.
    let only: Option<u8> = arg("--note").and_then(|v| v.parse().ok());
    // `--wav <path>` renders the single `--note` through the engine and writes
    // it, so what the engine PRODUCES can be listened to and measured beside
    // the source sample instead of reasoned about.
    let wav_out = arg("--wav");

    let patch = PlayerPatch::from_pack(Path::new(&pack))?;
    // The section and mic MUST come from the spec. Passing empty strings
    // builds a query that matches nothing in a convention-mode pack (whose
    // keys carry real section/mic ids), so every note reads as a dead key —
    // the test reporting a fault that only exists in the test.
    let section = patch
        .spec
        .sections
        .first()
        .map(|s| s.id.clone())
        .unwrap_or_default();
    let mic = patch
        .spec
        .mics
        .first()
        .map(|m| m.id.clone())
        .unwrap_or_default();
    let (lo, hi) = (21u8, 108u8);

    let mut systematic_tuning: Option<(i32, f64)> = None;
    let mut engine = SampleEngine::new(patch, SR, section.clone(), mic.clone());
    // Deliberately NO bulk preload. `warm_note_samples` below loads exactly
    // what each note needs, right before it is played, so the RAM budget can
    // never run out mid-sweep and turn "the budget filled" into "these keys
    // are unmapped". Those are different faults with different fixes, and a
    // bulk preload of a multi-GB library conflates them: it exhausts the
    // 4 GB ceiling partway through the keyboard and every note after that
    // reads as dead.
    engine.set_trace_enabled(true);
    println!("pack: {pack}");
    // A constant pitch error across the whole keyboard is almost always a
    // per-articulation transpose, so show them before the sweep.
    {
        let spec = &engine.patch().spec;
        let shifted: Vec<String> = spec
            .articulations
            .iter()
            .filter(|a| a.transpose != 0)
            .map(|a| format!("{}={:+}", a.id, a.transpose))
            .collect();
        if shifted.is_empty() {
            println!("articulation transposes: none");
        } else {
            println!("articulation transposes: {}", shifted.join(" "));
        }
        // Zoned packs carry tuning per zone, and on the zoned path `rate` is
        // TUNING ONLY (the transposition rides a pitch shifter). So a
        // systematic tuning offset here — every zone at +100 cents, say — is
        // the whole instrument playing a semitone out, and it will not show
        // up as a root-key mismatch anywhere.
        let mut tunes: BTreeMap<i32, usize> = BTreeMap::new();
        for z in &spec.zones {
            *tunes.entry(z.tune_cents.round() as i32).or_default() += 1;
        }
        let summary: Vec<String> = tunes
            .iter()
            .map(|(c, n)| format!("{c:+}c x{n}"))
            .collect();
        println!("master tune: {:+} cents", spec.performance.master_tune_cents);
        println!("zone tuning: {}", summary.join("  "));
        // The systematic-tuning check. A real library fine-tunes individual
        // zones by a few cents; it does not put the SAME large offset on all
        // of them. When the most common tuning is a long way from zero the
        // whole instrument plays out of tune, and no per-note comparison can
        // see it because every zone root is perfectly correct.
        if let Some((cents, n)) = tunes.iter().max_by_key(|(_, n)| **n) {
            let share = *n as f64 / spec.zones.len().max(1) as f64;
            if cents.abs() > 25 && share > 0.5 {
                systematic_tuning = Some((*cents, share));
            }
        }
    }
    println!("section={section:?} mic={mic:?}");
    // Whether the pack streams decides whether preload warms every zone's
    // HEAD (cheap, whole keyboard resident) or decodes whole samples until
    // the RAM budget runs out (and leaves the rest of the keyboard dead).
    println!(
        "streamable: {}   preload budget: {} MB",
        engine.cache_handle().is_streamable(),
        signal_sampler::engine::budget::limit_bytes() / (1024 * 1024)
    );
    println!("sweeping notes {lo}..={hi} at velocities {vels:?}, max stretch {max_stretch} st\n");

    if let Some((cents, share)) = systematic_tuning {
        println!(
            "\n!! SYSTEMATIC TUNING: {cents:+} cents on {:.0}% of zones — the whole \
             instrument plays out of tune.",
            share * 100.0
        );
    }

    let mut fails = Failures::default();
    let mut out = vec![0.0f32; BLOCK * 2];
    let mut roots: BTreeMap<u8, u8> = BTreeMap::new();

    let (lo, hi) = match only {
        Some(n) => (n, n),
        None => (lo, hi),
    };
    for note in lo..=hi {
        for &vel in &vels {
            // Warm this note and WAIT for it to land. `warm_note_samples` is
            // asynchronous — it hands the paths to the decode worker and
            // returns — so triggering straight after races the worker and
            // every note reads as NOT LOADED. That is a fact about the test's
            // timing, not about the pack, and conflating the two is how a
            // perfectly-mapped library gets called broken.
            // Load EVERY sample this note needs, synchronously. A note
            // spawns several zones (layers, mics), and warming only the first
            // — which is what `warm_note_samples` does — left the rest
            // unresident, so the trigger reported NOT LOADED for a pack whose
            // mapping is perfect. Off the audio thread a blocking decode is
            // exactly right.
            let cache = engine.cache_handle();
            for path in engine.resolve_note_sample_paths(note, vel) {
                let _ = cache.get(&path);
            }
            // Zoned packs spawn from zones the resolver above does not
            // enumerate (mic and layer variants), so load EVERY zone whose
            // key range covers this note. Without it the trigger reports NOT
            // LOADED for a pack whose mapping is perfect, and no pitch can be
            // measured because nothing sounds.
            {
                let patch = engine.patch();
                let paths: Vec<std::path::PathBuf> = patch
                    .spec
                    .zones
                    .iter()
                    .enumerate()
                    .filter(|(_, z)| note >= z.key_min && note <= z.key_max)
                    .filter_map(|(i, _)| patch.zone_paths.get(i).cloned())
                    .collect();
                for path in paths {
                    let _ = cache.get(&path);
                }
            }
            let before = engine.render_trace().events.len();
            engine.note_on(note, vel);
            out.fill(0.0);
            engine.render(&mut out);
            if let Some(path) = wav_out.as_deref() {
                // 4 s held, then release, so the attack, body and tail are all
                // in the file.
                let mut pcm: Vec<f32> = out.clone();
                let mut blk = vec![0.0f32; 4096 * 2];
                for i in 0..(SR as usize * 4 / 4096) {
                    if i == (SR as usize * 3 / 4096) {
                        engine.note_off(note);
                    }
                    blk.fill(0.0);
                    engine.render(&mut blk);
                    pcm.extend_from_slice(&blk);
                }
                write_wav(path, &pcm, SR)?;
                println!("wrote {path} ({} frames)", pcm.len() / 2);
            }
            engine.note_off(note);

            let trace = engine.render_trace();
            let fresh = &trace.events[before.min(trace.events.len())..];
            if only.is_some() {
                println!("-- note {note} vel {vel}: {} events", fresh.len());
                for e in fresh {
                    match &e.kind {
                        TraceKind::VoiceSpawn(v) => println!(
                            "   spawn kind={} note={} root={} rate={:.4} gain={:.3} artic={} start={} loop={}..{}\n        file={}",
                            v.voice_kind, v.note, v.root_key, v.rate, v.gain,
                            v.articulation,
                            v.start_frame, v.loop_start, v.loop_end, v.file
                        ),
                        TraceKind::SampleMiss { note, articulation, dynamic, rr, reason } => println!(
                            "   MISS note={note} artic={articulation} dyn={dynamic} rr={rr} reason={reason:?}"
                        ),
                        other => println!("   {other:?}"),
                    }
                }
            }

            // A BODY is any spawn that is not a release/click layer — those
            // sound on note-off and are exactly what you hear when the body
            // is missing.
            let mut body: Option<(u8, u8, f64)> = None; // (note, root_key, rate)
            let mut not_loaded = false;
            for e in fresh {
                match &e.kind {
                    TraceKind::VoiceSpawn(v)
                        if !v.voice_kind.eq_ignore_ascii_case("Release")
                            && v.note == note
                            && v.gain > 0.0 =>
                    {
                        body.get_or_insert((v.note, v.root_key, v.rate));
                    }
                    TraceKind::SampleMiss {
                        reason,
                        articulation,
                        dynamic,
                        rr,
                        ..
                    } => {
                        not_loaded |= *reason == MissReason::NotLoaded;
                        if fails.asked.len() < 400 {
                            fails.asked.push((
                                note,
                                vel,
                                articulation.clone(),
                                dynamic.clone(),
                                *rr,
                            ));
                        }
                    }
                    _ => {}
                }
            }
            match body {
                None if not_loaded => fails.not_loaded.push((note, vel)),
                None => fails.no_body.push((note, vel)),
                Some((_, root, rate)) => {
                    roots.insert(note, root);
                    let stretch = note as i16 - root as i16;
                    if stretch.abs() > max_stretch {
                        fails.pitch.push((note, vel, root, stretch));
                    }
                    // NOTE: no per-note check on `rate`. It is tempting and
                    // it is wrong — `rate` carries the sample-rate conversion
                    // on the convention path (a 44.1 kHz pack in a 48 kHz
                    // engine reads as -147 cents, entirely correct), and only
                    // tuning on the zoned path. Without each zone's source
                    // rate the two cannot be told apart, so systematic tuning
                    // is checked from the SPEC instead, before the sweep.
                    let _ = rate;
                }
            }
        }
    }

    let total = (hi - lo + 1) as usize * vels.len();
    let bad = fails.no_body.len() + fails.not_loaded.len() + fails.pitch.len();
    println!("{}/{} note×velocity triggers produced a correct body", total - bad, total);

    if !fails.no_body.is_empty() {
        println!("\nNO BODY ({}): a dead key — only a release/click sounds", fails.no_body.len());
        for (n, v) in fails.no_body.iter().take(30) {
            println!("  {} (note {n}) vel {v}", nm(*n));
        }
    }
    if !fails.not_loaded.is_empty() {
        println!(
            "\nNOT LOADED ({}): mapped but not resident — a preload/streaming gap, not a mapping one",
            fails.not_loaded.len()
        );
        for (n, v) in fails.not_loaded.iter().take(15) {
            println!("  {} (note {n}) vel {v}", nm(*n));
        }
    }
    if !fails.asked.is_empty() {
        // Collapse to the distinct (articulation, dynamic) pairs that failed:
        // a hundred dead keys are usually two or three broken lookups.
        let mut by_pair: BTreeMap<(String, String), usize> = BTreeMap::new();
        for (_, _, a, d, _) in &fails.asked {
            *by_pair.entry((a.clone(), d.clone())).or_default() += 1;
        }
        println!("\nLOOKUPS THAT MISSED, by (articulation, dynamic):");
        let mut pairs: Vec<_> = by_pair.into_iter().collect();
        pairs.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for ((a, d), n) in pairs.iter().take(20) {
            println!("  {n:5}x  artic={a:<24} dynamic={d}");
        }
    }
    if !fails.pitch.is_empty() {
        println!(
            "\nPITCH ({}): body sounds, but stretched more than {max_stretch} semitones from its zone root",
            fails.pitch.len()
        );
        for (n, v, root, st) in fails.pitch.iter().take(30) {
            println!(
                "  {} (note {n}) vel {v} ← root {} ({st:+} st)",
                nm(*n),
                nm(*root)
            );
        }
    }

    if bad == 0 && systematic_tuning.is_none() {
        println!("\nPASS: every key sounds a body within {max_stretch} semitones of its root,");
        println!("      and no systematic tuning offset.");
        return Ok(());
    }
    println!("\nFAIL");
    std::process::exit(1);
}
