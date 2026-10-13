//! Opens all 10 of the Rockstars album's real, organized `.RPP` projects in
//! one REAPER instance and proves `SetlistService` builds a real 10-song
//! setlist from them — the actual thing Recording Mode's `load_playlist`
//! does with a real album, not a synthetic single project.
//!
//! Needs `--virtual` for REAPER's own startup dialogs (no audio device on an
//! Xvfb host), which `TestRunner::run_reaper_tests` dismisses in the
//! background — see `VirtualDisplay::close_stray_dialogs`.
//!
//! These projects used to trip a "Project Load Warning" modal on open —
//! "11160 elements in the project were not understood", `LANE` and a wall of
//! base64. That was long read as REAPER's flake version not recognizing a
//! newer build's fixed-lane/comping state. It was not: `dynamic-template
//! --apply-buses` was re-serializing each project through a *typed* model
//! (`ReaperProject::to_rpp_string`), which could only write back the fields it
//! modelled and dropped the rest — the master track, `<RECORD_CFG>`, every
//! `RENDER_*`, per-item `CHANMODE`/`YPOS`, all the `<EXT>` blocks — while
//! duplicating every take and emitting a `LANE` token REAPER has never had.
//! The organize pass now edits the raw chunk tree instead (`apply::chunk`), so
//! untouched lines come back unchanged and the modal is gone. If it ever
//! returns, the pipeline has regressed — do not just dismiss it.
//!
//! Run via `session-extension-xtask` (needs `nix develop .#reaper-test`):
//!   cargo run -p session-extension-xtask -- --virtual rockstars

use daw::test::daw_test;

const ALBUM_ROOT: &str = "/run/media/AudioHaven/Project/Crescendum-Rockstars";

/// (folder name, `.organized.RPP` file name) — matches the on-disk layout
/// exactly (including "Kornesque "'s trailing space).
const SONGS: &[(&str, &str)] = &[
    ("Chained expectations", "Chained expectations.organized.RPP"),
    ("empty focus", "empty focus.organized.RPP"),
    ("heavify", "heavify.organized.RPP"),
    ("Intro", "Intro.organized.RPP"),
    ("it knows my name", "it knows my name.organized.RPP"),
    ("Kornesque ", "Kornesque .organized.RPP"),
    ("set in stone", "set in stone.organized.RPP"),
    ("The ballad", "The ballad.organized.RPP"),
    ("the plague", "the plague.organized.RPP"),
    ("unbreakable", "unbreakable.organized.RPP"),
];

async fn connect_setlist_service() -> eyre::Result<(
    session::SetlistServiceClient,
    session::services::setlist_service::SetlistServiceStreamClient,
)> {
    let socket = std::env::var("FTS_SOCKET").map_err(|_| {
        eyre::eyre!(
            "FTS_SOCKET not set — this test must run under session-extension-xtask, \
             not `cargo test` directly"
        )
    })?;
    let stream = tokio::net::UnixStream::connect(&socket)
        .await
        .map_err(|e| eyre::eyre!("connecting to {socket}: {e}"))?;
    let link = vox_stream::StreamLink::unix(stream);
    let connection = vox::initiator_on(link)
        .establish_connection()
        .await
        .map_err(|e| eyre::eyre!("vox handshake with {socket}: {e:?}"))?;
    let client = connection
        .open_lane::<session::SetlistServiceClient>()
        .await
        .map_err(|e| eyre::eyre!("open SetlistServiceClient lane: {e:?}"))?;
    let stream_client = connection
        .open_lane::<session::services::setlist_service::SetlistServiceStreamClient>()
        .await
        .map_err(|e| eyre::eyre!("open SetlistServiceStreamClient lane: {e:?}"))?;
    Ok((client, stream_client))
}

/// Drain the `active_indices` stream until it reports the expected
/// song/section, or time out. Mirrors `reaper_setlist_recording_mode.rs`'s
/// own helper — the seek's `main_thread::query` bounce and the follow-up
/// `refresh_active_indices` publish are both async round trips, so the
/// cursor doesn't necessarily reflect the new position in the very next
/// message when earlier in-flight publishes are still queued.
async fn wait_for_active_section(
    rx: &mut vox::Rx<session_proto::ActiveIndices>,
    song_index: usize,
    section_index: usize,
) -> eyre::Result<session_proto::ActiveIndices> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(eyre::eyre!(
                "active indices never reported song {song_index} / section {section_index}"
            ));
        }
        let Ok(Ok(Some(active_ref))) = tokio::time::timeout(remaining, rx.recv()).await else {
            return Err(eyre::eyre!(
                "active indices stream ended waiting for song {song_index} / section {section_index}"
            ));
        };
        let active = active_ref.get().clone();
        if active.song_index == Some(song_index) && active.section_index == Some(section_index) {
            return Ok(active);
        }
    }
}

