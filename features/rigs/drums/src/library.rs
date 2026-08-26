//! Sample-library catalog for the kit designer: scan the library's
//! `.signalengine` files, group them by engine type (kick, snare, tom, …), and
//! map a loaded preset's engine slots onto friendly labels so any piece can be
//! swapped for another of the same type.

use std::path::{Path, PathBuf};

use signal_drums_proto::LibraryPiece;
use signal_sampler::{EngineSpec, PresetSpec};

/// Recursively collect every `.signalengine` under `root` as a [`LibraryPiece`]
/// (name + absolute path + engine type). Engines whose type is empty/`effects`
/// are still listed under their raw type so nothing is silently dropped.
pub fn scan_engines(root: &Path) -> Vec<LibraryPiece> {
    let mut out = Vec::new();
    collect(root, &mut out);
    out.sort_by(|a, b| {
        (a.kind.clone(), a.name.to_lowercase()).cmp(&(b.kind.clone(), b.name.to_lowercase()))
    });
    out
}

fn collect(dir: &Path, out: &mut Vec<LibraryPiece>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("signalengine") {
            let (name, kind) = match EngineSpec::from_file(&path) {
                Ok(spec) => {
                    let name = if spec.name.is_empty() {
                        stem(&path)
                    } else {
                        spec.name.clone()
                    };
                    (name, spec.engine_type.to_ascii_lowercase())
                }
                Err(_) => (stem(&path), String::new()),
            };
            out.push(LibraryPiece {
                name,
                path: path.display().to_string(),
                kind,
            });
        }
    }
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("engine")
        .to_string()
}

/// The engine directory of a library root (kits keep engines in `Engines/`;
/// fall back to the root so a flat library still scans).
pub fn engines_dir(library_root: &Path) -> PathBuf {
    let e = library_root.join("Engines");
    if e.is_dir() {
        e
    } else {
        library_root.to_path_buf()
    }
}

/// Canonical piece label for a preset engine-slot id — the MM2 mixer strip
/// name, so it doubles as the mix-import match key. Handles the differing id
/// schemes across presets: Metal Monster uses `rtom1`/`hats`/`snare`, while
/// Pound/Organic use `rtom-a`/`hh-o`/`snare-a`/`kick-b`. Tom numbering comes
/// from a trailing `1`/`2` or `-a`/`-b`.
pub fn slot_label(slot_id: &str) -> String {
    let s = slot_id.to_ascii_lowercase();
    // Which of a pair of same-type pieces: `-b`/`2`/`3` → 2/3, else 1.
    let ord = |s: &str| -> u32 {
        if s.contains('3') {
            3
        } else if s.ends_with("-b") || s.contains('2') {
            2
        } else {
            1
        }
    };
    if s.starts_with("kick") {
        "Kick".into()
    } else if s.starts_with("snare") {
        "Snare".into()
    } else if s.starts_with("rtom") || s.starts_with("racktom") || s.starts_with("rack-tom") {
        format!("Rack Tom {}", ord(&s))
    } else if s.starts_with("ftom") || s.starts_with("floortom") || s.starts_with("floor-tom") {
        format!("Floor Tom {}", ord(&s))
    } else if s.starts_with("hh") || s.starts_with("hat") {
        "Hats".into()
    } else if s.starts_with("ride") {
        "Ride".into()
    } else if s.starts_with("splash") {
        "Splash".into()
    } else if s.starts_with("china") {
        "China".into()
    } else if s.starts_with("crash") {
        // crash-l / crash-r / crash-fl / crash-fr
        if s.contains("fl") {
            "Crash Far L".into()
        } else if s.contains("fr") {
            "Crash Far R".into()
        } else if s.ends_with('r') || s.contains("-r") {
            "Crash R".into()
        } else {
            "Crash L".into()
        }
    } else {
        title_case(slot_id)
    }
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Infer a slot's engine *kind* (for matching library pieces) from the slot id
/// when the current engine's type can't be read.
pub fn kind_from_slot(slot_id: &str) -> &'static str {
    let s = slot_id.to_ascii_lowercase();
    if s.contains("kick") {
        "kick"
    } else if s.contains("snare") {
        "snare"
    } else if s.contains("tom") {
        "tom"
    } else if s.contains("hat") || s == "hh" || s == "hats" {
        "hi-hat"
    } else if s.contains("ride") {
        "ride"
    } else if s.contains("crash") {
        "crash"
    } else if s.contains("china") {
        "china"
    } else if s.contains("splash") {
        "splash"
    } else {
        ""
    }
}

/// Read a preset's engine slots (id + current engine path/name). `preset_dir`
/// resolves the relative engine refs to absolute paths.
pub fn preset_slots(spec: &PresetSpec, preset_dir: &Path) -> Vec<(String, PathBuf)> {
    spec.engines
        .iter()
        .map(|e| {
            let p = PathBuf::from(&e.engine);
            let abs = if p.is_absolute() {
                p
            } else {
                preset_dir.join(p)
            };
            (e.id.clone(), abs)
        })
        .collect()
}
