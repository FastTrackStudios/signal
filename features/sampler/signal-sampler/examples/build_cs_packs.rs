//! Split a Cinematic Studio library into per-(section, articulation-group,
//! mic) signalpacks — lossless FLAC + Ogg Vorbis proxy.
//!
//!   cargo run -p signal-sampler --release --example build_cs_packs -- \
//!       "<lib_root>" <engine-config.styx> <groups.styx> \
//!       "<full_out_root>" "<proxy_out_root>" \
//!       [--sections "1st Violins,Cellos"] [--mics Main,Mix] [--groups Legato] \
//!       [--variant both|lossless|proxy] [--quality 0.6] [--dry-run] [--force]
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
//! Output: TWO parallel trees with IDENTICAL subpaths and filenames, so the
//! whole proxy tree is a transferable drop-in replacement for the full tree
//! (variant is distinguished by tree root + pack header kind, not filename):
//!   <full_out_root>/<Section>/<Group>/<Section> - <Group> - <Mic>.signalpack   (FLAC)
//!   <proxy_out_root>/<Section>/<Group>/<Section> - <Group> - <Mic>.signalpack  (Ogg Vorbis)
//! Convention: full_out_root  = …/Signal/Libraries/Full/<Category>/<Library>
//!             proxy_out_root = …/Signal/Libraries/Proxy/<Category>/<Library>

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use signal_sampler::engine::cache::{PackCodec, PackSpecSource, create_signal_pack_with};
use signal_sampler::styx_edit::{entry_field, find_list_block, scan, split_entries};

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
    /// Prefix patterns (Pacific-style libraries whose zonemap bakes RR/vel/
    /// release-variant suffixes into articulation ids — `normrel4`,
    /// `pluckrr3_64`). An articulation matches the group with the LONGEST
    /// matching prefix; exact `articulations` ids always win over prefixes.
    prefixes: Vec<String>,
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
            let list = |key: &str| -> Vec<String> {
                find_list_block(body, key)
                    .map(|(_, s, e, _)| {
                        body[s..e]
                            .split_whitespace()
                            .map(|t| t.trim_matches('"').to_string())
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let articulations = list("articulations");
            let prefixes = list("prefixes");
            assert!(
                !articulations.is_empty() || !prefixes.is_empty(),
                "group {name}: needs an articulations or prefixes list"
            );
            Group {
                name,
                articulations,
                prefixes,
            }
        })
        .collect()
}

/// Resolve an articulation id to its group: exact id first, else the group
/// with the longest matching prefix.
fn group_for<'g>(groups: &'g [Group], exact: &BTreeMap<&str, &'g str>, artic: &str) -> Option<&'g str> {
    if let Some(g) = exact.get(artic) {
        return Some(g);
    }
    groups
        .iter()
        .flat_map(|g| g.prefixes.iter().map(move |p| (p, g.name.as_str())))
        .filter(|(p, _)| artic.starts_with(p.as_str()))
        .max_by_key(|(p, _)| p.len())
        .map(|(_, name)| name)
}

/// Synthesize a minimal articulation declaration for a zone articulation id
/// the engine config does not declare (Pacific-style suffixed ids). Kind is
/// inferred from the id's tokens; undeclared articulations would otherwise be
/// silently unplayable (the #1 "pack is silent" gotcha).
fn synth_artic_kind(id: &str) -> &'static str {
    if id.contains("rel") {
        "@Release"
    } else if id.contains("trill") {
        "@Trill"
    } else if id.starts_with("leg") || id.contains("_leg") {
        "@Legato"
    } else if id.contains("sus") || id.starts_with("harm") {
        "@Sustain"
    } else if id.starts_with("fx") {
        "@OneShot"
    } else if ["rep", "atk", "stacc", "spicc", "pizz", "pluck", "snap", "marc"]
        .iter()
        .any(|t| id.contains(t))
    {
        "@Short"
    } else {
        "@Sustain"
    }
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
    let mut quality = 0.6f32;
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
    let [lib_root, config_path, groups_path, full_out_root, proxy_out_root]: [String; 5] = positional
        .try_into()
        .map_err(|_| "usage: build_cs_packs <lib_root> <engine-config.styx> <groups.styx> <full_out_root> <proxy_out_root> [--sections …] [--mics …] [--groups …] [--variant both|lossless|proxy] [--quality 0.6] [--dry-run] [--force]")?;
    let lib_root = PathBuf::from(lib_root);
    let full_out_root = PathBuf::from(full_out_root);
    let proxy_out_root = PathBuf::from(proxy_out_root);
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

        // Every articulation must be claimed by a group (exact or prefix) —
        // no silent drops.
        let unmapped: BTreeSet<&str> = zones
            .iter()
            .map(|z| z.articulation.as_str())
            .filter(|a| group_for(&groups, &artic_to_group, a).is_none())
            .collect();
        assert!(
            unmapped.is_empty(),
            "{section}: articulations not claimed by any group: {unmapped:?} — add them to the groups styx"
        );

        // (group, mic) → zones
        let mut buckets: BTreeMap<(String, String), Vec<&ZoneRef>> = BTreeMap::new();
        for z in &zones {
            let group = group_for(&groups, &artic_to_group, &z.articulation)
                .unwrap()
                .to_string();
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

            // Zone articulation ids the (filtered) config does NOT declare —
            // synthesize minimal declarations, else those zones are silently
            // unplayable. Pacific-style suffixed ids always land here; for CS
            // libraries this is a safety net (and is reported).
            let declared: BTreeSet<String> = find_list_block(&body, "articulations")
                .map(|(_, s, e, _)| {
                    let inner = &body[s..e];
                    split_entries(inner)
                        .into_iter()
                        .filter_map(|(a, b)| entry_field(&inner[a..b], "id"))
                        .collect()
                })
                .unwrap_or_default();
            let mut undeclared: Vec<&str> = bucket
                .iter()
                .map(|z| z.articulation.as_str())
                .filter(|a| !declared.contains(*a))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            // Sustains first so the engine's default-articulation pick lands
            // on a body articulation, not a release.
            undeclared.sort_by_key(|id| (synth_artic_kind(id) != "@Sustain", *id));
            let body = if undeclared.is_empty() {
                body
            } else {
                let mut synth = String::new();
                for id in &undeclared {
                    synth.push_str(&format!(
                        "    {{ id {id:?}, label {id:?}, kind {} }}\n",
                        synth_artic_kind(id)
                    ));
                }
                match find_list_block(&body, "articulations") {
                    Some((_, _, ie, _)) => format!("{}{synth}{}", &body[..ie], &body[ie..]),
                    None => format!("{body}\narticulations (\n{synth})\n"),
                }
            };
            assert!(
                n_artics > 0 || !undeclared.is_empty(),
                "{pack_name}: no engine-config articulations matched {member_ids:?}"
            );

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

            let rel = PathBuf::from(section)
                .join(group)
                .join(format!("{pack_name}.signalpack"));
            let mut jobs: Vec<(PathBuf, PackCodec)> = Vec::new();
            if variant != "proxy" {
                jobs.push((full_out_root.join(&rel), PackCodec::FlacI24));
            }
            if variant != "lossless" {
                jobs.push((proxy_out_root.join(&rel), PackCodec::OggVorbis { quality }));
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
