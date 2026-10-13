//! `--engine --project PATH` / `--setlist PATH`: what the headless engine
//! plays before any remote has connected.
//!
//! Opened exactly as the app opens a song in Engine mode — through
//! `session_daw::open::open_song_into`, the one way a song is opened: the
//! prepared `.session` when there is one (organized tracks, the chart, the
//! guides), otherwise the file itself, prepared and saved as its `.session`
//! once; into an engine `equip`ped like the app's (the guide instrument).
//! A setlist is read by the app's own reader (`session_daw::setlist::read_setlist`).
//! Then the setlist service is rebuilt from what is open, so the browser
//! remote sees what the daw facade sees.

use std::path::{Path, PathBuf};

use crate::engine_server::OpenTarget;
use crate::session_engine::SessionEngine;

/// Open `target` into the Live Mode engine.
///
/// # Errors
///
/// A project that could not be opened, or a setlist none of whose songs
/// opened. A setlist's songs that fail are skipped (logged), as in the app.
pub async fn open(engine: &'static SessionEngine, target: &OpenTarget) -> eyre::Result<()> {
    let songs = match target {
        OpenTarget::Project(path) => vec![path.clone()],
        OpenTarget::Setlist(path) => session_daw::setlist::read_setlist(path)?,
    };
    // Parsing, preparing and decoding are blocking, and can take seconds.
    let opened = tokio::task::spawn_blocking(move || open_songs(engine, &songs))
        .await
        .map_err(|e| eyre::eyre!("open task: {e}"))??;
    tracing::info!(engine.songs = opened, "--engine: opened");
    // A project with no song markers builds an empty setlist; the daw
    // facade still serves it, so that is not a failure.
    if let Err(e) = engine.client.build_from_open_projects().await {
        tracing::warn!(error = ?e, "--engine: the songs did not build a setlist");
    }
    Ok(())
}

/// Open every song through the app's one path; the first becomes current
/// (the audio thread attaches the output device to it). Returns how many
/// opened.
fn open_songs(engine: &SessionEngine, songs: &[PathBuf]) -> eyre::Result<usize> {
    let daw = &engine.standalone;
    session_daw::open::equip(daw);
    let mut first: Option<String> = None;
    let mut opened = 0usize;
    for path in songs {
        match open_song(daw, path) {
            Ok(guid) => {
                opened = opened.saturating_add(1);
                first.get_or_insert(guid);
            }
            Err(e) => {
                tracing::error!(engine.song = %path.display(), error = %e, "--engine: a song did not open; the set goes on without it")
            }
        }
    }
    let first = first.ok_or_else(|| eyre::eyre!("none of the {} songs opened", songs.len()))?;
    daw.set_current_project(&first);
    Ok(opened)
}

fn open_song(daw: &daw_standalone::sync::Standalone, path: &Path) -> eyre::Result<String> {
    let prepare = session_daw::prepare::Prepare::for_song(path);
    let (opened, plan) = session_daw::open::open_song_into(daw, path, &prepare)?;
    tracing::info!(
        engine.song = %opened.name,
        engine.opened = %plan.open.display(),
        engine.tracks = opened.track_count,
        "--engine: song opened"
    );
    Ok(opened.project_guid)
}
