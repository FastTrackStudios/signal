//! Split a Cinematic Studio library into per-(section, articulation-group,
//! mic) signalpacks — lossless FLAC + Ogg Vorbis proxy.
//!
//!   cargo run -p signal-sampler --release --example build_cs_packs -- \
//!       "<lib_root>" <engine-config.styx> <groups.styx> "<out_root>" \
//!       [--sections "1st Violins,Cellos"] [--mics Main,Mix] [--groups Legato] \
//!       [--variant both|lossless|proxy] [--quality 0.8] [--dry-run] [--force]
//!
//! Inputs:
//! - `<lib_root>/_patches/<Section>/library.styx` — the rich per-section zone
//!   specs (loops, legato intervals, arrival/lead-in) written by the CS
//!   extraction. Zone `{...}` blocks are reused **verbatim** — this tool never
//!   re-serializes specs (facet_styx::to_string is a known pack-killer).
//! - `<engine-config.styx>` — the library engine config
//!   (e.g. features/rigs/orchestra/specs/cinematic-strings.styx); text-filtered
//!   per pack: sections → the one section, mics → the one mic (forced default),
//!   articulations → the group's members. Everything else verbatim.
//! - `<groups.styx>` — pack grouping (features/rigs/orchestra/specs/
//!   cs-strings-packs.styx): friendly pack name ← raw articulation ids, with
//!   releases/legato folded into their body group.
//!
//! Output layout (filename fully denotes the pack):
//!   <out_root>/<Section>/<Group>/<Section> - <Group> - <Mic>.signalpack
//!   <out_root>/<Section>/<Group>/<Section> - <Group> - <Mic>.proxy.signalpack

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use signal_sampler::engine::cache::{PackCodec, PackSpecSource, create_signal_pack_with};

// ── Comment/string-aware styx text scanning ───────────────────────────────────
// The engine config embeds `{`/`(` inside `//` comments and quoted strings, so
// naive brace counting mis-nests. One tiny scanner used by everything below.

/// Iterate `text` bytes, calling `f(i, byte, in_code)` where `in_code` is false
/// inside `//` comments and `"…"` strings.
fn scan(text: &str, mut f: impl FnMut(usize, u8, bool)) {
    let bytes = text.as_bytes();
    let (mut in_str, mut in_comment) = (false, false);
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_comment {
            if b == b'\n' {
                in_comment = false;
            }
            f(i, b, false);
        } else if in_str {
            f(i, b, false);
            if b == b'"' {
                in_str = false;
            }
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            in_comment = true;
            f(i, b, false);
        } else if b == b'"' {
            in_str = true;
            f(i, b, false);
        } else {
            f(i, b, true);
        }
        i += 1;
    }
}

/// Find the top-level `key (` … `)` list block. Returns
/// `(block_start, inner_start, inner_end, block_end)` byte offsets.
fn find_list_block(text: &str, key: &str) -> Option<(usize, usize, usize, usize)> {
    let mut depth = 0i32;
    let mut result = None;
    let mut opened: Option<(usize, usize)> = None; // (block_start, inner_start)
    let mut pending_key_at: Option<usize> = None;
    let bytes = text.as_bytes();
    scan(text, |i, b, in_code| {
        if !in_code || result.is_some() {
            return;
        }
        match b {
            b'(' | b'{' => {
                if depth == 0 && b == b'(' {
                    if let Some(ks) = pending_key_at {
                        opened = Some((ks, i + 1));
                    }
                }
                depth += 1;
            }
            b')' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some((bs, is)) = opened.take() {
                        result = Some((bs, is, i, i + 1));
                    }
                }
            }
            _ => {
                if depth == 0 && !b.is_ascii_whitespace() {
                    // Token start? Check whether `key` begins here at a word
                    // boundary followed by whitespace/`(`.
                    let at_word_start = i == 0
                        || bytes[i - 1].is_ascii_whitespace()
                        || bytes[i - 1] == b')'
                        || bytes[i - 1] == b'}';
                    if at_word_start && text[i..].starts_with(key) {
                        let after = i + key.len();
                        let ok = bytes
                            .get(after)
                            .map(|c| c.is_ascii_whitespace() || *c == b'(')
                            .unwrap_or(false);
                        if ok {
                            pending_key_at = Some(i);
                            return;
                        }
                    }
                    // Any other token between blocks clears a pending key…
                    // unless we're still inside the key word itself or the
                    // whitespace run after it.
                    if let Some(ks) = pending_key_at {
                        if i >= ks + key.len() {
                            pending_key_at = Some(ks); // whitespace handled above; non-ws non-paren token:
                            if !text[ks + key.len()..i].chars().all(char::is_whitespace) {
                                pending_key_at = None;
                            }
                        }
                    }
                }
            }
        }
    });
    result
}

