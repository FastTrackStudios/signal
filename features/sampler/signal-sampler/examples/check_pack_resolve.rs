//! Static playability check for a `.signalpack`: load it via `from_pack`, then
//! resolve the default section/articulation/mic across the keyboard for every
//! declared dynamic. A pack that resolves everywhere the styx declares is
//! playable (the engine reaches exactly these map entries). Bypasses the audio
//! render path.
//!   cargo run -p signal-sampler --release --example check_pack_resolve -- "<pack>"

use std::path::Path;

use signal_sampler::spec::ArticulationKind;
use signal_sampler::PlayerPatch;

/// Mirror of the engine's default-articulation picker (engine/mod.rs) so we can
/// report which articulation actually plays by default.
fn engine_default(spec: &signal_sampler::spec::LibrarySpec) -> Option<String> {
    let is_aux = |id: &str| {
        let l = id.to_ascii_lowercase();
        l.contains("mch") || l.contains("mech") || l.contains("ped")
    };
    spec.articulations
        .iter()
        .find(|a| a.kind == ArticulationKind::Sustain && !is_aux(&a.id))
        .or_else(|| {
            spec.articulations.iter().find(|a| {
                !matches!(a.kind, ArticulationKind::Release | ArticulationKind::Legato)
                    && !is_aux(&a.id)
            })
        })
        .or_else(|| {
            spec.articulations
                .iter()
                .find(|a| !matches!(a.kind, ArticulationKind::Release | ArticulationKind::Legato))
        })
        .or_else(|| spec.articulations.first())
        .map(|a| a.id.clone())
}