#[daw_test]
async fn rockstars_album_opens_all_ten_songs(ctx: &daw::test::DawTestContext) -> eyre::Result<()> {
    for (folder, file) in SONGS {
        let path = format!("{ALBUM_ROOT}/{folder}/{file}");
        if !std::path::Path::new(&path).exists() {
            return Err(eyre::eyre!(
                "expected album file missing: {path} — is /run/media/AudioHaven mounted?"
            ));
        }
        println!("opening {path} …");
        let started = std::time::Instant::now();
        let project = ctx
            .daw
            .open_project(path.clone())
            .await
            .map_err(|e| eyre::eyre!("opening {path}: {e:?}"))?;
        println!("opened {path} in {:.1}s", started.elapsed().as_secs_f64());

        // Confirm per-track state (the mixer/record-arm status view's data
        // source) is reachable for a real, fully-loaded multitrack
        // project — not just the synthetic single-track fixture the other
        // REAPER test uses. Checked per-song, right after opening it,
        // rather than via `current_project()` later: which project counts
        // as "current" after several more opens and seeks is REAPER's own
        // focus state, not something this test controls.
        let tracks = project.tracks().all().await?;
        assert!(
            !tracks.is_empty(),
            "expected real tracks for {path}, got none"
        );
    }

    let (client, stream_client) = connect_setlist_service().await?;

    // Subscribe before building/seeking, the same ordering
    // `SessionEventBridge` uses, so no publish in between subscribing and
    // the first seek can be missed.
    let (tx, mut active_rx) = vox::channel::<session_proto::ActiveIndices>();
    tokio::spawn(async move {
        let _ = stream_client.active_indices(tx).await;
    });

    client
        .build_from_open_projects()
        .await
        .map_err(|e| eyre::eyre!("build_from_open_projects: {e:?}"))?;

    let setlist = client
        .setlist()
        .await
        .map_err(|e| eyre::eyre!("setlist: {e:?}"))?;

    let names: Vec<_> = setlist.songs.iter().map(|s| s.name.clone()).collect();

    // REAPER always starts with its own blank "Untitled" tab, and the
    // `#[daw_test]` harness's own default tab is another — both are real
    // open projects `build_from_open_projects` correctly includes, not a
    // bug. Filter them out rather than asserting on a count Recording
    // Mode's own `load_playlist` never has to deal with (it never opens
    // an album into a pre-existing blank tab).
    let album_songs: Vec<_> = setlist
        .songs
        .iter()
        .filter(|s| s.name != "Untitled")
        .collect();
    assert_eq!(
        album_songs.len(),
        SONGS.len(),
        "expected all {} Rockstars songs open as one setlist, got {}: {names:?}",
        SONGS.len(),
        album_songs.len(),
    );

    drop(album_songs);

    // Sections hydrate lazily per song — matching real Recording Mode's
    // `load_playlist`, which only seeks the first song after building —
    // so page through every song's first section the way a performer
    // would, then confirm SetlistService now reports it (this is the
    // exact structure the performance view reads from). "empty focus" has
    // no sections at all (a real gap in its source markers, not a bug —
    // confirmed separately via report-sections), so it's skipped.
    for (index, song) in setlist.songs.iter().enumerate() {
        if song.name == "Untitled" || song.name.starts_with("empty focus") {
            continue;
        }
        client
            .seek_to_section(index, 0)
            .await
            .map_err(|e| eyre::eyre!("seek_to_section({index}, 0) for '{}': {e:?}", song.name))?;
    }

    let setlist = client
        .setlist()
        .await
        .map_err(|e| eyre::eyre!("setlist (post-seek): {e:?}"))?;
    for song in &setlist.songs {
        if song.name == "Untitled" || song.name.starts_with("empty focus") {
            continue;
        }
        assert!(
            !song.sections.is_empty(),
            "song '{}' should have sections after seeking to it but has none",
            song.name
        );
    }

    // Seek to *every* section of *every* song, in setlist order — moving
    // between songs means REAPER switching its focused project tab
    // between each one, not just moving the cursor within one project, so
    // this exercises cross-project navigation the same way a performer
    // paging through the whole album would. Each seek is confirmed
    // against the live `active_indices` stream, not just a successful RPC
    // return — a `seek_to_section` that silently no-ops on the wrong
    // project/song would pass a bare `.is_ok()` check while leaving the
    // performance view stuck on whatever it showed before.
    for (song_index, song) in setlist.songs.iter().enumerate() {
        if song.name == "Untitled" || song.name.starts_with("empty focus") {
            continue;
        }
        for section_index in 0..song.sections.len() {
            client
                .seek_to_section(song_index, section_index)
                .await
                .map_err(|e| {
                    eyre::eyre!(
                        "seek_to_section({song_index}, {section_index}) for '{}': {e:?}",
                        song.name
                    )
                })?;
            let active = wait_for_active_section(&mut active_rx, song_index, section_index).await?;
            assert_eq!(
                active.section_index,
                Some(section_index),
                "after seeking '{}' to section {section_index} ({}), active indices should report it",
                song.name,
                song.sections[section_index].name,
            );
        }
        println!(
            "seeked through all {} sections of '{}'",
            song.sections.len(),
            song.name
        );
    }

    Ok(())
}
