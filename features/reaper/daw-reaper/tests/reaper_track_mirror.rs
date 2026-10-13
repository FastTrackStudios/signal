//! The track fields a second client is told about, inside a real REAPER.
//!
//! These were all readable on demand and none of them were announced,
//! so a window that had drawn one kept drawing it after it changed —
//! right on the first frame and a guess on every frame after, with
//! nothing on screen to say which. Found by pointing the session window
//! at a live REAPER (session#98).
//!
//! Pinned here as well as against standalone deliberately. The
//! standalone test proves the events exist and carry the right thing;
//! only this one proves REAPER's poller actually notices, which is the
//! half that was missing — the variants existed and the diff did not.
//!
//! Run with: `cargo run -p daw-reaper-xtask -- reaper_track_mirror`

use daw::test::reaper_test;
use daw_proto::track::{GroupFamily, GroupRole, TrackEvent};

/// How long to wait for the 30 Hz poller to notice.
///
/// Generous on purpose: the thing being measured is whether the diff
/// exists at all, and a tight bound would turn a busy REAPER into a
/// failure that reads like a missing event.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(5);

/// Wait for an event this test is interested in, ignoring the rest.
///
/// Attaching to a REAPER means the stream carries every track in the
/// tab, including whatever the harness did to set the test up, so a
/// test that took the FIRST event would be testing the setup.
async fn wait_for(
    stream: &mut daw::rpc::EventStream<daw_proto::track::TrackStreamEvent>,
    mut wanted: impl FnMut(&TrackEvent) -> bool,
) -> eyre::Result<TrackEvent> {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            eyre::bail!("no matching track event within {PATIENCE:?}");
        }
        match tokio::time::timeout(left, stream.recv()).await {
            Err(_) => eyre::bail!("no matching track event within {PATIENCE:?}"),
            Ok(Ok(Some(event))) => {
                let event = event.get().event.clone();
                if wanted(&event) {
                    return Ok(event);
                }
            }
            Ok(Ok(None)) => eyre::bail!("the track stream ended"),
            Ok(Err(error)) => eyre::bail!("the track stream failed: {error}"),
        }
    }
}

/// Wait until the poller has this project in its cache.
///
/// The cache is seeded the first time a tick finds a subscriber, and
/// everything already true at that moment is reported as `Added` rather
/// than as a field change. So a test that subscribes and immediately
/// flips a field races that tick: the flip lands before the seed, the
/// seed reports it as part of the track's initial state, and the field
/// event the test is waiting for is never produced because by then
/// nothing has changed.
///
/// A window does not hit this, because it subscribes once at startup
/// and everything it cares about happens later. A test has to wait for
/// the same thing to be true.
async fn settle(
    stream: &mut daw::rpc::EventStream<daw_proto::track::TrackStreamEvent>,
    guid: &str,
) -> eyre::Result<()> {
    wait_for(
        stream,
        |event| matches!(event, TrackEvent::Added(track) if track.guid == guid),
    )
    .await?;
    Ok(())
}

/// Flip the six fields the poller read and never reported.
///
/// One test rather than six, because the cost here is the REAPER tab
/// and the subscription, not the assertions — and because a run that
/// reports five of six passing hides the shape of the bug, which was
/// that the whole group of them was missing at once.
#[reaper_test(isolated)]
async fn the_fields_the_poller_used_to_drop_reach_a_subscriber(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    use daw_proto::primitives::AutomationMode;
    use daw_proto::track::InputMonitoringMode;

    let project = ctx.project().clone();
    let tracks = project.tracks();
    let kick = tracks.add("Mirror Kick", None).await?;
    let mut stream = tracks.subscribe().await?;
    settle(&mut stream, kick.guid()).await?;

    kick.set_phase_inverted(true).await?;
    wait_for(&mut stream, |event| {
        matches!(
            event,
            TrackEvent::PhaseInvertedChanged { inverted: true, .. }
        )
    })
    .await?;

    kick.set_input_monitor(InputMonitoringMode::NotWhenPlaying)
        .await?;
    wait_for(&mut stream, |event| {
        matches!(
            event,
            TrackEvent::InputMonitorChanged {
                monitor: InputMonitoringMode::NotWhenPlaying,
                ..
            }
        )
    })
    .await?;

    kick.set_automation_mode(AutomationMode::Latch).await?;
    wait_for(&mut stream, |event| {
        matches!(
            event,
            TrackEvent::AutomationModeChanged {
                mode: AutomationMode::Latch,
                ..
            }
        )
    })
    .await?;

    // The one an engineer changes while tracking, which is the worst
    // moment for a second screen to be showing the old answer.
    kick.set_record_input(daw_proto::track::RecordInput::Audio { channel: 3 })
        .await?;
    wait_for(&mut stream, |event| {
        matches!(
            event,
            TrackEvent::RecordInputChanged {
                input: daw_proto::track::RecordInput::Audio { channel: 3 },
                ..
            }
        )
    })
    .await?;

    kick.set_visibility(false, true).await?;
    wait_for(&mut stream, |event| {
        matches!(
            event,
            TrackEvent::TcpVisibilityChanged { visible: false, .. }
        )
    })
    .await?;

    kick.set_visibility(false, false).await?;
    wait_for(&mut stream, |event| {
        matches!(
            event,
            TrackEvent::MixerVisibilityChanged { visible: false, .. }
        )
    })
    .await?;

    Ok(())
}

