//! `signal` — the rig and its tone library from a terminal.
//!
//! A third front-end onto the engine's own backends, beside the desktop app
//! and the plugin editor. It drives the same [`signal_tone3000`] backend they
//! do rather than reimplementing any of it, which is why a capture fetched
//! here lands in the library with the same provenance and shows up in the app
//! without a reload.
//!
//! It does NOT talk to a running engine over vox. The tone library is files
//! and an OAuth session on this machine, so the CLI opens them directly and
//! works whether or not `signal-desktop --engine` happens to be up. The one
//! thing that does collide is the sign-in listener, which wants the redirect
//! port the engine also serves — `signal tone3000 login` says so when it is
//! taken.
//!
//! ```console
//! signal tone3000 status
//! signal tone3000 search "vox ac30" --gear amp-cab --sort downloads
//! signal tone3000 show 82521
//! signal tone3000 fetch 82521
//! ```

mod tone3000;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "signal",
    about = "The FastTrackStudio signal rig, from a terminal"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The TONE3000 tone library — browse it, and bring captures into the
    /// local NAM library the rig plays from.
    Tone3000 {
        #[command(subcommand)]
        command: tone3000::Command,
    },
}

fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,signal_tone3000=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("could not start the async runtime: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    match cli.command {
        Command::Tone3000 { command } => runtime.block_on(tone3000::run(command)),
    }
}
