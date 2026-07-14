//! Build `.signalpack`s from Omnisphere soundsource extractions — with the
//! sustain **loops** recovered from each FLAC's `STINFO` tag and browser
//! **tags** baked into the embedded spec.
//!
//! The loops/tags are **text-injected** into the verbatim original
//! `library.styx`, never round-tripped through `facet_styx::to_string`: that
//! emits defaulted `Option` fields (e.g. `dynamics.sustain_controller: None`)
//! as variant tags the styx *parser* rejects, which makes the pack unloadable.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use signal_sampler::LibrarySpec;
use signal_sampler::engine::cache::{PrepareStats, create_signal_pack};

/// Browser tags applied to a built pack.
#[derive(Debug, Clone, Default)]
pub struct PackTags {
    pub category: String,
    pub instrument: String,
    pub style: Vec<String>,
}

/// Build one soundsource dir (`<dir>/library.styx` + its audio) into `out`,
/// baking in STINFO loops + `tags`. Returns the pack stats.
pub fn build_soundsource_pack(
    dir: &Path,
    out: &Path,
    tags: &PackTags,
) -> Result<PrepareStats, String> {
    let styx = dir.join("library.styx");
    if !styx.exists() {
        return Err(format!("no library.styx in {}", dir.display()));
    }

    // `from_file` recovers each zone's sustain loop from its FLAC STINFO tag;
    // we read the (file → loop) map back out and text-inject it (see module doc).
    let spec = LibrarySpec::from_file(&styx).map_err(|e| e.to_string())?;
    let loops: HashMap<String, (u32, u32)> = spec
        .zones
        .iter()
        .filter(|z| z.loop_end > z.loop_start)
        .map(|z| (z.file.clone(), (z.loop_start, z.loop_end)))
        .collect();

    let original = std::fs::read_to_string(&styx).map_err(|e| e.to_string())?;
    let enriched = inject(&original, &loops, tags);

    let tmp_dir = std::env::temp_dir().join(format!(
        "omni-pack-{}-{}",
        std::process::id(),
        out.file_stem().and_then(|s| s.to_str()).unwrap_or("x")
    ));
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let tmp_styx = tmp_dir.join("library.styx");
    std::fs::write(&tmp_styx, &enriched).map_err(|e| e.to_string())?;
    // The injected spec must still parse (it embeds into the pack).
    LibrarySpec::from_file(&tmp_styx).map_err(|e| format!("enriched styx no longer parses: {e}"))?;

    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| matches!(e.to_ascii_lowercase().as_str(), "flac" | "wav" | "aif" | "aiff"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let stats = create_signal_pack(out, &tmp_styx, dir, paths.iter().map(PathBuf::as_path))
        .map_err(|e| e.to_string());
    std::fs::remove_dir_all(&tmp_dir).ok();
    let stats = stats?;
    if stats.prepared == 0 {
        return Err("no samples packed".into());
    }
    Ok(stats)
}

/// Inject `loop_start`/`loop_end` after each zone's `file "…"` line and the
/// category / instrument / style tags before the `zones (` block, preserving
/// the original styx verbatim otherwise.
fn inject(original: &str, loops: &HashMap<String, (u32, u32)>, tags: &PackTags) -> String {
    let mut out = String::with_capacity(original.len() + 4096);
    let mut tags_done = false;
    for line in original.lines() {
        let trimmed = line.trim_start();
        if !tags_done && trimmed.starts_with("zones") {
            if !tags.category.is_empty() {
                out.push_str(&format!("category {}\n", quote(&tags.category)));
            }
            if !tags.instrument.is_empty() {
                out.push_str(&format!("instrument {}\n", quote(&tags.instrument)));
            }
            if !tags.style.is_empty() {
                out.push_str("style (\n");
                for s in &tags.style {
                    out.push_str(&format!("    {}\n", quote(s)));
                }
                out.push_str(")\n");
            }
            tags_done = true;
        }
        out.push_str(line);
        out.push('\n');
        if let Some(name) = file_line_name(trimmed) {
            if let Some((ls, le)) = loops.get(name) {
                let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                out.push_str(&format!("{indent}loop_start {ls}\n{indent}loop_end {le}\n"));
            }
        }
    }
    out
}

/// If `trimmed` is a `file "NAME"` line, return NAME.
fn file_line_name(trimmed: &str) -> Option<&str> {
    trimmed
        .strip_prefix("file")?
        .trim_start()
        .strip_prefix('"')?
        .split('"')
        .next()
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}
