//! Streaming a song in: this window's own engine plays (some of) the song
//! the system it drives has current, sample-locked to it — Cue, and a
//! streamed Engine.
//!
//! A window in Remote plays nothing; the system it drives (REAPER, a
//! Session engine) plays the song. Streamed in, this window's own engine
//! plays too — the tracks its **selection** names
//! ([`session::load_selection::LoadSelection`]: Cue is the generated click,
//! count and guide; Engine is every track, or the template groups asked for
//! with `FTS_LOAD`):
//!
//! - **The song** opens the one way a song opens
//!   ([`crate::open::open_song_into_with`]) — its prepared `.session`, so
//!   the guide and every edit are the song's — with its media **deferred**:
//!   the structure is there at once, and the click and guide (MIDI and
//!   local samples) play straight away. Unselected tracks are muted *here*
//!   (their own mute kept, to restore); the shared session is untouched.
//! - **The media** of the selected tracks then loads in the order it will
//!   be heard — what covers the playhead, then what comes next, then what
//!   is behind — and the mode climbs Remote → Cue → Engine as it lands.
//! - **Which song** is the one the remote has current
//!   ([`crate::open::current_song`]); its file is found by its path, or by
//!   its name in this machine's setlist (`FTS_SESSION_SETLIST`).
//! - **The transport** is the remote's, to the sample: a
//!   [`daw::rpc::TransportLeader`] on the remote's `TransportSync` drives a
//!   [`daw_transport_sync::Follower`] on this engine's sync backend — a
//!   scheduled locate to start or follow a jump, then a resampled rate
//!   nudge of a fraction of a percent.
//!
//! One task does all of it, watching the mode and the remote's current
//! song: a pick here, a tab changed in REAPER, and the mode picker are all
//! the same thing to it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use daw::standalone::sync::Standalone;
use daw_transport_sync::{Correction, Follower, TransportBackend};

use daw::standalone::audio_engine::materialize::{
    PendingMedia, materialize_take_via_bay, pending_media,
};
use session::load_selection::{LoadSelection, group_of};

use crate::open::{Media, begin_asset_load, set_assets_progress, set_cue_ready};

/// How often the follower corrects (and the task looks at the mode and
/// the song).
const TICK: Duration = Duration::from_millis(20);

/// Start the task (once per process). It idles unless this window streams
/// a song in (Cue, or Engine with a target).
pub fn start() {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let Some(runtime) = crate::open::runtime() else {
        tracing::warn!("stream-in: no engine runtime; nothing streams in");
        return;
    };
    runtime.spawn(run());
}

/// A song as this engine has it.
struct Song {
    /// Its project in this process's engine.
    local: String,
    /// The selection its mutes were set for.
    selection: Option<LoadSelection>,
    /// Each track's own mute, before a selection muted it.
    own_mutes: HashMap<String, bool>,
    /// Its tracks' groups (template folder), by track guid.
    groups: HashMap<String, String>,
    /// Selected media still to load.
    pending: Vec<PendingMedia>,
    /// How many of the selected takes are loaded.
    loaded: usize,
    total: usize,
    /// Mirrored from the engine this window drives (not on this machine):
    /// its proxies stream in by range.
    peer: Option<crate::song_stream::StreamedSong>,
    /// The selected takes streaming in, and the fetcher bringing them.
    streamed: Arc<std::sync::Mutex<Vec<daw::standalone::audio_engine::media_fetch::StreamedTake>>>,
    fetching: Option<crate::song_stream::FetchGuard>,
}

/// What is being followed now.
struct Following {
    /// The selection it plays.
    selection: LoadSelection,
    /// The remote's project.
    remote: String,
    local: String,
    leader: daw::rpc::TransportLeader,
    backend: Arc<dyn TransportBackend + Send + Sync>,
    follower: Follower,
}

