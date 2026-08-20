//! Build `.signalpack`s for the NI Essential Pianos from a Kontakt extraction.
//!
//! ```text
//! cargo run -p signal-sampler --release --example build_ni_packs -- \
//!     "<sampled_root>" <ni-pianos.styx> "<full_out_root>" "<proxy_out_root>" \
//!     [--libraries "The Grandeur,The Giant"] [--packs Piano] \
//!     [--variant both|lossless|proxy] [--quality 0.8] [--dry-run] [--force] [--allow-partial]
//! ```
//!
//! Inputs, per library, under `<sampled_root>/<dir>/`:
//! - `<instrument>/zones.tsv` — the extracted zone map (key/vel ranges, root,
//!   tune, loop points, frame count) with a `sample` column pointing at the
//!   `.NCW` inside its `.nkx` monolith.
//! - `wav/<MONOLITH>/<rest>.wav` — the decoded audio, addressed by rewriting
//!   that same path. Every zone in all five instruments resolves; a miss is a
//!   hard error, because a silently dropped zone is a hole in the keyboard.
//!
//! Unlike the Cinematic Studio builder there is no hand-authored per-section
//! `library.styx` to filter — the spec is generated here from `zones.tsv`, in
//! **zone mode** (explicit `zones (…)`), because the extraction knows the real
//! root keys, tunings and loop points and no filename convention would.
//!
//! Output mirrors the CS layout — two trees, identical subpaths, so the proxy
//! tree is a drop-in replacement:
//! ```text
//! <full_out_root>/<Library>/<Library> - <Pack>.signalpack   (FLAC i24)
//! <proxy_out_root>/<Library>/<Library> - <Pack>.signalpack  (Ogg Vorbis)
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use facet::Facet;
use signal_sampler::engine::cache::{create_signal_pack_with, PackCodec, PackSpecSource};

// ── The grouping spec ────────────────────────────────────────────────────────

#[derive(Debug, Facet)]
struct NiPianoSpec {
    #[facet(default)]
    vendor: String,
    #[facet(default)]
    classify: Vec<ClassifyRule>,
    #[facet(default)]
    default_group: String,
    #[facet(default)]
    packs: Vec<PackGroup>,
    #[facet(default)]
    libraries: Vec<LibraryEntry>,
}

#[derive(Debug, Facet)]
struct ClassifyRule {
    group: String,
    #[facet(default)]
    r#match: Vec<String>,
}

#[derive(Debug, Facet)]
struct PackGroup {
    name: String,
    #[facet(default)]
    groups: Vec<String>,
}

#[derive(Debug, Facet)]
struct LibraryEntry {
    dir: String,
    instrument: String,
    name: String,
}

impl NiPianoSpec {
    /// The KSP group a sample belongs to, by first-matching keyword.
    fn classify(&self, sample_path: &str) -> &str {
        let base = sample_path
            .rsplit('/')
            .next()
            .unwrap_or(sample_path)
            .to_ascii_uppercase();
        for rule in &self.classify {
            if rule
                .r#match
                .iter()
                .any(|m| base.contains(&m.to_ascii_uppercase()))
            {
                return &rule.group;
            }
        }
        &self.default_group
    }
}

// ── zones.tsv ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Zone {
    key_lo: u8,
    key_hi: u8,
    vel_lo: u8,
    vel_hi: u8,
    root: u8,
    tune_cents: f32,
    loop_start: i64,
    loop_end: i64,
    /// Path relative to the library's `wav/` root.
    wav: String,
    group: String,
}

/// Rewrite a zones.tsv `sample` path onto the decoded-WAV tree:
/// `../Samples/GRANDEUR_001.nkx/Samples/Grandeur/X.NCW` → `GRANDEUR_001/Samples/Grandeur/X.wav`.
fn wav_path_for(sample: &str) -> String {
    let p = sample.trim_start_matches("../Samples/");
    let joined = match p.split_once(".nkx/") {
        Some((monolith, rest)) => format!("{monolith}/{rest}"),
        None => p.to_string(),
    };
    match joined.rsplit_once('.') {
        Some((stem, _ext)) => format!("{stem}.wav"),
        None => joined,
    }
}

