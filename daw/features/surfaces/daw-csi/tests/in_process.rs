//! Full gesture → service → event-bus round trip against an
//! in-process Standalone: the exact path the hardware loop runs,
//! minus the MIDI port. Proves daw-csi works against any backend
//! reachable through `daw_control::Daw`.

use daw_csi::driver::{DriverState, execute_intent};
use daw_csi::{mcu, taper};
use daw_proto::ProjectInfo;
use daw_proto::event_bus::{BusFilter, DawEvent};
use daw_proto::track::TrackEvent;
use daw_standalone::bootstrap::build_in_process_daw;
use daw_standalone::sync::Standalone;

fn seeded() -> Standalone {
    let s = Standalone::new();
    s.seed_project(ProjectInfo {
        guid: "csi-test".into(),
        name: "csi".into(),
        path: String::new(),
    });
    s
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fader_gesture_reaches_engine_and_echoes_on_bus() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let transport = project.transport();

    // Session: two tracks.
    project.add_track("Drums", None).await?;
    project.add_track("Bass", None).await?;
    let tracks = project.tracks().all().await?;
    assert_eq!(tracks.len(), 2);

    let mut state = DriverState::with_builtin_zones(tracks, "master".into(), 1.0);
    // Builtin boots in folder mode; these are plain tracks — hop to
    // the flat list (GLOBAL VIEW).
    state.handle_midi(&[0x90, 0x33, 0x7F], 0);
    state.handle_midi(&[0x90, 0x33, 0x00], 50);

    // Subscribe the bus BEFORE the gesture so the echo is observable.
    //
    // This used to need a `bus_attached` helper polling the hub's
    // subscriber count, because `subscribe` returned before the server
    // had the sink and anything published in the gap went to nobody.
    // `subscribe` now waits for the backend's `Attached` marker, which
    // the attach itself puts at the front of the mailbox — so being
    // here means the sink is attached, for a remote client as much as
    // for this in-process one. See session#85.
    let mut bus = bundle
        .daw
        .events()
        .subscribe(BusFilter {
            tracks: true,
            ..Default::default()
        })
        .await?;

    // Fader strip 0 to the unity mark.
    let raw = mcu::encode_fader(0, taper::volume_to_fader(0.5));
    let intents = state.handle_midi(&raw, 0);
    assert_eq!(intents.len(), 1);
    for intent in intents {
        execute_intent(&project, &transport, intent).await?;
    }

    // Engine state changed…
    let after = project.tracks().all().await?;
    let drums = after.iter().find(|t| t.name == "Drums").unwrap();
    assert!(
        (drums.volume - 0.5).abs() < 1e-2,
        "engine volume {} != gesture 0.5",
        drums.volume
    );

    // …and the echo event arrives on the bus.
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), bus.recv())
        .await
        .expect("bus echo timed out")?
        .expect("bus closed");
    let mut got = None;
    let _ = event.map(|e| got = Some(e));
    let Some(DawEvent::Track(te)) = got else {
        panic!("expected a track event");
    };
    assert!(matches!(te.event, TrackEvent::VolumeChanged { .. }));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn buttons_toggle_engine_state() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let transport = project.transport();

    project.add_track("Vox", None).await?;
    let tracks = project.tracks().all().await?;
    let mut state = DriverState::with_builtin_zones(tracks, "master".into(), 1.0);
    state.handle_midi(&[0x90, 0x33, 0x7F], 0); // → flat track list
    state.handle_midi(&[0x90, 0x33, 0x00], 50);

    // Mute strip 0 via the surface.
    for intent in state.handle_midi(&[0x90, 0x10, 0x7F], 0) {
        execute_intent(&project, &transport, intent).await?;
    }
    let t = &project.tracks().all().await?[0];
    assert!(t.muted, "mute button didn't reach the engine");

    // Transport: play.
    for intent in state.handle_midi(&[0x90, 0x5E, 0x7F], 0) {
        execute_intent(&project, &transport, intent).await?;
    }
    assert!(transport.is_playing().await?);

    // Stop.
    for intent in state.handle_midi(&[0x90, 0x5D, 0x7F], 0) {
        execute_intent(&project, &transport, intent).await?;
    }
    assert!(!transport.is_playing().await?);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn event_echo_drives_motor_fader_feedback() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;

    project.add_track("Keys", None).await?;
    let tracks = project.tracks().all().await?;
    let guid = tracks[0].guid.clone();
    let mut state = DriverState::with_builtin_zones(tracks, "master".into(), 1.0);
    state.handle_midi(&[0x90, 0x33, 0x7F], 0); // → flat track list
    state.handle_midi(&[0x90, 0x33, 0x00], 50);
    let _ = state.render(); // settle the shadow

    // Someone moves the fader in the UI → service → bus event.
    let handle = project.tracks().by_guid(&guid).await?.unwrap();
    handle.set_volume(0.25).await?;

    // The driver applies the event and the motor fader follows.
    state.apply_track_event(&TrackEvent::VolumeChanged { guid, volume: 0.25 });
    let msgs = state.render();
    let expected = taper::volume_to_fader(0.25);
    assert!(
        msgs.iter()
            .any(|m| m[0] == 0xE0 && (((m[2] as u16) << 7) | m[1] as u16) == expected),
        "no motor fader message for the echoed volume: {msgs:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fader_writes_automation_in_write_mode() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let transport = project.transport();

    let track = project.add_track("Keys", None).await?;
    track.select_exclusive().await?;
    // WRITE mode + rolling transport = every fader move records.
    track
        .set_automation_mode(daw_proto::AutomationMode::Write)
        .await?;
    transport.play().await?;
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;

    let tracks = project.tracks().all().await?;
    assert_eq!(
        tracks[0].automation_mode,
        daw_proto::AutomationMode::Write,
        "mode should round-trip"
    );
    let mut state = DriverState::with_builtin_zones(tracks, "master".into(), 1.0);
    state.handle_midi(&[0x90, 0x33, 0x7F], 0); // → flat track list
    state.handle_midi(&[0x90, 0x33, 0x00], 50);

    // Ride the fader: a few moves while playing.
    for (i, pos) in [4000u16, 8000, 12000].into_iter().enumerate() {
        for intent in state.handle_midi(&mcu::encode_fader(0, pos), 100 + i as u64 * 50) {
            execute_intent(&project, &transport, intent).await?;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    transport.stop().await?;

    // The volume envelope now carries recorded points.
    let env = track
        .envelopes()
        .by_type(daw_proto::EnvelopeType::Volume)
        .await?
        .expect("volume envelope should exist after writing");
    let points = env.points().await?;
    assert!(
        !points.is_empty(),
        "write-mode fader moves should record envelope points"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn metronome_toggle_round_trips_and_clicks() -> eyre::Result<()> {
    use daw_standalone::audio_engine::render::ProjectRenderer;

    let standalone = seeded();
    let bundle = build_in_process_daw(standalone).await?;
    let project = bundle.daw.current_project().await?;
    let transport = project.transport();

    assert!(!transport.metronome_enabled().await?);
    transport.set_metronome(true).await?;
    assert!(transport.metronome_enabled().await?);

    // The renderer overlays clicks on the beat grid — block at t=0
    // contains the downbeat burst.
    let r = ProjectRenderer::new(&bundle.standalone, "csi-test", 48_000);
    let block = r.render_block(0, 2048);
    let energy: f64 = block.samples.iter().map(|s| (*s as f64).abs()).sum();
    assert!(energy > 0.0, "metronome should be audible at the downbeat");

    transport.set_metronome(false).await?;
    let r2 = ProjectRenderer::new(&bundle.standalone, "csi-test", 48_000);
    let block = r2.render_block(0, 2048);
    let energy: f64 = block.samples.iter().map(|s| (*s as f64).abs()).sum();
    assert!(energy == 0.0, "click must stop when the metronome is off");
    Ok(())
}
