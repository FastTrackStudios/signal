//! Playability sweep of multi-soundsource packs (the Keyscape soundsource
//! packs: one pack per library, each soundsource an articulation listed in
//! `<Pack>.soundsources.txt` beside it).
//!
//! For every soundsource: notes across its range × velocities 30/80/127,
//! held 0.6 s then released 0.8 s, checking it sounds, doesn't clip, plays at
//! the note's pitch (strongest partial within ±60 cents of the note's
//! series), gets louder with velocity, and — when it has release zones —
//! sounds after the key.
//!
//!   cargo run -p signal-sampler --release --example soundsource_check -- <pack-dir> [name-filter]
//!   (env PACKS=Dolceola,Clavichord limits it to those packs)
//!
//! Prints one line per soundsource (`ok` / `WARN` / `FAIL` and why) and a
//! summary; exits non-zero when anything fails.
use std::path::{Path, PathBuf};

const SR: u32 = 48_000;

fn rms_db(x: &[f32]) -> f32 {
    let e = x.iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>() / x.len().max(1) as f64;
    (10.0 * (e + 1e-20).log10()) as f32
}

/// Cents from the nearest partial of `f0` of the strongest spectral peak in
/// the note's first four partial windows, and that peak's level re the
/// spectrum's loudest bin (dB). `None` when the window holds nothing.
fn pitch_check(x: &[f32], f0: f32) -> Option<(f32, f32)> {
    let n = x.len().next_power_of_two().min(1 << 16);
    let seg = &x[..n.min(x.len())];
    let mut buf: Vec<f64> = seg
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / seg.len() as f64).cos();
            f64::from(*v) * w
        })
        .collect();
    buf.resize(n, 0.0);
    // Plain DFT magnitude on the bins we need (partials ±60 cents): cheap.
    let bin_hz = f64::from(SR) / n as f64;
    let mag = |hz: f64| -> f64 {
        let k = hz / bin_hz;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, v) in buf.iter().enumerate() {
            let ph = std::f64::consts::TAU * k * i as f64 / n as f64;
            re += v * ph.cos();
            im -= v * ph.sin();
        }
        (re * re + im * im).sqrt()
    };
    let mut best: Option<(f64, f64, usize)> = None; // (mag, hz, partial)
    for p in 1..=4usize {
        let centre = f64::from(f0) * p as f64;
        if centre > 16_000.0 {
            break;
        }
        for step in -12..=12 {
            let hz = centre * 2f64.powf(f64::from(step) * 5.0 / 1200.0);
            let m = mag(hz);
            if best.is_none_or(|b| m > b.0) {
                best = Some((m, hz, p));
            }
        }
    }
    let (m, hz, p) = best?;
    // Reference: the loudest of a coarse sweep.
    let mut peak = 1e-12f64;
    let mut hz2 = 40.0;
    while hz2 < 12_000.0 {
        peak = peak.max(mag(hz2));
        hz2 *= 2f64.powf(1.0 / 24.0);
    }
    let cents = 1200.0 * (hz / (f64::from(f0) * p as f64)).log2();
    Some((cents as f32, (20.0 * (m / peak).log10()) as f32))
}

struct Take {
    held: f32,
    /// Level over the whole held part, attack included (a short noise
    /// is over before `held`'s window opens).
    held_all: f32,
    peak: f32,
    tail: f32,
    pitch: Option<(f32, f32)>,
}