async fn run() {
    let mut songs: HashMap<String, Song> = HashMap::new();
    let mut following: Option<Following> = None;
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let mode = crate::open::mode();
        if !mode.streams_in() {
            // Not streaming in (any more): let go, and be silent.
            if let Some(was) = following.take() {
                was.backend
                    .stop(was.backend.snapshot().map_or(0.0, |s| s.playhead_seconds));
                set_cue_ready(false);
                tracing::info!(stream.song = %was.remote, "stream-in: stopped");
            }
            continue;
        }
        let Some(remote) = crate::open::current_song() else {
            continue;
        };
        let selection = mode.selection.clone();
        if following
            .as_ref()
            .is_none_or(|f| f.remote != remote || f.selection != selection)
        {
            set_cue_ready(false);
            following = follow(&remote, &selection, &mut songs).await;
            if following.is_some() {
                set_cue_ready(true);
            }
        }
        // Bring in the next media, what will be heard soonest first — from
        // disk here, or (a mirrored song) report what has streamed in.
        if let Some(f) = following.as_ref()
            && let Some(song) = songs.get_mut(&f.remote)
        {
            if !song.pending.is_empty() {
                let playhead = f.backend.snapshot().map_or(0.0, |s| s.playhead_seconds);
                load_next(song, playhead).await;
            } else if song.peer.is_some() && song.loaded < song.total {
                let arrived = crate::song_stream::arrived(&song.streamed);
                if arrived != song.loaded {
                    song.loaded = arrived;
                    set_assets_progress(song.loaded, song.total);
                    if song.loaded == song.total {
                        tracing::info!(
                            stream.media = song.total,
                            "stream-in: every selected proxy has streamed in"
                        );
                    }
                }
            }
        }
        if let Some(f) = following.as_mut() {
            let correction = f.leader.tick(&mut f.follower, f.backend.as_ref());
            if let Some(c) = correction
                && !matches!(c, Correction::Hold)
            {
                tracing::debug!(
                    stream.correction = ?c,
                    stream.drift_us = f.follower.controller().last_drift().map(|d| d * 1e6),
                    stream.offset_us = f.leader.offset_micros(),
                    "stream-in: following"
                );
            }
        }
    }
}

/// Make `remote` the song this engine plays: open it (once, its media
/// deferred), apply `selection`, queue its selected media, put the audio on
/// it, and lock its transport to the remote's. `None` when the song is not
/// on this machine (logged).
async fn follow(
    remote: &str,
    selection: &LoadSelection,
    songs: &mut HashMap<String, Song>,
) -> Option<Following> {
    let daw = crate::open::cue_engine();
    if !songs.contains_key(remote) {
        // Here, or mirrored from the engine this window drives (its proxies
        // then stream in by range).
        let (path, peer) = match local_song_file(remote).await {
            Some(path) => (path, None),
            None => match stream_from_elsewhere(remote).await {
                Some(streamed) => (streamed.project.clone(), Some(streamed)),
                None => return None,
            },
        };
        let engine = daw.clone();
        match tokio::task::spawn_blocking(move || open_song(&engine, &path)).await {
            Ok(Ok(mut song)) => {
                song.peer = peer;
                songs.insert(remote.to_owned(), song);
            }
            Ok(Err(e)) => {
                tracing::warn!(stream.song = remote, error = %e, "stream-in: the song did not open");
                return None;
            }
            Err(e) => {
                tracing::warn!(stream.song = remote, error = %e, "stream-in: the open task failed");
                return None;
            }
        }
    }
    let song = songs.get_mut(remote)?;
    select(daw, song, selection);
    begin_asset_load(song.total);
    set_assets_progress(song.loaded, song.total);
    let local = song.local.clone();
    crate::open::switch_to(daw, &local, true);
    let backend: Arc<dyn TransportBackend + Send + Sync> = Arc::new(daw.sync_backend(&local)?);
    // A mirrored song: fetch its streamed takes' bytes from this engine's
    // playhead on.
    if let Some(peer) = song.peer.as_ref()
        && song.fetching.is_none()
    {
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let at = Arc::clone(&backend);
        peer.fetch(
            Arc::clone(&song.streamed),
            Arc::new(move || at.snapshot().map_or(0.0, |s| s.playhead_seconds)),
            Arc::clone(&stop),
        );
        song.fetching = Some(crate::song_stream::FetchGuard(stop));
    }
    let remote_daw = daw::rpc::Daw::try_get()?;
    let project = remote_daw.project(remote).await.ok()?;
    let leader = project
        .transport_sync()
        .leader(daw::standalone::transport_sync::now_micros);
    tracing::info!(stream.song = remote, stream.local = %local, stream.selection = %selection.label(), "stream-in: following the remote");
    Some(Following {
        selection: selection.clone(),
        remote: remote.to_owned(),
        local,
        leader,
        backend,
        follower: Follower::default(),
    })
}

