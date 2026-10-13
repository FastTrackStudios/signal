//! Proves `session-desktop`'s Recording Mode connection actually works.
//!
//! Written against `daw::test` — the backend-generic integration-test
//! framework (`#[daw_test]`/`DawTestContext`; `#[reaper_test]`/
//! `ReaperTestContext` are its older, REAPER-specific aliases) — run here
//! against the REAPER backend specifically, via `session-extension` (a
//! minimal test-only REAPER extension that mounts only session's own
//! services: `SetlistServiceImpl<daw_reaper::Reaper>` plus the mode /
//! take-ranking / record-control control surfaces — see its crate doc).
//!
//! `apps/desktop/src/reaper_engine.rs` connects to a live REAPER's DAW
//! socket and opens `SetlistServiceClient`/`SetlistServiceStreamClient`
//! lanes — the exact same thing this test does.
//!
//! Run via `session-extension-xtask`, not `cargo test` directly — it needs
//! a real REAPER with `session-extension` installed and `FTS_SOCKET`
//! pointed at it:
//!   cargo run -p session-extension-xtask

use daw::test::daw_test;
use session::SetlistServiceClient;
use session::ruler_lanes::CoreLane;
use session::services::setlist_service::SetlistServiceStreamClient;

/// Open the SetlistService lanes over the rig's DAW socket, the same way
/// `reaper_engine::connect_to` does for a real `session-desktop` process.
async fn connect_setlist_service()
-> eyre::Result<(SetlistServiceClient, SetlistServiceStreamClient)> {
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
        .open_lane::<SetlistServiceClient>()
        .await
        .map_err(|e| eyre::eyre!("open SetlistServiceClient lane: {e:?}"))?;
    let stream_client = connection
        .open_lane::<SetlistServiceStreamClient>()
        .await
        .map_err(|e| eyre::eyre!("open SetlistServiceStreamClient lane: {e:?}"))?;
    Ok((client, stream_client))
}

/// The exact sequence `reaper_engine::load_playlist` runs after opening
/// each song's `.RPP`: `build_from_open_projects` then `seek_to_section`.
/// This is the seam Recording Mode actually depends on — proving it here
/// means a broken `SetlistServiceImpl<daw_reaper::Reaper>` fails loudly in
/// CI instead of silently in front of an audience.
#[daw_test]
async fn recording_mode_builds_setlist_from_open_reaper_project(
    ctx: &DawTestContext,
) -> eyre::Result<()> {
    let (client, _stream_client) = connect_setlist_service().await?;

    client
        .build_from_open_projects()
        .await
        .map_err(|e| eyre::eyre!("build_from_open_projects: {e:?}"))?;

    let setlist = client
        .setlist()
        .await
        .map_err(|e| eyre::eyre!("setlist: {e:?}"))?;
    assert!(
        !setlist.songs.is_empty(),
        "setlist should have at least the test rig's own open project as a song"
    );

    let project_info = ctx.daw.current_project().await?.info().await?;
    assert!(
        setlist
            .songs
            .iter()
            .any(|s| s.project_guid == project_info.guid),
        "setlist should include the currently open project (guid {}); got {:?}",
        project_info.guid,
        setlist
            .songs
            .iter()
            .map(|s| (&s.name, &s.project_guid))
            .collect::<Vec<_>>()
    );

    // Recording Mode seeks to song 0 / section 0 right after building —
    // a broken seek would strand the performance view on "no active song"
    // even though the setlist itself built fine.
    client
        .seek_to_section(0, 0)
        .await
        .map_err(|e| eyre::eyre!("seek_to_section: {e:?}"))?;

    Ok(())
}

/// Seek to *every* section of *every* song and confirm the active-indices
/// cursor actually lands there. `#[daw_test(isolated)]` — this test writes
/// real regions onto its own project tab, so it must not share state with
/// the other tests in this binary.
#[daw_test(isolated)]
async fn recording_mode_can_seek_to_every_section(ctx: &DawTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let sections_lane = CoreLane::Sections.lane_index();

    // A real house-convention section layout: five 8-second sections back
    // to back, each on the Sections lane with the abbreviation
    // `parse_region_section_type` (session's own naming vocabulary)
    // recognizes — the exact shape `convert-markers`/the FTS_SESSION
    // region-insert actions produce.
    let section_names = ["IN", "VS", "CH", "BR", "OUT"];
    let section_len = 8.0;
    for (i, name) in section_names.iter().enumerate() {
        let start = i as f64 * section_len;
        let end = start + section_len;
        let id = project.regions().add(start, end, name).await?;
        project.regions().set_lane(id, Some(sections_lane)).await?;
    }

    let (client, stream_client) = connect_setlist_service().await?;

    // Subscribe to the active-indices stream *before* seeking — the same
    // ordering `SessionEventBridge` uses, so no publish in between the
    // subscribe and the first seek can be missed.
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
    let project_info = project.info().await?;
    let song_index = setlist
        .songs
        .iter()
        .position(|s| s.project_guid == project_info.guid)
        .ok_or_else(|| {
            eyre::eyre!(
                "setlist should include this test's project (guid {}); got {:?}",
                project_info.guid,
                setlist
                    .songs
                    .iter()
                    .map(|s| (&s.name, &s.project_guid))
                    .collect::<Vec<_>>()
            )
        })?;
    let song = &setlist.songs[song_index];
    assert_eq!(
        song.sections.len(),
        section_names.len(),
        "expected one section per inserted region; got {:?}",
        song.sections.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    // Seek to every section in order and confirm the active-indices cursor
    // actually lands on (song_index, section_index) each time — not just
    // that the RPC call returned Ok. A `seek_to_section` that silently
    // no-ops (e.g. `daw.select` failing on the wrong guid) would pass a
    // bare `.is_ok()` check while leaving the performance view stuck on
    // whatever section it was on before.
    for section_index in 0..song.sections.len() {
        client
            .seek_to_section(song_index, section_index)
            .await
            .map_err(|e| eyre::eyre!("seek_to_section({song_index}, {section_index}): {e:?}"))?;

        let active = wait_for_active_section(&mut active_rx, song_index, section_index).await?;
        assert_eq!(
            active.section_index,
            Some(section_index),
            "after seeking to section {section_index} ({}), active indices should report it",
            song.sections[section_index].name
        );
    }

    Ok(())
}

/// Drain the `active_indices` stream until it reports the expected
/// song/section, or time out. The seek's own `main_thread::query` bounce +
/// the subsequent `refresh_active_indices` publish are both async round
/// trips, so the cursor doesn't necessarily reflect the new position in
/// the very next message — earlier in-flight publishes can still be
/// queued.
async fn wait_for_active_section(
    rx: &mut vox::Rx<session_proto::ActiveIndices>,
    song_index: usize,
    section_index: usize,
) -> eyre::Result<session_proto::ActiveIndices> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(4);
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
