//! In-process `Daw` client backed by `Standalone`.
//!
//! Proves the backend-swap path: a `daw_control::Daw` instance built
//! from a `Standalone` server is functionally identical (for the
//! domains Standalone implements) to one backed by REAPER. This is
//! the foundation for running `daw-synchronization`-style tests
//! against Standalone.

#![cfg(feature = "bootstrap")]

use daw_proto::{PlayState, ProjectInfo};
use daw_proto::{
    ProjectContext, TrackRef,
    event_bus::{BusFilter, DawEvent},
    marker::{MarkerEvent, MarkerStreamEvent},
    region::{RegionEvent, RegionStreamEvent},
    tempo_map::{TempoMapEvent, TempoMapStreamEvent},
    track::{ReorderTracksBehavior, TrackEvent, TrackStreamEvent},
};
use daw_standalone::bootstrap::build_in_process_daw;
use daw_standalone::sync::Standalone;

fn seeded() -> Standalone {
    let s = Standalone::new();
    s.seed_project(ProjectInfo {
        guid: "test-proj".into(),
        name: "test".into(),
        path: String::new(),
    });
    s
}

/// Wait for an in-flight `#[subscribe]` attach to land on the server
/// hub before mutating. The stream call is fire-and-hold (it never
/// completes while the subscription lives), so the client can't
/// observe the attach — poll the backend's subscriber count, exactly
/// like architect's layered-services example does before publishing.
async fn settle_subscription(count: impl Fn() -> usize, at_least: usize) {
    for _ in 0..200 {
        if count() >= at_least {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("subscription attach never landed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_project_through_in_process_daw() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    // GUID round-trips through the RPC client.
    let info = project.info().await?;
    assert_eq!(info.guid, "test-proj");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transport_play_through_in_process_daw() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let transport = project.transport();

    assert!(!transport.is_playing().await?);
    transport.play().await?;
    // Drive a few soft-clock ticks so the engine advances measurably.
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    assert!(transport.is_playing().await?);
    let pos = transport.get_position().await?;
    assert!(pos > 0.05, "expected playhead to advance, got {pos}s");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transport_reaper_parity_through_in_process_daw() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let transport = project.transport();

    assert_eq!(transport.get_play_state().await?, PlayState::Stopped);
    assert!(!transport.is_playing().await?);
    assert!(!transport.is_recording().await?);

    transport.play().await?;
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    assert_eq!(transport.get_play_state().await?, PlayState::Playing);
    assert!(transport.is_playing().await?);
    assert!(
        transport.get_position().await? > 0.05,
        "standalone playhead should advance while playing"
    );

    transport.pause().await?;
    assert_eq!(transport.get_play_state().await?, PlayState::Paused);
    assert!(!transport.is_playing().await?);

    transport.play_pause().await?;
    assert_eq!(transport.get_play_state().await?, PlayState::Playing);
    transport.play_pause().await?;
    assert_eq!(transport.get_play_state().await?, PlayState::Paused);

    transport.play_stop().await?;
    assert_eq!(transport.get_play_state().await?, PlayState::Playing);
    transport.play_stop().await?;
    assert_eq!(transport.get_play_state().await?, PlayState::Stopped);
    assert!(!transport.is_playing().await?);

    transport.set_position(5.0).await?;
    assert!((transport.get_position().await? - 5.0).abs() < 0.05);
    transport.goto_start().await?;
    assert!(transport.get_position().await? < 0.05);

    let state = transport.get_state().await?;
    assert_eq!(state.play_state, PlayState::Stopped);
    assert!(state.tempo.bpm > 0.0);

    let original_tempo = transport.get_tempo().await?;
    transport.set_tempo(140.0).await?;
    assert!((transport.get_tempo().await? - 140.0).abs() < 1e-9);
    transport.set_tempo(original_tempo).await?;

    transport.set_loop(true).await?;
    assert!(transport.is_looping().await?);
    transport.toggle_loop().await?;
    assert!(!transport.is_looping().await?);

    transport.set_time_selection(4.0, 2.0).await?;
    let selection = transport
        .get_time_selection()
        .await?
        .expect("time selection should be set");
    assert_eq!(selection.start_seconds, 2.0);
    assert_eq!(selection.end_seconds, 4.0);
    transport.clear_time_selection().await?;
    assert!(transport.get_time_selection().await?.is_none());

    transport.set_playrate(0.5).await?;
    assert!((transport.get_playrate().await? - 0.5).abs() < 1e-9);
    transport.set_playrate(1.0).await?;

    let ts = transport.get_time_signature().await?;
    assert!(ts.numerator > 0);
    assert!(ts.denominator > 0);

    transport.record().await?;
    assert_eq!(transport.get_play_state().await?, PlayState::Recording);
    assert!(transport.is_playing().await?);
    assert!(transport.is_recording().await?);
    transport.stop_recording().await?;
    assert_eq!(transport.get_play_state().await?, PlayState::Stopped);
    assert!(!transport.is_recording().await?);

    transport.toggle_recording().await?;
    assert_eq!(transport.get_play_state().await?, PlayState::Recording);
    transport.toggle_recording().await?;
    assert_eq!(transport.get_play_state().await?, PlayState::Stopped);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tempo_round_trip_through_in_process_daw() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    project.transport().set_tempo(140.0).await?;
    let bpm = project.transport().get_tempo().await?;
    assert!((bpm - 140.0).abs() < 1e-9);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn marker_add_through_in_process_daw() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let _ = project.markers().add(1.5, "test-marker").await?;
    let markers = project.markers().all().await?;
    assert!(
        markers.iter().any(|m| m.name == "test-marker"),
        "marker should appear in list, got {:?}",
        markers.iter().map(|m| &m.name).collect::<Vec<_>>()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn track_structure_routing_and_colors_through_in_process_daw() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let tracks = project.tracks();

    let band = tracks.add("Band", None).await?;
    let drums = tracks.add("Drums", None).await?;
    let kick = tracks.add("Kick", None).await?;
    let snare = tracks.add("Snare", None).await?;
    let bass = tracks.add("Bass", None).await?;
    let drum_bus = tracks.add("Drum Bus", None).await?;

    band.set_folder_depth(1).await?;
    drums.set_folder_depth(1).await?;
    snare.set_folder_depth(-1).await?;
    bass.set_folder_depth(-1).await?;

    band.set_color(0x334455).await?;
    drums.set_color(0x223344).await?;
    kick.set_color(0xAA3300).await?;
    snare.set_color(0x00AA66).await?;
    bass.set_color(0x3333AA).await?;
    drum_bus.set_color(0x8844CC).await?;

    kick.set_visibility(true, false).await?;
    snare.set_tcp_height(72).await?;
    kick.set_parent_send(false).await?;

    let send = kick.sends().add_to(drum_bus.guid()).await?;
    send.set_volume(0.75).await?;
    send.set_pan(-0.25).await?;

    let all = tracks.all().await?;
    let band_info = all.iter().find(|track| track.guid == band.guid()).unwrap();
    let drums_info = all.iter().find(|track| track.guid == drums.guid()).unwrap();
    let kick_info = all.iter().find(|track| track.guid == kick.guid()).unwrap();
    let snare_info = all.iter().find(|track| track.guid == snare.guid()).unwrap();
    let bass_info = all.iter().find(|track| track.guid == bass.guid()).unwrap();
    let drum_bus_info = all
        .iter()
        .find(|track| track.guid == drum_bus.guid())
        .unwrap();

    assert!(band_info.is_folder);
    assert!(drums_info.is_folder);
    assert_eq!(drums_info.parent_guid.as_deref(), Some(band.guid()));
    assert_eq!(kick_info.parent_guid.as_deref(), Some(drums.guid()));
    assert_eq!(snare_info.parent_guid.as_deref(), Some(drums.guid()));
    assert_eq!(bass_info.parent_guid.as_deref(), Some(band.guid()));
    assert_eq!(drum_bus_info.parent_guid, None);

    assert_eq!(band_info.color, Some(0x334455));
    assert_eq!(kick_info.color, Some(0xAA3300));
    assert!(kick_info.visible_in_tcp);
    assert!(!kick_info.visible_in_mixer);
    assert_eq!(snare_info.folder_depth, -1);
    assert_eq!(bass_info.folder_depth, -1);

    let ctx = ProjectContext::Project(project.info().await?.guid);
    assert_eq!(
        bundle
            .standalone
            .track_tcp_height(&ctx, &TrackRef::Guid(snare.guid().to_string())),
        Some(72)
    );

    let sends = kick.sends().all().await?;
    assert_eq!(sends.len(), 1);
    assert_eq!(sends[0].dest_track_guid.as_deref(), Some(drum_bus.guid()));
    assert!((sends[0].volume - 0.75).abs() < 1e-9);
    assert!((sends[0].pan + 0.25).abs() < 1e-9);

    // A receive belongs to the track it is listed on, and names the
    // track feeding it at the far end — the same field a send puts its
    // destination in, which is what REAPER does. See daw#30: a mirror
    // that copied both ends unchanged made this list say "drum bus is
    // fed by drum bus".
    let receives = drum_bus.receives().all().await?;
    assert_eq!(receives.len(), 1);
    assert_eq!(receives[0].source_track_guid, drum_bus.guid());
    assert_eq!(receives[0].dest_track_guid.as_deref(), Some(kick.guid()));
    assert_eq!(receives[0].dest_track_name.as_deref(), Some("Kick"));
    assert!((receives[0].volume - 0.75).abs() < 1e-9);

    kick.select().await?;
    snare.select().await?;
    tracks
        .reorder_selected(5, ReorderTracksBehavior::Normal)
        .await?;

    let names_after_reorder = tracks
        .all()
        .await?
        .into_iter()
        .map(|track| track.name)
        .collect::<Vec<_>>();
    assert_eq!(
        names_after_reorder,
        vec!["Band", "Drums", "Bass", "Kick", "Snare", "Drum Bus"]
    );

    let all_after_reorder = tracks.all().await?;
    let kick_after = all_after_reorder
        .iter()
        .find(|track| track.guid == kick.guid())
        .unwrap();
    let snare_after = all_after_reorder
        .iter()
        .find(|track| track.guid == snare.guid())
        .unwrap();
    assert_eq!(kick_after.index, 3);
    assert_eq!(snare_after.index, 4);
    assert_eq!(kick_after.parent_guid.as_deref(), Some(band.guid()));
    assert_eq!(snare_after.parent_guid.as_deref(), Some(band.guid()));

    Ok(())
}

async fn next_track_event(
    rx: &mut daw_control::EventStream<TrackStreamEvent>,
) -> eyre::Result<TrackStreamEvent> {
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .map_err(|_| eyre::eyre!("timed out waiting for track event"))??
        .ok_or_else(|| eyre::eyre!("track event stream closed"))?;
    let mut out = None;
    let _ = event.map(|event| out = Some(event));
    Ok(out.expect("vox SelfRef::map runs once"))
}

async fn next_marker_event(
    rx: &mut daw_control::EventStream<MarkerStreamEvent>,
) -> eyre::Result<MarkerStreamEvent> {
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .map_err(|_| eyre::eyre!("timed out waiting for marker event"))??
        .ok_or_else(|| eyre::eyre!("marker event stream closed"))?;
    let mut out = None;
    let _ = event.map(|event| out = Some(event));
    Ok(out.expect("vox SelfRef::map runs once"))
}

async fn wait_for_marker_event(
    rx: &mut daw_control::EventStream<MarkerStreamEvent>,
    pred: impl Fn(&MarkerEvent) -> bool,
) -> eyre::Result<MarkerStreamEvent> {
    let deadline = web_time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let event = next_marker_event(rx).await?;
        if pred(&event.event) {
            return Ok(event);
        }
        if web_time::Instant::now() >= deadline {
            eyre::bail!("timed out waiting for matching marker event");
        }
    }
}

async fn next_region_event(
    rx: &mut daw_control::EventStream<RegionStreamEvent>,
) -> eyre::Result<RegionStreamEvent> {
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .map_err(|_| eyre::eyre!("timed out waiting for region event"))??
        .ok_or_else(|| eyre::eyre!("region event stream closed"))?;
    let mut out = None;
    let _ = event.map(|event| out = Some(event));
    Ok(out.expect("vox SelfRef::map runs once"))
}

async fn wait_for_region_event(
    rx: &mut daw_control::EventStream<RegionStreamEvent>,
    pred: impl Fn(&RegionEvent) -> bool,
) -> eyre::Result<RegionStreamEvent> {
    let deadline = web_time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let event = next_region_event(rx).await?;
        if pred(&event.event) {
            return Ok(event);
        }
        if web_time::Instant::now() >= deadline {
            eyre::bail!("timed out waiting for matching region event");
        }
    }
}

async fn next_tempo_event(
    rx: &mut daw_control::EventStream<TempoMapStreamEvent>,
) -> eyre::Result<TempoMapStreamEvent> {
    let event = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
        .await
        .map_err(|_| eyre::eyre!("timed out waiting for tempo event"))??
        .ok_or_else(|| eyre::eyre!("tempo event stream closed"))?;
    let mut out = None;
    let _ = event.map(|event| out = Some(event));
    Ok(out.expect("vox SelfRef::map runs once"))
}

async fn wait_for_tempo_event(
    rx: &mut daw_control::EventStream<TempoMapStreamEvent>,
    pred: impl Fn(&TempoMapEvent) -> bool,
) -> eyre::Result<TempoMapStreamEvent> {
    let deadline = web_time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let event = next_tempo_event(rx).await?;
        if pred(&event.event) {
            return Ok(event);
        }
        if web_time::Instant::now() >= deadline {
            eyre::bail!("timed out waiting for matching tempo event");
        }
    }
}

async fn next_daw_event(rx: &mut daw_control::EventStream<DawEvent>) -> eyre::Result<DawEvent> {
    let event = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
        .await
        .map_err(|_| eyre::eyre!("timed out waiting for daw event"))??
        .ok_or_else(|| eyre::eyre!("daw event stream closed"))?;
    let mut out = None;
    let _ = event.map(|event| out = Some(event));
    Ok(out.expect("vox SelfRef::map runs once"))
}

async fn wait_for_daw_event(
    rx: &mut daw_control::EventStream<DawEvent>,
    pred: impl Fn(&DawEvent) -> bool,
) -> eyre::Result<DawEvent> {
    let deadline = web_time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let event = next_daw_event(rx).await?;
        if pred(&event) {
            return Ok(event);
        }
        if web_time::Instant::now() >= deadline {
            eyre::bail!("timed out waiting for matching daw event");
        }
    }
}