/// A group written through the facade is announced.
///
/// The FTS grouping watcher is what manages groups, so this is the path
/// every group FTS makes travels. A window attached beside the watcher
/// used to watch a VCA appear in REAPER and not in itself.
#[reaper_test(isolated)]
async fn a_group_written_through_the_facade_is_announced(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();
    let kick = tracks.add("Grouped Kick", None).await?;
    let mut stream = tracks.subscribe().await?;
    settle(&mut stream, kick.guid()).await?;

    kick.set_group_flags(128, GroupFamily::Vca, GroupRole::Lead)
        .await?;
    let event = wait_for(&mut stream, |event| {
        matches!(event, TrackEvent::GroupingChanged { .. })
    })
    .await?;

    // The event carries the state AFTER the change, so a client applies
    // it without asking again. Compared against a fresh read, because an
    // event that merely says "something changed" would pass a weaker
    // assertion and still leave every client one round trip behind.
    let TrackEvent::GroupingChanged { grouping, .. } = &event else {
        eyre::bail!("not a grouping event");
    };
    assert_eq!(
        grouping,
        &kick.group_flags().await?,
        "the event disagreed with a fresh read"
    );
    assert_eq!(
        grouping.role(GroupFamily::Vca, 128),
        GroupRole::Lead,
        "the VCA lead the watcher just wrote is not in the event"
    );

    Ok(())
}

/// A subscriber that arrives late is told what the groups already are.
///
/// This is the sweep, and it is the only thing that can see a group
/// this process did not write — the writers announce what they change
/// and nothing else, and the bulk track read leaves grouping empty
/// because reading it is about a hundred calls a track. So a window
/// attaching to a REAPER mid-session had no way to learn the grouping
/// at all, and a group edited by hand in REAPER's own matrix dialog
/// goes through no writer and would never be seen again.
///
/// Tested through a fresh subscription rather than through the dialog,
/// which cannot be driven: both reach the window by the same route, and
/// the route is the thing that was missing.
#[reaper_test(isolated)]
async fn a_late_subscriber_is_told_what_the_groups_already_are(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();
    let snare = tracks.add("Swept Snare", None).await?;

    // Written and then forgotten about, exactly as a group that was
    // already in the project when the window opened.
    snare
        .set_group_flags(7, GroupFamily::Volume, GroupRole::Follow)
        .await?;

    // Subscribed only now, so this window is the late one. The sweep
    // drops its cache whenever nothing is listening, which is what
    // makes "late" mean the same thing for the second window as for
    // the first — a cache left warm from a previous window would let
    // this one learn nothing.
    let mut stream = tracks.subscribe().await?;
    let event = wait_for(&mut stream, |event| {
        matches!(event, TrackEvent::GroupingChanged { grouping, .. }
            if grouping.role(GroupFamily::Volume, 7) == GroupRole::Follow)
    })
    .await?;
    let TrackEvent::GroupingChanged { guid, .. } = &event else {
        eyre::bail!("not a grouping event");
    };
    assert_eq!(guid, snare.guid(), "the sweep named the wrong track");

    Ok(())
}

/// The negative control for the whole file.
///
/// A rename has been diffed by `poll_and_broadcast_tracks` since long
/// before any of this work, so if it does not arrive, nothing above is
/// testing its own subject — it is testing whether live track streaming
/// works at all. Nothing in this repo had ever subscribed to a stream
/// inside a real REAPER, so that question had no answer until now.
#[reaper_test(isolated)]
async fn a_rename_reaches_a_subscriber(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();
    let probe = tracks.add("Probe", None).await?;
    let mut stream = tracks.subscribe().await?;
    settle(&mut stream, probe.guid()).await?;
    probe.rename("Probe Renamed").await?;
    wait_for(
        &mut stream,
        |event| matches!(event, TrackEvent::Renamed { name, .. } if name == "Probe Renamed"),
    )
    .await?;
    Ok(())
}

