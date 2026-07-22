//! `fts signal pack …` — the official signalpack inspection/mutation CLI.
//!
//! Mounted by fts-cli (`fts signal pack <cmd>`); also runnable standalone in
//! tests via [`cli_main`]. Agent-friendly: `inspect --json` for machine
//! output, `check` exit codes (0 = PASS), and all spec mutation is text-level
//! via [`crate::styx_edit`] — embedded specs are never re-serialized through
//! facet (the known silent-pack killer).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use eyre::{Context, Result, bail};

use crate::engine::cache::{
    PackCodec, PackSpecSource, SampleCache, SignalPcmPack, create_signal_pack_with,
    extract_signal_pack, load_sample, transcode_signal_pack,
};
use crate::pack_rewrite::{read_embedded_spec, rewrite_embedded_spec};
use crate::spec::LibrarySpec;
use crate::styx_edit;

#[derive(Parser)]
#[command(name = "pack", about = "Inspect, edit, and build .signalpack files")]
struct PackCli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Header, spec summary, and zone stats for a pack.
    Inspect {
        pack: PathBuf,
        /// Machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Read or replace the embedded library spec.
    Spec {
        #[command(subcommand)]
        command: SpecCmd,
    },
    /// Targeted edits to zone fields inside the embedded spec (or a loose .styx).
    Zones {
        #[command(subcommand)]
        command: ZonesCmd,
    },
    /// Loop-point tools.
    Loops {
        #[command(subcommand)]
        command: LoopsCmd,
    },
    /// Validate a pack: zone coverage, entry presence, codec decode probes.
    /// Exit 0 = PASS, non-zero = PARTIAL/FAIL.
    Check {
        pack: PathBuf,
        /// Extraction root — also A/B the decode against source files
        /// (exact length + correlation).
        #[arg(long)]
        src_root: Option<PathBuf>,
    },
    /// Decode every entry back to WAVs.
    Extract { pack: PathBuf, out_dir: PathBuf },
    /// Build a pack from a flat samples dir (embeds `<dir>/library.styx`).
    Build {
        samples_root: PathBuf,
        out: PathBuf,
        #[arg(long, default_value = "flac")]
        codec: String,
        /// Vorbis base quality (-0.2..=1.0; 0.6 ≈ oggenc q6).
        #[arg(long, default_value_t = 0.6)]
        quality: f32,
    },
    /// Re-encode a pack's audio with a different codec (Full ⇄ Proxy) —
    /// no source samples needed; spec + frame metadata copied verbatim.
    Transcode {
        input: PathBuf,
        out: PathBuf,
        #[arg(long)]
        codec: String,
        #[arg(long, default_value_t = 0.6)]
        quality: f32,
    },
}

#[derive(Subcommand)]
enum SpecCmd {
    /// Print the embedded spec text to stdout.
    Get { pack: PathBuf },
    /// Replace the embedded spec with the given file (atomic rewrite;
    /// audio body untouched). Validates the new spec parses first.
    Set {
        pack: PathBuf,
        spec: PathBuf,
        /// Skip the parse-validation of the new spec.
        #[arg(long)]
        no_validate: bool,
    },
}

