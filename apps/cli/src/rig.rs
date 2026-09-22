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
}

pub fn run(command: Command) -> ExitCode {
    match command {
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
