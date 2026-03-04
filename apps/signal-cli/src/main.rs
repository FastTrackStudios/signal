//! signal — CLI tool for the Signal library (presets, rigs, profiles, macros).

use std::path::PathBuf;

use clap::Parser;
use eyre::Result;

#[derive(Parser)]
#[command(
    name = "signal",
    about = "Query and manipulate the Signal library"
)]
struct Cli {
    /// Signal DB path (defaults to ~/Music/FastTrackStudio/Library/signal.db)
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: signal_cli::SignalCommand,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    signal_cli::run(cli.db, cli.command, cli.json).await
}