fn read_zones(tsv: &Path, spec: &NiPianoSpec) -> Result<Vec<Zone>, String> {
    let text = std::fs::read_to_string(tsv).map_err(|e| format!("read {}: {e}", tsv.display()))?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().ok_or("empty zones.tsv")?.split('\t').collect();
    let col = |name: &str| -> Result<usize, String> {
        header
            .iter()
            .position(|h| *h == name)
            .ok_or_else(|| format!("zones.tsv has no {name:?} column"))
    };
    let (c_klo, c_khi) = (col("key_lo")?, col("key_hi")?);
    let (c_vlo, c_vhi) = (col("vel_lo")?, col("vel_hi")?);
    let (c_root, c_tune) = (col("root")?, col("tune")?);
    let (c_ls, c_le) = (col("loop_start")?, col("loop_end")?);
    let c_sample = col("sample")?;

    let mut out = Vec::new();
    for (n, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        let get = |i: usize| -> Result<&str, String> {
            f.get(i)
                .copied()
                .ok_or_else(|| format!("zones.tsv line {}: short row", n + 2))
        };
        let num = |i: usize| -> Result<i64, String> {
            get(i)?
                .trim()
                .parse::<f64>()
                .map(|v| v as i64)
                .map_err(|e| format!("zones.tsv line {}: {e}", n + 2))
        };
        let sample = get(c_sample)?.trim().to_string();
        out.push(Zone {
            key_lo: num(c_klo)?.clamp(0, 127) as u8,
            key_hi: num(c_khi)?.clamp(0, 127) as u8,
            vel_lo: num(c_vlo)?.clamp(0, 127) as u8,
            vel_hi: num(c_vhi)?.clamp(0, 127) as u8,
            root: num(c_root)?.clamp(0, 127) as u8,
            // Kontakt stores tune in semitones; the spec wants cents.
            tune_cents: get(c_tune)?.trim().parse::<f32>().unwrap_or(0.0) * 100.0,
            loop_start: num(c_ls)?,
            loop_end: num(c_le)?,
            group: spec.classify(&sample).to_string(),
            wav: wav_path_for(&sample),
        });
    }
    Ok(out)
}

// ── library.styx generation ──────────────────────────────────────────────────

fn note_name(midi: u8) -> String {
    const N: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", N[midi as usize % 12], midi as i32 / 12 - 1)
}

fn styx_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Emit a zone-mode `library.styx` for one pack's worth of zones.
///
/// Written as text rather than via `facet_styx::to_string`, which is a known
/// pack-killer (see `build_cs_packs`).
fn library_styx(lib_name: &str, pack: &str, vendor: &str, zones: &[Zone]) -> String {
    let mut s = String::new();
    let lo = zones.iter().map(|z| z.key_lo).min().unwrap_or(21);
    let hi = zones.iter().map(|z| z.key_hi).max().unwrap_or(108);
    s.push_str(&format!(
        "name \"{} - {}\"\n",
        styx_escape(lib_name),
        styx_escape(pack)
    ));
    s.push_str("version \"1.0\"\n");
    s.push_str(&format!("vendor \"{}\"\n\n", styx_escape(vendor)));
    s.push_str("sections ({\n  id main\n");
    s.push_str(&format!("  label \"{}\"\n", styx_escape(lib_name)));
    s.push_str("  note_grid ()\n");
    s.push_str(&format!(
        "  lowest_note {}\n  highest_note {}\n}})\n\n",
        note_name(lo),
        note_name(hi)
    ));
    s.push_str("mics ({\n  id Main\n  label Main\n  kind blended\n  default true\n})\n\n");
    s.push_str("dynamics {\n  short_note_controller velocity\n}\n\n");

    // One articulation per KSP group present.
    //
    // Kind and ORDER both matter: `engine::default_articulation` picks the
    // first `@Sustain` articulation that isn't obviously auxiliary, so a naive
    // alphabetical emit makes "Damper" — a two-key noise group — the default
    // voice, and the pack plays damper thuds instead of a piano. The noise
    // groups are one-shots, which is both semantically right and enough to
    // keep them out of that race; DryTones is emitted first regardless.
    let present: BTreeSet<&str> = zones.iter().map(|z| z.group.as_str()).collect();
    let has_release = present.contains("Release");
    let kind_of = |g: &str| match g {
        "Release" => "@Release",
        // Struck/mechanical noises: fire once, no sustain, never the default.
        "Hammer" | "Damper" | "Pedal" | "Stringnoise" => "@OneShot",
        // DryTones, Resonance, SSR — held, velocity-switched.
        _ => "@Sustain",
    };
    let groups: Vec<&str> = std::iter::once("DryTones")
        .filter(|g| present.contains(g))
        .chain(present.iter().copied().filter(|g| *g != "DryTones"))
        .collect();
    s.push_str("articulations (");
    for (i, g) in groups.iter().enumerate() {
        let kind = kind_of(g);
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!(
            "{{\n  id {g}\n  label \"{g}\"\n  kind {kind}\n  rr 1\n  dyn_ctrl velocity\n"
        ));
        if has_release && *g == "DryTones" {
            s.push_str("  release_artic Release\n");
        }
        s.push('}');
    }
    s.push_str(")\n\nzones (\n");
    for z in zones {
        s.push_str("  {\n");
        s.push_str(&format!("    file         \"{}\"\n", styx_escape(&z.wav)));
        s.push_str(&format!("    key_min      {}\n", z.key_lo));
        s.push_str(&format!("    key_max      {}\n", z.key_hi));
        s.push_str(&format!("    root_key     {}\n", z.root));
        s.push_str(&format!("    vel_min      {}\n", z.vel_lo));
        s.push_str(&format!("    vel_max      {}\n", z.vel_hi));
        s.push_str(&format!("    articulation \"{}\"\n", z.group));
        s.push_str("    mic          \"Main\"\n");
        if z.tune_cents != 0.0 {
            s.push_str(&format!("    tune_cents   {:.3}\n", z.tune_cents));
        }
        // Kontakt writes -1 for "no loop"; the spec's sentinel is 0/0.
        if z.loop_start >= 0 && z.loop_end > z.loop_start {
            s.push_str(&format!("    loop_start   {}\n", z.loop_start));
            s.push_str(&format!("    loop_end     {}\n", z.loop_end));
        }
        s.push_str("  }\n");
    }
    s.push_str(")\n");
    s
}

