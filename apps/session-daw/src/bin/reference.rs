//! Bounce each song's reference: its whole mix but the guide, as one
//! stereo stream (see `session_daw::reference`).
//!
//! ```sh
//! reference "Song" ["Other Song" …]   # a song's folder, .session or .RPP
//! ```

use std::path::PathBuf;

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,session_daw=info")),
        )
        .init();
    let songs: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if songs.is_empty() {
        eyre::bail!("usage: reference <song folder | .session | .RPP> …");
    }
    for song in songs {
        session_daw::reference::bounce(&song)?;
    }
    Ok(())
}