fn play(e: &mut signal_sampler::SampleEngine, note: u8, vel: u8) -> Take {
    e.note_on(note, vel);
    let mut held = vec![0.0f32; (SR as usize * 6 / 10) * 2];
    for chunk in held.chunks_mut(512) {
        e.render(chunk);
    }
    e.note_off(note);
    let mut tail = vec![0.0f32; (SR as usize * 8 / 10) * 2];
    for chunk in tail.chunks_mut(512) {
        e.render(chunk);
    }
    // Let everything die before the next note.
    let mut drain = vec![0.0f32; SR as usize * 2];
    for _ in 0..4 {
        for chunk in drain.chunks_mut(512) {
            e.render(chunk);
        }
    }
    let mono = |b: &[f32]| b.chunks(2).map(|c| 0.5 * (c[0] + c[1])).collect::<Vec<f32>>();
    let h = mono(&held);
    let t = mono(&tail);
    let peak = held.iter().chain(&tail).fold(0.0f32, |m, v| m.max(v.abs()));
    let f0 = 440.0 * 2f32.powf((f32::from(note) - 69.0) / 12.0);
    let body = &h[SR as usize / 10..];
    Take {
        held: rms_db(body),
        held_all: rms_db(&h),
        peak,
        tail: rms_db(&t[SR as usize / 10..SR as usize / 2]),
        pitch: pitch_check(body, f0),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(args.get(1).expect("usage: soundsource_check <pack-dir> [filter]"));
    let filter = args.get(2).map(|s| s.to_lowercase());
    let mut packs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read pack dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "signalpack"))
        .collect();
    packs.sort();
    // `PACKS=Dolceola,Clavichord`: only those packs (by file stem).
    if let Ok(only) = std::env::var("PACKS") {
        let only: Vec<String> = only.split(',').map(|s| s.trim().to_lowercase()).collect();
        packs.retain(|p| {
            p.file_stem().is_some_and(|s| only.contains(&s.to_string_lossy().to_lowercase()))
        });
    }
    let (mut ok, mut warn, mut fail) = (0, 0, 0);
    for pack in packs {
        let stem = pack.file_stem().unwrap().to_string_lossy().into_owned();
        let list = pack.with_file_name(format!("{stem}.soundsources.txt"));
        let Ok(names) = std::fs::read_to_string(&list) else { continue };
        let names: Vec<String> = names
            .lines()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .filter(|n| filter.as_ref().is_none_or(|f| n.to_lowercase().contains(f)))
            .map(str::to_string)
            .collect();
        if names.is_empty() {
            continue;
        }
        let patch = match signal_sampler::PlayerPatch::from_pack(Path::new(&pack)) {
            Ok(p) => p,
            Err(e) => {
                println!("FAIL {stem}: pack does not open: {e}");
                fail += names.len();
                continue;
            }
        };
        for name in names {
            let zones: Vec<&signal_sampler::spec::ZoneSpec> =
                patch.spec.zones.iter().filter(|z| z.articulation == name).collect();
            let body: Vec<_> = zones.iter().filter(|z| z.trigger_mode.is_empty()).collect();
            let releases = zones.iter().filter(|z| z.trigger_mode == "release").count();
            let pedals = zones.iter().filter(|z| z.trigger_mode.starts_with("pedal")).count();
            let mut issues: Vec<String> = Vec::new();
            let mut notes_w: Vec<String> = Vec::new();
            if zones.is_empty() {
                println!("FAIL {name} [{stem}]: no zones");
                fail += 1;
                continue;
            }
            // A key-up / pedal soundsource (a stray note-on zone or two
            // aside): nothing on note-on to check.
            if body.len() * 20 < zones.len() {
                // A release / pedal-noise soundsource: plays on key-up / CC64,
                // nothing on note-on to check here.
                println!("ok   {name} [{stem}]: {} zones ({releases} release, {pedals} pedal) — no note-on body", zones.len());
                ok += 1;
                continue;
            }
            // The range the body's samples are rooted in.
            let lo = body.iter().map(|z| z.root_key).min().unwrap();
            let hi = body.iter().map(|z| z.root_key).max().unwrap();
            let notes: Vec<u8> = (0..5).map(|i| lo + ((u32::from(hi - lo) * i) / 4) as u8).collect();
            let mut e = signal_sampler::SampleEngine::new(patch.clone(), SR, "", "");
            e.set_articulation(name.clone());
            e.set_resample_transpose(true);
            let cache = e.cache_handle();
            let paths = e.sample_paths_playable(60);
            cache.preload(paths.iter().map(PathBuf::as_path));
            // Key-up, pedal and mechanical noises have no pitch to check.
            let lower = name.to_lowercase();
            let pitched = !["noise", "mechanical", "release", "pedal"].iter().any(|w| lower.contains(w));
            let extreme = |n: u8| n == lo || n == hi;
            for &n in &notes {
                let takes: Vec<(u8, Take)> = [30u8, 80, 127].iter().map(|&v| (v, play(&mut e, n, v))).collect();
                for (v, t) in &takes {
                    let level = if pitched { t.held } else { t.held_all };
                    if level < -80.0 {
                        issues.push(format!("note {n} vel {v} silent"));
                    }
                    // Unity gain here; the layer's gain comes later, so only
                    // a gross overshoot is a fault.
                    if t.peak >= 2.0 {
                        issues.push(format!("note {n} vel {v} clips ({:.2})", t.peak));
                    } else if t.peak >= 1.0 {
                        notes_w.push(format!("note {n} vel {v} peaks {:.2}", t.peak));
                    }
                }
                let (_, loud) = &takes[2];
                let (_, soft) = &takes[0];
                if loud.held + 1.0 < soft.held {
                    notes_w.push(format!("note {n}: vel 127 ({:.0} dB) quieter than vel 30 ({:.0})", loud.held, soft.held));
                }
                if pitched {
                    match loud.pitch {
                        // The A/B against Omnisphere is the authority on pitch;
                        // here only a gross miss away from the range ends
                        // fails (the extremes are bright and inharmonic).
                        Some((c, lvl)) if lvl > -30.0 && c.abs() > 50.0 && !extreme(n) => {
                            issues.push(format!("note {n} off pitch {c:+.0} cents"))
                        }
                        Some((c, lvl)) if lvl > -30.0 && c.abs() > 25.0 => {
                            notes_w.push(format!("note {n} pitch {c:+.0} cents"))
                        }
                        Some((_, lvl)) if lvl <= -30.0 => notes_w.push(format!("note {n}: pitch unclear")),
                        None => notes_w.push(format!("note {n}: pitch unclear")),
                        _ => {}
                    }
                }
                if releases > 0 && loud.tail < -90.0 {
                    notes_w.push(format!("note {n}: nothing after key-up"));
                }
            }
            let summary = format!(
                "{} zones ({releases} release, {pedals} pedal), roots {lo}-{hi}",
                zones.len()
            );
            if !issues.is_empty() {
                println!("FAIL {name} [{stem}]: {summary}; {}", issues.join("; "));
                fail += 1;
            } else if !notes_w.is_empty() {
                println!("WARN {name} [{stem}]: {summary}; {}", notes_w.join("; "));
                warn += 1;
            } else {
                println!("ok   {name} [{stem}]: {summary}");
                ok += 1;
            }
        }
    }
    println!("\n{ok} ok, {warn} warn, {fail} fail");
    if fail > 0 {
        std::process::exit(1);
    }
}
