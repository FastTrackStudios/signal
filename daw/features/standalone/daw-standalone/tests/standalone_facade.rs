//! Exercises the newly-implemented standalone trait surfaces:
//! `Projects`, `WindowManager`, `Diagnostics`, `BatchExecution`,
//! plus the FX preset / named-config side stores. Each test is a
//! quick happy-path check that the trait method behaves the way a
//! consumer would expect against the in-memory backend.

use daw_proto::project::ProjectContext;
use daw_proto::{ProjectInfo, Projects};
use daw_standalone::sync::Standalone;

#[test]
fn projects_lifecycle_create_list_select_close() {
    let daw = Standalone::new();
    assert!(daw.list().is_empty(), "fresh daw has no projects");
    assert!(daw.current().is_none());

    // `seed_project` is the long-standing setup helper. We also want
    // `Projects::create` to mint a fresh tab.
    let created = daw.create().expect("create() yields a project info");
    assert!(!created.guid.is_empty());

    let listed = daw.list();
    assert_eq!(listed.len(), 1, "create should add one project");
    assert_eq!(listed[0].guid, created.guid);
    assert_eq!(daw.current().map(|p| p.guid), Some(created.guid.clone()));
    assert_eq!(
        daw.get(&created.guid).map(|p| p.guid),
        Some(created.guid.clone())
    );
    assert_eq!(
        daw.get_by_slot(0).map(|p| p.guid),
        Some(created.guid.clone())
    );

    // Add a second project explicitly so list ordering is stable.
    daw.seed_project(ProjectInfo {
        guid: "second".into(),
        name: "Zeta".into(),
        path: String::new(),
    });
    let listed = daw.list();
    assert_eq!(listed.len(), 2);
    // Sorted by name → "Untitled-…" before "Zeta".
    assert!(listed[0].name.starts_with("Untitled"));
    assert_eq!(listed[1].name, "Zeta");

    // Select switches the current pointer.
    assert!(daw.select("second"));
    assert_eq!(daw.current().map(|p| p.guid), Some("second".into()));
    assert!(!daw.select("does-not-exist"));

    // Close the focused project — current should fall back.
    assert!(daw.close("second"));
    assert!(daw.get("second").is_none());
    assert_eq!(daw.current().map(|p| p.guid), Some(created.guid));
}

#[test]
fn projects_info_string_and_numeric_round_trip() {
    let daw = Standalone::new();
    let info = daw.create().unwrap();
    let ctx = ProjectContext::Project(info.guid.clone());

    daw.set_project_info_string(ctx.clone(), "PROJECT_NAME", "My Song");
    daw.set_project_info_string(ctx.clone(), "PROJECT_NOTES", "scratch notes");
    daw.set_project_info(ctx.clone(), "PROJECT_BPM", 140.0);

    assert_eq!(
        daw.get_project_info_string(ctx.clone(), "PROJECT_NAME"),
        "My Song"
    );
    assert_eq!(
        daw.get_project_info_string(ctx.clone(), "PROJECT_NOTES"),
        "scratch notes"
    );
    assert!((daw.get_project_info(ctx.clone(), "PROJECT_BPM") - 140.0).abs() < 1e-6);
    assert_eq!(daw.get_project_info(ctx, "UNKNOWN_KEY"), 0.0);
}

#[test]
fn ruler_lane_names_round_trip() {
    let daw = Standalone::new();
    let info = daw.create().unwrap();
    let ctx = ProjectContext::Project(info.guid);

    daw.set_ruler_lane_name(ctx.clone(), 0, "Markers");
    daw.set_ruler_lane_name(ctx.clone(), 1, "Regions");
    assert_eq!(daw.get_ruler_lane_name(ctx.clone(), 0), "Markers");
    assert_eq!(daw.get_ruler_lane_name(ctx.clone(), 1), "Regions");
    assert_eq!(daw.ruler_lane_count(ctx), 2);
}

#[test]
fn window_manager_save_apply_delete() {
    use daw_proto::window_manager::{WindowLayout, WindowLayoutOptions, WindowManager};

    let daw = Standalone::new();
    let layout = WindowLayout {
        name: "Mixing".into(),
        ..Default::default()
    };
    let save = daw.save_layout(layout.clone());
    assert!(save.ok, "save should succeed: {save:?}");

    let listed = daw.list_layouts();
    assert!(listed.iter().any(|l| l.name == "Mixing"));

    let apply = daw.apply_layout("Mixing".into(), WindowLayoutOptions::default());
    assert!(apply.ok);
    assert_eq!(daw.current_layout().map(|l| l.name), Some("Mixing".into()));

    let fetched = daw.get_layout("Mixing".into());
    assert!(fetched.is_some());

    assert!(daw.delete_layout("Mixing".into()).ok);
    assert!(daw.get_layout("Mixing".into()).is_none());
    assert!(daw.current_layout().is_none());

    // Applying a missing layout fails cleanly.
    let bad = daw.apply_layout("DoesNotExist".into(), WindowLayoutOptions::default());
    assert!(!bad.ok);
}

