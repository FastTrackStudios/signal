//! Prepare a song once and save it as a `.session`.
//!
//! ```sh
//! prepare "Song/Song.RPP" [Song/Song.kf]
//! ```
//!
//! Organizes the multitrack, builds the song from its chart and generates
//! the click and guide — what the app does the first time it opens a
//! multitrack — and writes `Song/Song.session` beside it, which the app
//! then opens instead. The chart defaults to the one `.kf` beside the
//! project. One song per run: the engine's facade is process-wide.

use std::path::PathBuf;

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,session_daw=info")),
        )
        .init();
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let rpp = args
        .next()
        .ok_or_else(|| eyre::eyre!("usage: prepare <Song.RPP> [Song.kf]"))?;
    let chart = args
        .next()
        .or_else(|| session_daw::prepare::chart_beside(&rpp));
    let saved = session_daw::prepare::prepare_and_save(&rpp, chart)?;
    tracing::info!(session.saved = %saved.display(), "prepared");
    Ok(())
}