#[derive(Subcommand)]
enum ZonesCmd {
    /// Set scalar fields on matching zones: `--set loop_xfade=4800 …`.
    Set {
        /// A .signalpack (embedded spec is rewritten) or a loose .styx
        /// (rewritten in place, .bak sibling kept).
        target: PathBuf,
        /// field=value pairs; allowed fields: loop_start loop_end loop_xfade
        /// gain_db tune_cents vel_min vel_max key_min key_max root_key
        /// sample_start sample_end
        #[arg(long = "set", required = true)]
        sets: Vec<String>,
        /// Only zones whose `file` contains this substring.
        #[arg(long)]
        file: Option<String>,
        /// Only zones with this exact articulation id.
        #[arg(long)]
        articulation: Option<String>,
        /// Only zones with this exact mic id.
        #[arg(long)]
        mic: Option<String>,
        /// Report matches without writing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum LoopsCmd {
    /// Inject NKI loop points (from an `nki --zones` zones.tsv) into a pack's
    /// embedded spec or a loose library.styx — the Cinematic Studio flow.
    Inject {
        /// A .signalpack or a loose library.styx.
        target: PathBuf,
        /// zones.tsv from the `nki` decoder.
        #[arg(long)]
        from_tsv: PathBuf,
        /// Section label used in extracted WAV paths (e.g. "1st Violins").
        #[arg(long)]
        section: String,
        /// Compare against loops already present instead of writing.
        #[arg(long)]
        check: bool,
        /// Report what would be injected without writing.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Entry point for fts-cli mounting (and tests). `argv` should NOT include
/// the program name; pass the args after `fts signal pack`.
pub fn cli_main(argv: impl IntoIterator<Item = OsString>) -> Result<()> {
    let argv = std::iter::once(OsString::from("pack")).chain(argv);
    match PackCli::parse_from(argv).command {
        Cmd::Inspect { pack, json } => inspect(&pack, json),
        Cmd::Spec {
            command: SpecCmd::Get { pack },
        } => {
            print!("{}", read_embedded_spec(&pack)?);
            Ok(())
        }
        Cmd::Spec {
            command:
                SpecCmd::Set {
                    pack,
                    spec,
                    no_validate,
                },
        } => spec_set(&pack, &spec, no_validate),
        Cmd::Zones {
            command:
                ZonesCmd::Set {
                    target,
                    sets,
                    file,
                    articulation,
                    mic,
                    dry_run,
                },
        } => zones_set(&target, &sets, file, articulation, mic, dry_run),
        Cmd::Loops {
            command:
                LoopsCmd::Inject {
                    target,
                    from_tsv,
                    section,
                    check,
                    dry_run,
                },
        } => loops_inject(&target, &from_tsv, &section, check, dry_run),
        Cmd::Check { pack, src_root } => {
            if run_check(&pack, src_root.as_deref())? {
                Ok(())
            } else {
                bail!("check: PARTIAL")
            }
        }
        Cmd::Extract { pack, out_dir } => {
            let stats = extract_signal_pack(&pack, &out_dir)?;
            println!("extracted {} sample(s) ({} failed)", stats.prepared, stats.failed);
            Ok(())
        }
        Cmd::Build {
            samples_root,
            out,
            codec,
            quality,
        } => build(&samples_root, &out, &codec, quality),
        Cmd::Transcode {
            input,
            out,
            codec,
            quality,
        } => {
            let codec = parse_codec(&codec, quality)?;
            let stats = transcode_signal_pack(&input, &out, codec)?;
            if stats.failed > 0 {
                bail!("transcode: {} entr(ies) failed", stats.failed);
            }
            let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
            println!(
                "transcoded {} entr(ies) -> {} ({:.1} MB)",
                stats.prepared,
                out.display(),
                size as f64 / 1e6
            );
            Ok(())
        }
    }
}

fn parse_codec(name: &str, quality: f32) -> Result<PackCodec> {
    match name {
        "flac" => Ok(PackCodec::FlacI24),
        "ogg" | "vorbis" | "ogg-vorbis" => Ok(PackCodec::OggVorbis { quality }),
        other => bail!("unknown codec {other:?} (flac | ogg)"),
    }
}

// ── inspect ───────────────────────────────────────────────────────────────────

fn inspect(pack_path: &Path, json: bool) -> Result<()> {
    let pack = SignalPcmPack::open(pack_path)?;
    let spec = crate::pack::read_pack_header(pack_path)?.spec;
    let looped = spec.zones.iter().filter(|z| z.loop_end > z.loop_start).count();

    // Per-articulation zone stats.
    let mut artics: BTreeMap<&str, (usize, u8, u8)> = BTreeMap::new();
    for z in &spec.zones {
        let e = artics.entry(z.articulation.as_str()).or_insert((0, 127, 0));
        e.0 += 1;
        e.1 = e.1.min(z.key_min);
        e.2 = e.2.max(z.key_max);
    }
    let size = std::fs::metadata(pack_path).map(|m| m.len()).unwrap_or(0);

    if json {
        let artics_json: Vec<serde_json::Value> = artics
            .iter()
            .map(|(id, (n, lo, hi))| {
                serde_json::json!({ "id": id, "zones": n, "key_min": lo, "key_max": hi })
            })
            .collect();
        let v = serde_json::json!({
            "path": pack_path,
            "kind": pack.kind_label(),
            "size_bytes": size,
            "entries": pack.entry_count(),
            "name": spec.name,
            "vendor": spec.vendor,
            "version": spec.version,
            "sections": spec.sections.iter().map(|s| s.label.clone()).collect::<Vec<_>>(),
            "mics": spec.mics.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
            "zones": spec.zones.len(),
            "looped_zones": looped,
            "articulations": artics_json,
        });
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        println!("pack     {}", pack_path.display());
        println!("kind     {}  ({:.1} MB)", pack.kind_label(), size as f64 / 1e6);
        println!("name     {}  [{} · {}]", spec.name, spec.vendor, spec.version);
        println!(
            "entries  {}   zones {}   looped {}",
            pack.entry_count(),
            spec.zones.len(),
            looped
        );
        println!(
            "sections {:?}   mics {:?}",
            spec.sections.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
            spec.mics.iter().map(|m| m.id.as_str()).collect::<Vec<_>>()
        );
        for (id, (n, lo, hi)) in &artics {
            println!("  artic {id:<16} {n:>6} zones  keys {lo}..={hi}");
        }
    }
    Ok(())
}

// ── spec set ─────────────────────────────────────────────────────────────────

fn spec_set(pack: &Path, spec_file: &Path, no_validate: bool) -> Result<()> {
    let new_spec = std::fs::read_to_string(spec_file)?;
    if !no_validate {
        LibrarySpec::from_styx(&new_spec)
            .map_err(|e| eyre::eyre!("new spec does not parse: {e}"))?;
    }
    rewrite_embedded_spec(pack, |_| new_spec.clone())?;
    println!("spec replaced in {}", pack.display());
    Ok(())
}

// ── spec text access for pack-or-styx targets ────────────────────────────────

enum Target {
    Pack(PathBuf),
    Styx(PathBuf),
}

impl Target {
    fn open(path: &Path) -> Result<(Self, String)> {
        if path.extension().and_then(|e| e.to_str()) == Some("styx") {
            Ok((Target::Styx(path.to_owned()), std::fs::read_to_string(path)?))
        } else {
            Ok((Target::Pack(path.to_owned()), read_embedded_spec(path)?))
        }
    }

    fn write(&self, new_spec: &str) -> Result<()> {
        // Guard: the edited spec must still parse before it lands anywhere.
        LibrarySpec::from_styx(new_spec)
            .map_err(|e| eyre::eyre!("edited spec does not parse (refusing to write): {e}"))?;
        match self {
            Target::Pack(p) => {
                rewrite_embedded_spec(p, |_| new_spec.to_string())?;
                println!("wrote embedded spec in {}", p.display());
            }
            Target::Styx(p) => {
                let bak = p.with_extension("styx.bak");
                std::fs::copy(p, &bak)?;
                std::fs::write(p, new_spec)?;
                println!("wrote {} (backup at {})", p.display(), bak.display());
            }
        }
        Ok(())
    }
}

/// Rewrite each zone block via `edit` (returning `Some(new_block)` to change
/// it). Returns (new_spec_text, matched_count).
fn edit_zones(
    spec_text: &str,
    mut edit: impl FnMut(&str) -> Option<String>,
) -> Result<(String, usize)> {
    let (_, is, ie, _) = styx_edit::find_list_block(spec_text, "zones")
        .ok_or_else(|| eyre::eyre!("spec has no `zones (…)` block"))?;
    let inner = &spec_text[is..ie];
    let mut new_inner = String::with_capacity(inner.len());
    let mut cursor = 0usize;
    let mut changed = 0usize;
    for (s, e) in styx_edit::split_entries(inner) {
        new_inner.push_str(&inner[cursor..s]);
        let block = &inner[s..e];
        match edit(block) {
            Some(new_block) => {
                new_inner.push_str(&new_block);
                changed += 1;
            }
            None => new_inner.push_str(block),
        }
        cursor = e;
    }
    new_inner.push_str(&inner[cursor..]);
    let mut out = String::with_capacity(spec_text.len());
    out.push_str(&spec_text[..is]);
    out.push_str(&new_inner);
    out.push_str(&spec_text[ie..]);
    Ok((out, changed))
}

// ── zones set ────────────────────────────────────────────────────────────────

const SETTABLE: &[&str] = &[
    "loop_start",
    "loop_end",
    "loop_xfade",
    "gain_db",
    "tune_cents",
    "vel_min",
    "vel_max",
    "key_min",
    "key_max",
    "root_key",
    "sample_start",
    "sample_end",
];

fn zones_set(
    target: &Path,
    sets: &[String],
    file: Option<String>,
    articulation: Option<String>,
    mic: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let pairs: Vec<(String, String)> = sets
        .iter()
        .map(|s| {
            let (k, v) = s
                .split_once('=')
                .ok_or_else(|| eyre::eyre!("--set expects field=value, got {s:?}"))?;
            if !SETTABLE.contains(&k) {
                bail!("field {k:?} is not settable (allowed: {SETTABLE:?})");
            }
            // Values must be numeric — these are all scalar numeric fields.
            v.parse::<f64>()
                .with_context(|| format!("--set {k}: value {v:?} is not numeric"))?;
            Ok((k.to_string(), v.to_string()))
        })
        .collect::<Result<_>>()?;

    let (tgt, spec_text) = Target::open(target)?;
    let mut matched = 0usize;
    let (new_spec, changed) = edit_zones(&spec_text, |block| {
        if let Some(f) = &file {
            if !styx_edit::entry_field(block, "file").is_some_and(|v| v.contains(f.as_str())) {
                return None;
            }
        }
        if let Some(a) = &articulation {
            if styx_edit::entry_field(block, "articulation").as_deref() != Some(a.as_str()) {
                return None;
            }
        }
        if let Some(m) = &mic {
            if styx_edit::entry_field(block, "mic").as_deref() != Some(m.as_str()) {
                return None;
            }
        }
        matched += 1;
        let mut b = block.to_string();
        for (k, v) in &pairs {
            b = styx_edit::set_entry_field(&b, k, v);
        }
        Some(b)
    })?;

    println!("matched {matched} zone(s), edited {changed}");
    if matched == 0 {
        bail!("no zones matched the filters");
    }
    if dry_run {
        println!("(dry run — nothing written)");
        return Ok(());
    }
    tgt.write(&new_spec)
}

// ── loops inject (Cinematic Studio NKI flow) ─────────────────────────────────

/// nki-styx's CSS articulation → category-folder taxonomy (+ the "Pizz"
/// token the non-1st-Violins sections use).
fn cs_category(artic: &str) -> Option<&'static str> {
    Some(match artic {
        "Nonvib" | "Vibsus" => "Sustain",
        "Leg" | "Legzero" | "NVLeg" | "NVLegzero" => "Legato",
        "Marcato" | "Sfz" | "Spiccato" | "Staccatissimo" | "Staccato" => "Short",
        "Bartokpizz" | "Pizzicato" | "Pizz" => "Pizzicato",
        "Clegno" | "Harm" | "HTrills" | "Port" | "Tremolo" | "WTrills" => "Special",
        "Harmrel" | "HTrel" | "Marcrel" | "NVrel" | "Tremrel" | "Vsusrel" | "WTrel" => "Releases",
        _ => return None,
    })
}

/// wav-relative-path → (loop_start, loop_end) for looped NKI zones.
/// First-wins on per-file loop variants (matches nki-styx accumulation).
fn load_loop_map(tsv: &Path, section_label: &str) -> Result<BTreeMap<String, (u64, u64)>> {
    let text = std::fs::read_to_string(tsv)?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .ok_or_else(|| eyre::eyre!("empty zones.tsv"))?
        .split('\t')
        .collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .ok_or_else(|| eyre::eyre!("zones.tsv missing column {name:?}"))
    };
    let (c_ls, c_le, c_sample) = (col("loop_start")?, col("loop_end")?, col("sample")?);

    let mut loops = BTreeMap::new();
    let mut conflicts = 0usize;
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        let (Some(ls), Some(le)) = (
            f.get(c_ls).and_then(|v| v.parse::<u64>().ok()),
            f.get(c_le).and_then(|v| v.parse::<u64>().ok()),
        ) else {
            continue;
        };
        if le <= ls || le == 0 {
            continue;
        }
        let Some(sample) = f.get(c_sample) else { continue };
        let base = sample.rsplit('/').next().unwrap_or(sample);
        let stem = base
            .strip_suffix(".ncw")
            .or_else(|| base.strip_suffix(".wav"))
            .unwrap_or(base);
        let parts: Vec<&str> = stem.split('_').collect();
        if parts.len() < 4 {
            continue;
        }
        let (artic, mic) = (parts[1], parts[2]);
        let Some(cat) = cs_category(artic) else {
            eprintln!("  WARNING: looped zone with unmapped articulation {artic:?}: {base}");
            continue;
        };
        let rest = parts[3..].join("_");
        let wav = format!("{mic}/{section_label}/{cat}/{artic}/{rest}.wav");
        if loops.contains_key(&wav) {
            if loops[&wav] != (ls, le) {
                conflicts += 1;
            }
            continue; // first-wins
        }
        loops.insert(wav, (ls, le));
    }
    if conflicts > 0 {
        eprintln!("  NOTE: {conflicts} file(s) had differing loops across zones (first wins)");
    }
    Ok(loops)
}

