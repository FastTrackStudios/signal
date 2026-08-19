//! `build_omni_pack` — build a `.signalpack` for one Omnisphere soundsource,
//! baking in the sustain **loops** (recovered from each FLAC's `STINFO` tag by
//! [`LibrarySpec::from_file`]) and browser **tags** (category / instrument /
//! style). The pack re-encodes the FLAC bodies for disk streaming, so the loop
//! points must live in the embedded spec — which this tool writes.
//!
//! The embedded spec is produced by **text-injecting** loop + tag lines into
//! the original `library.styx`, NOT by re-serializing the parsed spec:
//! `facet_styx::to_string` emits defaulted `Option` fields (e.g.
//! `dynamics.sustain_controller: None`) as variant tags the styx *parser*
//! rejects, which would make the pack unloadable. Injecting into the verbatim
//! original keeps it parseable.
//!
//! ```text
//! cargo run -p signal-synth --release --example build_omni_pack -- \
//!     "<soundsource_dir>" "<out.signalpack>" [category] [instrument] [style,style,…]
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use signal_sampler::engine::cache::create_signal_pack;
use signal_sampler::LibrarySpec;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir =
        PathBuf::from(args.next().expect(
            "usage: build_omni_pack <dir> <out.signalpack> [category] [instrument] [styles]",
        ));
    let out = PathBuf::from(args.next().expect("missing <out.signalpack>"));
    let category = args.next().unwrap_or_default();
    let instrument = args.next().unwrap_or_default();
    let styles: Vec<String> = args
        .next()
        .map(|s| {
            s.split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let styx = dir.join("library.styx");
    assert!(styx.exists(), "no library.styx in {}", dir.display());

    // `from_file` recovers each zone's sustain loop from its FLAC STINFO tag.
    // We only read the resulting (file -> loop) map back out; the pack's spec is
    // built by text-injecting into the original styx (see module docs).
    let spec = LibrarySpec::from_file(&styx)?;
    let loops: HashMap<String, (u32, u32)> = spec
        .zones
        .iter()
        .filter(|z| z.loop_end > z.loop_start)
        .map(|z| (z.file.clone(), (z.loop_start, z.loop_end)))
        .collect();
    eprintln!(
        "build_omni_pack: {} — {} zones, {} with sustain loops",
        spec.name,
        spec.zones.len(),
        loops.len()
    );
    if loops.is_empty() {
        eprintln!(
            "build_omni_pack: WARNING — no loops recovered (STINFO missing?); pad zones will decay"
        );
    }

    let original = std::fs::read_to_string(&styx)?;
    let enriched = inject(&original, &loops, &category, &instrument, &styles);

    // Serialize the enriched spec to a temp library.styx, then pack.
    let tmp_dir = std::env::temp_dir().join(format!("omni-pack-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir)?;
    let tmp_styx = tmp_dir.join("library.styx");
    std::fs::write(&tmp_styx, &enriched)?;

    // Sanity: the injected spec must still parse (it embeds into the pack).
    LibrarySpec::from_file(&tmp_styx)
        .map_err(|e| format!("enriched styx no longer parses: {e}"))?;

    // Every audio file directly under the soundsource dir.
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| {
                    matches!(
                        e.to_ascii_lowercase().as_str(),
                        "flac" | "wav" | "aif" | "aiff"
                    )
                })
                .unwrap_or(false)
        })
        .collect();
    paths.sort();

    let t = std::time::Instant::now();
    let stats = create_signal_pack(&out, &tmp_styx, &dir, paths.iter().map(PathBuf::as_path))?;
    std::fs::remove_dir_all(&tmp_dir).ok();
    eprintln!(
        "build_omni_pack: done in {:?} — {stats:?}\n  -> {}",
        t.elapsed(),
        out.display()
    );
    if stats.prepared == 0 {
        return Err("no samples packed".into());
    }
    Ok(())
}

/// Inject `loop_start`/`loop_end` after each zone's `file "…"` line, and the
/// category / instrument / style tags before the `zones (` block — preserving
/// the original styx verbatim otherwise.
fn inject(
    original: &str,
    loops: &HashMap<String, (u32, u32)>,
    category: &str,
    instrument: &str,
    styles: &[String],
) -> String {
    let mut out = String::with_capacity(original.len() + 4096);
    let mut tags_done = false;
    for line in original.lines() {
        let trimmed = line.trim_start();

        // Tags: emit once, right before the zones block.
        if !tags_done && trimmed.starts_with("zones") {
            if !category.is_empty() {
                out.push_str(&format!("category {}\n", quote(category)));
            }
            if !instrument.is_empty() {
                out.push_str(&format!("instrument {}\n", quote(instrument)));
            }
            if !styles.is_empty() {
                out.push_str("style (\n");
                for s in styles {
                    out.push_str(&format!("    {}\n", quote(s)));
                }
                out.push_str(")\n");
            }
            tags_done = true;
        }

        out.push_str(line);
        out.push('\n');

        // Loops: after a zone's `file "NAME"` line, add its loop window.
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
    let rest = trimmed.strip_prefix("file")?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    rest.split('"').next()
}

/// Quote a styx scalar string.
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}