async fn wait_for_track_event(
    rx: &mut daw_control::EventStream<TrackStreamEvent>,
    pred: impl Fn(&TrackEvent) -> bool,
) -> eyre::Result<TrackStreamEvent> {
    let deadline = web_time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let event = next_track_event(rx).await?;
        if pred(&event.event) {
            return Ok(event);
        }
        if web_time::Instant::now() >= deadline {
            eyre::bail!("timed out waiting for matching track event");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn marker_region_lanes_and_events_through_in_process_daw() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let project_guid = project.info().await?.guid;

    let markers = project.markers();
    let mut marker_rx = markers.subscribe().await?;
    {
        use daw_proto::marker::MarkersStreamSource;
        let hub = bundle.standalone.events_hub().clone();
        settle_subscription(move || hub.subscriber_count(), 1).await;
    }
    let marker_id = markers.add(2.5, "Verse").await?;
    let added_marker = wait_for_marker_event(&mut marker_rx, |event| {
        matches!(event, MarkerEvent::Added(marker) if marker.id == Some(marker_id) && marker.name == "Verse")
    })
    .await?;
    assert_eq!(added_marker.project_guid, project_guid);

    markers.set_lane(marker_id, Some(3)).await?;
    markers.set_color(marker_id, 0x112233).await?;
    markers.rename(marker_id, "Verse Marker").await?;
    let lane_marker = wait_for_marker_event(&mut marker_rx, |event| {
        matches!(event, MarkerEvent::Changed(marker) if marker.id == Some(marker_id) && marker.lane == Some(3))
    })
    .await?;
    assert_eq!(lane_marker.project_guid, project_guid);

    let marker = markers.get(marker_id).await?.expect("marker exists");
    assert_eq!(marker.lane, Some(3));
    assert_eq!(marker.color, Some(0x112233));
    assert_eq!(marker.name, "Verse Marker");

    markers.remove(marker_id).await?;
    wait_for_marker_event(
        &mut marker_rx,
        |event| matches!(event, MarkerEvent::Removed(id) if *id == marker_id),
    )
    .await?;

    let regions = project.regions();
    let mut region_rx = regions.subscribe().await?;
    {
        use daw_proto::region::RegionsStreamSource;
        let hub = bundle.standalone.events_hub().clone();
        settle_subscription(move || hub.subscriber_count(), 1).await;
    }
    let region_id = regions.add(4.0, 12.0, "Chorus").await?;
    let added_region = wait_for_region_event(&mut region_rx, |event| {
        matches!(event, RegionEvent::Added(region) if region.id == Some(region_id) && region.name == "Chorus")
    })
    .await?;
    assert_eq!(added_region.project_guid, project_guid);

    regions.set_lane(region_id, Some(2)).await?;
    regions.set_color(region_id, 0x445566).await?;
    regions.set_bounds(region_id, 5.0, 14.0).await?;
    let lane_region = wait_for_region_event(&mut region_rx, |event| {
        matches!(event, RegionEvent::Changed(region) if region.id == Some(region_id) && region.lane == Some(2))
    })
    .await?;
    assert_eq!(lane_region.project_guid, project_guid);

    let region = regions.get(region_id).await?.expect("region exists");
    assert_eq!(region.lane, Some(2));
    assert_eq!(region.color, Some(0x445566));
    assert!((region.time_range.start_seconds() - 5.0).abs() < 1e-9);
    assert!((region.time_range.end_seconds() - 14.0).abs() < 1e-9);

    regions.remove(region_id).await?;
    wait_for_region_event(
        &mut region_rx,
        |event| matches!(event, RegionEvent::Removed(id) if *id == region_id),
    )
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tempo_map_points_time_signatures_and_events_through_in_process_daw() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let project_guid = project.info().await?.guid;
    let tempo_map = project.tempo_map();
    let mut rx = tempo_map.subscribe().await?;
    {
        use daw_proto::tempo_map::TempoMapStreamSource;
        let hub = bundle.standalone.events_hub().clone();
        settle_subscription(move || hub.subscriber_count(), 1).await;
    }

    tempo_map.set_default_tempo(120.0).await?;
    tempo_map.set_default_time_signature(4, 4).await?;

    let idx = tempo_map.add_point(4.0, 90.0).await?;
    assert_eq!(idx, 0);
    let added = wait_for_tempo_event(&mut rx, |event| {
        matches!(event, TempoMapEvent::PointAdded(point) if (point.position_seconds() - 4.0).abs() < 1e-9 && (point.bpm - 90.0).abs() < 1e-9)
    })
    .await?;
    assert_eq!(added.project_guid, project_guid);

    let early_idx = tempo_map.add_point(0.0, 120.0).await?;
    assert_eq!(early_idx, 0);
    wait_for_tempo_event(&mut rx, |event| {
        matches!(event, TempoMapEvent::PointAdded(point) if (point.position_seconds() - 0.0).abs() < 1e-9 && (point.bpm - 120.0).abs() < 1e-9)
    })
    .await?;

    tempo_map.set_time_signature_at(1, 3, 4).await?;
    wait_for_tempo_event(&mut rx, |event| {
        matches!(event, TempoMapEvent::PointChanged(point) if (point.position_seconds() - 4.0).abs() < 1e-9 && point.time_signature.as_ref().is_some_and(|ts| ts.numerator == 3 && ts.denominator == 4))
    })
    .await?;
    assert_eq!(tempo_map.time_signature_at(4.5).await?, (3, 4));

    tempo_map.move_point(1, 6.0).await?;
    wait_for_tempo_event(&mut rx, |event| {
        matches!(event, TempoMapEvent::PointChanged(point) if (point.position_seconds() - 6.0).abs() < 1e-9)
    })
    .await?;
    assert!((tempo_map.tempo_at(6.5).await? - 90.0).abs() < 1e-9);

    let musical = tempo_map.time_to_musical(2.0).await?;
    assert_eq!(musical.0, 2);
    assert_eq!(musical.1, 1);
    assert!((tempo_map.musical_to_time(2, 1, 0.0).await? - 2.0).abs() < 1e-9);

    tempo_map.remove_point(1).await?;
    wait_for_tempo_event(
        &mut rx,
        |event| matches!(event, TempoMapEvent::PointRemoved(index) if *index == 1),
    )
    .await?;
    assert_eq!(tempo_map.points().await?.len(), 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn musical_time_conversion_walks_bpm_and_time_signature_segments() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let tempo_map = project.tempo_map();

    tempo_map.set_default_tempo(120.0).await?;
    tempo_map.set_default_time_signature(4, 4).await?;
    assert_eq!(tempo_map.add_point(0.0, 120.0).await?, 0);
    assert_eq!(tempo_map.add_point(8.0, 60.0).await?, 1);
    tempo_map.set_time_signature_at(1, 3, 4).await?;

    assert_eq!(tempo_map.time_to_musical(0.0).await?, (1, 1, 0.0));
    assert_eq!(tempo_map.time_to_musical(2.0).await?, (2, 1, 0.0));
    assert_eq!(tempo_map.time_to_musical(8.0).await?, (5, 1, 0.0));
    assert_eq!(tempo_map.time_to_musical(11.0).await?, (6, 1, 0.0));
    assert_eq!(tempo_map.time_to_musical(15.0).await?, (7, 2, 0.0));

    assert!((tempo_map.musical_to_time(2, 1, 0.0).await? - 2.0).abs() < 1e-9);
    assert!((tempo_map.musical_to_time(5, 1, 0.0).await? - 8.0).abs() < 1e-9);
    assert!((tempo_map.musical_to_time(6, 1, 0.0).await? - 11.0).abs() < 1e-9);
    assert!((tempo_map.musical_to_time(7, 2, 0.0).await? - 15.0).abs() < 1e-9);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn musical_time_conversion_honors_time_signature_denominator() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let tempo_map = project.tempo_map();

    tempo_map.set_default_tempo(120.0).await?;
    tempo_map.set_default_time_signature(6, 8).await?;

    assert_eq!(tempo_map.time_to_musical(0.0).await?, (1, 1, 0.0));
    assert_eq!(tempo_map.time_to_musical(0.125).await?, (1, 1, 0.5));
    assert_eq!(tempo_map.time_to_musical(0.25).await?, (1, 2, 0.0));
    assert_eq!(tempo_map.time_to_musical(3.0).await?, (3, 1, 0.0));
    assert!((tempo_map.musical_to_time(2, 4, 0.0).await? - 2.25).abs() < 1e-9);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn event_bus_multiplexes_standalone_marker_region_tempo_events() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let project_guid = project.info().await?.guid;
    let mut rx = bundle
        .daw
        .events()
        .subscribe(
            BusFilter {
                markers: true,
                regions: true,
                tempo_map: true,
                ..BusFilter::default()
            }
            .for_project(project_guid),
        )
        .await?;
    {
        use daw_proto::event_bus::EventBusStreamSource;
        let hub = bundle.standalone.events_hub().clone();
        settle_subscription(move || hub.subscriber_count(), 1).await;
    }

    let marker_id = project.markers().add(1.0, "Bus Marker").await?;
    wait_for_daw_event(&mut rx, |event| {
        matches!(event, DawEvent::Marker(event) if matches!(&event.event, MarkerEvent::Added(marker) if marker.id == Some(marker_id)))
    })
    .await?;

    let region_id = project.regions().add(2.0, 3.0, "Bus Region").await?;
    wait_for_daw_event(&mut rx, |event| {
        matches!(event, DawEvent::Region(event) if matches!(&event.event, RegionEvent::Added(region) if region.id == Some(region_id)))
    })
    .await?;

    project.tempo_map().add_point(0.0, 128.0).await?;
    wait_for_daw_event(&mut rx, |event| {
        matches!(event, DawEvent::TempoMap(event) if matches!(&event.event, TempoMapEvent::PointAdded(point) if (point.bpm - 128.0).abs() < 1e-9))
    })
    .await?;

    Ok(())
}

/// `EventBus` promises every domain on one channel, and FX and routing were
/// the two it did not deliver under REAPER: the events were built correctly
/// and published into in-process broadcasters that nothing bridged to the
/// bus. #139 bridged them.
///
/// Pinned here, against the standalone backend, because REAPER cannot be
/// driven in CI — and because a consumer written against standalone and
/// then run under REAPER is exactly who the gap silently failed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn event_bus_carries_fx_and_routing_events() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let project_guid = project.info().await?.guid;
    let tracks = project.tracks();
    let source = tracks.add("Source", None).await?;
    let dest = tracks.add("Dest", None).await?;

    let mut rx = bundle
        .daw
        .events()
        .subscribe(
            BusFilter {
                fx: true,
                routing: true,
                ..BusFilter::default()
            }
            .for_project(project_guid.clone()),
        )
        .await?;
    {
        use daw_proto::event_bus::EventBusStreamSource;
        let hub = bundle.standalone.events_hub().clone();
        settle_subscription(move || hub.subscriber_count(), 1).await;
    }

    // An FX add reaches the bus.
    source.fx_chain().add("ReaEQ").await?;
    wait_for_daw_event(&mut rx, |event| {
        matches!(event, DawEvent::Fx(event) if matches!(&event.event, daw_proto::FxEvent::Added { .. }))
    })
    .await?;

    // And a send change does too.
    let send = source.sends().add_to(dest.guid()).await?;
    send.set_volume(0.5).await?;
    wait_for_daw_event(&mut rx, |event| matches!(event, DawEvent::Routing(_))).await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn track_subscribe_emits_mutation_events_through_in_process_daw() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let project_guid = project.info().await?.guid;
    let tracks = project.tracks();
    let mut rx = tracks.subscribe().await?;
    {
        use daw_proto::track::TracksStreamSource;
        let hub = bundle.standalone.events_hub().clone();
        settle_subscription(move || hub.subscriber_count(), 1).await;
    }

    let guitar = tracks.add("Guitar", None).await?;
    let added = wait_for_track_event(
        &mut rx,
        |event| matches!(event, TrackEvent::Added(track) if track.guid == guitar.guid()),
    )
    .await?;
    assert_eq!(added.project_guid, project_guid);

    guitar.rename("Electric Guitar").await?;
    wait_for_track_event(&mut rx, |event| {
        matches!(event, TrackEvent::Renamed { guid, name } if guid == guitar.guid() && name == "Electric Guitar")
    })
    .await?;

    guitar.set_volume(0.42).await?;
    wait_for_track_event(&mut rx, |event| {
        matches!(event, TrackEvent::VolumeChanged { guid, volume } if guid == guitar.guid() && (volume - 0.42).abs() < 1e-9)
    })
    .await?;

    guitar.set_visibility(false, true).await?;
    wait_for_track_event(&mut rx, |event| {
        matches!(event, TrackEvent::TcpVisibilityChanged { guid, visible } if guid == guitar.guid() && !visible)
    })
    .await?;
    wait_for_track_event(&mut rx, |event| {
        matches!(event, TrackEvent::MixerVisibilityChanged { guid, visible } if guid == guitar.guid() && *visible)
    })
    .await?;

    let keys = tracks.add("Keys", None).await?;
    wait_for_track_event(
        &mut rx,
        |event| matches!(event, TrackEvent::Added(track) if track.guid == keys.guid()),
    )
    .await?;

    keys.select_exclusive().await?;
    wait_for_track_event(&mut rx, |event| {
        matches!(event, TrackEvent::SelectionChanged { guid, selected } if guid == keys.guid() && *selected)
    })
    .await?;
    tracks
        .reorder_selected(0, ReorderTracksBehavior::Normal)
        .await?;
    wait_for_track_event(&mut rx, |event| {
        matches!(event, TrackEvent::Moved { guid, old_index, new_index } if guid == keys.guid() && *old_index == 1 && *new_index == 0)
    })
    .await?;

    tracks
        .remove(TrackRef::Guid(guitar.guid().to_string()))
        .await?;
    wait_for_track_event(
        &mut rx,
        |event| matches!(event, TrackEvent::Removed(guid) if guid == guitar.guid()),
    )
    .await?;

    Ok(())
}

/// **The two events a second client needed and never got.**
///
/// Both fields were written, read back correctly by anyone who asked
/// again, and never announced — so a window that had already read them
/// kept showing the old value with nothing to say otherwise. Found by
/// pointing the session window at a live REAPER.
///
/// Pinned against standalone for the same reason as the test above:
/// REAPER cannot be driven in CI, and a consumer written against
/// standalone and then run under REAPER is exactly who the gap failed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn record_input_and_grouping_reach_a_subscriber() -> eyre::Result<()> {
    use daw_proto::track::{GroupFamily, GroupRole};

    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let tracks = project.tracks();
    let kick = tracks.add("Kick", None).await?;

    let mut rx = tracks.subscribe().await?;
    {
        use daw_proto::track::TracksStreamSource;
        let hub = bundle.standalone.events_hub().clone();
        settle_subscription(move || hub.subscriber_count(), 1).await;
    }

    // Applying a patch list sets every source track's input in one undo
    // step. Without this event a second screen keeps showing where a
    // take used to come from, which is the worst possible field to be
    // stale about while tracking.
    kick.set_record_input(daw_proto::track::RecordInput::Audio { channel: 3 })
        .await?;
    let input = wait_for_track_event(&mut rx, |event| {
        matches!(event, TrackEvent::RecordInputChanged { .. })
    })
    .await?;
    assert!(matches!(
        input.event,
        TrackEvent::RecordInputChanged {
            input: daw_proto::track::RecordInput::Audio { channel: 3 },
            ..
        }
    ));

    // Grouping is written by the FTS grouping watcher; a window attached
    // beside it would never have seen a VCA appear.
    kick.set_group_flags(128, GroupFamily::Vca, GroupRole::Lead)
        .await?;
    let grouped = wait_for_track_event(&mut rx, |event| {
        matches!(event, TrackEvent::GroupingChanged { .. })
    })
    .await?;

    // The event carries the state AFTER the change, so a client applies
    // it without asking again — a round trip per change on a link that
    // is not free is how a mirror falls behind what it mirrors.
    let TrackEvent::GroupingChanged { grouping, .. } = &grouped.event else {
        eyre::bail!("not a grouping event");
    };
    let read_back = kick.group_flags().await?;
    assert_eq!(
        grouping, &read_back,
        "the event disagreed with a fresh read"
    );
    Ok(())
}

