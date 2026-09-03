//! `fts-convert` — swap third-party plugins in a REAPER project for FTS ones.
//!
//! Pro-Q 4 becomes FTS EQ and Pro-C 3 becomes FTS Comp. Adding a third is a
//! decode-and-translate arm in `signal_import::rpp::convert`; the project
//! surgery, the report and the verification below are shared.
//!
//! ```sh
//! fts-convert song.rpp                    # writes song.fts.rpp, keeps the original
//! fts-convert song.rpp --in-place         # rewrites song.rpp, backing it up first
//! fts-convert song.rpp --dry-run          # says what it would do and writes nothing
//! ```
//!
//! The default is deliberately the timid one. A project file is somebody's
//! session; the converter earns the right to overwrite it by being asked.

use std::path::{Path, PathBuf};

use signal_import::rpp::{convert, Document};

mod verify;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);

    if args.is_empty() || flag("--help") || flag("-h") {
        usage();
        return;
    }

    let dry_run = flag("--dry-run");
    // Verification is the default. Turning it off is a choice a user can make
    // — it costs a few seconds per instance — but it should not be one they
    // make by omission.
    let verifying = !flag("--no-verify");
    let engine_too = flag("--engine-too");
    // `--curves <track>` prints the three response curves for one instance.
    let curves_for = args
        .iter()
        .position(|a| a == "--curves")
        .and_then(|i| args.get(i + 1).cloned());
    let in_place = flag("--in-place");
    let quiet = flag("--quiet");

    let inputs: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(PathBuf::from)
        .collect();
    if inputs.is_empty() {
        eprintln!("nothing to convert — give me a .rpp file");
        std::process::exit(2);
    }

    let mut any_failed = false;
    for input in &inputs {
        if !run(input, dry_run, in_place, quiet, verifying, engine_too, curves_for.as_deref()) {
            any_failed = true;
        }
    }
    if any_failed {
        std::process::exit(1);
    }
}

fn usage() {
    println!(
        "fts-convert <project.rpp>... [--in-place] [--dry-run] [--quiet]\n\
         \n\
         Replaces every FabFilter Pro-Q 4 with FTS EQ and every Pro-C 3 with\n\
         FTS Comp, carrying the preset across. Everything else in the project\n\
         is left byte for byte as it was — FX order, bypass state and the rest\n\
         of the chain included.\n\
         \n\
           --in-place   rewrite the project, saving the original alongside it\n\
                        as <name>.rpp.bak (the default writes <name>.fts.rpp)\n\
           --dry-run    report what would change and write nothing\n\
           --no-verify  skip rendering each instance through both plugins\n\
           --engine-too report the EQ engine's own error beside the plugin's,\n\
                        which says whether a gap is in the DSP or in this\n\
                        converter's parameter map\n\
           --quiet      only report instances that could not be converted\n\
         \n\
         Verification is on by default: every converted instance is rendered\n\
         through the real Pro-Q and the real FTS EQ and compared band by band.\n\
         The number reported is the mean difference across a third-octave\n\
         grid, in dB, on mid and side separately."
    );
}

