//! Proves `session-desktop --engine`'s LAN control surface actually works
//! against a real, running REAPER — not just Live Mode's in-process
//! player, and not just the unix socket `daw::test` normally connects
//! over.
//!
//! `session-extension` (this test's REAPER host) additionally binds a
//! real axum `/vox` WebSocket server on `127.0.0.1:<OS-assigned port>`,
//! serving the exact same `session::daw_services::layer_services_with_daw`
//! router it publishes over the unix socket — the identical
//! `architect::axum_ws::serve_router` path `engine_server.rs` uses. This
//! test connects a real `vox_websocket::WsLink` (native, tokio-tungstenite)
//! to it and drives the setlist through that connection exclusively.
//!
//! Run via `session-extension-xtask` (needs `nix develop .#reaper-test`):
//!   cargo run -p session-extension-xtask -- lan_engine

use daw::test::daw_test;

/// Read the LAN test server's bound port from ExtState (published by
/// `session-extension`'s `spawn_lan_test_server`) and open the
/// `SetlistService` lanes over a real WebSocket to it.
async fn connect_over_lan(
    ctx: &daw::test::DawTestContext,
) -> eyre::Result<(
    session::SetlistServiceClient,
    session::services::setlist_service::SetlistServiceStreamClient,
)> {
    let port: u16 = ctx
        .daw
        .ext_state()
        .get("FTS_SESSION_EXT", "lan_port")
        .await
        .map_err(|e| eyre::eyre!("ext_state get lan_port: {e:?}"))?
        .ok_or_else(|| {
            eyre::eyre!("FTS_SESSION_EXT/lan_port not set — did the LAN test server start?")
        })?
        .parse()
        .map_err(|e| eyre::eyre!("lan_port ExtState value not a valid port: {e}"))?;

    let url = format!("ws://127.0.0.1:{port}/vox");
    let link = vox_websocket::WsLink::connect(&url)
        .await
        .map_err(|e| eyre::eyre!("connecting to {url}: {e}"))?;
    let connection = vox::initiator_on(link)
        .establish_connection()
        .await
        .map_err(|e| eyre::eyre!("vox handshake with {url}: {e:?}"))?;
    let client = connection
        .open_lane::<session::SetlistServiceClient>()
        .await
        .map_err(|e| eyre::eyre!("open SetlistServiceClient lane over LAN: {e:?}"))?;
    let stream_client = connection
        .open_lane::<session::services::setlist_service::SetlistServiceStreamClient>()
        .await
        .map_err(|e| eyre::eyre!("open SetlistServiceStreamClient lane over LAN: {e:?}"))?;
    Ok((client, stream_client))
}

/// Drain the `active_indices` stream until it reports the expected
/// song/section, or time out. Mirrors the other REAPER tests' own helper.
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

#[daw_test(isolated)]
async fn lan_engine_drives_real_reaper_over_websocket(
    ctx: &daw::test::DawTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let sections_lane = session::ruler_lanes::CoreLane::Sections.lane_index();

    let section_names = ["IN", "VS", "CH", "BR", "OUT"];
    let section_len = 8.0;
    for (i, name) in section_names.iter().enumerate() {
        let start = i as f64 * section_len;
        let end = start + section_len;
        let id = project.regions().add(start, end, name).await?;
        project.regions().set_lane(id, Some(sections_lane)).await?;
    }

    let (client, stream_client) = connect_over_lan(ctx).await?;

    let (tx, mut active_rx) = vox::channel::<session_proto::ActiveIndices>();
    tokio::spawn(async move {
        let _ = stream_client.active_indices(tx).await;
    });

    client
        .build_from_open_projects()
        .await
        .map_err(|e| eyre::eyre!("build_from_open_projects (over LAN): {e:?}"))?;

    let setlist = client
        .setlist()
        .await
        .map_err(|e| eyre::eyre!("setlist (over LAN): {e:?}"))?;
    let project_info = project.info().await?;
    let song_index = setlist
        .songs
        .iter()
        .position(|s| s.project_guid == project_info.guid)
        .ok_or_else(|| eyre::eyre!("setlist should include this test's project"))?;
    let song = &setlist.songs[song_index];
    assert_eq!(
        song.sections.len(),
        section_names.len(),
        "expected one section per inserted region; got {:?}",
        song.sections.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    // Seek every section over the LAN connection and confirm the active-
    // indices cursor (also over the LAN connection) actually lands on it —
    // proving both directions of the real WebSocket path, not just that
    // the RPC call itself returned Ok.
    for section_index in 0..song.sections.len() {
        client
            .seek_to_section(song_index, section_index)
            .await
            .map_err(|e| {
                eyre::eyre!("seek_to_section({song_index}, {section_index}) over LAN: {e:?}")
            })?;
        let active = wait_for_active_section(&mut active_rx, song_index, section_index).await?;
        assert_eq!(
            active.section_index,
            Some(section_index),
            "after seeking to section {section_index} over LAN, active indices should report it"
        );
    }

    Ok(())
}
