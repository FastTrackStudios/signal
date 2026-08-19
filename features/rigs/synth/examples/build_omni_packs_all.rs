//! `build_omni_packs_all` — batch-rebuild every multisample Omnisphere
//! soundsource into a `.signalpack` **with loops** (STINFO) + folder-derived
//! tags, **mirroring the extraction's folder hierarchy** under the packs root.
//!
//! ```text
//! cargo run -p signal-synth --release --example build_omni_packs_all -- \
//!     "<extraction_root>" "<packs_root>" [--force]
//! ```
//! Defaults: extraction `/run/media/AudioHaven/Sampled/Keys/Omnisphere`,
//! packs `/run/media/AudioHaven/Signal/Libraries/Keys/Omnisphere/Packs`.
//!
//! Resumable: a soundsource whose output pack already carries loops is skipped
//! (re-run to continue); `--force` rebuilds everything. Only multisample dirs
//! (`<name>/library.styx`) are processed — flat one-shots don't loop.

use std::path::{Path, PathBuf};

use signal_synth::pack::{build_soundsource_pack, PackTags};

const EXTRACTION: &str = "/run/media/AudioHaven/Sampled/Keys/Omnisphere";
const PACKS: &str = "/run/media/AudioHaven/Signal/Libraries/Keys/Omnisphere/Packs";

fn main() {
    let mut args = std::env::args().skip(1).peekable();
    let ext_root = PathBuf::from(
        args.peek()
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .inspect(|_| {
                args.next();
            })
            .unwrap_or_else(|| EXTRACTION.into()),
    );
    let packs_root = PathBuf::from(
        args.peek()
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .inspect(|_| {
                args.next();
            })
            .unwrap_or_else(|| PACKS.into()),
    );
    let force = std::env::args().any(|a| a == "--force");

    // Discover every multisample soundsource dir (has library.styx).
    let mut sources = Vec::new();
    let mut stack = vec![ext_root.clone()];
    while let Some(dir) = stack.pop() {
        if dir.join("library.styx").exists() {
            sources.push(dir);
            continue; // don't descend into a soundsource
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if p.is_dir() && !name.starts_with('.') {
                stack.push(p);
            }
        }
    }
    sources.sort();
    eprintln!(
        "build_omni_packs_all: {} multisample soundsources",
        sources.len()
    );

    let t = std::time::Instant::now();
    let (mut built, mut skipped, mut failed, mut zones_looped) = (0u32, 0u32, 0u32, 0u64);
    for (i, dir) in sources.iter().enumerate() {
        let rel = dir.strip_prefix(&ext_root).unwrap_or(dir);
        let out = packs_root.join(rel).with_extension("signalpack");

        if !force && already_looped(&out) {
            skipped += 1;
            continue;
        }
        let tags = tags_for(rel);
        match build_soundsource_pack(dir, &out, &tags) {
            Ok(_stats) => {
                built += 1;
                if let Ok(h) = signal_sampler::read_pack_header(&out) {
                    zones_looped += h
                        .spec
                        .zones
                        .iter()
                        .filter(|z| z.loop_end > z.loop_start)
                        .count() as u64;
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("  FAIL {}: {e}", rel.display());
            }
        }
        if (i + 1) % 25 == 0 || i + 1 == sources.len() {
            eprintln!(
                "  {}/{}  built={built} skipped={skipped} failed={failed}  ({:.0}s)",
                i + 1,
                sources.len(),
                t.elapsed().as_secs_f32()
            );
        }
    }
    eprintln!(
        "build_omni_packs_all: done in {:?} — built={built} skipped={skipped} failed={failed}, {zones_looped} looped zones",
        t.elapsed()
    );
}

/// A pack already carries loops ⇒ it was rebuilt with the STINFO fix; skip it.
fn already_looped(out: &Path) -> bool {
    out.exists()
        && signal_sampler::read_pack_header(out)
            .ok()
            .map(|h| h.spec.zones.iter().any(|z| z.loop_end > z.loop_start))
            .unwrap_or(false)
}

/// Tags from the extraction path components (`Core Soundsources/<family>/<name>`):
/// category = immediate family folder, style = the folder trail.
fn tags_for(rel: &Path) -> PackTags {
    let comps: Vec<String> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str().map(str::to_string))
        .collect();
    let n = comps.len();
    let folders = &comps[..n.saturating_sub(1)]; // drop the soundsource name
    let category = folders.last().cloned().unwrap_or_default();
    PackTags {
        category,
        instrument: String::new(),
        style: folders.to_vec(),
    }
}