/// Height, folder depth and lanes — the three a window drew and was
/// never told about.
///
/// They were not merely undiffed: no `TrackEvent` carried them at all,
/// so a second window's copy of any of the three was right once, on the
/// read that built it, and a guess forever after. Height decides how
/// tall a row is drawn, depth decides which folder a track is IN, and
/// the lane fields decide what a comp view shows — none of them small
/// enough to be wrong quietly.
#[reaper_test(isolated)]
async fn height_depth_and_lanes_reach_a_subscriber(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();
    let tom = tracks.add("Mirror Tom", None).await?;
    let mut stream = tracks.subscribe().await?;
    // Wait for the poller to seed this project before touching
    // anything. It reports everything already true as `Added` on the
    // first tick that finds a subscriber, so a change made before that
    // is folded into the snapshot instead of arriving as a change.
    wait_for(
        &mut stream,
        |event| matches!(event, TrackEvent::Added(track) if track.guid == tom.guid()),
    )
    .await?;

    tom.set_tcp_height(96).await?;
    wait_for(&mut stream, |event| {
        matches!(
            event,
            TrackEvent::HeightChanged {
                height: Some(96),
                ..
            }
        )
    })
    .await?;

    // A folder is a depth, not a container, which is why this is a
    // track field and not a tree operation.
    tom.set_folder_depth(1).await?;
    wait_for(&mut stream, |event| {
        matches!(
            event,
            TrackEvent::FolderDepthChanged {
                folder_depth: 1,
                ..
            }
        )
    })
    .await?;

    tom.set_lane_count(3).await?;
    let lanes = wait_for(&mut stream, |event| {
        matches!(event, TrackEvent::LanesChanged { lane_count: 3, .. })
    })
    .await?;
    let TrackEvent::LanesChanged { lane_play_mask, .. } = &lanes else {
        eyre::bail!("not a lanes event");
    };
    assert_ne!(
        *lane_play_mask, 0,
        "three lanes and none of them audible is a track that went silent"
    );

    // Renaming a lane carries the whole set, so a view never has the
    // count without the names.
    tom.set_lane_name(1, "Comp").await?;
    let named = wait_for(&mut stream, |event| {
        matches!(event, TrackEvent::LanesChanged { lane_names, .. }
            if lane_names.iter().any(|n| n == "Comp"))
    })
    .await?;
    let TrackEvent::LanesChanged { lane_count, .. } = &named else {
        eyre::bail!("not a lanes event");
    };
    assert_eq!(*lane_count, 3, "the names arrived without the count");

    Ok(())
}

/// A send appearing changes the strip's IO indicator.
///
/// The counts are on the track rather than behind the routing service
/// for the same reason `record_input` is: a mixer draws this on every
/// strip, and asking per track would cost N round trips for two
/// numbers. Before this the indicator was drawn from a hard-coded
/// false, so a session full of sends showed none.
#[reaper_test(isolated)]
async fn a_send_changes_the_route_counts(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();
    let source = tracks.add("Route Source", None).await?;
    let bus = tracks.add("Route Bus", None).await?;
    let mut stream = tracks.subscribe().await?;
    settle(&mut stream, source.guid()).await?;

    let source_guid = source.guid().to_owned();
    let bus_guid = bus.guid().to_owned();
    source.sends().add_to(bus.guid()).await?;

    // Read both back first. If the COUNTS are wrong the diff can never
    // fire, and a wrong read and a missing diff look identical from a
    // timeout.
    assert_eq!(
        source.info().await?.send_count,
        1,
        "the bulk read does not see the send"
    );
    assert_eq!(
        bus.info().await?.receive_count,
        1,
        "the bulk read does not see the receive"
    );

    // One action changes two tracks, and the poller reports them in
    // whatever order it walks the project — which is not an order this
    // test should depend on. So it collects until it has both rather
    // than waiting for one and then the other.
    let mut saw_send = false;
    let mut saw_receive = false;
    while !(saw_send && saw_receive) {
        let event = wait_for(&mut stream, |event| {
            matches!(event, TrackEvent::RouteCountsChanged { .. })
        })
        .await?;
        let TrackEvent::RouteCountsChanged {
            guid,
            send_count,
            receive_count,
        } = &event
        else {
            eyre::bail!("not a route-counts event");
        };
        if *guid == source_guid {
            assert_eq!(*send_count, 1, "the sender's send was not counted");
            assert_eq!(
                *receive_count, 0,
                "the source gained a receive it never got"
            );
            saw_send = true;
        } else if *guid == bus_guid {
            assert_eq!(
                *receive_count, 1,
                "the destination's receive was not counted"
            );
            saw_receive = true;
        }
    }

    Ok(())
}