fn run(
    input: &Path,
    dry_run: bool,
    in_place: bool,
    quiet: bool,
    verifying: bool,
    engine_too: bool,
    curves_for: Option<&str>,
) -> bool {
    let text = match std::fs::read_to_string(input) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}: {e}", input.display());
            return false;
        }
    };

    let mut doc = Document::parse(&text);
    let report = convert::convert(&mut doc);

    println!("\n{}", input.display());
    if report.is_empty() {
        println!(
            "  nothing to convert ({} other plugins left alone)",
            report.untouched_fx
        );
        return true;
    }

    // The rig bridges each plugin pair the first time it meets one, and
    // keeps them for the whole project.
    let mut rig = if verifying { Some(verify::Rig::new()) } else { None };

    let mut worst_seen = 0.0f64;
    if !quiet || rig.is_some() {
        for c in &report.converted {
            let preset = c.preset.as_deref().unwrap_or("—");
            let verdict = match rig.as_mut() {
                None => String::new(),
                Some(rig) => {
                    if curves_for == Some(c.track.as_str()) {
                        if let Some((pq, plug, eng, hz)) = c
                            .native_params
                            .as_ref()
                            .and_then(|n| rig.curves(c.family, &c.source_state, &c.our_state, n))
                        {
                            println!("\n  {} — mid response, dB", c.track);
                            println!(
                                "  {:>8} {:>9} {:>9} {:>9}",
                                "Hz",
                                c.family.source_name(),
                                "plugin",
                                "engine"
                            );
                            for i in 0..hz.len() {
                                println!(
                                    "  {:>8.0} {:>9.2} {:>9.2} {:>9.2}",
                                    hz[i], pq[i], plug[i], eng[i]
                                );
                            }
                            println!();
                        }
                    }
                    // The engine column, where the family has one — it is
                    // what says whether a gap is in the translation or in the
                    // DSP under it.
                    let engine = match (engine_too, c.native_params.as_ref()) {
                        (true, Some(native)) => rig
                            .compare_engine(c.family, &c.source_state, native)
                            .map(|d| format!(" | engine {:>5.2}", d.mean_db))
                            .unwrap_or_default(),
                        _ => String::new(),
                    };
                    match rig.compare(c.family, &c.source_state, &c.our_state) {
                        Some(d) => {
                            worst_seen = worst_seen.max(d.mean_db);
                            format!(
                                "  {:>5.2} dB mean, {:>5.2} at {:.0} Hz {}{engine}",
                                d.mean_db,
                                d.worst_db,
                                d.worst_hz,
                                if d.worst_in_side { "(side)" } else { "(mid)" }
                            )
                        }
                        // Either plugin declining to process is reported as
                        // that, not as an infinite error.
                        None => "  not measurable (one side was silent)".to_string(),
                    }
                }
            };
            println!(
                "  {:<20} fx {:<3} {:<8} {:<22} {:<24}{verdict}",
                truncate(&c.track, 20),
                c.slot,
                c.family.source_name(),
                truncate(preset, 22),
                truncate(&c.summary, 24),
            );
            for missing in &c.unmapped {
                println!("      ! {missing} does not cross");
            }
        }
    }
    for s in &report.skipped {
        println!("  {:<28} fx {:<3} LEFT  {}", truncate(&s.track, 28), s.slot, s.reason);
    }

    println!(
        "  {} converted, {} left alone, {} other plugins untouched",
        report.converted.len(),
        report.skipped.len(),
        report.untouched_fx
    );
    if rig.is_some() && !report.converted.is_empty() {
        println!("  worst instance {worst_seen:.2} dB mean across the band grid");
    }
    for track in &report.automated_chains {
        println!(
            "  ! {track}: this chain has parameter automation. The envelope now\n    \
             points at an FTS EQ parameter that is not the one it was written\n    \
             for — check it before you trust the mix."
        );
    }

    if dry_run {
        println!("  (dry run — nothing written)");
        return true;
    }
    if report.converted.is_empty() {
        println!("  (nothing converted — nothing written)");
        return true;
    }

    let output = if in_place {
        let backup = with_suffix(input, "rpp.bak");
        if let Err(e) = std::fs::write(&backup, &text) {
            eprintln!("  could not write the backup {}: {e} — refusing to overwrite", backup.display());
            return false;
        }
        println!("  original saved as {}", backup.display());
        input.to_path_buf()
    } else {
        with_suffix(input, "fts.rpp")
    };

    match std::fs::write(&output, doc.render()) {
        Ok(()) => {
            println!("  wrote {}", output.display());
            true
        }
        Err(e) => {
            eprintln!("  {}: {e}", output.display());
            false
        }
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{stem}.{suffix}"))
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).chain(std::iter::once('…')).collect()
    }
}