/// Open a song into this engine the one way songs open, its media
/// deferred; note each track's group and own mute.
fn open_song(daw: &Standalone, path: &Path) -> eyre::Result<Song> {
    let prepare = crate::prepare::Prepare::for_song(path);
    let (opened, plan) = crate::open::open_song_into_with(daw, path, &prepare, Media::Deferred)?;
    let ctx = daw::service::ProjectContext::Project(opened.project_guid.clone());
    let tracks = daw::service::Tracks::all(daw, ctx);
    let by_guid: HashMap<&str, &daw::service::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let mut groups = HashMap::new();
    let mut own_mutes = HashMap::new();
    for track in &tracks {
        // Its folders, outermost first.
        let mut ancestors = Vec::new();
        let mut at = track.parent_guid.as_deref();
        while let Some(guid) = at
            && let Some(parent) = by_guid.get(guid)
        {
            ancestors.push(parent.name.as_str());
            at = parent.parent_guid.as_deref();
        }
        ancestors.reverse();
        groups.insert(
            track.guid.clone(),
            group_of(&ancestors, &track.name, track.is_folder),
        );
        own_mutes.insert(track.guid.clone(), track.muted);
    }
    tracing::info!(stream.song = %opened.name, stream.opened = %plan.open.display(), stream.tracks = tracks.len(), "stream-in: song open, media deferred");
    Ok(Song {
        local: opened.project_guid,
        selection: None,
        own_mutes,
        groups,
        pending: Vec::new(),
        loaded: 0,
        total: 0,
        peer: None,
        streamed: Arc::new(std::sync::Mutex::new(Vec::new())),
        fetching: None,
    })
}

/// Apply `selection` to a song: unselected tracks muted here (selected
/// ones back to their own mute), and its selected media queued.
fn select(daw: &Standalone, song: &mut Song, selection: &LoadSelection) {
    if song.selection.as_ref() == Some(selection) {
        return;
    }
    let ctx = daw::service::ProjectContext::Project(song.local.clone());
    for (guid, group) in &song.groups {
        let own = song.own_mutes.get(guid).copied().unwrap_or(false);
        let muted = if selection.includes(group) { own } else { true };
        if let Err(e) = daw::service::Tracks::set_muted(
            daw,
            ctx.clone(),
            daw::service::TrackRef::Guid(guid.clone()),
            muted,
        ) {
            tracing::warn!(stream.track = %guid, error = %e, "stream-in: a track's mute could not be set");
        }
    }
    let selected: Vec<PendingMedia> = pending_media(daw, &song.local)
        .into_iter()
        .filter(|m| {
            song.groups
                .get(&m.track_guid)
                .is_some_and(|g| selection.includes(g))
        })
        .collect();
    if let Some(peer) = song.peer.as_ref() {
        // Mirrored: each selected take streams its proxy (attached once);
        // the fetcher is told which, and progress is what has fully arrived.
        let before = song.streamed.lock().map(|s| s.clone()).unwrap_or_default();
        let stem = |p: &str| {
            Path::new(p)
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
        };
        let mut streamed = Vec::new();
        for media in &selected {
            // Already streaming (a selection widened), or attach it now.
            let existing = before.iter().find(|t| {
                stem(&t.path) == stem(&media.path) && (t.start - media.start).abs() < 1e-9
            });
            match existing
                .cloned()
                .or_else(|| peer.attach(daw, &song.local, media).map(|a| a.take))
            {
                Some(take) => streamed.push(take),
                None => {
                    tracing::warn!(stream.media = %media.path, "stream-in: the source has no indexed proxy for this take")
                }
            }
        }
        song.total = streamed.len();
        if let Ok(mut slot) = song.streamed.lock() {
            *slot = streamed;
        }
        song.loaded = crate::song_stream::arrived(&song.streamed);
        song.pending = Vec::new();
    } else {
        song.total = selected.len();
        song.loaded = selected
            .iter()
            .filter(|m| {
                daw::standalone::audio_engine::materialize::is_loaded(
                    daw,
                    &song.local,
                    &m.take_guid,
                )
            })
            .count();
        song.pending = selected
            .into_iter()
            .filter(|m| {
                !daw::standalone::audio_engine::materialize::is_loaded(
                    daw,
                    &song.local,
                    &m.take_guid,
                )
            })
            .collect();
    }
    song.selection = Some(selection.clone());
    tracing::info!(
        stream.selection = %selection.label(),
        stream.media = song.total,
        stream.loaded = song.loaded,
        "stream-in: selection applied"
    );
}

