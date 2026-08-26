//! Heuristics + driver for retagging existing `.signalpack` files in place.
//!
//! Walks a library tree, derives `instrument` / `category` / `style` / `tags`
//! from each pack's path, and rewrites the embedded styx via
//! [`crate::pack_rewrite::rewrite_embedded_spec`]. Audio bodies are copied
//! verbatim — this is the fast path (no re-encoding).
//!
//! Heuristics are intentionally simple and conservative — anything we can't
//! confidently classify is left blank rather than misclassified.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use rayon::prelude::*;

use crate::pack_rewrite::rewrite_embedded_spec;
use crate::SamplerError;

#[derive(Debug, Default, Clone)]
pub struct Derived {
    pub instrument: String,
    pub category: String,
    pub style: Vec<String>,
    pub tags: Vec<TagEntry>,
}

#[derive(Debug, Clone)]
pub struct TagEntry {
    /// Matches `signal_proto::tagging::TagCategory::as_str()`.
    pub category: &'static str,
    pub value: String,
}

#[derive(Debug, Default)]
pub struct RetagSummary {
    pub ok: usize,
    pub failed: usize,
    pub elapsed_secs: f64,
}

/// Walk `root`, retag every `.signalpack` (skipping any path containing one
/// of the `skip` substrings). Parallel via rayon.
pub fn retag_tree(
    root: &Path,
    skip: &[String],
    progress: impl Fn(usize, usize) + Sync,
) -> Result<RetagSummary, SamplerError> {
    if !root.is_dir() {
        return Err(SamplerError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("not a directory: {}", root.display()),
        )));
    }
    let packs = discover_packs(root, skip);
    let total = packs.len();
    let ok = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let start = Instant::now();

    packs.par_iter().for_each(|path| {
        let derived = derive(path, root);
        let appended = render_styx_appendix(&derived);
        let res = rewrite_embedded_spec(path, |old| {
            let stripped = strip_managed_fields(old);
            format!("{stripped}\n{appended}")
        });
        match res {
            Ok(()) => {
                let n = ok.fetch_add(1, Ordering::Relaxed) + 1;
                progress(n, total);
            }
            Err(e) => {
                failed.fetch_add(1, Ordering::Relaxed);
                tracing::warn!("retag {}: {e}", path.display());
            }
        }
    });

    Ok(RetagSummary {
        ok: ok.load(Ordering::Relaxed),
        failed: failed.load(Ordering::Relaxed),
        elapsed_secs: start.elapsed().as_secs_f64(),
    })
}

/// Discover packs without retagging — useful for `--dry-run` previews.
pub fn discover_packs(root: &Path, skip: &[String]) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(12)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "signalpack")
                && !skip.iter().any(|s| p.to_string_lossy().contains(s))
        })
        .collect()
}

