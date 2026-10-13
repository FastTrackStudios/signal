//! End-to-end test: soft-clock advances playhead through the proto
//! `Transport` service.

use std::time::Duration;

use daw_proto::transport::service::Transport;
use daw_proto::{PlayState, ProjectContext, ProjectInfo};
use daw_standalone::sync::Standalone;

fn seeded() -> (Standalone, String) {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "test-proj".into(),
        name: "test".into(),
        path: String::new(),
    });
    (daw, guid)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn soft_clock_advances_playhead_when_playing() {
    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;

    assert_eq!(daw.get_position(ctx.clone()), 0.0);
    Transport::play(&daw, ctx.clone()).unwrap();
    tokio::time::sleep(Duration::from_millis(120)).await;

    let pos = daw.get_position(ctx);
    assert!(
        pos > 0.05,
        "expected playhead to have advanced ~100ms, got {pos}s"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_freezes_playhead() {
    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;

    Transport::play(&daw, ctx.clone()).unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    Transport::stop(&daw, ctx.clone()).unwrap();
    let a = daw.get_position(ctx.clone());
    tokio::time::sleep(Duration::from_millis(80)).await;
    let b = daw.get_position(ctx);
    assert!(
        (b - a).abs() < 0.01,
        "expected playhead frozen after stop: a={a}, b={b}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn varispeed_doubles_advance_rate() {
    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;

    Transport::set_playrate(&daw, ctx.clone(), 2.0).unwrap();
    Transport::play(&daw, ctx.clone()).unwrap();
    tokio::time::sleep(Duration::from_millis(120)).await;
    let pos = daw.get_position(ctx);
    // At 2x for ~100ms expect ~0.2s.
    assert!(pos > 0.12, "expected >0.12s at 2x for 120ms, got {pos}s");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn time_selection_loop_wraps_playhead() {
    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;

    Transport::set_time_selection(&daw, ctx.clone(), 0.0, 0.1).unwrap();
    Transport::set_loop(&daw, ctx.clone(), true).unwrap();
    Transport::play(&daw, ctx.clone()).unwrap();
    // ~300ms of playback against a 100ms loop should keep playhead
    // inside [0, 0.1].
    tokio::time::sleep(Duration::from_millis(300)).await;
    let pos = daw.get_position(ctx);
    assert!(
        (0.0..=0.11).contains(&pos),
        "expected playhead wrapped within loop, got {pos}s"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tempo_map_dynamic_drives_musical_position() {
    use daw_proto::TempoMap;

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;

    // Two segments: 120 BPM from 0..2s, 60 BPM thereafter.
    TempoMap::add_tempo_point(&daw, ctx.clone(), 0.0, 120.0).unwrap();
    TempoMap::add_tempo_point(&daw, ctx.clone(), 2.0, 60.0).unwrap();

    // At t=1s (mid-first-segment) we expect 2 beats.
    let (_m1, _b1, _f1) = TempoMap::time_to_musical(&daw, ctx.clone(), 1.0);
    // Reading via the engine directly through subscribe path:
    let bundle = daw.transport_engine_for("test-proj");
    let map = bundle.dynamic_tempo().expect("dynamic map installed");
    let clock = daw_standalone::transport_engine::SampleClock::new(bundle.shared.sample_rate());
    // At t=3s = 4 beats (first 2s) + 1 beat (60BPM for 1s).
    let s = clock.seconds_to_samples(daw_standalone::transport_engine::InstantSeconds(3.0));
    let mu = map.samples_to_musical(s, 1.0, &clock);
    assert!(
        (mu.0 - 5.0).abs() < 1e-6,
        "expected 5 beats at t=3s, got {}",
        mu.0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tempo_map_falls_back_to_static_with_zero_or_one_points() {
    use daw_proto::TempoMap;

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;

    // No points → static fallback.
    let bundle = daw.transport_engine_for("test-proj");
    assert!(bundle.dynamic_tempo().is_none());

    // One point → still falls back to static, but BPM is mirrored.
    TempoMap::add_tempo_point(&daw, ctx.clone(), 0.0, 90.0).unwrap();
    assert!(bundle.dynamic_tempo().is_none());
    assert!((bundle.shared.tempo_bpm() - 90.0).abs() < 1e-9);

    // Second point → dynamic map installs.
    TempoMap::add_tempo_point(&daw, ctx.clone(), 1.0, 180.0).unwrap();
    assert!(bundle.dynamic_tempo().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn routing_send_mirrors_to_dest_receive() {
    use daw_proto::{RouteLocation, RouteRef, RouteType, Routing, TrackRef, Tracks};

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let src = Tracks::add(&daw, ctx.clone(), "Drums", None).unwrap();
    let dest = Tracks::add(&daw, ctx.clone(), "Drum Bus", None).unwrap();

    let i = Routing::add_send(
        &daw,
        ctx.clone(),
        TrackRef::Guid(src.clone()),
        TrackRef::Guid(dest.clone()),
    )
    .expect("send created");
    assert_eq!(i, 0);

    let sends = Routing::sends(&daw, ctx.clone(), TrackRef::Guid(src.clone()));
    assert_eq!(sends.len(), 1);
    assert_eq!(sends[0].dest_track_guid.as_deref(), Some(dest.as_str()));
    assert_eq!(sends[0].route_type, RouteType::Send);

    // Receive auto-mirrored on the destination, with the ends the way
    // round REAPER writes them: the route belongs to the track it is
    // listed on, and `dest_track_*` is the OTHER end — where the signal
    // goes for a send, where it comes from for a receive. (daw#30.)
    let receives = Routing::receives(&daw, ctx.clone(), TrackRef::Guid(dest.clone()));
    assert_eq!(receives.len(), 1);
    assert_eq!(receives[0].source_track_guid, dest);
    assert_eq!(receives[0].dest_track_guid.as_deref(), Some(src.as_str()));
    assert_eq!(receives[0].dest_track_name.as_deref(), Some("Drums"));
    assert_eq!(receives[0].route_type, RouteType::Receive);

    // Mutating the send propagates to the mirror.
    Routing::set_volume(
        &daw,
        ctx.clone(),
        RouteLocation {
            track: TrackRef::Guid(src.clone()),
            route_type: RouteType::Send,
            route: RouteRef::Index(0),
        },
        0.5,
    )
    .unwrap();
    let mirror = &Routing::receives(&daw, ctx.clone(), TrackRef::Guid(dest.clone()))[0];
    assert!((mirror.volume - 0.5).abs() < 1e-9);

    // Removing the send also removes the receive mirror.
    Routing::remove_route(
        &daw,
        ctx.clone(),
        RouteLocation {
            track: TrackRef::Guid(src.clone()),
            route_type: RouteType::Send,
            route: RouteRef::Index(0),
        },
    )
    .unwrap();
    assert_eq!(
        Routing::send_count(&daw, ctx.clone(), TrackRef::Guid(src)),
        0
    );
    assert_eq!(Routing::receive_count(&daw, ctx, TrackRef::Guid(dest)), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn routing_send_inherits_source_channel_count() {
    use daw_proto::{Routing, TrackRef, Tracks};

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let src = Tracks::add(&daw, ctx.clone(), "8ch", None).unwrap();
    let dest = Tracks::add(&daw, ctx.clone(), "16ch", None).unwrap();
    Tracks::set_num_channels(&daw, ctx.clone(), TrackRef::Guid(src.clone()), 8).unwrap();
    Tracks::set_num_channels(&daw, ctx.clone(), TrackRef::Guid(dest.clone()), 16).unwrap();

    Routing::add_send(
        &daw,
        ctx.clone(),
        TrackRef::Guid(src.clone()),
        TrackRef::Guid(dest.clone()),
    )
    .unwrap();
    let sends = Routing::sends(&daw, ctx, TrackRef::Guid(src));
    assert_eq!(sends[0].source_channels.num_channels, 8);
    assert_eq!(sends[0].dest_channels.num_channels, 16);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn routing_parent_send_toggles() {
    use daw_proto::{Routing, TrackRef, Tracks};

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let g = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();

    // Default is enabled.
    assert!(Routing::parent_send_enabled(
        &daw,
        ctx.clone(),
        TrackRef::Guid(g.clone())
    ));
    Routing::set_parent_send_enabled(&daw, ctx.clone(), TrackRef::Guid(g.clone()), false).unwrap();
    assert!(!Routing::parent_send_enabled(&daw, ctx, TrackRef::Guid(g)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automation_envelope_round_trip() {
    use daw_proto::automation::{
        AddPointParams, Automation, EnvelopeLocation, EnvelopeRef, EnvelopeShape, EnvelopeType,
    };
    use daw_proto::primitives::PositionInSeconds;
    use daw_proto::{TrackRef, Tracks};

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let tg = Tracks::add(&daw, ctx.clone(), "Vox", None).unwrap();
    let loc = EnvelopeLocation::new(
        TrackRef::Guid(tg.clone()),
        EnvelopeRef::Type(EnvelopeType::Volume),
    );

    // No points yet → value at any time = 0.
    assert!(
        Automation::value_at(
            &daw,
            ctx.clone(),
            loc.clone(),
            PositionInSeconds::from_seconds(1.0)
        ) == 0.0
    );

    Automation::add_point(
        &daw,
        ctx.clone(),
        loc.clone(),
        AddPointParams::linear(PositionInSeconds::from_seconds(0.0), 0.0),
    );
    Automation::add_point(
        &daw,
        ctx.clone(),
        loc.clone(),
        AddPointParams::linear(PositionInSeconds::from_seconds(2.0), 1.0),
    );
    // Insert out-of-order, expect sort.
    Automation::add_point(
        &daw,
        ctx.clone(),
        loc.clone(),
        AddPointParams::linear(PositionInSeconds::from_seconds(1.0), 0.5),
    );

    let points = Automation::points(&daw, ctx.clone(), loc.clone());
    assert_eq!(points.len(), 3);
    assert!(points[0].time.as_seconds() < points[1].time.as_seconds());
    assert!(points[1].time.as_seconds() < points[2].time.as_seconds());

    // Linear interp at t=0.5 between (0, 0) and (1, 0.5) = 0.25.
    let v = Automation::value_at(
        &daw,
        ctx.clone(),
        loc.clone(),
        PositionInSeconds::from_seconds(0.5),
    );
    assert!((v - 0.25).abs() < 1e-9, "expected 0.25, got {v}");

    // Square shape on first segment freezes value.
    Automation::set_point(
        &daw,
        ctx.clone(),
        loc.clone(),
        daw_proto::automation::SetPointParams {
            index: 0,
            time: PositionInSeconds::from_seconds(0.0),
            value: 0.0,
            shape: EnvelopeShape::Square,
        },
    );
    let v = Automation::value_at(
        &daw,
        ctx.clone(),
        loc.clone(),
        PositionInSeconds::from_seconds(0.5),
    );
    assert!(
        (v - 0.0).abs() < 1e-9,
        "expected 0.0 (Square hold), got {v}"
    );

    // Envelope query exposes point count.
    let env = Automation::envelope(&daw, ctx, loc).unwrap();
    assert_eq!(env.point_count, 3);
    assert_eq!(env.track_guid, tg);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fx_add_list_remove_round_trip() {
    use daw_proto::Tracks;
    use daw_proto::fx::{Effects, FxChainContext, FxRef, FxTarget, SetParameterRequest};

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let track_guid = Tracks::add(&daw, ctx.clone(), "Bus", None).unwrap();
    let chain = FxChainContext::Track(track_guid.clone());

    assert_eq!(Effects::count(&daw, ctx.clone(), chain.clone()), 0);
    let fx_a = Effects::add(&daw, ctx.clone(), chain.clone(), "ReaComp").expect("add A");
    let fx_b = Effects::add(&daw, ctx.clone(), chain.clone(), "VST3: Pro-Q 4").expect("add B");
    assert_eq!(Effects::count(&daw, ctx.clone(), chain.clone()), 2);

    let list = Effects::list(&daw, ctx.clone(), chain.clone());
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].index, 0);
    assert_eq!(list[1].index, 1);
    assert_eq!(list[0].plugin_name, "ReaComp");

    // Set a parameter and read it back.
    let target_b = FxTarget::new(chain.clone(), FxRef::Guid(fx_b.clone()));
    Effects::set_parameter(
        &daw,
        ctx.clone(),
        SetParameterRequest {
            target: target_b.clone(),
            index: 3,
            value: 0.75,
        },
    )
    .unwrap();
    let p = Effects::parameter(&daw, ctx.clone(), target_b.clone(), 3).unwrap();
    assert!((p.value - 0.75).abs() < 1e-9);
    assert_eq!(p.name, "Param 4");

    // Move A to index 1.
    Effects::move_to(
        &daw,
        ctx.clone(),
        FxTarget::new(chain.clone(), FxRef::Guid(fx_a.clone())),
        1,
    )
    .unwrap();
    let list = Effects::list(&daw, ctx.clone(), chain.clone());
    assert_eq!(list[0].guid, fx_b);
    assert_eq!(list[1].guid, fx_a);
    assert_eq!(list[1].index, 1);

    // Remove B.
    Effects::remove(
        &daw,
        ctx.clone(),
        FxTarget::new(chain.clone(), FxRef::Guid(fx_b)),
    )
    .unwrap();
    let list = Effects::list(&daw, ctx, chain);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].guid, fx_a);
    assert_eq!(list[0].index, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fx_state_chunk_round_trip() {
    use daw_proto::Tracks;
    use daw_proto::fx::{Effects, FxChainContext, FxRef, FxTarget};

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let tg = Tracks::add(&daw, ctx.clone(), "Bus", None).unwrap();
    let chain = FxChainContext::Track(tg);
    let fx_guid = Effects::add(&daw, ctx.clone(), chain.clone(), "ReaEQ").unwrap();
    let target = FxTarget::new(chain, FxRef::Guid(fx_guid));

    Effects::set_state_chunk(&daw, ctx.clone(), target.clone(), b"FOO BAR BAZ".to_vec()).unwrap();
    let chunk = Effects::state_chunk(&daw, ctx, target).unwrap();
    assert_eq!(chunk, b"FOO BAR BAZ");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn midi_create_item_and_add_notes() {
    use daw_proto::midi::{Midi, MidiNoteCreate};
    use daw_proto::{TrackRef, Tracks};

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let track_guid = Tracks::add(&daw, ctx.clone(), "MIDI", None).unwrap();

    let loc = Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(track_guid), 0.0, 2.0)
        .expect("MIDI item created");

    assert_eq!(Midi::note_count(&daw, loc.clone()), 0);

    let i1 = Midi::add_note(&daw, loc.clone(), MidiNoteCreate::new(60, 100, 0.0, 1.0));
    let i2 = Midi::add_note(&daw, loc.clone(), MidiNoteCreate::new(64, 100, 1.0, 1.0));
    let i3 = Midi::add_note(&daw, loc.clone(), MidiNoteCreate::new(67, 100, 2.0, 1.0));
    assert_eq!((i1, i2, i3), (0, 1, 2));
    assert_eq!(Midi::note_count(&daw, loc.clone()), 3);

    // Transpose just index 1 up an octave.
    Midi::transpose_notes(&daw, loc.clone(), vec![1], 12);
    let notes = Midi::notes(&daw, loc.clone());
    assert_eq!(notes[0].pitch, 60);
    assert_eq!(notes[1].pitch, 76);
    assert_eq!(notes[2].pitch, 67);

    // Delete middle note; remaining renumber.
    Midi::delete_note(&daw, loc.clone(), 1);
    let notes = Midi::notes(&daw, loc.clone());
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].index, 0);
    assert_eq!(notes[1].index, 1);
    assert_eq!(notes[0].pitch, 60);
    assert_eq!(notes[1].pitch, 67);

    // Range query.
    let in_range =
        Midi::notes_in_range(&daw, loc.clone(), daw_proto::midi::PpqRange::new(0.5, 1.5));
    // After delete: notes are at 0..1 (pitch 60) and 2..3 (pitch 67).
    // Range 0.5..1.5 overlaps only the first.
    assert_eq!(in_range.len(), 1);
    assert_eq!(in_range[0].pitch, 60);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn midi_quantize_snaps_to_grid() {
    use daw_proto::midi::{Midi, MidiNoteCreate, QuantizeParams};
    use daw_proto::{TrackRef, Tracks};

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let tg = Tracks::add(&daw, ctx.clone(), "MIDI", None).unwrap();
    let loc = Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(tg), 0.0, 4.0).unwrap();

    Midi::add_note(&daw, loc.clone(), MidiNoteCreate::new(60, 100, 0.97, 0.5));
    Midi::add_note(&daw, loc.clone(), MidiNoteCreate::new(60, 100, 2.05, 0.5));

    Midi::quantize_notes(
        &daw,
        loc.clone(),
        QuantizeParams {
            indices: vec![0, 1],
            grid_ppq: 1.0,
            strength: 1.0, // full snap
        },
    );
    let notes = Midi::notes(&daw, loc);
    assert!((notes[0].start_ppq - 1.0).abs() < 1e-9);
    assert!((notes[1].start_ppq - 2.0).abs() < 1e-9);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn track_channels_round_trip() {
    use daw_proto::{RecordInput, TrackRef, Tracks};

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let guid = Tracks::add(&daw, ctx.clone(), "Vocals", None).unwrap();
    let tref = TrackRef::Guid(guid);

    // Default = stereo until set.
    assert_eq!(daw.track_num_channels(&ctx, &tref), Some(2));

    Tracks::set_num_channels(&daw, ctx.clone(), tref.clone(), 57).unwrap();
    assert_eq!(daw.track_num_channels(&ctx, &tref), Some(57));

    // Clamped above 128.
    Tracks::set_num_channels(&daw, ctx.clone(), tref.clone(), 999).unwrap();
    assert_eq!(daw.track_num_channels(&ctx, &tref), Some(128));

    // Mono allowed.
    Tracks::set_num_channels(&daw, ctx.clone(), tref.clone(), 1).unwrap();
    assert_eq!(daw.track_num_channels(&ctx, &tref), Some(1));

    // Record input round-trips.
    Tracks::set_record_input(&daw, ctx.clone(), tref.clone(), RecordInput::midi_all()).unwrap();
    assert!(matches!(
        daw.track_record_input(&ctx, &tref),
        Some(RecordInput::Midi { .. })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn playhead_reads_stay_monotonic_under_concurrent_soft_clock() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    Transport::play(&daw, ctx.clone()).unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let mut readers = Vec::new();
    for _ in 0..3 {
        let daw_ = daw.clone();
        let stop_ = stop.clone();
        let ctx_ = ctx.clone();
        readers.push(tokio::spawn(async move {
            let mut prev = -1.0f64;
            let mut samples = 0;
            while !stop_.load(Ordering::Relaxed) {
                let now = Transport::get_position(&daw_, ctx_.clone());
                assert!(
                    now >= prev - 1e-6,
                    "playhead went backwards: prev={prev}, now={now}"
                );
                prev = now;
                samples += 1;
                // Let other tasks run.
                if samples % 64 == 0 {
                    tokio::task::yield_now().await;
                }
            }
            samples
        }));
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    stop.store(true, Ordering::Relaxed);
    for h in readers {
        let n = h.await.unwrap();
        assert!(n > 100, "reader saw only {n} samples in 200ms");
    }
    // Final playhead should be ≥ 100ms.
    assert!(daw.get_position(ctx) > 0.1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_state_mirrors_engine_playhead() {
    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;

    Transport::play(&daw, ctx.clone()).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let state = daw.get_state(ctx);
    assert!(matches!(state.play_state, PlayState::Playing));
    let secs = state.playhead_position.time.as_ref().unwrap().as_seconds();
    assert!(secs > 0.05, "state-mirrored playhead = {secs}");
}

/// `add_notes` takes REAPER's contract — start in project quarter notes,
/// length in ticks at 960 a quarter — and stores quarter notes, which is
/// what `notes()` and the renderer read. A generated quarter-note click
/// (length 960) is one quarter long, not 960.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn add_notes_takes_lengths_in_ticks() {
    use daw_proto::midi::{Midi, MidiNoteCreate};
    use daw_proto::{TrackRef, Tracks};

    let (daw, _guid) = seeded();
    let ctx = ProjectContext::Current;
    let track = Tracks::add(&daw, ctx.clone(), "Click", None).unwrap();
    let loc = Midi::create_midi_item(&daw, ctx, TrackRef::Guid(track), 0.0, 4.0)
        .expect("MIDI item created");
    Midi::add_notes(
        &daw,
        loc.clone(),
        vec![MidiNoteCreate::new(60, 100, 2.0, 960.0)],
    );
    Midi::add_notes_ppq(
        &daw,
        loc.clone(),
        vec![MidiNoteCreate::new(62, 100, 3.0, 0.5)],
    );
    let notes = Midi::notes(&daw, loc);
    assert_eq!(notes[0].start_ppq, 2.0);
    assert!(
        (notes[0].length_ppq - 1.0).abs() < 1e-9,
        "{}",
        notes[0].length_ppq
    );
    assert!(
        (notes[1].length_ppq - 0.5).abs() < 1e-9,
        "the round trip is as given"
    );
}
