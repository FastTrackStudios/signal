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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pack = std::env::args().nth(1).expect("usage: check_pack_resolve <pack>");
    let patch = PlayerPatch::from_pack(Path::new(&pack))?;
    let spec = &patch.spec;
    println!(">> engine default articulation = {:?}", engine_default(spec));

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
                if patch.resolve(&section.id, &art.id, mic, d, n, "", 0).is_some() {
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
            if patch.resolve(&section.id, &art.id, mic, d, n, "", 0).is_some() {
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