fn check_zone_mode(patch: &PlayerPatch) -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::{BTreeMap, BTreeSet};

    let spec = &patch.spec;
    let pack = patch.pack.as_ref().ok_or("zone-mode check requires a pack-loaded patch")?;
    println!(
        "pack: {}  (zone mode: {} zones, {} pack entries)",
        spec.name,
        spec.zones.len(),
        pack.entry_count()
    );

    // Per-articulation key coverage: contiguous within [min_key, max_key]?
    let mut by_artic: BTreeMap<&str, BTreeSet<u8>> = BTreeMap::new();
    for z in &spec.zones {
        let keys = by_artic.entry(z.articulation.as_str()).or_default();
        for k in z.key_min..=z.key_max {
            keys.insert(k);
        }
    }
    let mut gaps = 0usize;
    for (artic, keys) in &by_artic {
        let (lo, hi) = (*keys.first().unwrap(), *keys.last().unwrap());
        // The CS whole-tone grid leaves ≤2-semitone holes the engine fills by
        // pitch-shifting (performance.zone_pitch_tolerance) — only wider holes
        // are real gaps.
        let tol = spec.performance.zone_pitch_tolerance.max(1) as u8;
        let mut missing: Vec<u8> = Vec::new();
        let mut last = lo;
        for k in keys.iter().copied() {
            if k > last && k - last > tol + 1 {
                missing.extend(last + 1..k);
            }
            last = k;
        }
        if missing.is_empty() {
            println!("  artic '{artic}': keys {lo}..={hi} covered ({} keys)", keys.len());
        } else {
            gaps += 1;
            println!("  artic '{artic}': keys {lo}..={hi} — GAPS beyond pitch tolerance: {missing:?}");
        }
    }

    // Every zone file must have a pack entry (exact relative-path match).
    let entries: BTreeSet<&Path> = pack.entry_paths().collect();
    let missing_files: Vec<&str> = spec
        .zones
        .iter()
        .map(|z| z.file.as_str())
        .filter(|f| !entries.contains(Path::new(f)))
        .collect();
    if !missing_files.is_empty() {
        println!(
            "  {} zone file(s) have NO pack entry, e.g. {:?}",
            missing_files.len(),
            &missing_files[..missing_files.len().min(5)]
        );
    }

    // Decode a few entries end-to-end (first / middle / last zone) — proves
    // the codec path (FLAC or Ogg Vorbis) actually yields audio. With
    // `--src-root <lib_root>` also A/B the decode against the source WAV:
    // exact length match + normalized correlation (1.0 lossless, ≥0.95 proxy).
    let src_root = {
        let args: Vec<String> = std::env::args().collect();
        args.iter()
            .position(|a| a == "--src-root")
            .and_then(|i| args.get(i + 1))
            .map(std::path::PathBuf::from)
    };
    let cache = signal_sampler::engine::cache::SampleCache::with_pack(pack.clone());
    let picks = [0usize, spec.zones.len() / 2, spec.zones.len() - 1];
    let mut decode_fail = 0usize;
    for i in picks {
        let z = &spec.zones[i];
        match cache.get(Path::new(&z.file)) {
            Ok(data) => {
                print!(
                    "  decode '{}': {} ch, {} Hz, {} frames ok",
                    z.file, data.channels, data.sample_rate, data.num_frames
                );
                if let Some(root) = &src_root {
                    match signal_sampler::engine::cache::load_sample(&root.join(&z.file)) {
                        Ok(src) if src.num_frames == data.num_frames => {
                            let dot: f64 = data
                                .frames
                                .iter()
                                .zip(src.frames.iter())
                                .map(|(a, b)| (*a as f64) * (*b as f64))
                                .sum();
                            let na: f64 = data.frames.iter().map(|a| (*a as f64).powi(2)).sum();
                            let nb: f64 = src.frames.iter().map(|b| (*b as f64).powi(2)).sum();
                            let corr = dot / (na.sqrt() * nb.sqrt()).max(f64::EPSILON);
                            print!("  corr={corr:.5}");
                            if corr < 0.95 {
                                decode_fail += 1;
                                print!("  LOW");
                            }
                        }
                        Ok(src) => {
                            decode_fail += 1;
                            print!(
                                "  LENGTH MISMATCH vs source ({} != {})",
                                data.num_frames, src.num_frames
                            );
                        }
                        Err(e) => print!("  (source unreadable: {e})"),
                    }
                }
                println!();
            }
            Err(e) => {
                decode_fail += 1;
                println!("  decode '{}': FAILED — {e}", z.file);
            }
        }
    }

    let looped = spec.zones.iter().filter(|z| z.loop_end > z.loop_start).count();
    println!("  looped zones: {looped}");

    if gaps == 0 && missing_files.is_empty() && decode_fail == 0 {
        println!("PASS");
        Ok(())
    } else {
        println!("PARTIAL");
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pack = std::env::args().nth(1).expect("usage: check_pack_resolve <pack>");
    let patch = PlayerPatch::from_pack(Path::new(&pack))?;
    let spec = &patch.spec;
    println!(">> engine default articulation = {:?}", engine_default(spec));

    // ── Zone-mode packs (CSS/CSB per-articulation packs, Omnisphere) ─────────
    // Convention-mode `patch.resolve` never fires for these; coverage comes
    // from the zone map itself. Check per-articulation key coverage, that
    // every zone's file has a pack entry, and that a few entries decode.
    if !spec.zones.is_empty() {
        return check_zone_mode(&patch);
    }

    let section = spec.sections.first().ok_or("no sections")?;
    let mic = spec.mics.first().map(|m| m.id.as_str()).unwrap_or("");
    let lo = note_name_midi(&section.lowest_note);
    let hi = note_name_midi(&section.highest_note);
    println!(
        "pack: {}  section={} mic={} range={}..={}  articulations={}",
        spec.name,
        section.id,
        mic,
        lo,
        hi,
        spec.articulations.len()
    );

    // Report coverage for EVERY declared articulation (the engine can select any).
    for art in &spec.articulations {
        let dyns: Vec<String> =
            if art.dynamics.is_empty() { vec![String::new()] } else { art.dynamics.clone() };
        let (mut tot, mut hit, mut playable) = (0usize, 0usize, 0usize);
        for n in lo..=hi {
            let mut any = false;
            for d in &dyns {
                tot += 1;
                if patch.resolve(&signal_sampler::SampleQuery {
                    section_id: &section.id,
                    articulation_id: &art.id,
                    mic_id: mic,
                    dynamic: d,
                    target_note: n,
                    direction: "",
                    rr: 0,
                }).is_some() {
                    hit += 1;
                    any = true;
                }
            }
            if any {
                playable += 1;
            }
        }
        println!("  artic '{}': {hit}/{tot} note×dyn, {playable}/{} notes", art.id, (lo..=hi).count());
    }

    // Check the first (default) articulation — the one the engine plays by default.
    let art = spec.articulations.first().ok_or("no articulations")?;
    let dyns: Vec<String> =
        if art.dynamics.is_empty() { vec![String::new()] } else { art.dynamics.clone() };

    let mut total = 0usize;
    let mut hits = 0usize;
    let mut miss_notes: Vec<u8> = Vec::new();
    let mut n = lo;
    while n <= hi {
        // A note counts as playable if ANY declared dynamic resolves.
        let mut any = false;
        for d in &dyns {
            total += 1;
            if patch.resolve(&signal_sampler::SampleQuery {
                    section_id: &section.id,
                    articulation_id: &art.id,
                    mic_id: mic,
                    dynamic: d,
                    target_note: n,
                    direction: "",
                    rr: 0,
                }).is_some() {
                any = true;
                hits += 1;
            }
        }
        if !any {
            miss_notes.push(n);
        }
        n += 1;
    }

    let playable_notes = (lo..=hi).count() - miss_notes.len();
    println!(
        "default articulation '{}': {}/{} note×dyn resolved; {} of {} notes playable",
        art.id,
        hits,
        total,
        playable_notes,
        (lo..=hi).count()
    );
    if !miss_notes.is_empty() {
        println!("  notes with NO sample at any dynamic: {miss_notes:?}");
    }
    println!("{}", if miss_notes.is_empty() { "PASS — every note resolves" } else { "PARTIAL" });
    Ok(())
}

fn note_name_midi(s: &str) -> u8 {
    // Minimal note-name → MIDI (C-1 = 0). Falls back to a sane default.
    let b = s.as_bytes();
    if b.is_empty() {
        return 21;
    }
    let step = match b[0].to_ascii_uppercase() {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return 21,
    };
    let mut i = 1;
    let mut semis = step;
    if i < b.len() && (b[i] == b'#' || b[i] == b's') {
        semis += 1;
        i += 1;
    } else if i < b.len() && b[i] == b'b' {
        semis -= 1;
        i += 1;
    }
    let oct: i32 = std::str::from_utf8(&b[i..]).ok().and_then(|o| o.parse().ok()).unwrap_or(4);
    ((oct + 1) * 12 + semis).clamp(0, 127) as u8
}