#[test]
fn diagnostics_returns_empties_no_panic() {
    use daw_proto::diagnostics::Diagnostics;

    let daw = Standalone::new();
    let info = daw.create().unwrap();
    let ctx = ProjectContext::Project(info.guid);

    assert!(daw.hub_publish_latency_us(ctx, 4).is_empty());
    assert!(daw.audio_sync_snapshot().is_none());
    assert!(daw.audio_sync_observe(8, 100).is_empty());
    assert!(daw.audio_sync_peers().is_empty());
    assert_eq!(daw.audio_sync_self_peer_id(), "");
    assert!(daw.audio_sync_seed_peer("p1", "127.0.0.1:7000").is_ok());
    let drift = daw.audio_sync_drift_decision();
    assert!(drift.drift_seconds.is_nan());
    assert!(daw.audio_sync_peer_projects("p1").is_empty());
    assert!(daw.audio_sync_local_projects().is_empty());
}

#[test]
fn batch_execution_routes_into_service_impls() {
    use daw_proto::batch::{
        BatchExecution, BatchInstruction, BatchOp, BatchOptions, BatchRequest, ProjectArg,
        StepOutcome,
    };
    use daw_proto::marker::MarkersOp;

    let daw = Standalone::new();
    let info = daw.create().unwrap();
    let arg = ProjectArg::Literal(ProjectContext::Project(info.guid));
    let req = BatchRequest {
        instructions: vec![
            BatchInstruction {
                step: 0,
                op: BatchOp::Marker(MarkersOp::Add {
                    project: arg.clone(),
                    position: 1.5,
                    name: "from-batch".into(),
                }),
            },
            BatchInstruction {
                step: 1,
                op: BatchOp::Marker(MarkersOp::All { project: arg }),
            },
        ],
        options: BatchOptions::default(),
    };
    let resp = daw.execute(req);
    assert_eq!(resp.results.len(), 2);
    for r in &resp.results {
        assert!(
            matches!(r.outcome, StepOutcome::Ok(_)),
            "batch step should execute for real on standalone: {:?}",
            r.outcome
        );
    }
    let daw_proto::batch::StepOutcome::Ok(daw_proto::batch::BatchOpOutput::Marker(
        daw_proto::marker::MarkersOpOutput::All(markers),
    )) = &resp.results[1].outcome
    else {
        panic!("step 1 should list the marker added by step 0");
    };
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].name, "from-batch");
}

#[test]
fn fx_preset_set_get_round_trip() {
    use daw_proto::Tracks;
    use daw_proto::fx::{Effects, FxChainContext, FxRef, FxTarget};

    let daw = Standalone::new();
    let info = daw.create().unwrap();
    let ctx = ProjectContext::Project(info.guid);
    let track = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let fx_guid = Effects::add(
        &daw,
        ctx.clone(),
        FxChainContext::Track(track.clone()),
        "ReaComp",
    )
    .expect("Effects::add");

    let target = FxTarget {
        context: FxChainContext::Track(track),
        fx: FxRef::Guid(fx_guid),
    };
    // Initially no preset set → preset_index returns None.
    assert!(Effects::preset_index(&daw, ctx.clone(), target.clone()).is_none());
    Effects::set_preset(&daw, ctx.clone(), target.clone(), 7).expect("set_preset");
    let idx = Effects::preset_index(&daw, ctx.clone(), target.clone()).unwrap();
    assert_eq!(idx.index, Some(7));
    Effects::next_preset(&daw, ctx.clone(), target.clone()).unwrap();
    let idx = Effects::preset_index(&daw, ctx.clone(), target.clone()).unwrap();
    assert_eq!(idx.index, Some(8));
    Effects::prev_preset(&daw, ctx.clone(), target.clone()).unwrap();
    Effects::prev_preset(&daw, ctx.clone(), target.clone()).unwrap();
    let idx = Effects::preset_index(&daw, ctx, target).unwrap();
    assert_eq!(idx.index, Some(6));
}

