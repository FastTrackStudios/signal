//! The panel, attached to a REAPER it did not start.
//!
//! Everything else in this repo tests the window against a project it
//! owns, or against events handed to `apply_event` by hand. Both are
//! worth having and neither can answer the only question that matters
//! for a control surface: does an edit made HERE reach REAPER, and does
//! a change made THERE come back?
//!
//! So this attaches the way the window attaches — `attach_to_reaper`
//! discovering the host's socket, the global facade installed, the same
//! `Applier` and the same `Watch` — rather than reaching in through the
//! handle the harness already holds. A test that used the harness's
//! handle would prove the services work and say nothing about whether
//! the window can find a DAW it did not start.
//!
//! Run with: `just reaper-test` (see `session-reaper-xtask`).
//!
//! # What the runner says afterwards
//!
//! The scenarios take about four seconds; the run is then killed at the
//! timeout and reported as "Some tests failed", and neither of those is
//! about the tests. The harness's teardown calls `close_project`, and
//! that never returns while this window holds its subscriptions open —
//! daw#31. Read the `test result:` line and the per-scenario timings
//! below it; they are the ones that mean something until that is
//! fixed.

#![cfg(feature = "reaper-tests")]

use daw::test::reaper_test;
use daw_proto::track::TrackEvent;
use session_daw::engine::{Applied, Applier, Edit, Watch, apply_event};

/// How long to give the host's 30 Hz poller.
///
/// Generous: what is being measured is whether an event is produced at
/// all, and a tight bound turns a busy machine into a failure that
/// reads like a missing feature.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(5);

/// The project the WINDOW is looking at.
///
/// Not `ctx.project()`, which is the tab the harness made for this
/// test. The window attaches and follows REAPER's CURRENT project —
/// that is what a control surface does, and its `Watch` filters the
/// stream to that project's guid — so a test driving some other tab
/// would make edits the window is right to ignore, and then fail for
/// the wrong reason.
async fn window_project() -> eyre::Result<daw::rpc::Project> {
    let daw = daw::rpc::Daw::try_get().ok_or_else(|| eyre::eyre!("the facade is not up"))?;
    Ok(daw.current_project().await?)
}

/// Attach the window's own facade to whatever REAPER is running.
///
/// Idempotent across tests in one binary: they share a process, so the
/// second call finds the facade already installed and that is fine —
/// it is the same REAPER.
/// Run on a thread of its own, because the window attaches with a
/// `block_on` and this test is already inside a runtime — nesting one
/// runtime in another panics. A window has no runtime around it when it
/// attaches, so this is the test accommodating the test harness rather
/// than the window doing anything unusual.
fn attach() -> eyre::Result<()> {
    if daw::rpc::Daw::try_get().is_some() {
        return Ok(());
    }
    std::thread::spawn(|| {
        session_daw::open::attach_to_reaper(None).map(|attached| {
            println!(
                "attached to {:?} ({} tracks)",
                attached.name, attached.track_count
            );
        })
    })
    .join()
    .map_err(|_| eyre::eyre!("the attach thread panicked"))?
    .map_err(|e| eyre::eyre!("the window could not attach: {e}"))
}