/// Subscribing to the bus and immediately causing an event does not
/// lose the event.
///
/// `Events::subscribe` parks the in-flight `events(tx)` call on a task
/// and returns, and the server attaches the sink some time after that.
/// `PubSub` has no replay, so everything published in that gap used to
/// be delivered to nobody — and the consumer waited forever for an
/// event that had already happened.
///
/// Every other subscription test here works around it by polling the
/// backend's subscriber count (`settle_subscription`), which is correct
/// for an in-process test holding the `Standalone` and no help at all to
/// a remote client. This one deliberately does NOT: the backend now
/// answers a new subscriber with `DawEvent::Attached` as the first frame
/// of its mailbox, and `subscribe` waits for it before returning. If
/// that regresses, this test hangs until its timeout rather than going
/// quietly green. See session#85.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bus_subscriber_does_not_miss_what_it_causes() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;

    let mut bus = bundle
        .daw
        .events()
        .subscribe(BusFilter {
            tracks: true,
            ..Default::default()
        })
        .await?;

    // No settle, no sleep, no poll. Straight to the mutation.
    project.add_track("Drums", None).await?;

    let event = tokio::time::timeout(std::time::Duration::from_secs(5), bus.recv())
        .await
        .expect("the event a subscriber caused never arrived")?
        .expect("bus closed");
    let mut got = None;
    let _ = event.map(|e| got = Some(e));
    assert!(
        matches!(got, Some(DawEvent::Track(_))),
        "expected the track event this test caused, got {got:?}"
    );
    Ok(())
}

/// And the marker itself never reaches a consumer.
///
/// It is an implementation detail of the attach, filtered out on both
/// routes — `subscribe` swallows it and `admits` rejects it — so a
/// consumer matching exhaustively on `DawEvent` never has to think
/// about it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_attached_marker_is_never_delivered() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;

    let mut bus = bundle
        .daw
        .events()
        .subscribe(BusFilter {
            tracks: true,
            ..Default::default()
        })
        .await?;
    project.add_track("Bass", None).await?;

    for _ in 0..4 {
        let Ok(Ok(Some(event))) =
            tokio::time::timeout(std::time::Duration::from_millis(500), bus.recv()).await
        else {
            break;
        };
        let mut got = None;
        let _ = event.map(|e| got = Some(e));
        assert!(
            !matches!(got, Some(DawEvent::Attached)),
            "the attach marker reached a consumer"
        );
    }
    Ok(())
}
