//! Audit every MM2 Cradle mix in a library's `Mixes/` dir: parse, count
//! strips/cables/fx, inventory FX types (mapped vs unmapped DSP), and check
//! kit-name pairing against `Presets/*.signalpreset`.
//!   cargo run -p signal-drums --example mix_audit [library-dir]

use std::collections::BTreeMap;
use std::path::PathBuf;

use signal_drums::{cradle, mm2fx};

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lib = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2",
            )
        });
    let kits: Vec<String> = std::fs::read_dir(lib.join("Presets"))?
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension().and_then(|x| x.to_str()) == Some("signalpreset"))
                .then(|| p.file_stem().unwrap().to_string_lossy().into_owned())
        })
        .collect();
    let mut fx_counts: BTreeMap<String, (usize, bool)> = BTreeMap::new();
    let mut unpaired = Vec::new();
    let mut mixes: Vec<PathBuf> = std::fs::read_dir(lib.join("Mixes"))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("preset"))
        .collect();
    mixes.sort();
    println!("{} mixes, {} kit presets\n", mixes.len(), kits.len());
    for path in &mixes {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(path)?;
        let mixer = match cradle::parse_mixer(&text) {
            Ok(m) => m,
            Err(e) => {
                println!("✗ {name}: PARSE FAILED: {e}");
                continue;
            }
        };
        let mut mapped = 0usize;
        let mut skipped: Vec<&str> = Vec::new();
        for s in &mixer.strips {
            for fx in s.fx_slots() {
                let e = fx_counts.entry(fx.fx_type.clone()).or_insert((0, false));
                e.0 += 1;
                if fx.bypass {
                    continue;
                }
                if mm2fx::build_instance(&fx, 48_000.0).is_some() {
                    e.1 = true;
                    mapped += 1;
                } else {
                    skipped.push(&s.name);
                }
            }
        }
        let paired = kits.iter().any(|k| norm(k) == norm(&name));
        if !paired && norm(&name) != "default" {
            unpaired.push(name.clone());
        }
        println!(
            "{} {name}: {} strips, {} cables, fx {} mapped / {} skipped",
            if paired { "✓" } else { "○" },
            mixer.strips.len(),
            mixer.cables.len(),
            mapped,
            skipped.len(),
        );
    }
    println!("\nFX types across all mixes (count, DSP mapping):");
    for (ty, (n, have)) in &fx_counts {
        println!("  {} {ty:<22} ×{n}", if *have { "✓" } else { "✗" });
    }
    if !unpaired.is_empty() {
        println!("\nmixes with NO matching kit preset:");
        for m in &unpaired {
            println!("  ○ {m}");
        }
    }
    Ok(())
}