#[test]
fn fx_named_config_round_trips() {
    use daw_proto::Tracks;
    use daw_proto::fx::{Effects, FxChainContext, FxRef, FxTarget, SetNamedConfigRequest};

    let daw = Standalone::new();
    let info = daw.create().unwrap();
    let ctx = ProjectContext::Project(info.guid);
    let track = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let fx_guid = Effects::add(
        &daw,
        ctx.clone(),
        FxChainContext::Track(track.clone()),
        "ReaEQ",
    )
    .unwrap();

    let target = FxTarget {
        context: FxChainContext::Track(track),
        fx: FxRef::Guid(fx_guid),
    };
    assert!(Effects::named_config(&daw, ctx.clone(), target.clone(), "vendor:setting").is_none());
    Effects::set_named_config(
        &daw,
        ctx.clone(),
        SetNamedConfigRequest {
            target: target.clone(),
            key: "vendor:setting".into(),
            value: "42".into(),
        },
    )
    .unwrap();
    assert_eq!(
        Effects::named_config(&daw, ctx, target, "vendor:setting").as_deref(),
        Some("42"),
    );
}

#[test]
fn fx_state_chunk_encoded_round_trip() {
    use daw_proto::Tracks;
    use daw_proto::fx::{Effects, FxChainContext, FxRef, FxTarget};

    let daw = Standalone::new();
    let info = daw.create().unwrap();
    let ctx = ProjectContext::Project(info.guid);
    let track = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let fx_guid = Effects::add(
        &daw,
        ctx.clone(),
        FxChainContext::Track(track.clone()),
        "ReaComp",
    )
    .unwrap();

    let target = FxTarget {
        context: FxChainContext::Track(track),
        fx: FxRef::Guid(fx_guid),
    };
    let chunk = "<VST ReaComp ...>\n  STATE-BYTES\n>";
    Effects::set_state_chunk(&daw, ctx.clone(), target.clone(), chunk.as_bytes().to_vec())
        .expect("set_state_chunk");
    let encoded = Effects::state_chunk_encoded(&daw, ctx.clone(), target.clone()).unwrap();
    // Round-trip through base64.
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .unwrap();
    assert_eq!(decoded, chunk.as_bytes());

    // set_state_chunk_encoded inverse.
    let new_chunk = "<VST ReaComp ...>\n  DIFFERENT\n>";
    let new_enc = base64::engine::general_purpose::STANDARD.encode(new_chunk.as_bytes());
    Effects::set_state_chunk_encoded(&daw, ctx.clone(), target.clone(), &new_enc).unwrap();
    let round = Effects::state_chunk_encoded(&daw, ctx, target).unwrap();
    assert_eq!(round, new_enc);
}

#[test]
fn humanize_notes_is_deterministic_and_bounded() {
    use daw_proto::midi::{HumanizeParams, Midi, MidiNoteCreate, MidiTakeLocation};
    use daw_proto::{ItemRef, TrackRef, Tracks};

    let daw = Standalone::new();
    daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Project("p".into());
    let track = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let loc = Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(track), 0.0, 2.0).unwrap();
    let item_guid = match loc.item {
        ItemRef::Guid(g) => g,
        _ => panic!(),
    };
    let take_loc = MidiTakeLocation::active(ctx, ItemRef::Guid(item_guid));
    // 5 notes at deterministic positions/velocities.
    for n in 0..5u32 {
        Midi::add_note(
            &daw,
            take_loc.clone(),
            MidiNoteCreate::new(60 + n as u8, 100, n as f64, 0.5),
        );
    }
    let before: Vec<(f64, u8)> = Midi::notes(&daw, take_loc.clone())
        .into_iter()
        .map(|n| (n.start_ppq, n.velocity))
        .collect();

    Midi::humanize_notes(
        &daw,
        take_loc.clone(),
        HumanizeParams {
            indices: Vec::new(),
            timing_range_ppq: 0.05,
            velocity_range: 10,
        },
    );
    let after: Vec<(f64, u8)> = Midi::notes(&daw, take_loc)
        .into_iter()
        .map(|n| (n.start_ppq, n.velocity))
        .collect();

    assert_eq!(before.len(), after.len());
    let mut moved = 0;
    for (b, a) in before.iter().zip(after.iter()) {
        assert!((b.0 - a.0).abs() <= 0.05 + 1e-9, "timing within range");
        assert!(
            (a.1 as i32 - b.1 as i32).abs() <= 10,
            "velocity within range"
        );
        if b != a {
            moved += 1;
        }
    }
    assert!(moved > 0, "humanize should mutate at least one note");
}
