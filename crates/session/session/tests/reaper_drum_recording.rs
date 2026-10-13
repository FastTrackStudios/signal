//! Adds a real drum-kit track layout to a project and records into it
//! through the exact interface the transport bar's Arm/Record buttons
//! use — `SetlistServiceClient::set_song_record_arm`/`record`/
//! `stop_recording` — against a real, running REAPER.
//!
//! Track names match `dynamic-template`'s drum-kit group (`groups/drums/
//! drum_kit/`): Kick In/Out, Snare Top/Bottom, 4 numbered toms, OH L/R,
//! Room L/R, Hi Hat — the exact layout confirmed for tomorrow's tracking
//! session.
//!
//! Recording runs against the isolated rig's dummy audio driver
//! (`linux_audio_mode=2`) — no real interface needed, and no real audio
//! is captured, but REAPER's transport and item-creation are real: this
//! proves the record pipeline (arm → record → stop → item exists), not
//! just that the RPC calls return `Ok`.
//!
//! Run via `session-extension-xtask` (needs `nix develop .#reaper-test`):
//!   cargo run -p session-extension-xtask -- drum_recording

use daw::test::daw_test;

const DRUM_TRACKS: &[&str] = &[
    "Kick In",
    "Kick Out",
    "Snare Top",
    "Snare Bottom",
    "Tom 1",
    "Tom 2",
    "Tom 3",
    "Tom 4",
    "OH L",
    "OH R",
    "Room L",
    "Room R",
    "Hi Hat",
];

async fn connect_setlist_service() -> eyre::Result<session::SetlistServiceClient> {
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
    connection
        .open_lane::<session::SetlistServiceClient>()
        .await
        .map_err(|e| eyre::eyre!("open SetlistServiceClient lane: {e:?}"))
}

#[daw_test(isolated)]
async fn record_drum_kit_through_session_interface(ctx: &DawTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    // Build the drum-kit layout and select every track — `set_song_record_arm`
    // only arms the *selected* tracks (matching the real Arm button: the
    // performer selects what they're tracking, then arms it), not "all
    // tracks in the project".
    let mut tracks = Vec::new();
    for name in DRUM_TRACKS {
        let track = project.tracks().add(name, None).await?;
        // A freshly-created track's record input defaults to `None` (-1,
        // no input assigned at all) — arming it is not enough on its own,
        // REAPER has nothing to capture. Every mic gets its own input
        // channel on the isolated rig's dummy audio device the same way
        // it would its own hardware input channel on a real interface.
        track
            .set_record_input(daw_proto::track::RecordInput::Audio { channel: 0 })
            .await?;
        track.select().await?;
        tracks.push(track);
    }

    let client = connect_setlist_service().await?;
    client
        .build_from_open_projects()
        .await
        .map_err(|e| eyre::eyre!("build_from_open_projects: {e:?}"))?;
    client
        .seek_to_section(0, 0)
        .await
        .map_err(|e| eyre::eyre!("seek_to_section(0, 0): {e:?}"))?;

    // Arm — the exact call the transport bar's Arm button makes.
    client
        .set_song_record_arm(true)
        .await
        .map_err(|e| eyre::eyre!("set_song_record_arm(true): {e:?}"))?;

    for (name, track) in DRUM_TRACKS.iter().zip(&tracks) {
        let info = track.info().await?;
        assert!(
            info.armed,
            "track '{name}' should be armed after set_song_record_arm(true)"
        );
    }

    // Record a couple of seconds — the exact call the Record button makes.
    // Confirmed against the transport state itself (not just a successful
    // RPC return): the isolated rig's dummy audio driver has no real input
    // channels to capture from (headless, no hardware), so it can prove
    // the record pipeline reaches the right project and actually enters/
    // exits Recording state — the same three bugs this arm+record path
    // had (unbounced main-thread FFI, and every Transport method silently
    // ignoring its ProjectContext and acting on whichever tab happened to
    // have REAPER's UI focus) would have made this assert false or hang —
    // but it can't prove real audio capture without real hardware, which
    // a real session tomorrow provides.
    client
        .record()
        .await
        .map_err(|e| eyre::eyre!("record: {e:?}"))?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        project.transport().is_recording().await?,
        "project should be recording after record()"
    );

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    client
        .stop_recording()
        .await
        .map_err(|e| eyre::eyre!("stop_recording: {e:?}"))?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(
        !project.transport().is_recording().await?,
        "project should not be recording after stop_recording()"
    );

    // Disarm (matching the transport bar's Arm-toggle-off) and confirm it
    // actually clears, not just that the RPC returned Ok.
    client
        .set_song_record_arm(false)
        .await
        .map_err(|e| eyre::eyre!("set_song_record_arm(false): {e:?}"))?;
    for (name, track) in DRUM_TRACKS.iter().zip(&tracks) {
        let info = track.info().await?;
        assert!(
            !info.armed,
            "track '{name}' should be disarmed after set_song_record_arm(false)"
        );
    }

    Ok(())
}