/// Split a list block's inner text into top-level `{ … }` entry spans.
fn split_entries(inner: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    scan(inner, |i, b, in_code| {
        if !in_code {
            return;
        }
        match b {
            b'{' | b'(' => {
                if depth == 0 && b == b'{' {
                    start = i;
                }
                depth += 1;
            }
            b'}' | b')' => {
                depth -= 1;
                if depth == 0 && b == b'}' {
                    spans.push((start, i + 1));
                }
            }
            _ => {}
        }
    });
    spans
}

/// First `key <value>` field in an entry (any position — handles both
/// line-per-field and inline `{id Mix, label Mix}` styles). Unquotes.
fn entry_field(entry: &str, key: &str) -> Option<String> {
    let bytes = entry.as_bytes();
    let mut depth = 0i32;
    let mut found_at: Option<usize> = None;
    scan(entry, |i, b, in_code| {
        if found_at.is_some() || !in_code {
            return;
        }
        match b {
            b'{' | b'(' => depth += 1,
            b'}' | b')' => depth -= 1,
            _ => {
                // depth 1 = directly inside the entry's braces
                if depth == 1 && text_has_key_at(entry, bytes, i, key) {
                    found_at = Some(i + key.len());
                }
            }
        }
    });
    let after = found_at?;
    let rest = entry[after..].trim_start();
    let value = if let Some(stripped) = rest.strip_prefix('"') {
        stripped.split('"').next()?.to_string()
    } else {
        rest.split([',', '}', '\n'])
            .next()?
            .split_whitespace()
            .next()?
            .to_string()
    };
    Some(value)
}

fn text_has_key_at(text: &str, bytes: &[u8], i: usize, key: &str) -> bool {
    let boundary_before = i == 0
        || bytes[i - 1].is_ascii_whitespace()
        || bytes[i - 1] == b'{'
        || bytes[i - 1] == b',';
    if !boundary_before || !text[i..].starts_with(key) {
        return false;
    }
    bytes
        .get(i + key.len())
        .map(|c| c.is_ascii_whitespace())
        .unwrap_or(false)
}

/// Replace a top-level list block's entries with `kept` (verbatim entry texts).
fn filter_list_block(text: &str, key: &str, keep: impl Fn(&str) -> bool) -> (String, usize) {
    let Some((block_start, inner_start, inner_end, block_end)) = find_list_block(text, key) else {
        return (text.to_string(), 0);
    };
    let inner = &text[inner_start..inner_end];
    let kept: Vec<&str> = split_entries(inner)
        .into_iter()
        .map(|(s, e)| &inner[s..e])
        .filter(|entry| keep(entry))
        .collect();
    let n = kept.len();
    let mut rebuilt = String::new();
    rebuilt.push_str(&text[..block_start]);
    rebuilt.push_str(key);
    rebuilt.push_str(" (\n");
    for entry in kept {
        rebuilt.push_str("    ");
        rebuilt.push_str(entry);
        rebuilt.push('\n');
    }
    rebuilt.push(')');
    rebuilt.push_str(&text[block_end..]);
    (rebuilt, n)
}

/// Drop top-level scalar lines (`name …` / `version …` / `vendor …`) from the
/// config body, capturing the removed values.
fn strip_top_level_scalars(text: &str) -> (String, BTreeMap<String, String>) {
    // Compute paren/brace depth at the start of each line.
    let mut depth_at_line = Vec::new();
    let mut depth = 0i32;
    depth_at_line.push(0);
    scan(text, |_i, b, in_code| {
        if in_code {
            match b {
                b'(' | b'{' => depth += 1,
                b')' | b'}' => depth -= 1,
                _ => {}
            }
        }
        if b == b'\n' {
            depth_at_line.push(depth);
        }
    });

    let mut captured = BTreeMap::new();
    let mut out = String::new();
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let key = trimmed.split_whitespace().next().unwrap_or("");
        if depth_at_line.get(idx).copied().unwrap_or(0) == 0
            && matches!(key, "name" | "version" | "vendor")
        {
            let value = trimmed[key.len()..].trim().trim_matches('"').to_string();
            captured.insert(key.to_string(), value);
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    (out, captured)
}

/// Ensure a mic entry carries `default true` (the pack's only mic must be the
/// default or `from_pack` resolves a mic with zero samples → silence).
fn force_mic_default(entry: &str) -> String {
    if entry.contains("default true") {
        return entry.to_string();
    }
    let mut e = entry.trim_end().to_string();
    // Drop an explicit `default false` if present, then splice before `}`.
    let e2 = e.replace(", default false", "").replace("default false", "");
    e = e2;
    match e.rfind('}') {
        Some(pos) => {
            let mut out = e[..pos].trim_end().to_string();
            out.push_str(", default true}");
            out
        }
        None => e,
    }
}

// ── Groups config ─────────────────────────────────────────────────────────────

struct Group {
    name: String,
    articulations: Vec<String>,
}

fn parse_groups(text: &str) -> Vec<Group> {
    let (_, inner_start, inner_end, _) =
        find_list_block(text, "groups").expect("groups.styx: no `groups (…)` block");
    let inner = &text[inner_start..inner_end];
    split_entries(inner)
        .into_iter()
        .map(|(s, e)| {
            let entry = &inner[s..e];
            let name = entry_field(entry, "name")
                .unwrap_or_else(|| panic!("group entry missing name: {entry}"));
            // Strip the entry's own braces — find_list_block scans at depth 0.
            let body = entry
                .strip_prefix('{')
                .and_then(|e| e.strip_suffix('}'))
                .unwrap_or(entry);
            let (_, a_inner_s, a_inner_e, _) = find_list_block(body, "articulations")
                .unwrap_or_else(|| panic!("group {name}: no articulations list"));
            let articulations = body[a_inner_s..a_inner_e]
                .split_whitespace()
                .map(str::to_string)
                .collect();
            Group {
                name,
                articulations,
            }
        })
        .collect()
}

// ── Zone extraction from a per-section spec ───────────────────────────────────

struct ZoneRef<'t> {
    text: &'t str,
    articulation: String,
    mic: String,
    file: String,
}