pub fn derive(pack_path: &Path, root: &Path) -> Derived {
    let rel = pack_path.strip_prefix(root).unwrap_or(pack_path);
    let segs: Vec<&str> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    let mut d = Derived::default();
    let top = segs.first().copied().unwrap_or("");
    let lib = segs.get(1).copied().unwrap_or("");

    match top {
        "Drum Kits" => {
            if lib.contains("Stylus RMX") {
                d.category = "groove".into();
                d.instrument = "drums".into();
                d.tags.push(TagEntry {
                    category: "instrument",
                    value: "drums".into(),
                });
                d.tags.push(TagEntry {
                    category: "context",
                    value: "groove".into(),
                });
                d.tags.push(TagEntry {
                    category: "vendor",
                    value: "Spectrasonics".into(),
                });
                if let Some(suite) = segs.iter().rev().nth(1) {
                    d.style.push((*suite).to_string());
                    if let Some(bpm) = extract_leading_bpm(suite) {
                        d.style.push(format!("{bpm}bpm"));
                    }
                }
            } else {
                d.category = "drum-kit".into();
                d.instrument = drum_piece_from_path(&segs).unwrap_or_else(|| "drums".into());
                d.tags.push(TagEntry {
                    category: "instrument",
                    value: d.instrument.clone(),
                });
                d.style.push(lib.to_string());
                if let Some(v) = vendor_from_lib(lib) {
                    d.tags.push(TagEntry {
                        category: "vendor",
                        value: v.into(),
                    });
                }
            }
        }
        "Keys" => {
            d.category = "keys".into();
            d.instrument = keys_instrument(lib, pack_path);
            d.tags.push(TagEntry {
                category: "instrument",
                value: d.instrument.clone(),
            });
            if !lib.is_empty() {
                d.tags.push(TagEntry {
                    category: "vendor",
                    value: lib.into(),
                });
            }
            if matches!(lib, "Omnisphere" | "Trilian") {
                d.category = if lib == "Trilian" {
                    "synth-bass".into()
                } else {
                    "synth".into()
                };
                d.tags.push(TagEntry {
                    category: "engine_type",
                    value: "synth".into(),
                });
            }
        }
        "Orchestral" => {
            // segments: ["Orchestral", "<series>", "<library>", "Packs", "<patch>", ...]
            d.category = "orchestral".into();
            let lib_name = segs.get(2).copied().unwrap_or("");
            let patch = segs.get(4).copied().unwrap_or("");
            d.instrument = orchestral_instrument(lib_name, patch);
            d.tags.push(TagEntry {
                category: "instrument",
                value: d.instrument.clone(),
            });
            d.tags.push(TagEntry {
                category: "context",
                value: "orchestral".into(),
            });
            if let Some(vendor) = orchestral_vendor(lib_name) {
                d.tags.push(TagEntry {
                    category: "vendor",
                    value: vendor.into(),
                });
            }
            if !patch.is_empty() {
                d.style.push(patch.into());
            }
        }
        _ => {}
    }
    d
}

fn extract_leading_bpm(name: &str) -> Option<u32> {
    let s = name.trim_start_matches(|c: char| !c.is_ascii_digit());
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    let bpm: u32 = digits.parse().ok()?;
    (40..=220).contains(&bpm).then_some(bpm)
}

fn vendor_from_lib(lib: &str) -> Option<&'static str> {
    if lib.starts_with("GGD") {
        Some("GetGoodDrums")
    } else {
        None
    }
}

fn drum_piece_from_path(segs: &[&str]) -> Option<String> {
    const PIECES: &[(&str, &str)] = &[
        ("Kick", "kick"),
        ("Snare", "snare"),
        ("Hi-Hat", "hi-hat"),
        ("Hat", "hi-hat"),
        ("Ride", "ride"),
        ("Crash", "crash"),
        ("China", "china"),
        ("Splash", "splash"),
        ("Tom", "tom"),
        ("Stack", "stack"),
        ("Effect", "effects"),
        ("Cymbal", "cymbal"),
    ];
    for seg in segs.iter().rev() {
        for (needle, value) in PIECES {
            if seg.contains(needle) {
                return Some((*value).into());
            }
        }
    }
    None
}

fn keys_instrument(lib: &str, pack_path: &Path) -> String {
    let stem = pack_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if lib == "Keyscape" {
        match () {
            _ if stem.contains("rhodes") => "rhodes".into(),
            _ if stem.contains("wurl") => "wurlitzer".into(),
            _ if stem.contains("clav") => "clavinet".into(),
            _ if stem.contains("harpsichord") => "harpsichord".into(),
            _ if stem.contains("toy") => "toy-piano".into(),
            _ if stem.contains("celest") => "celesta".into(),
            _ if stem.contains("vibe") || stem.contains("vibraphone") => "vibraphone".into(),
            _ if stem.contains("glock") => "glockenspiel".into(),
            _ if stem.contains("mks") || stem.contains("jd-800") || stem.contains("dx") => {
                "synth-keys".into()
            }
            _ if stem.contains("piano") || stem.contains("grand") || stem.contains("upright") => {
                "piano".into()
            }
            _ => "keys".into(),
        }
    } else if lib == "Trilian" {
        "synth-bass".into()
    } else if lib == "Omnisphere" {
        "synth".into()
    } else {
        "keys".into()
    }
}