/// How soon a take will be heard from `playhead`: now if its item covers
/// the playhead, then by how far ahead it starts; what is behind comes
/// after everything ahead.
fn heard_in(m: &PendingMedia, playhead: f64) -> f64 {
    if m.start <= playhead && playhead < m.end {
        0.0
    } else if m.start >= playhead {
        m.start - playhead
    } else {
        // Behind: after all that is ahead (a day), nearest first.
        86_400.0 + (playhead - m.end)
    }
}

/// Load the selected take that will be heard soonest.
async fn load_next(song: &mut Song, playhead: f64) {
    let Some(next) = song
        .pending
        .iter()
        .enumerate()
        .min_by(|a, b| heard_in(a.1, playhead).total_cmp(&heard_in(b.1, playhead)))
        .map(|(i, _)| i)
    else {
        return;
    };
    let media = song.pending.swap_remove(next);
    let local = song.local.clone();
    let daw = crate::open::cue_engine().clone();
    let take = media.take_guid.clone();
    let path = media.path.clone();
    let done =
        tokio::task::spawn_blocking(move || materialize_take_via_bay(&daw, &local, &take, &path))
            .await;
    match done {
        Ok(Ok(())) => {
            song.loaded = song.loaded.saturating_add(1);
            set_assets_progress(song.loaded, song.total);
            if song.pending.is_empty() {
                tracing::info!(
                    stream.media = song.total,
                    "stream-in: every selected take is in"
                );
            }
        }
        Ok(Err(e)) => {
            // Counted as done — a missing file is not coming later — and said once.
            song.total = song.total.saturating_sub(1);
            set_assets_progress(song.loaded, song.total);
            tracing::warn!(stream.media = %media.path, error = %e, "stream-in: a take's media did not load");
        }
        Err(e) => tracing::warn!(error = %e, "stream-in: the load task failed"),
    }
}

/// Which source to stream from even when the song is here: `peer` (the
/// engine this window drives), `share` (a share link) or `task` (the
/// library).
const STREAM_SOURCE_ENV: &str = "FTS_STREAM_SOURCE";

/// Share links to songs' sessions, whitespace-separated: `slug=url`, or a
/// bare `url` for whichever song is open (the demo streams by these).
const SHARE_LINKS_ENV: &str = "FTS_SHARE_LINKS";

/// The share link for the song `slug` in `links` (see [`SHARE_LINKS_ENV`]).
fn share_link_for(links: &str, slug: &str) -> Option<String> {
    let mut any = None;
    for entry in links.split_whitespace() {
        match entry
            .split_once('=')
            .filter(|(k, _)| !k.contains('/') && !k.contains(':'))
        {
            Some((song, url)) if song == slug => return Some(url.to_owned()),
            Some(_) => {}
            None => any = any.or_else(|| Some(entry.to_owned())),
        }
    }
    any
}

