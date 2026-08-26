//! Static playability check for a `.signalpack`: load it via `from_pack`, then
//! resolve the default section/articulation/mic across the keyboard for every
//! declared dynamic. A pack that resolves everywhere the styx declares is
//! playable (the engine reaches exactly these map entries). Bypasses the audio
//! render path.
//!   cargo run -p signal-sampler --release --example check_pack_resolve -- "<pack>"

use std::path::Path;

use signal_sampler::PlayerPatch;

/// Mirror of the engine's default-articulation picker (engine/mod.rs) so we can
/// report which articulation actually plays by default.
use signal_sampler::engine::default_articulation as engine_default;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pack = std::env::args()
        .nth(1)
        .expect("usage: check_pack_resolve <pack>");
    let patch = PlayerPatch::from_pack(Path::new(&pack))?;
    let spec = &patch.spec;
    println!(
        ">> engine default articulation = {:?}",
        engine_default(spec)
    );

    // ── Zone-mode packs (CSS/CSB per-articulation packs, Omnisphere) ─────────
    // Convention-mode `patch.resolve` never fires for these; the shared CLI
    // check (`fts signal pack check`) covers zone coverage, entry presence,
    // and decode probes (+ `--src-root` source A/B).
    if !spec.zones.is_empty() {
        let src_root = {
            let args: Vec<String> = std::env::args().collect();
            args.iter()
                .position(|a| a == "--src-root")
                .and_then(|i| args.get(i + 1))
                .map(std::path::PathBuf::from)
        };
        let pass = signal_sampler::pack_cli::run_check(Path::new(&pack), src_root.as_deref())?;
        if !pass {
            std::process::exit(1);
        }
        return Ok(());
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
        let dyns: Vec<String> = if art.dynamics.is_empty() {
            vec![String::new()]
        } else {
            art.dynamics.clone()
        };
        let (mut tot, mut hit, mut playable) = (0usize, 0usize, 0usize);
        for n in lo..=hi {
            let mut any = false;
            for d in &dyns {
                tot += 1;
                if patch
                    .resolve(&signal_sampler::SampleQuery {
                        section_id: &section.id,
                        articulation_id: &art.id,
                        mic_id: mic,
                        dynamic: d,
                        target_note: n,
                        direction: "",
                        rr: 0,
                    })
                    .is_some()
                {
                    hit += 1;
                    any = true;
                }
            }
            if any {
                playable += 1;
            }
        }
        println!(
            "  artic '{}': {hit}/{tot} note×dyn, {playable}/{} notes",
            art.id,
            (lo..=hi).count()
        );
    }

    // Check the first (default) articulation — the one the engine plays by default.
    // Measure the articulation the ENGINE will actually start on, not
    // whichever happens to be declared first (they differ: Keyscape's C7
    // lists an empty pedal layer first).
    let default_id = engine_default(spec).ok_or("no articulations")?;
    let art = spec
        .articulations
        .iter()
        .find(|a| a.id == default_id)
        .ok_or("default articulation missing from spec")?;
    let dyns: Vec<String> = if art.dynamics.is_empty() {
        vec![String::new()]
    } else {
        art.dynamics.clone()
    };

    let mut total = 0usize;
    let mut hits = 0usize;
    let mut miss_notes: Vec<u8> = Vec::new();
    let mut n = lo;
    while n <= hi {
        // A note counts as playable if ANY declared dynamic resolves.
        let mut any = false;
        for d in &dyns {
            total += 1;
            if patch
                .resolve(&signal_sampler::SampleQuery {
                    section_id: &section.id,
                    articulation_id: &art.id,
                    mic_id: mic,
                    dynamic: d,
                    target_note: n,
                    direction: "",
                    rr: 0,
                })
                .is_some()
            {
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
    println!(
        "{}",
        if miss_notes.is_empty() {
            "PASS — every note resolves"
        } else {
            "PARTIAL"
        }
    );
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
    let oct: i32 = std::str::from_utf8(&b[i..])
        .ok()
        .and_then(|o| o.parse().ok())
        .unwrap_or(4);
    ((oct + 1) * 12 + semis).clamp(0, 127) as u8
}