// ── main ─────────────────────────────────────────────────────────────────────

struct Args {
    sampled_root: PathBuf,
    spec_path: PathBuf,
    full_root: PathBuf,
    proxy_root: PathBuf,
    libraries: Option<Vec<String>>,
    packs: Option<Vec<String>>,
    variant: String,
    quality: f32,
    dry_run: bool,
    force: bool,
    allow_partial: bool,
}

fn parse_args() -> Args {
    let mut pos: Vec<String> = Vec::new();
    let mut a = Args {
        sampled_root: PathBuf::new(),
        spec_path: PathBuf::new(),
        full_root: PathBuf::new(),
        proxy_root: PathBuf::new(),
        libraries: None,
        packs: None,
        variant: "both".into(),
        quality: 0.8,
        dry_run: false,
        force: false,
        allow_partial: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let csv = |v: String| Some(v.split(',').map(|s| s.trim().to_string()).collect());
        match arg.as_str() {
            "--libraries" => a.libraries = csv(it.next().expect("--libraries needs a value")),
            "--packs" => a.packs = csv(it.next().expect("--packs needs a value")),
            "--variant" => a.variant = it.next().expect("--variant needs a value"),
            "--quality" => a.quality = it.next().expect("--quality").parse().expect("quality"),
            "--dry-run" => a.dry_run = true,
            "--force" => a.force = true,
            "--allow-partial" => a.allow_partial = true,
            _ => pos.push(arg),
        }
    }
    assert!(
        pos.len() >= 4,
        "usage: build_ni_packs <sampled_root> <ni-pianos.styx> <full_out_root> <proxy_out_root> [flags]"
    );
    a.sampled_root = PathBuf::from(&pos[0]);
    a.spec_path = PathBuf::from(&pos[1]);
    a.full_root = PathBuf::from(&pos[2]);
    a.proxy_root = PathBuf::from(&pos[3]);
    a
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The packer reports undecodable samples through `tracing::warn!`. Without
    // a subscriber those vanish and a pack with holes in it looks like a clean
    // build, so install one at warn by default.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_target(false)
        .init();

    let args = parse_args();
    let spec_text = std::fs::read_to_string(&args.spec_path)?;
    let spec: NiPianoSpec = facet_styx::from_str(&spec_text)
        .map_err(|e| format!("{}: {e}", args.spec_path.display()))?;

    let wanted_lib = |name: &str| {
        args.libraries
            .as_ref()
            .is_none_or(|l| l.iter().any(|w| w == name))
    };
    let wanted_pack = |name: &str| {
        args.packs
            .as_ref()
            .is_none_or(|l| l.iter().any(|w| w == name))
    };

    let mut built = 0usize;
    let mut skipped = 0usize;
    for lib in spec.libraries.iter().filter(|l| wanted_lib(&l.name)) {
        let lib_dir = args.sampled_root.join(&lib.dir);
        let tsv = lib_dir.join(&lib.instrument).join("zones.tsv");
        if !tsv.exists() {
            eprintln!("{}: no zones.tsv — skipping", lib.name);
            continue;
        }
        let wav_root = lib_dir.join("wav");
        let zones = read_zones(&tsv, &spec)?;

        // Census first: this is the guard against a library whose filenames
        // break the keyword rules. A group that silently empties would be a
        // pack with a hole in it.
        let mut census: BTreeMap<&str, usize> = BTreeMap::new();
        for z in &zones {
            *census.entry(z.group.as_str()).or_default() += 1;
        }
        eprintln!(
            "\n=== {} — {} zones: {}",
            lib.name,
            zones.len(),
            census
                .iter()
                .map(|(g, n)| format!("{g} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        );

        // Every zone must resolve to a decoded WAV. A missing file is a hole
        // in the keyboard, so refuse rather than build a partial pack.
        let missing: Vec<&Zone> = zones
            .iter()
            .filter(|z| !wav_root.join(&z.wav).exists())
            .collect();
        if !missing.is_empty() {
            for z in missing.iter().take(5) {
                eprintln!("  MISSING {}", wav_root.join(&z.wav).display());
            }
            return Err(format!(
                "{}: {} of {} zones have no decoded WAV",
                lib.name,
                missing.len(),
                zones.len()
            )
            .into());
        }

        for pack in spec.packs.iter().filter(|p| wanted_pack(&p.name)) {
            let members: BTreeSet<&str> = pack.groups.iter().map(String::as_str).collect();
            let subset: Vec<Zone> = zones
                .iter()
                .filter(|z| members.contains(z.group.as_str()))
                .cloned()
                .collect();
            if subset.is_empty() {
                eprintln!("  {} / {}: no zones — skipping", lib.name, pack.name);
                continue;
            }
            // One entry per distinct file: a pack stores each sample once even
            // when several zones (velocity layers, key ranges) point at it.
            let files: BTreeSet<&str> = subset.iter().map(|z| z.wav.as_str()).collect();
            let paths: Vec<PathBuf> = files.iter().map(|f| wav_root.join(f)).collect();
            let styx = library_styx(&lib.name, &pack.name, &spec.vendor, &subset);

            let rel =
                PathBuf::from(&lib.name).join(format!("{} - {}.signalpack", lib.name, pack.name));
            eprintln!(
                "  {} / {}: {} zones, {} samples",
                lib.name,
                pack.name,
                subset.len(),
                paths.len()
            );
            if args.dry_run {
                skipped += 1;
                continue;
            }

            for (variant, root, codec) in [
                ("lossless", &args.full_root, PackCodec::FlacI24),
                (
                    "proxy",
                    &args.proxy_root,
                    PackCodec::OggVorbis {
                        quality: args.quality,
                    },
                ),
            ] {
                if args.variant != "both" && args.variant != variant {
                    continue;
                }
                let out = root.join(&rel);
                if out.exists() && !args.force {
                    eprintln!("    {variant}: exists — skipping (use --force)");
                    skipped += 1;
                    continue;
                }
                let t = std::time::Instant::now();
                let stats = create_signal_pack_with(
                    &out,
                    PackSpecSource::Text {
                        text: &styx,
                        format: "styx",
                    },
                    &wav_root,
                    paths.iter().map(PathBuf::as_path),
                    codec,
                )?;
                eprintln!(
                    "    {variant}: {stats:?} in {:?} -> {}",
                    t.elapsed(),
                    out.display()
                );
                if stats.prepared == 0 {
                    return Err(format!("{}: nothing packed", out.display()).into());
                }
                // An undecodable sample is a silent hole in the keyboard, so
                // it stops the build by default — the tracing warnings above
                // name the files. Pass --allow-partial once you have looked at
                // them and decided the loss is acceptable.
                if stats.failed > 0 && !args.allow_partial {
                    return Err(format!(
                        "{}: {} sample(s) failed to encode (see warnings above); \
                         re-run with --allow-partial to ship the pack anyway",
                        out.display(),
                        stats.failed
                    )
                    .into());
                }
                built += 1;
            }
        }
    }
    eprintln!("\nbuild_ni_packs: {built} pack(s) built, {skipped} skipped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_paths_rewrite_off_the_monolith() {
        assert_eq!(
            wav_path_for("../Samples/GRANDEUR_001.nkx/Samples/Grandeur/GI_PP_SD_A-1_421.NCW"),
            "GRANDEUR_001/Samples/Grandeur/GI_PP_SD_A-1_421.wav"
        );
        // Already-flat paths survive unchanged apart from the extension.
        assert_eq!(wav_path_for("Samples/X.ncw"), "Samples/X.wav");
    }

    #[test]
    fn note_names_match_midi_convention() {
        assert_eq!(note_name(21), "A0"); // lowest piano key
        assert_eq!(note_name(60), "C4"); // middle C
        assert_eq!(note_name(108), "C8"); // highest piano key
    }
}
