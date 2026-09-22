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
    /// List the module presets as the rig loads them (modules.styx).
    Modules,
    /// Level every preset snapshot to the same loudness (writes each
    /// snapshot's `level_db` into presets.styx). The rig picks it up on the
    /// next change — no restart.
    LevelPresets {
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
        #[arg(long, default_value_t = 8)]
        threads: usize,
    },
}

pub fn run(command: Command) -> ExitCode {
    match command {
        Command::LevelPresets { dry_run, sample_rate, threads } => {
            let cal = signal_guitar::levelling::apply_nam_calibration();
            println!("NAM calibration: {}", cal.map_or("off".to_string(), |c| format!("{c} dBu interface")));
            let lib = signal_guitar::library::RigLibrary::load_or_bootstrap();
            let mut comp = signal_guitar::library::RigLibrary::load_compositions();
            if comp.presets.is_empty() {
                eprintln!("no presets loaded");
                return ExitCode::FAILURE;
            }
            let started = std::time::Instant::now();
            let results =
                signal_guitar::compose::level_presets(&mut comp, &lib.profile, &lib.drive_presets, sample_rate, threads);
            let mut failed = 0;
            let mut last = String::new();
            for r in &results {
                if r.preset != last {
                    println!("{}", r.preset);
                    last.clone_from(&r.preset);
                }
                match r.lufs {
                    Some(l) => println!("  {:<20} {l:>7.1} LUFS  →  {:+.1} dB", r.snapshot, r.level_db),
                    None => {
                        failed += 1;
                        println!("  {:<20} did not render — left at {:+.1} dB", r.snapshot, r.level_db);
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
            if failed > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS }
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
            let mut dangling = 0;
            for p in &comp.presets {
                for snap in &p.snapshots {
                    for pick in &snap.modules {
                        let ok = comp.module(&pick.module, &pick.preset).is_some_and(|m| {
                            pick.snapshot.is_empty() || m.snapshots.iter().any(|s| s.name.eq_ignore_ascii_case(&pick.snapshot))
                        });
                        if !ok {
                            dangling += 1;
                            eprintln!("  {} / {}: no {} preset {} / {}", p.name, snap.name, pick.module, pick.preset, pick.snapshot);
                        }
                    }
                }
            }
            println!(
                "\n{} module presets ({} files missing); {} presets, {} snapshots ({} picks dangling)",
                comp.modules.len(),
                missing,
                comp.presets.len(),
                comp.presets.iter().map(|p| p.snapshots.len()).sum::<usize>(),
                dangling
            );
            missing += dangling;
            if missing > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS }
        }
        Command::Migrate { profile, write } => match signal_guitar::compose::migrate_profile(&profile, !write) {
            Ok(m) => {
                println!("Module presets:");
                for module in &m.modules {
                    let snaps: Vec<&str> = module.snapshots.iter().map(|s| s.name.as_str()).collect();
                    println!("  {:<6} {:<24} {}", module.module, module.name, snaps.join(" · "));
                }
                println!("\nPresets:");
                for preset in &m.presets {
                    println!("  {}", preset.name);
                    for snap in &preset.snapshots {
                        let picks: Vec<String> =
                            snap.modules.iter().map(|p| format!("{}: {} / {}", p.module, p.preset, p.snapshot)).collect();
                        println!("    {:<22} {}  (+{} overrides)", snap.name, picks.join(", "), snap.overrides.len());
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
                    println!("\nLeft as they are (a second amp, or no pool capture): {}", left.join(", "));
                }
                println!("{}", if write { "\nwritten." } else { "\n(dry run — pass --write to save)" });
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Command::Level {
            profile,
            dry_run,
            sample_rate,
        } => match signal_guitar::levelling::level_profile(profile.as_deref(), sample_rate, dry_run) {
            Ok(results) => {
                let mut failed = false;
                for r in &results {
                    match r.lufs {
                        Some(lufs) => println!("{:<24} {lufs:>7.1} LUFS  →  level {:+.1} dB", r.patch, r.level_db),
                        None => {
                            failed = true;
                            println!("{:<24} did not render — level left at {:+.1} dB", r.patch, r.level_db);
                        }
                    }
                }
                if dry_run {
                    println!("\n(dry run — nothing written)");
                }
                if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
    }
}