fn loops_inject(
    target: &Path,
    from_tsv: &Path,
    section: &str,
    check: bool,
    dry_run: bool,
) -> Result<()> {
    let loops = load_loop_map(from_tsv, section)?;
    println!("{section}: {} looped file(s) mapped from {}", loops.len(), from_tsv.display());

    let (tgt, spec_text) = Target::open(target)?;
    let mut unmatched = loops.clone();
    let (mut injected, mut already, mut matches, mut mismatches) = (0usize, 0usize, 0usize, 0usize);
    let (new_spec, _) = edit_zones(&spec_text, |block| {
        let file = styx_edit::entry_field(block, "file")?;
        let &(ls, le) = loops.get(&file)?;
        unmatched.remove(&file);
        let existing_ls = styx_edit::entry_field(block, "loop_start")
            .and_then(|v| v.parse::<u64>().ok());
        if let Some(els) = existing_ls {
            already += 1;
            if check {
                let ele = styx_edit::entry_field(block, "loop_end")
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
                if (els, ele) == (ls, le) {
                    matches += 1;
                } else {
                    mismatches += 1;
                    println!("  MISMATCH {file}: spec ({els}, {ele}) vs nki ({ls}, {le})");
                }
            }
            return None;
        }
        injected += 1;
        let b = styx_edit::set_entry_field(block, "loop_start", &ls.to_string());
        Some(styx_edit::set_entry_field(&b, "loop_end", &le.to_string()))
    })?;

    print!("  injected={injected} already={already}");
    if check {
        print!(" match={matches} mismatch={mismatches}");
    }
    println!(" loops-without-spec-zone={}", unmatched.len());
    for wav in unmatched.keys().take(5) {
        println!("    e.g. no spec zone for looped {wav}");
    }

    if check {
        if mismatches > 0 {
            bail!("loop check: {mismatches} mismatch(es)");
        }
        return Ok(());
    }
    if dry_run || injected == 0 {
        println!("(nothing written)");
        return Ok(());
    }
    tgt.write(&new_spec)
}

