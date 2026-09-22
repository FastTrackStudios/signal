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
}

pub fn run(command: Command) -> ExitCode {
    match command {
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
