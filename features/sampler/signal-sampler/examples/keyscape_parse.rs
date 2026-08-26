//! Parse every sample stem in a Keyscape extraction and report how each
//! articulation maps — to catch mis-parsed release / pedal / mechanical-noise
//! samples.
//!   cargo run -p signal-sampler --example keyscape_parse -- "<extraction dir>"

use std::collections::BTreeMap;
use std::path::Path;

use signal_sampler::sample_map::parse_sample_stem;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| {
        "/run/media/AudioHaven/Sampled/Keys/Keyscape/Rhodes - LA Custom".to_string()
    });
    let mut total = 0usize;
    let mut unparsed: Vec<String> = Vec::new();
    // (articulation, direction) -> (count, min note, max note, sample note set size)
    let mut by_artic: BTreeMap<(String, String), (usize, u8, u8)> = BTreeMap::new();
    // articulation -> set of notes seen
    let mut notes: BTreeMap<String, std::collections::BTreeSet<u8>> = BTreeMap::new();

    for entry in walk(Path::new(&dir)) {
        let Some(stem) = entry.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if entry.extension().and_then(|e| e.to_str()) != Some("flac") {
            continue;
        }
        total += 1;
        match parse_sample_stem(stem) {
            Some(k) => {
                let e = by_artic
                    .entry((k.articulation.clone(), k.direction.clone()))
                    .or_insert((0, 255, 0));
                e.0 += 1;
                e.1 = e.1.min(k.note);
                e.2 = e.2.max(k.note);
                notes
                    .entry(k.articulation.clone())
                    .or_default()
                    .insert(k.note);
            }
            None => {
                if unparsed.len() < 20 {
                    unparsed.push(stem.to_string());
                }
            }
        }
    }

    println!("{total} samples\n");
    println!(
        "{:<28} {:<8} {:>6} {:>10} {:>10}",
        "articulation", "dir", "count", "note range", "#notes"
    );
    for ((artic, dir), (count, lo, hi)) in &by_artic {
        let nn = notes.get(artic).map(|s| s.len()).unwrap_or(0);
        println!(
            "{:<28} {:<8} {:>6} {:>4}..{:<4} {:>10}",
            artic, dir, count, lo, hi, nn
        );
    }
    let parsed: usize = by_artic.values().map(|(c, ..)| *c).sum();
    println!("\nparsed {parsed}/{total}; unparsed {}", total - parsed);
    for s in &unparsed {
        println!("  UNPARSED: {s}");
    }
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}