/// A song not on this machine, mirrored and streamed from the engine this
/// window drives, or else from the Task library (the song by its name).
async fn stream_from_elsewhere(remote: &str) -> Option<crate::song_stream::StreamedSong> {
    use crate::song_stream::{PeerSource, ShareSource, SongSource, TaskSource, mirror};
    let only = std::env::var(STREAM_SOURCE_ENV)
        .ok()
        .filter(|v| !v.is_empty());
    let cache = std::env::temp_dir().join("fts-stream");
    if only.as_deref().is_none_or(|o| o == "peer") {
        match PeerSource::new(remote).await {
            Ok(source) => match mirror(
                std::sync::Arc::new(source),
                crate::song_stream::Keep::Disk(cache.clone()),
            )
            .await
            {
                Ok(streamed) => return Some(streamed),
                Err(e) => {
                    tracing::info!(stream.song = remote, error = %e, "stream-in: the engine cannot send this song")
                }
            },
            Err(e) => {
                tracing::info!(stream.song = remote, error = %e, "stream-in: no engine to stream from")
            }
        }
    }
    if only.as_deref() == Some("peer") {
        return None;
    }
    let name = song_name(remote).await?;
    let slug = session_library::slugify(&name);
    let link = std::env::var(SHARE_LINKS_ENV)
        .ok()
        .and_then(|links| share_link_for(&links, &slug));
    if only.as_deref() != Some("task")
        && let Some(link) = link
    {
        match ShareSource::new(&link) {
            Ok(source) => match mirror(
                std::sync::Arc::new(source),
                crate::song_stream::Keep::Disk(cache.clone()),
            )
            .await
            {
                Ok(streamed) => return Some(streamed),
                Err(e) => {
                    tracing::warn!(stream.song = %name, error = %e, "stream-in: the share link's copy could not be mirrored")
                }
            },
            Err(e) => {
                tracing::warn!(stream.song = %name, error = %e, "stream-in: the share link is not a URL")
            }
        }
    }
    if only.as_deref() == Some("share") {
        return None;
    }
    let source: std::sync::Arc<dyn SongSource> = match TaskSource::new(&slug).await {
        Ok(source) => std::sync::Arc::new(source),
        Err(e) => {
            tracing::warn!(stream.song = %name, stream.slug = %slug, error = %e, "stream-in: this song is not here, nor in the library");
            return None;
        }
    };
    match mirror(source, crate::song_stream::Keep::Disk(cache.clone())).await {
        Ok(streamed) => Some(streamed),
        Err(e) => {
            tracing::warn!(stream.song = %name, error = %e, "stream-in: the library's copy could not be mirrored");
            None
        }
    }
}

/// The remote project's song name: its file's stem, or its name.
async fn song_name(remote: &str) -> Option<String> {
    let daw = daw::rpc::Daw::try_get()?;
    let info = daw.project(remote).await.ok()?.info().await.ok()?;
    let stem = Path::new(&info.path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned());
    Some(stem.filter(|s| !s.is_empty()).unwrap_or(info.name))
}

/// The song file on this machine for the remote's project: its own path
/// when that exists here, else the song of the same name in this machine's
/// setlist.
async fn local_song_file(remote: &str) -> Option<PathBuf> {
    // As if the song were on no disk here: stream it even on the machine
    // that has it (a test, a one-machine demo).
    if crate::collab::env_set(STREAM_SOURCE_ENV) {
        return None;
    }
    let daw = daw::rpc::Daw::try_get()?;
    let info = daw.project(remote).await.ok()?.info().await.ok()?;
    let path = PathBuf::from(&info.path);
    if !info.path.is_empty() && path.exists() {
        return Some(path);
    }
    let name = path
        .file_stem()
        .map_or_else(|| info.name.clone(), |s| s.to_string_lossy().into_owned());
    let setlist = std::env::var_os("FTS_SESSION_SETLIST")?;
    crate::setlist::read_setlist(Path::new(&setlist))
        .ok()?
        .into_iter()
        .find(|song| {
            song.file_stem()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(&name))
        })
}

#[cfg(test)]
mod tests {
    use super::share_link_for;

    #[test]
    fn a_song_takes_its_own_share_link_before_the_catch_all() {
        let links = "http://t/org/d/share/any washed=http://t/org/d/share/w";
        assert_eq!(
            share_link_for(links, "washed").as_deref(),
            Some("http://t/org/d/share/w")
        );
        assert_eq!(
            share_link_for(links, "who-else").as_deref(),
            Some("http://t/org/d/share/any")
        );
        assert_eq!(share_link_for("washed=http://t/x", "who-else"), None);
    }
}