fn parse_zones(spec_text: &str) -> Vec<ZoneRef<'_>> {
    let (_, inner_start, inner_end, _) =
        find_list_block(spec_text, "zones").expect("section spec: no `zones (…)` block");
    let inner = &spec_text[inner_start..inner_end];
    split_entries(inner)
        .into_iter()
        .map(|(s, e)| {
            let text = &inner[s..e];
            ZoneRef {
                text,
                articulation: entry_field(text, "articulation")
                    .unwrap_or_else(|| panic!("zone missing articulation: {text}")),
                mic: entry_field(text, "mic")
                    .unwrap_or_else(|| panic!("zone missing mic: {text}")),
                file: entry_field(text, "file")
                    .unwrap_or_else(|| panic!("zone missing file: {text}")),
            }
        })
        .collect()
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut positional = Vec::new();
    let mut filter_sections: Option<BTreeSet<String>> = None;
    let mut filter_mics: Option<BTreeSet<String>> = None;
    let mut filter_groups: Option<BTreeSet<String>> = None;
    let mut variant = "both".to_string();
    let mut quality = 0.8f32;
    let mut dry_run = false;
    let mut force = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let csv = |v: String| Some(v.split(',').map(|s| s.trim().to_string()).collect());
        match arg.as_str() {
            "--sections" => filter_sections = csv(args.next().expect("--sections value")),
            "--mics" => filter_mics = csv(args.next().expect("--mics value")),
            "--groups" => filter_groups = csv(args.next().expect("--groups value")),
            "--variant" => variant = args.next().expect("--variant value"),
            "--quality" => quality = args.next().expect("--quality value").parse()?,
            "--dry-run" => dry_run = true,
            "--force" => force = true,
            _ => positional.push(arg),
        }
    }
    let [lib_root, config_path, groups_path, out_root]: [String; 4] = positional
        .try_into()
        .map_err(|_| "usage: build_cs_packs <lib_root> <engine-config.styx> <groups.styx> <out_root> [--sections …] [--mics …] [--groups …] [--variant both|lossless|proxy] [--quality 0.8] [--dry-run] [--force]")?;
    let lib_root = PathBuf::from(lib_root);
    let out_root = PathBuf::from(out_root);
    assert!(
        matches!(variant.as_str(), "both" | "lossless" | "proxy"),
        "--variant must be both|lossless|proxy"
    );

    let config_text = std::fs::read_to_string(&config_path)?;
    let groups = parse_groups(&std::fs::read_to_string(&groups_path)?);
    let artic_to_group: BTreeMap<&str, &str> = groups
        .iter()
        .flat_map(|g| g.articulations.iter().map(|a| (a.as_str(), g.name.as_str())))
        .collect();

    let (config_body, config_meta) = strip_top_level_scalars(&config_text);
    let vendor = config_meta.get("vendor").cloned().unwrap_or_default();
    let version = config_meta.get("version").cloned().unwrap_or_default();

    // Discover sections from _patches/.
    let patches = lib_root.join("_patches");
    let mut sections: Vec<String> = std::fs::read_dir(&patches)?
        .flatten()
        .filter(|e| e.path().join("library.styx").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    sections.sort();
    if let Some(f) = &filter_sections {
        sections.retain(|s| f.contains(s));
    }
    assert!(!sections.is_empty(), "no sections to build");

    let mut planned = 0usize;
    let mut built = 0usize;
    let started = std::time::Instant::now();

    for section in &sections {
        let spec_text = std::fs::read_to_string(patches.join(section).join("library.styx"))?;
        let zones = parse_zones(&spec_text);

        // Every articulation must be claimed by a group — no silent drops.
        let unmapped: BTreeSet<&str> = zones
            .iter()
            .map(|z| z.articulation.as_str())
            .filter(|a| !artic_to_group.contains_key(a))
            .collect();
        assert!(
            unmapped.is_empty(),
            "{section}: articulations not claimed by any group: {unmapped:?} — add them to the groups styx"
        );

        // (group, mic) → zones
        let mut buckets: BTreeMap<(String, String), Vec<&ZoneRef>> = BTreeMap::new();
        for z in &zones {
            let group = artic_to_group[z.articulation.as_str()].to_string();
            buckets.entry((group, z.mic.clone())).or_default().push(z);
        }

        for ((group, mic), bucket) in &buckets {
            if let Some(f) = &filter_groups {
                if !f.contains(group) {
                    continue;
                }
            }
            if let Some(f) = &filter_mics {
                if !f.contains(mic) {
                    continue;
                }
            }
            planned += 1;

            let member_ids: BTreeSet<&str> = groups
                .iter()
                .find(|g| g.name == *group)
                .unwrap()
                .articulations
                .iter()
                .map(String::as_str)
                .collect();

            // Compose the pack spec: filtered engine config + verbatim zones.
            let pack_name = format!("{section} - {group} - {mic}");
            let (body, n_sections) = filter_list_block(&config_body, "sections", |e| {
                entry_field(e, "label").as_deref() == Some(section.as_str())
            });
            assert!(n_sections == 1, "{pack_name}: section {section:?} not found in engine config");
            let (body, n_mics) = {
                let (b, n) = filter_list_block(&body, "mics", |e| {
                    entry_field(e, "id").as_deref() == Some(mic.as_str())
                });
                // Force the surviving mic to be the default (the pack's only
                // mic must be default or resolution lands on a missing mic).
                let (bs, is, ie, be) = find_list_block(&b, "mics").unwrap();
                let inner = &b[is..ie];
                let rebuilt: String = split_entries(inner)
                    .into_iter()
                    .map(|(s, e)| format!("    {}\n", force_mic_default(&inner[s..e])))
                    .collect();
                (format!("{}mics (\n{rebuilt})\n{}", &b[..bs], &b[be..]), n)
            };
            assert!(n_mics == 1, "{pack_name}: mic {mic:?} not found in engine config");
            let (body, n_artics) = filter_list_block(&body, "articulations", |e| {
                entry_field(e, "id")
                    .map(|id| member_ids.contains(id.as_str()))
                    .unwrap_or(false)
            });
            assert!(n_artics > 0, "{pack_name}: no engine-config articulations matched {member_ids:?}");

            let mut pack_spec = format!(
                "name \"{pack_name}\"\nvendor \"{vendor}\"\nversion \"{version}\"\n\n{body}\nzones (\n"
            );
            for z in bucket {
                pack_spec.push_str("    ");
                pack_spec.push_str(z.text);
                pack_spec.push('\n');
            }
            pack_spec.push_str(")\n");

            let sample_paths: Vec<PathBuf> = bucket
                .iter()
                .map(|z| lib_root.join(&z.file))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();

            let dir = out_root.join(section).join(group);
            let mut jobs: Vec<(PathBuf, PackCodec)> = Vec::new();
            if variant != "proxy" {
                jobs.push((dir.join(format!("{pack_name}.signalpack")), PackCodec::FlacI24));
            }
            if variant != "lossless" {
                jobs.push((
                    dir.join(format!("{pack_name}.proxy.signalpack")),
                    PackCodec::OggVorbis { quality },
                ));
            }

            for (out_path, codec) in jobs {
                if out_path.exists() && !force {
                    eprintln!("skip (exists): {}", out_path.display());
                    continue;
                }
                eprintln!(
                    "[{planned}] {pack_name} [{codec:?}] — {} zones, {} files",
                    bucket.len(),
                    sample_paths.len()
                );
                if dry_run {
                    continue;
                }
                let stats = create_signal_pack_with(
                    &out_path,
                    PackSpecSource::Text {
                        text: &pack_spec,
                        format: "styx",
                    },
                    &lib_root,
                    sample_paths.iter().map(PathBuf::as_path),
                    codec,
                )?;
                let size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
                eprintln!(
                    "  -> {} ({:.1} MB, {} packed, {} failed)",
                    out_path.display(),
                    size as f64 / 1e6,
                    stats.prepared,
                    stats.failed
                );
                if stats.prepared == 0 {
                    return Err(format!("{}: nothing packed", out_path.display()).into());
                }
                built += 1;
            }
        }
    }

    eprintln!(
        "build_cs_packs: {built} pack(s) built ({planned} bucket(s) planned) in {:?}",
        started.elapsed()
    );
    Ok(())
}