fn orchestral_instrument(lib: &str, patch: &str) -> String {
    let p = patch.to_lowercase();
    if p.contains("violin overtone") {
        return "violin-overtones".into();
    }
    if p.contains("violin") {
        return "violin".into();
    }
    if p.contains("viola") {
        return "viola".into();
    }
    if p.contains("cello") {
        return "cello".into();
    }
    if p.contains("contrabass") && !p.contains("bone") {
        return "contrabass".into();
    }
    if p.contains("bass") && !p.contains("trombone") {
        return "double-bass".into();
    }
    if p.contains("harp") {
        return "harp".into();
    }
    if p.contains("trumpet") {
        return "trumpet".into();
    }
    if p.contains("trombone") || p.contains("bone") {
        return "trombone".into();
    }
    if p.contains("french horn") || p.contains("horn") {
        return "french-horn".into();
    }
    if p.contains("tuba") {
        return "tuba".into();
    }
    if p.contains("flute") {
        return "flute".into();
    }
    if p.contains("oboe") {
        return "oboe".into();
    }
    if p.contains("clarinet") {
        return "clarinet".into();
    }
    if p.contains("bassoon") {
        return "bassoon".into();
    }
    if p.contains("piano") || p.contains("grand") {
        return "piano".into();
    }
    if p.contains("ensemble") {
        return "ensemble-strings".into();
    }
    // CS Woodwinds short codes.
    match patch {
        "Saf" => return "alto-flute".into(),
        "Spi" => return "piccolo".into(),
        "Sbc" => return "bass-clarinet".into(),
        "Scb" => return "contra-bassoon".into(),
        "Sca" => return "contra-alto-clarinet".into(),
        _ => {}
    }
    let _ = lib;
    "".into()
}

fn orchestral_vendor(lib: &str) -> Option<&'static str> {
    if lib.starts_with("Cinematic Studio") {
        Some("Cinematic Studio")
    } else if lib.starts_with("Pacific") {
        Some("Performance Samples")
    } else {
        None
    }
}

pub fn render_styx_appendix(d: &Derived) -> String {
    let mut out = String::new();
    if !d.instrument.is_empty() {
        out.push_str(&format!("instrument {}\n", quote(&d.instrument)));
    }
    if !d.category.is_empty() {
        out.push_str(&format!("category {}\n", quote(&d.category)));
    }
    if !d.style.is_empty() {
        out.push_str("style (\n");
        for s in &d.style {
            out.push_str(&format!("    {}\n", quote(s)));
        }
        out.push_str(")\n");
    }
    if !d.tags.is_empty() {
        out.push_str("tags (\n");
        for t in &d.tags {
            // facet-styx wants one field per line.
            out.push_str("    {\n");
            out.push_str(&format!(
                "        category @{}\n",
                tag_category_variant(t.category)
            ));
            out.push_str(&format!("        value    {}\n", quote(&t.value)));
            out.push_str("        source   @Manual\n");
            out.push_str("        weight   60\n");
            out.push_str("    }\n");
        }
        out.push_str(")\n");
    }
    out
}

fn tag_category_variant(short: &str) -> &'static str {
    match short {
        "rig_type" => "RigType",
        "engine_type" => "EngineType",
        "domain_level" => "DomainLevel",
        "instrument" => "Instrument",
        "tone" => "Tone",
        "character" => "Character",
        "genre" => "Genre",
        "context" => "Context",
        "module" => "Module",
        "block" => "Block",
        "vendor" => "Vendor",
        "plugin" => "Plugin",
        "workflow" => "Workflow",
        _ => "Custom",
    }
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Drop top-level lines/blocks for fields this tool manages, so re-runs are
/// idempotent. Other top-level structure is preserved character-for-character.
fn strip_managed_fields(spec: &str) -> String {
    let mut out = Vec::with_capacity(spec.len());
    let lines: Vec<&str> = spec.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let is_managed_scalar =
            trimmed.starts_with("instrument ") || trimmed.starts_with("category ");
        let is_managed_block = trimmed.starts_with("style (") || trimmed.starts_with("tags (");
        if line.starts_with(' ') || line.starts_with('\t') {
            out.push(line);
            i += 1;
            continue;
        }
        if is_managed_scalar {
            i += 1;
            continue;
        }
        if is_managed_block {
            i += 1;
            while i < lines.len() {
                let l = lines[i];
                i += 1;
                if l.trim() == ")" && !l.starts_with(' ') && !l.starts_with('\t') {
                    break;
                }
            }
            continue;
        }
        out.push(line);
        i += 1;
    }
    let mut joined = out.join("\n");
    while joined.ends_with("\n\n") {
        joined.pop();
    }
    joined
}