/// Drain the window's watch until it sees what it is waiting for.
///
/// Polled rather than awaited because `Watch` hands the window a
/// non-blocking drain, which is what a frame wants: the window asks
/// once per frame and applies whatever arrived.
fn watch_for(
    watch: &Watch,
    tracks: &mut Vec<daw_proto::Track>,
    mut wanted: impl FnMut(&TrackEvent) -> bool,
) -> eyre::Result<TrackEvent> {
    let deadline = std::time::Instant::now() + PATIENCE;
    loop {
        for event in watch.drain() {
            // Applied through the window's own applier, so what this
            // asserts is the list the window would DRAW, not the event
            // that happened to arrive.
            let _ = apply_event(tracks, &event);
            if wanted(&event) {
                return Ok(event);
            }
        }
        if std::time::Instant::now() > deadline {
            eyre::bail!("the window saw no matching event within {PATIENCE:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Wait until the host's poller has seeded its cache for this project.
///
/// The cache is seeded the first tick that finds a subscriber, and
/// everything already true then arrives as `Added` rather than as a
/// field change. A window never notices, because it subscribes at
/// startup and the edits come later; a test makes its edit immediately,
/// so it has to wait for the same thing to be true or the edit is
/// folded into the snapshot that was meant to precede it.
fn settle(watch: &Watch, tracks: &mut Vec<daw_proto::Track>, guid: &str) -> eyre::Result<()> {
    watch_for(
        &watch,
        tracks,
        |event| matches!(event, TrackEvent::Added(track) if track.guid == guid),
    )?;
    Ok(())
}

/// The window, attached to a REAPER it did not start.
///
/// **One test, several scenarios**, and that is a finding rather than a
/// style choice: a second subscription to the same service on the same
/// connection is silently refused. The host's subscriber count stays at
/// one however many `Watch`es are started, and every one after the
/// first sits there receiving nothing. A window makes exactly one and
/// never notices. A file with one test per scenario makes one each, and
/// every scenario after the first watches a dead stream — which passes
/// or fails for reasons that have nothing to do with its subject.
///
/// So this is one window: it attaches once, watches once, edits through
/// one applier, and the scenarios run in the order a session does them.
#[reaper_test(isolated)]
async fn the_window_is_a_control_surface_over_reaper(
    _ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    attach()?;
    let project = window_project().await?;

    // Watching FIRST, which is the order a window has: it attaches, it
    // subscribes, and then things happen. Creating a track before
    // subscribing races the host's 40 ms tick — the `Added` can be
    // published before the subscription exists, and it is never
    // published again.
    let mut tracks: Vec<daw_proto::Track> = project.tracks().all().await?;
    let watch = Watch::start().ok_or_else(|| eyre::eyre!("the window could not watch"))?;
    let applier = Applier::start().ok_or_else(|| eyre::eyre!("the window could not edit"))?;

    // Timed, and printed: this suite shares one REAPER and one
    // timeout, so a scenario that quietly grows into thirty seconds
    // takes the whole file over the edge — and the failure it causes
    // lands on whichever scenario happened to be last.
    let mut at = std::time::Instant::now();
    let mut lap = |what: &str, at: &mut std::time::Instant| {
        println!("  [{what}] {:?}", at.elapsed());
        *at = std::time::Instant::now();
    };
    drives_and_hears_back(&project, &watch, &applier, &mut tracks).await?;
    lap("drives and hears back", &mut at);
    every_parameter_round_trips(&project, &watch, &applier, &mut tracks).await?;
    lap("every parameter", &mut at);
    tempo_mapping_moves_a_bar_line(&project, &applier).await?;
    lap("tempo mapping", &mut at);
    sends_reach_reaper_and_come_back(&project, &applier).await?;
    lap("sends", &mut at);
    Ok(())
}

/// A send, made and changed and taken away by the window.
///
/// The panel's arithmetic is tested on its own; this is the half that
/// cannot be — that the send the window asks for is a send REAPER
/// makes, that the number it then addresses is the one REAPER gave it,
/// and that the far end sees a receive. All four are things a panel
/// gets silently wrong against a mock.
async fn sends_reach_reaper_and_come_back(
    project: &daw::rpc::Project,
    applier: &Applier,
) -> eyre::Result<()> {
    let tracks = project.tracks().all().await?;
    let (Some(from), Some(to)) = (tracks.first(), tracks.get(1)) else {
        eyre::bail!(
            "a send needs two tracks and this project has {}",
            tracks.len()
        );
    };
    let (source, dest) = (from.guid.clone(), to.guid.clone());

    // Waits on REAPER's answer rather than on a sleep: the applier's
    // worker and the host's tick are both asynchronous to this thread.
    let sends = async |guid: &str| -> eyre::Result<Vec<daw_proto::routing::TrackRoute>> {
        let Some(track) = project.tracks().by_guid(guid).await? else {
            return Ok(Vec::new());
        };
        Ok(track.sends().all().await?)
    };
    let settle = async |want: &dyn Fn(&[daw_proto::routing::TrackRoute]) -> bool,
                        what: &str|
           -> eyre::Result<Vec<daw_proto::routing::TrackRoute>> {
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            let routes = sends(&source).await?;
            if want(&routes) {
                return Ok(routes);
            }
            if std::time::Instant::now() > deadline {
                eyre::bail!("REAPER never {what} within {PATIENCE:?}: {routes:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    };

    applier.send(Edit::AddSend(source.clone(), dest.clone()));
    let made = settle(
        &|routes| {
            routes
                .iter()
                .any(|route| route.dest_track_guid.as_deref() == Some(dest.as_str()))
        },
        "made the send",
    )
    .await?;
    // The number REAPER gave it — not one the window chose, which is
    // the whole reason the panel reads the list back after adding.
    let number = made
        .iter()
        .find(|route| route.dest_track_guid.as_deref() == Some(dest.as_str()))
        .map(|route| route.index)
        .ok_or_else(|| eyre::eyre!("the send went missing between two reads"))?;

    // The far end has a receive, and the panel can name who is feeding
    // it whichever field this backend put the partner in.
    let Some(far) = project.tracks().by_guid(&dest).await? else {
        eyre::bail!("the destination went away");
    };
    let receives = far.receives().all().await?;
    assert!(
        !receives.is_empty(),
        "the destination has no receive for a send that exists"
    );
    assert_eq!(
        receives
            .first()
            .and_then(|route| session_daw::routes::partner(route, &dest)),
        Some(source.as_str()),
        "the receive does not name the track feeding it"
    );

    let send = session_daw::routes::At::new(daw_proto::routing::RouteType::Send, number);
    applier.send(Edit::SetRouteVolume(source.clone(), send, 0.5));
    settle(
        &|routes| {
            routes
                .iter()
                .any(|route| route.index == number && (route.volume - 0.5).abs() < 0.001)
        },
        "took the level",
    )
    .await?;

    // A receive is the SAME object as the send feeding it, so editing
    // it on the destination moves the send on the source. That is the
    // claim the panel makes by letting a receive be dragged at all, and
    // it is only true because REAPER keeps one route and lists it at
    // both ends — a mock would let it be false.
    let receive = session_daw::routes::At::new(daw_proto::routing::RouteType::Receive, 0);
    applier.send(Edit::SetRouteVolume(dest.clone(), receive, 0.25));
    settle(
        &|routes| {
            routes
                .iter()
                .any(|route| route.index == number && (route.volume - 0.25).abs() < 0.001)
        },
        "moved the send from its receive",
    )
    .await?;

    applier.send(Edit::SetRouteMute(source.clone(), send, true));
    settle(
        &|routes| {
            routes
                .iter()
                .any(|route| route.index == number && route.muted)
        },
        "took the mute",
    )
    .await?;

    applier.send(Edit::SetSendMode(
        source.clone(),
        number,
        daw_proto::routing::SendMode::PreFx,
    ));
    settle(
        &|routes| {
            routes.iter().any(|route| {
                route.index == number && route.send_mode == daw_proto::routing::SendMode::PreFx
            })
        },
        "took the mode",
    )
    .await?;

    applier.send(Edit::RemoveRoute(source.clone(), send));
    settle(
        &|routes| {
            !routes
                .iter()
                .any(|route| route.dest_track_guid.as_deref() == Some(dest.as_str()))
        },
        "removed the send",
    )
    .await?;
    Ok(())
}

/// Tempo mapping, end to end against a real REAPER.
///
/// The arithmetic is tested on its own; this is the half that cannot
/// be: that the tempo the window writes is the tempo REAPER ends up
/// with, and that the bar line therefore lands where the click was.
///
/// Checked by asking REAPER where the bar is afterwards rather than by
/// trusting the number we sent. A tempo write that silently landed on
/// the wrong marker would pass every test that only reads back the
/// tempo.
async fn tempo_mapping_moves_a_bar_line(
    project: &daw::rpc::Project,
    applier: &Applier,
) -> eyre::Result<()> {
    let map = project.tempo_map();
    let before = map.points().await?.len();

    // Halve the tempo at the start: bar two should move from two
    // seconds to four.
    applier.send(Edit::SetTempo(String::new(), 0.0, 60.0));

    let deadline = std::time::Instant::now() + PATIENCE;
    let mut tempo = 0.0;
    while std::time::Instant::now() < deadline {
        tempo = map.tempo_at(1.0).await.unwrap_or(0.0);
        if (tempo - 60.0).abs() < 1e-6 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        (tempo - 60.0).abs() < 1e-6,
        "REAPER is at {tempo}, not the 60 the window wrote"
    );

    // And where the bar line actually is, which is the point of the
    // whole exercise.
    let bar_two = map.musical_to_time(2, 1, 0.0).await?;
    assert!(
        (bar_two - 4.0).abs() < 0.01,
        "bar two is at {}s; at 60 bpm in four four it should be 4",
        bar_two
    );

    // Writing the same point again corrects it rather than adding a
    // second marker beside it — a bar with two tempos is a bar with
    // none, and tempo mapping is a hundred small corrections.
    applier.send(Edit::SetTempo(String::new(), 0.0, 90.0));
    let deadline = std::time::Instant::now() + PATIENCE;
    while std::time::Instant::now() < deadline {
        if (map.tempo_at(1.0).await.unwrap_or(0.0) - 90.0).abs() < 1e-6 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(
        map.points().await?.len(),
        before.max(1),
        "correcting a tempo added a marker instead of moving one"
    );
    Ok(())
}

/// The window, driving a REAPER it did not start.
///
/// **One test, not three**, and that is a finding rather than a style
/// choice: a second subscription to the same service on the same
/// connection is silently refused. The subscriber count on the host
/// stays at one however many `Watch`es are started, and every watch
/// after the first sits there receiving nothing. A window makes exactly
/// one and never notices; a test file with three tests makes three, and
/// two of them are testing a dead stream.
///
/// So this is one window, watching once, doing three things in the
/// order a session does them.
async fn drives_and_hears_back(
    project: &daw::rpc::Project,
    watch: &Watch,
    applier: &Applier,
    tracks: &mut Vec<daw_proto::Track>,
) -> eyre::Result<()> {
    // ── A track appearing in REAPER changes the window's LIST ────────
    //
    // The distinction is load-bearing: a field event repaints one row,
    // where the list changing invalidates every row map and the scene
    // resolved against it. A window told the wrong one draws the right
    // value in the wrong place.
    let before = tracks.len();
    let kick = project.tracks().add("Panel Kick", None).await?;
    let guid = kick.guid().to_owned();

    let deadline = std::time::Instant::now() + PATIENCE;
    let mut structural = false;
    while std::time::Instant::now() < deadline && !structural {
        for event in watch.drain() {
            if apply_event(tracks, &event) == Applied::Structure
                && matches!(&event, TrackEvent::Added(track) if track.guid == guid)
            {
                structural = true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(structural, "the window was never told the list changed");
    assert_eq!(tracks.len(), before + 1, "the window's list did not grow");

    // ── An edit made in the window reaches REAPER ────────────────────
    //
    // Through the window's own applier — the queue, the worker thread,
    // the coalescing — exactly as a click does.
    applier.send(Edit::ToggleMute(guid.clone()));
    let seen = watch_for(watch, tracks, |event| {
        matches!(event, TrackEvent::MuteChanged { muted: true, .. })
    })?;
    let TrackEvent::MuteChanged { guid: muted, .. } = &seen else {
        eyre::bail!("not a mute event");
    };
    assert_eq!(muted, &guid, "REAPER muted a different track");

    // The event is one claim and the DAW's own answer is another. A
    // window that believed only the echo of its own edit would look
    // perfect right up until something rejected one.
    assert!(
        kick.info().await?.muted,
        "the event said muted and the DAW says otherwise"
    );
    assert!(
        tracks
            .iter()
            .find(|t| t.guid == guid)
            .is_some_and(|t| t.muted),
        "the window applied the event to the wrong track, or not at all"
    );

    // ── A change made in REAPER, by nobody the window knows ──────────
    //
    // This is the half a window cannot fake. Its own edits it predicts;
    // this one it can only be told about, and being told is the whole
    // difference between a control surface and a remote control.
    kick.set_volume(0.25).await?;
    watch_for(
        &watch,
        tracks,
        |event| matches!(event, TrackEvent::VolumeChanged { volume, .. } if (volume - 0.25).abs() < 1e-6),
    )?;
    let shown = tracks
        .iter()
        .find(|t| t.guid == guid)
        .map(|t| t.volume)
        .ok_or_else(|| eyre::eyre!("the window lost the track"))?;
    assert!(
        (shown - 0.25).abs() < 1e-6,
        "the window is drawing {shown}, REAPER is at 0.25"
    );

    Ok(())
}

/// Every track parameter the window can set, set through the window.
///
/// One test again, for the subscription reason above. It walks the
/// whole surface rather than sampling it, because "most parameters
/// sync" is the state this work started in and the state that is
/// indistinguishable, from the outside, from all of them syncing.
///
/// Each one is asserted twice: the event says what happened, and the
/// DAW is asked afterwards. The event alone would pass if the window
/// were talking to itself; the read alone would pass if nothing were
/// ever announced and the window redrew from a poll.
async fn every_parameter_round_trips(
    project: &daw::rpc::Project,
    watch: &Watch,
    applier: &Applier,
    tracks: &mut Vec<daw_proto::Track>,
) -> eyre::Result<()> {
    use daw_proto::primitives::AutomationMode;
    use daw_proto::track::{GroupFamily, GroupRole, InputMonitoringMode, RecordInput};

    let track = project.tracks().add("Every Parameter", None).await?;
    let guid = track.guid().to_owned();
    settle(watch, tracks, &guid)?;

    // Levels and the name — the ones that always worked, here as the
    // control: if these fail, the harness is wrong rather than the
    // parameter.
    applier.send(Edit::Rename(guid.clone(), "Renamed By Window".into()));
    watch_for(
        &watch,
        tracks,
        |e| matches!(e, TrackEvent::Renamed { name, .. } if name == "Renamed By Window"),
    )?;
    applier.send(Edit::SetVolume(guid.clone(), 0.5));
    watch_for(
        &watch,
        tracks,
        |e| matches!(e, TrackEvent::VolumeChanged { volume, .. } if (volume - 0.5).abs() < 1e-6),
    )?;
    applier.send(Edit::SetPan(guid.clone(), -0.5));
    watch_for(
        &watch,
        tracks,
        |e| matches!(e, TrackEvent::PanChanged { pan, .. } if (pan + 0.5).abs() < 1e-6),
    )?;

    // The toggles.
    applier.send(Edit::ToggleSolo(guid.clone()));
    watch_for(watch, tracks, |e| {
        matches!(e, TrackEvent::SoloChanged { soloed: true, .. })
    })?;
    applier.send(Edit::ToggleArm(guid.clone()));
    watch_for(watch, tracks, |e| {
        matches!(e, TrackEvent::ArmChanged { armed: true, .. })
    })?;

    // The signal-path ones: polarity changes what the track SOUNDS
    // like without changing a level, and monitoring changes what the
    // performer hears.
    applier.send(Edit::SetPhase(guid.clone(), true));
    watch_for(watch, tracks, |e| {
        matches!(e, TrackEvent::PhaseInvertedChanged { inverted: true, .. })
    })?;
    applier.send(Edit::SetInputMonitor(
        guid.clone(),
        InputMonitoringMode::NotWhenPlaying,
    ));
    watch_for(watch, tracks, |e| {
        matches!(
            e,
            TrackEvent::InputMonitorChanged {
                monitor: InputMonitoringMode::NotWhenPlaying,
                ..
            }
        )
    })?;
    applier.send(Edit::SetRecordInput(
        guid.clone(),
        RecordInput::Audio { channel: 3 },
    ));
    watch_for(watch, tracks, |e| {
        matches!(
            e,
            TrackEvent::RecordInputChanged {
                input: RecordInput::Audio { channel: 3 },
                ..
            }
        )
    })?;
    applier.send(Edit::SetParentSend(guid.clone(), false));
    watch_for(watch, tracks, |e| {
        matches!(e, TrackEvent::ParentSendChanged { enabled: false, .. })
    })?;
    applier.send(Edit::SetAutomationMode(guid.clone(), AutomationMode::Latch));
    watch_for(watch, tracks, |e| {
        matches!(
            e,
            TrackEvent::AutomationModeChanged {
                mode: AutomationMode::Latch,
                ..
            }
        )
    })?;

    // The view ones, which live in the PROJECT and so are everybody's.
    applier.send(Edit::SetColor(guid.clone(), 0x00_44_88));
    watch_for(
        &watch,
        tracks,
        |e| matches!(e, TrackEvent::ColorChanged { color: Some(c), .. } if *c == 0x00_44_88),
    )?;
    applier.send(Edit::SetHeight(guid.clone(), 96));
    watch_for(watch, tracks, |e| {
        matches!(
            e,
            TrackEvent::HeightChanged {
                height: Some(96),
                ..
            }
        )
    })?;

    // Grouping: what a VCA actually is.
    applier.send(Edit::SetGroupFlags(
        guid.clone(),
        128,
        GroupFamily::Vca,
        GroupRole::Lead,
    ));
    watch_for(watch, tracks, |e| {
        matches!(e, TrackEvent::GroupingChanged { grouping, .. }
            if grouping.role(GroupFamily::Vca, 128) == GroupRole::Lead)
    })?;

    // Now ask the DAW, rather than believing the events. Every
    // assertion above could pass on a window talking to itself.
    let info = track.info().await?;
    assert_eq!(info.name, "Renamed By Window");
    assert!((info.volume - 0.5).abs() < 1e-6, "volume: {}", info.volume);
    assert!((info.pan + 0.5).abs() < 1e-6, "pan: {}", info.pan);
    assert!(info.soloed && info.armed, "the toggles did not take");
    assert!(info.phase_inverted, "polarity did not take");
    assert_eq!(info.input_monitor, InputMonitoringMode::NotWhenPlaying);
    assert_eq!(info.record_input, RecordInput::Audio { channel: 3 });
    assert!(!info.parent_send, "parent send did not take");
    assert_eq!(info.automation_mode, AutomationMode::Latch);
    assert_eq!(info.color, Some(0x00_44_88));
    assert_eq!(
        track.group_flags().await?.role(GroupFamily::Vca, 128),
        GroupRole::Lead
    );

    // And the window's own list, which is what it DRAWS from. An event
    // applied to the wrong track, or not applied at all, shows here and
    // nowhere else.
    let drawn = tracks
        .iter()
        .find(|t| t.guid == guid)
        .ok_or_else(|| eyre::eyre!("the window lost the track"))?;
    assert_eq!(drawn.name, "Renamed By Window");
    assert!(drawn.soloed && drawn.armed && drawn.phase_inverted);
    assert_eq!(drawn.record_input, RecordInput::Audio { channel: 3 });
    assert_eq!(drawn.automation_mode, AutomationMode::Latch);
    assert_eq!(drawn.color, Some(0x00_44_88));
    assert_eq!(drawn.height, Some(96));
    assert_eq!(
        drawn.grouping.role(GroupFamily::Vca, 128),
        GroupRole::Lead,
        "the window is not drawing the VCA it just made"
    );

    Ok(())
}
