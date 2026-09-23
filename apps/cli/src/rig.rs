//! `signal rig …` — the guitar rig's library, without the app.

use std::process::ExitCode;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum Command {
    /// Level every patch of a profile to the same loudness (writes each
    /// patch's `level_db`; the player's own `trim_db` is left alone).
    /// Restart the app afterwards — it reads the profile at load.
    Level {
        /// Profile name (e.g. Metal). Default: the active profile.
        profile: Option<String>,
        /// Measure and print, write nothing.
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
    },
    /// Regroup a profile into presets with snapshots, built from module
    /// presets (amp captures grouped by family, the pedal board as a Drive
    /// preset). Sounds are unchanged. Prints the proposal; `--write` saves.
    Migrate {
        profile: String,
        #[arg(long)]
        write: bool,
    },
    /// Level the building blocks on their own: every amp module snapshot
    /// through its own cab (to the rig's target), every drive option to unity
    /// at drive 0.5. Writes each block's Output Level. Run before
    /// `level-presets` and `level`.
    LevelModules {
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
    },
    /// Name every preset for the gear it plays: dissolve the profile-named
    /// presets (`Worship Clean`, `Metal Rhythm`, …) into the amp presets
    /// their snapshots play, and repoint every profile at them. Sounds are
    /// unchanged. Dry run unless `--write`.
    RegroupPresets {
        #[arg(long)]
        write: bool,
    },
    /// List the module presets as the rig loads them (modules.styx).
    Modules,
    /// Make a profile pure references: each patch becomes a snapshot of a
    /// "<profile> <stack>" preset (its overrides, its amp mapped through
    /// `--map`, the board as a Pedalboard snapshot); the profile keeps only
    /// stacks and references. Dry run unless `--write`.
    Recompose {
        profile: String,
        /// Lines of `old capture = Amp preset / snapshot`.
        #[arg(long)]
        map: std::path::PathBuf,
        #[arg(long)]
        write: bool,
    },
    /// Level every preset snapshot to the same loudness (writes each
    /// snapshot's `level_db` into presets.styx). The rig picks it up on the
    /// next change — no restart.
    /// Write the rig as the browser plays it: rig.json (every patch's
    /// resolved chain) plus its models and IRs under content-addressed keys.
    WebBundle {
        out: std::path::PathBuf,
        /// Only these profiles (default: all).
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Dial each preset snapshot's Post Comp threshold to its compressor
    /// preset's target gain reduction, on that snapshot's own amp (writes a
    /// `Post Comp threshold` override per snapshot into presets.styx). The
    /// Pre Comp is never dialled: it hears only the guitar. Run
    /// `level-presets` after.
    DialPostComp {
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
        /// Render threads (0 = every core).
        #[arg(long, default_value_t = 0)]
        threads: usize,
    },
    LevelPresets {
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
        /// Render threads (0 = every core).
        #[arg(long, default_value_t = 0)]
        threads: usize,
    },
}

pub fn run(command: Command) -> ExitCode {
    match command {
        Command::WebBundle { out, profiles } => {
            match signal_guitar::web_bundle::export(&out, &profiles) {
                Ok((bundle, skipped)) => {
                    let patches: usize = bundle.profiles.iter().map(|p| p.patches.len()).sum();
                    let bytes: u64 = bundle.assets.iter().map(|a| a.bytes).sum();
                    println!(
                        "{} profiles, {patches} patches, {} assets ({:.1} MB) → {}",
                        bundle.profiles.len(),
                        bundle.assets.len(),
                        bytes as f64 / 1e6,
                        out.display()
                    );
                    if skipped.plugin_blocks > 0 {
                        eprintln!(
                            "skipped {} plugin blocks (no plugin host in a browser)",
                            skipped.plugin_blocks
                        );
                    }
                    for m in &skipped.missing {
                        eprintln!("missing, block dropped: {m}");
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Recompose {
            profile,
            map,
            write,
        } => {
            let Ok(text) = std::fs::read_to_string(&map) else {
                eprintln!("cannot read {}", map.display());
                return ExitCode::FAILURE;
            };
            let amp_map = signal_guitar::compose::parse_amp_map(&text);
            let lib = signal_guitar::library::RigLibrary::load_or_bootstrap();
            let Some(mut def) = lib
                .profiles
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&profile))
                .cloned()
            else {
                eprintln!("no profile named {profile:?}");
                return ExitCode::FAILURE;
            };
            let mut comp = signal_guitar::library::RigLibrary::load_compositions();
            let unmapped = signal_guitar::compose::recompose(
                &mut def,
                &mut comp,
                &amp_map,
                &lib.drive_presets,
            );
            if !unmapped.is_empty() {
                eprintln!(
                    "no mapping for the amp of: {} — add them to the map",
                    unmapped.join(", ")
                );
                return ExitCode::FAILURE;
            }
            for p in &def.patches {
                println!("  {:<22} → {} / {}", p.name, p.rig_preset, p.snapshot);
            }
            // Every pick must land on something real.
            let mut bad = 0;
            for p in comp
                .presets
                .iter()
                .filter(|p| p.name.starts_with(&def.name))
            {
                for snap in &p.snapshots {
                    for pick in &snap.modules {
                        if !comp.module(&pick.module, &pick.preset).is_some_and(|m| {
                            m.snapshots
                                .iter()
                                .any(|s| s.name.eq_ignore_ascii_case(&pick.snapshot))
                        }) {
                            bad += 1;
                            eprintln!(
                                "  {} / {}: no {} {} / {}",
                                p.name, snap.name, pick.module, pick.preset, pick.snapshot
                            );
                        }
                    }
                }
            }
            if bad > 0 {
                return ExitCode::FAILURE;
            }
            if write {
                signal_guitar::library::RigLibrary::save_compositions(&comp);
                signal_guitar::library::RigLibrary::save_profile(&def);
                println!("written.");
            } else {
                println!("(dry run — pass --write to save)");
            }
            ExitCode::SUCCESS
        }
        Command::LevelModules { dry_run, sample_rate } => {
            let started = std::time::Instant::now();
            let results = signal_guitar::levelling::level_modules(sample_rate, dry_run);
            let mut failed = 0;
            for r in &results {
                match r.lufs {
                    Some(l) => println!("{:<60} {l:>7.1} LUFS  →  {:+.1} dB", r.what, r.level_db),
                    None => {
                        failed += 1;
                        println!("{:<60} did not render", r.what);
                    }
                }
            }
            println!(
                "\n{} modules in {:.0}s, {failed} failed{}",
                results.len(),
                started.elapsed().as_secs_f32(),
                if dry_run { " (dry run — nothing written)" } else { "" }
            );
            ExitCode::SUCCESS
        }
        Command::RegroupPresets { write } => {
            let lib = signal_guitar::library::RigLibrary::load_or_bootstrap();
            let mut comp = signal_guitar::library::RigLibrary::load_compositions();
            let mut profiles = lib.profiles.clone();
            let moved = signal_guitar::compose::regroup_by_gear(&mut comp, &mut profiles);
            let mut last = String::new();
            for m in &moved {
                if m.from_preset != last {
                    println!("{}", m.from_preset);
                    last.clone_from(&m.from_preset);
                }
                println!(
                    "  {:<22} → {} · {}{}",
                    m.from_snapshot,
                    m.to_preset,
                    m.to_snapshot,
                    if m.reused { "  (same as existing)" } else { "" }
                );
            }
            println!("\nPatches:");
            for p in &profiles {
                for patch in &p.patches {
                    println!("  {:<8} {:<22} → {} · {}", p.name, patch.name, patch.rig_preset, patch.snapshot);
                }
            }
            println!("\nPresets now: {}", comp.presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "));
            if write {
                signal_guitar::library::RigLibrary::save_compositions(&comp);
                for p in &profiles {
                    signal_guitar::library::RigLibrary::save_profile(p);
                }
                println!("written.");
            } else {
                println!("(dry run — pass --write to save)");
            }
            ExitCode::SUCCESS
        }
        Command::DialPostComp {
            dry_run,
            sample_rate,
            threads,
        } => {
            signal_guitar::levelling::apply_nam_calibration();
            let lib = signal_guitar::library::RigLibrary::load_or_bootstrap();
            let mut comp = signal_guitar::library::RigLibrary::load_compositions();
            let started = std::time::Instant::now();
            let results = signal_guitar::compose::dial_post_comp(
                &mut comp,
                &lib.profile,
                &lib.drive_presets,
                sample_rate,
                threads,
            );
            let mut failed = 0;
            let mut last = String::new();
            for r in &results {
                if r.preset != last {
                    println!("{}", r.preset);
                    last.clone_from(&r.preset);
                }
                match r.dialled {
                    Some((t, gr)) => println!(
                        "  {:<22} {:<13} threshold {t:>6.1} dB  GR {gr:>4.1} dB (target {:.1})",
                        r.snapshot, r.comp, r.target_gr_db
                    ),
                    None => {
                        failed += 1;
                        println!("  {:<22} did not render — left as it was", r.snapshot);
                    }
                }
            }
            if !dry_run {
                signal_guitar::library::RigLibrary::save_compositions(&comp);
            }
            println!(
                "\n{} snapshots in {:.0}s, {} failed{}",
                results.len(),
                started.elapsed().as_secs_f64(),
                failed,
                if dry_run { " (dry run — nothing written)" } else { "" }
            );
            if failed == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        Command::LevelPresets {
            dry_run,
            sample_rate,
            threads,
        } => {
            let cal = signal_guitar::levelling::apply_nam_calibration();
            println!(
                "NAM calibration: {}",
                cal.map_or("off".to_string(), |c| format!("{c} dBu interface"))
            );
            let lib = signal_guitar::library::RigLibrary::load_or_bootstrap();
            let mut comp = signal_guitar::library::RigLibrary::load_compositions();
            if comp.presets.is_empty() {
                eprintln!("no presets loaded");
                return ExitCode::FAILURE;
            }
            let started = std::time::Instant::now();
            let results = signal_guitar::compose::level_presets(
                &mut comp,
                &lib.profile,
                &lib.drive_presets,
                sample_rate,
                threads,
            );
            let mut failed = 0;
            let mut last = String::new();
            for r in &results {
                if r.preset != last {
                    println!("{}", r.preset);
                    last.clone_from(&r.preset);
                }
                match r.lufs {
                    Some(l) => println!(
                        "  {:<20} {l:>7.1} LUFS  →  {:+.1} dB",
                        r.snapshot, r.level_db
                    ),
                    None => {
                        failed += 1;
                        println!(
                            "  {:<20} did not render — left at {:+.1} dB",
                            r.snapshot, r.level_db
                        );
                    }
                }
            }
            if !dry_run {
                signal_guitar::library::RigLibrary::save_compositions(&comp);
            }
            println!(
                "\n{} snapshots in {:.0}s, {} failed{}",
                results.len(),
                started.elapsed().as_secs_f64(),
                failed,
                if dry_run {
                    " (dry run — nothing written)"
                } else {
                    ""
                }
            );
            if failed > 0 {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Command::Modules => {
            let comp = signal_guitar::library::RigLibrary::load_compositions();
            if comp.modules.is_empty() {
                eprintln!("no module presets loaded — modules.styx is missing or did not parse");
                return ExitCode::FAILURE;
            }
            let mut missing = 0;
            for m in &comp.modules {
                let snaps: Vec<&str> = m.snapshots.iter().map(|s| s.name.as_str()).collect();
                println!("{:<6} {:<24} {}", m.module, m.name, snaps.join(" · "));
                for s in &m.snapshots {
                    for p in [&s.nam, &s.cab, &s.nam2, &s.cab2] {
                        if !p.is_empty() && !std::path::Path::new(p).exists() {
                            missing += 1;
                            eprintln!("  missing: {p}");
                        }
                    }
                }
            }
            for b in &comp.blocks {
                println!(
                    "Block  {:<11} {:<24} {}",
                    b.block_type,
                    b.name,
                    if b.bypass { "(off)" } else { "" }
                );
            }
            let mut dangling = 0;
            let block_ok =
                |c: &signal_guitar::compose::BlockChoiceDef| comp.block_preset(&c.preset).is_some();
            for m in &comp.modules {
                for snap in &m.snapshots {
                    for c in snap.blocks.iter().filter(|c| !block_ok(c)) {
                        dangling += 1;
                        eprintln!(
                            "  {} {} / {}: no block preset {} for {}",
                            m.module, m.name, snap.name, c.preset, c.block
                        );
                    }
                }
            }
            for p in &comp.presets {
                for snap in &p.snapshots {
                    for c in snap.blocks.iter().filter(|c| !block_ok(c)) {
                        dangling += 1;
                        eprintln!(
                            "  {} / {}: no block preset {} for {}",
                            p.name, snap.name, c.preset, c.block
                        );
                    }
                }
            }
            for p in &comp.presets {
                for snap in &p.snapshots {
                    for pick in &snap.modules {
                        let ok = comp.module(&pick.module, &pick.preset).is_some_and(|m| {
                            pick.snapshot.is_empty()
                                || m.snapshots
                                    .iter()
                                    .any(|s| s.name.eq_ignore_ascii_case(&pick.snapshot))
                        });
                        if !ok {
                            dangling += 1;
                            eprintln!(
                                "  {} / {}: no {} preset {} / {}",
                                p.name, snap.name, pick.module, pick.preset, pick.snapshot
                            );
                        }
                    }
                }
            }
            println!(
                "\n{} block presets; {} module presets ({} files missing); {} presets, {} snapshots ({} picks dangling)",
                comp.blocks.len(),
                comp.modules.len(),
                missing,
                comp.presets.len(),
                comp.presets
                    .iter()
                    .map(|p| p.snapshots.len())
                    .sum::<usize>(),
                dangling
            );
            missing += dangling;
            if missing > 0 {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Command::Migrate { profile, write } => {
            match signal_guitar::compose::migrate_profile(&profile, !write) {
                Ok(m) => {
                    println!("Module presets:");
                    for module in &m.modules {
                        let snaps: Vec<&str> =
                            module.snapshots.iter().map(|s| s.name.as_str()).collect();
                        println!(
                            "  {:<6} {:<24} {}",
                            module.module,
                            module.name,
                            snaps.join(" · ")
                        );
                    }
                    println!("\nPresets:");
                    for preset in &m.presets {
                        println!("  {}", preset.name);
                        for snap in &preset.snapshots {
                            let picks: Vec<String> = snap
                                .modules
                                .iter()
                                .map(|p| format!("{}: {} / {}", p.module, p.preset, p.snapshot))
                                .collect();
                            println!(
                                "    {:<22} {}  (+{} overrides)",
                                snap.name,
                                picks.join(", "),
                                snap.overrides.len()
                            );
                        }
                    }
                    let left: Vec<&str> = m
                        .profile
                        .patches
                        .iter()
                        .filter(|p| p.rig_preset.is_empty())
                        .map(|p| p.name.as_str())
                        .collect();
                    if !left.is_empty() {
                        println!(
                            "\nLeft as they are (a second amp, or no pool capture): {}",
                            left.join(", ")
                        );
                    }
                    println!(
                        "{}",
                        if write {
                            "\nwritten."
                        } else {
                            "\n(dry run — pass --write to save)"
                        }
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Level {
            profile,
            dry_run,
            sample_rate,
        } => {
            match signal_guitar::levelling::level_profile(profile.as_deref(), sample_rate, dry_run)
            {
                Ok(results) => {
                    let mut failed = false;
                    for r in &results {
                        match r.lufs {
                            Some(lufs) => println!(
                                "{:<24} {lufs:>7.1} LUFS  →  level {:+.1} dB",
                                r.patch, r.level_db
                            ),
                            None => {
                                failed = true;
                                println!(
                                    "{:<24} did not render — level left at {:+.1} dB",
                                    r.patch, r.level_db
                                );
                            }
                        }
                    }
                    if dry_run {
                        println!("\n(dry run — nothing written)");
                    }
                    if failed {
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                    }
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