// ── check ────────────────────────────────────────────────────────────────────

/// Zone-mode pack validation: per-articulation key coverage (vs the spec's
/// pitch tolerance), zone-file → pack-entry presence, decode probes (with
/// optional A/B correlation against source files). Returns Ok(true) = PASS.
pub fn run_check(pack_path: &Path, src_root: Option<&Path>) -> Result<bool> {
    let patch = crate::PlayerPatch::from_pack(pack_path)?;
    let spec = &patch.spec;
    if spec.zones.is_empty() {
        bail!(
            "{}: not a zone-mode pack — use the check_pack_resolve example for \
             convention-mode packs",
            pack_path.display()
        );
    }
    let pack = patch
        .pack
        .as_ref()
        .ok_or_else(|| eyre::eyre!("patch has no backing pack"))?;
    println!(
        "pack: {}  ({}, {} zones, {} entries)",
        spec.name,
        pack.kind_label(),
        spec.zones.len(),
        pack.entry_count()
    );

    // Per-articulation key coverage within the engine's pitch tolerance.
    let mut by_artic: BTreeMap<&str, std::collections::BTreeSet<u8>> = BTreeMap::new();
    for z in &spec.zones {
        let keys = by_artic.entry(z.articulation.as_str()).or_default();
        for k in z.key_min..=z.key_max {
            keys.insert(k);
        }
    }
    let tol = spec.performance.zone_pitch_tolerance.max(1) as u8;
    let mut gaps = 0usize;
    for (artic, keys) in &by_artic {
        let (lo, hi) = (*keys.first().unwrap(), *keys.last().unwrap());
        let mut missing: Vec<u8> = Vec::new();
        let mut last = lo;
        for k in keys.iter().copied() {
            if k > last && k - last > tol + 1 {
                missing.extend(last + 1..k);
            }
            last = k;
        }
        if missing.is_empty() {
            println!("  artic {artic:<16} keys {lo}..={hi} covered ({} keys)", keys.len());
        } else {
            gaps += 1;
            println!("  artic {artic:<16} keys {lo}..={hi} — GAPS beyond tolerance: {missing:?}");
        }
    }

    // Every zone file must have a pack entry.
    let entries: std::collections::BTreeSet<&Path> = pack.entry_paths().collect();
    let missing_files = spec
        .zones
        .iter()
        .map(|z| z.file.as_str())
        .filter(|f| !entries.contains(Path::new(f)))
        .collect::<Vec<_>>();
    if !missing_files.is_empty() {
        println!(
            "  {} zone file(s) with NO pack entry, e.g. {:?}",
            missing_files.len(),
            &missing_files[..missing_files.len().min(5)]
        );
    }

    // Decode probes (first/middle/last zone), optional source A/B.
    let cache = SampleCache::with_pack(pack.clone());
    let mut decode_fail = 0usize;
    for i in [0usize, spec.zones.len() / 2, spec.zones.len() - 1] {
        let z = &spec.zones[i];
        match cache.get(Path::new(&z.file)) {
            Ok(data) => {
                print!(
                    "  decode {}: {} ch, {} Hz, {} frames ok",
                    z.file, data.channels, data.sample_rate, data.num_frames
                );
                if let Some(root) = src_root {
                    match load_sample(&root.join(&z.file)) {
                        Ok(src) if src.num_frames == data.num_frames => {
                            let corr = correlation(&data.frames, &src.frames);
                            print!("  corr={corr:.5}");
                            if corr < 0.95 {
                                decode_fail += 1;
                                print!("  LOW");
                            }
                        }
                        Ok(src) => {
                            decode_fail += 1;
                            print!(
                                "  LENGTH MISMATCH vs source ({} != {})",
                                data.num_frames, src.num_frames
                            );
                        }
                        Err(e) => print!("  (source unreadable: {e})"),
                    }
                }
                println!();
            }
            Err(e) => {
                decode_fail += 1;
                println!("  decode {}: FAILED — {e}", z.file);
            }
        }
    }

    let looped = spec.zones.iter().filter(|z| z.loop_end > z.loop_start).count();
    println!("  looped zones: {looped}");

    let pass = gaps == 0 && missing_files.is_empty() && decode_fail == 0;
    println!("{}", if pass { "PASS" } else { "PARTIAL" });
    Ok(pass)
}

fn correlation(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| (*x as f64) * (*y as f64)).sum();
    let na: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum();
    let nb: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum();
    dot / (na.sqrt() * nb.sqrt()).max(f64::EPSILON)
}

// ── build ────────────────────────────────────────────────────────────────────

fn build(samples_root: &Path, out: &Path, codec: &str, quality: f32) -> Result<()> {
    let codec = parse_codec(codec, quality)?;
    let spec = samples_root.join("library.styx");
    if !spec.exists() {
        bail!("no library.styx in {}", samples_root.display());
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(samples_root)?
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
    let stats = create_signal_pack_with(
        out,
        PackSpecSource::Path(&spec),
        samples_root,
        paths.iter().map(PathBuf::as_path),
        codec,
    )?;
    println!("built {} — {} packed, {} failed", out.display(), stats.prepared, stats.failed);
    if stats.prepared == 0 {
        bail!("no samples packed");
    }
    Ok(())
}
