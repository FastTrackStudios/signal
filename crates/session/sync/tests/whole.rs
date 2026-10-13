//! The whole project through the doc: a daw-standalone project holding
//! every kind of state the engine keeps — comping, every take, input and
//! monitoring and master FX with their parameters, channel counts and
//! record inputs, MIDI beyond notes, every kind of envelope with its
//! automation items, the master bus, ext-state, ruler lanes, the bay —
//! hosted into the doc and rebuilt from it in an empty project, which
//! reads back the same; then each kind edited as one step, and undone
//! exactly.

#![cfg(feature = "engine")]

use std::sync::Arc;

use daw_proto::automation::{
    AutomationItem, EnvelopeRef, EnvelopeShape, EnvelopeType, SendEnvelopeKind,
};
use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_proto::{CompArea, ProjectContext, ProjectInfo, RecordInput, TrackRef};
use daw_standalone::sync::{EnvelopeKey, FxChainKey, ProjectState, Standalone};
use session_sync::engine::{MediaRoot, MemoryBlobs};
use session_sync::{Bridge, SessionDoc, SessionModel};

#[test]
fn everything_round_trips_and_every_kind_undoes_exactly() -> eyre::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()?;
    rt.block_on(inner())
}

fn at(seconds: f64) -> PositionInSeconds {
    PositionInSeconds::from_seconds(seconds)
}

/// An edit straight into the engine's state, for what no service sets.
fn edit(standalone: &Standalone, f: impl FnOnce(&mut ProjectState)) {
    standalone.write_project("a", f).expect("project a");
}

async fn inner() -> eyre::Result<()> {
    let standalone = Standalone::new();
    for guid in ["a", "b"] {
        standalone.seed_project(ProjectInfo {
            guid: guid.into(),
            name: guid.into(),
            path: String::new(),
        });
    }
    let bundle = daw_standalone::bootstrap::build_in_process_daw(standalone.clone()).await?;
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?,
    );
    daw::init_from_parts(bundle.daw.clone(), runtime);
    let direct: Option<Arc<dyn session_sync::engine::Direct>> = Some(Arc::new(standalone.clone()));
    let media = MediaRoot(std::env::temp_dir());
    let blobs = Arc::new(MemoryBlobs::default());

    let a = bundle.daw.project("a").await?;
    let ids = build(&standalone, &a).await?;

    let mut me = Bridge::host(
        a.clone(),
        media.clone(),
        SessionDoc::new(),
        String::new(),
        blobs.clone(),
        direct.clone(),
    )
    .await?;
    let replica = SessionDoc::new();
    replica.import(&me.doc().snapshot()?, "remote")?;
    let b = bundle.daw.project("b").await?;
    let joined = Bridge::join(b, media.clone(), replica, blobs.clone(), direct.clone()).await?;

    let built = me.shown().clone();
    let kick = built.track("{KICK}").expect("the kick");
    assert_eq!(kick.channels, 4, "its channels were read");
    assert_eq!(kick.comping.record_lane, Some(2), "its record lane");
    assert_eq!(kick.comping.areas.len(), 1, "its comp area");
    assert_eq!(kick.input_fx.len(), 1, "its input FX");
    assert_eq!(kick.input_fx[0].preset, Some(3), "with its preset");
    assert_eq!(kick.hw_outputs.len(), 1, "its hardware output");
    assert!(kick.envelopes.contains_key("send:0:pan"), "its send's pan");
    assert_eq!(
        kick.envelopes["volume"].items.len(),
        1,
        "an automation item"
    );
    assert_eq!(built.items["{TAKES}"].other_takes.len(), 1, "both takes");
    assert_eq!(
        built.items[&ids.groove].take.midi_events.ccs.len(),
        2,
        "the CCs"
    );
    assert_eq!(built.project.monitoring_fx.len(), 1, "the monitoring FX");
    assert_eq!(built.project.master.fx.len(), 1, "the master's FX");
    assert!(
        (built.project.master.volume - 0.8).abs() < 1e-12,
        "the master"
    );
    assert!(built.project.bay.is_some(), "the bay");
    assert_eq!(&built, joined.shown(), "rebuilt the same, whole");
    rebuilt_state(&standalone);

    undo_each_kind(&standalone, &a, &mut me, &ids).await
}

/// What the test needs to find again: the engine's own ids.
struct Ids {
    groove: String,
    input_fx: String,
}

/// Project `a` with a little of every kind of state.
async fn build(standalone: &Standalone, a: &daw_control::Project) -> eyre::Result<Ids> {
    let kick = a.tracks().add_with_guid("{KICK}", "Kick", None).await?;
    let bass = a.tracks().add_with_guid("{BASS}", "Bass", None).await?;
    // Comping: three lanes, an area, the comp and the one before it.
    kick.set_lane_count(3).await?;
    kick.set_lane_name(0, "COMP").await?;
    kick.set_comp_areas(vec![CompArea {
        start: at(1.0),
        end: at(2.0),
        source_lane: 2,
        comp_lane: 0,
        fade_in: Duration::from_seconds(0.01),
        fade_out: Duration::from_seconds(0.02),
    }])
    .await?;
    kick.set_active_comp(Some(1)).await?;
    kick.set_active_comp(Some(0)).await?;
    // Recording: armed, monitored, its input, its channels, its height.
    kick.arm().await?;
    kick.set_input_monitor(daw_proto::InputMonitoringMode::Normal)
        .await?;
    kick.set_record_input(RecordInput::Audio { channel: 3 })
        .await?;
    kick.set_tcp_height(80).await?;
    daw_proto::Tracks::set_num_channels(
        standalone,
        ProjectContext::Project("a".into()),
        TrackRef::Guid("{KICK}".into()),
        4,
    )?;
    // FX on every chain.
    kick.fx_chain().add("Gate").await?;
    let input_fx = kick
        .input_fx_chain()
        .add("Input EQ")
        .await?
        .info()
        .await?
        .guid;
    a.monitoring_fx().add("Room EQ").await?;
    a.tracks()
        .master()
        .await?
        .fx_chain()
        .add("Bus Comp")
        .await?;
    // Routing: a send with its switches, a hardware output.
    let send = kick.sends().add_to("{BASS}").await?;
    send.set_volume(0.5).await?;
    send.set_mono(true).await?;
    send.set_phase_inverted(true).await?;
    kick.hardware_outputs().add(1).await?;
    // Automation: the fader's, the send's pan.
    kick.envelopes()
        .volume()
        .add_point(at(1.0), 0.5, EnvelopeShape::Linear)
        .await?;
    kick.envelopes()
        .of(EnvelopeRef::Send {
            send_index: 0,
            kind: SendEnvelopeKind::Pan,
        })
        .add_point(at(2.0), -0.5, EnvelopeShape::Square)
        .await?;
    // An item with two takes, the second playing, each its own source.
    let takes = a
        .items()
        .add_with_guid("{KICK}", "{TAKES}", at(2.0), Duration::from_seconds(1.0))
        .await?;
    let first = takes.takes().add().await?;
    first
        .set_source_file(&std::env::temp_dir().join("Kick-1.wav").to_string_lossy())
        .await?;
    first.set_name("Take 1").await?;
    let second = takes.takes().add().await?;
    second
        .set_source_file(&std::env::temp_dir().join("Kick-2.wav").to_string_lossy())
        .await?;
    second.set_name("Take 2").await?;
    second.make_active().await?;
    second
        .envelopes()
        .await?
        .pan()
        .add_point(at(0.25), 0.3, EnvelopeShape::Linear)
        .await?;
    // A MIDI item.
    let groove = bass
        .items()
        .create_midi_item_with_notes(4.0, 6.0, Vec::new())
        .await?
        .expect("a MIDI item")
        .info()
        .await?
        .guid;
    fill(standalone, &groove, &input_fx);
    a.transport().set_metronome(true).await?;
    Ok(Ids { groove, input_fx })
}

/// What no service sets, straight into the engine's state.
fn fill(standalone: &Standalone, groove: &str, input_fx: &str) {
    edit(standalone, |p| {
        p.track_ext
            .entry("{KICK}".into())
            .or_default()
            .comping
            .record_lane = Some(2);
        if let Some(t) = p.tracks.iter_mut().find(|t| t.guid == "{KICK}") {
            t.width = Some(120);
            t.lane_display = daw_proto::LaneDisplay::Big;
            t.grouping.volume_reverse = 1;
        }
        p.track_ext_state
            .insert(("{KICK}".into(), "FTS".into(), "rig".into()), "bass".into());
        if let Some(e) = p
            .fx_chains
            .get_mut(&FxChainKey::Input("{KICK}".into()))
            .and_then(|c| c.first_mut())
        {
            e.params.insert(0, 0.3);
        }
        p.fx_preset_index.insert(input_fx.into(), 3);
        p.fx_named_config
            .insert((input_fx.into(), "mode".into()), "linear".into());
        if let Some(volume) = p.envelopes.get_mut(&(
            "{KICK}".to_owned(),
            EnvelopeKey::Track(EnvelopeType::Volume),
        )) {
            volume.in_own_lane = true;
            volume.lane_height = 60;
            volume.automation_items.push((
                AutomationItem {
                    index: 0,
                    pool_id: 7,
                    name: "Swell".into(),
                    position: at(4.0),
                    length: Duration::from_seconds(2.0),
                    start_offset: Duration::from_seconds(0.0),
                    play_rate: 1.0,
                    baseline: 0.5,
                    amplitude: 1.0,
                    loop_source: true,
                    selected: false,
                },
                Vec::new(),
            ));
        }
        let take = p.takes.get(groove).map(|l| l.takes[0].guid.clone());
        if let Some(take) = take {
            midi(p, &take);
        }
        let second = p.takes.get("{TAKES}").map(|l| l.takes[1].guid.clone());
        if let Some(second) = second {
            p.stretch_modes
                .insert(second, daw_proto::StretchMode::Transient);
        }
        if let Some(item) = p.items.get_mut("{TAKES}") {
            item.item.beat_attach_mode = daw_proto::BeatAttachMode::Beats;
            item.item.group_id = Some(4);
        }
        p.master_volume = 0.8;
        p.master_pan = -0.2;
        p.master_muted = true;
        p.project_ext_state
            .insert(("FTS".into(), "song".into()), "Unbreakable".into());
        p.ruler_lanes.insert(
            1,
            daw_standalone::sync::RulerLane {
                name: "Sections".into(),
                flags: 8,
            },
        );
        p.global_automation_override = Some(daw_proto::AutomationMode::Read);
        let mut bay = daw_standalone::media_bay::BayState::default();
        bay.retained.insert("Media/Kick-1.wav".into());
        p.bay_state = Some(bay);
        p.transport.record_mode = daw_proto::RecordMode::Item;
    });
}

/// A take's MIDI beyond notes, one or two of each kind, and a muted note.
fn midi(p: &mut ProjectState, take: &str) {
    p.midi_notes.insert(
        take.into(),
        vec![daw_proto::MidiNote {
            index: 0,
            channel: 0,
            pitch: 40,
            velocity: 100,
            start_ppq: 0.0,
            length_ppq: 480.0,
            selected: false,
            muted: true,
        }],
    );
    p.midi_ccs.insert(
        take.into(),
        (0..2)
            .map(|i| daw_proto::midi::MidiCC {
                index: i,
                channel: 0,
                controller: 64,
                value: if i == 0 { 127 } else { 0 },
                position_ppq: f64::from(i) * 480.0,
                selected: false,
            })
            .collect(),
    );
    p.midi_pitch_bends.insert(
        take.into(),
        vec![daw_proto::midi::MidiPitchBend {
            index: 0,
            channel: 1,
            value: -4096,
            position_ppq: 120.0,
            selected: false,
        }],
    );
    p.midi_program_changes.insert(
        take.into(),
        vec![daw_proto::midi::MidiProgramChange {
            index: 0,
            channel: 0,
            program: 33,
            position_ppq: 0.0,
        }],
    );
    p.midi_sysex.insert(
        take.into(),
        vec![daw_proto::midi::MidiSysEx {
            index: 0,
            position_ppq: 0.0,
            data: vec![0xF0, 0x7E, 0x7F, 0x09, 0x01, 0xF7],
        }],
    );
    p.midi_channel_pressures.insert(
        take.into(),
        vec![daw_proto::midi::MidiChannelPressure {
            index: 0,
            channel: 2,
            pressure: 90,
            position_ppq: 240.0,
            selected: false,
        }],
    );
    p.midi_poly_pressures.insert(
        take.into(),
        vec![daw_proto::midi::MidiPolyPressure {
            index: 0,
            channel: 2,
            note: 40,
            pressure: 70,
            position_ppq: 260.0,
            selected: false,
        }],
    );
    p.midi_note_expressions.insert(
        take.into(),
        vec![daw_proto::midi::MidiNoteExpression {
            index: 0,
            channel: 2,
            note: 40,
            dimension: daw_proto::midi::NoteExpressionDim::Tuning,
            value: 0.25,
            position_ppq: 300.0,
            selected: false,
        }],
    );
}

/// The rebuilt project holds, in the engine's own state, what was built.
fn rebuilt_state(standalone: &Standalone) {
    let a = standalone.read_project("a", summary).expect("a");
    let b = standalone.read_project("b", summary).expect("b");
    assert_eq!(a, b, "the engine's own state rebuilt the same");
}

/// What of a project the doc's model cannot show is written the same:
/// a digest of the parts kept by guid (with engine-minted guids left
/// out), and the receives mirrored from the sends.
fn summary(p: &ProjectState) -> String {
    let mut out = Vec::new();
    let ext = &p.track_ext["{KICK}"];
    out.push(format!(
        "kick: {} {:?} {} {:?}",
        ext.num_channels, ext.record_input, ext.tcp_height_pixels, ext.comping
    ));
    let mut receives: Vec<String> = p
        .receives
        .values()
        .flatten()
        .map(|r| {
            format!(
                "{} <- {:?} {} {} {}",
                r.source_track_guid, r.dest_track_guid, r.volume, r.mono, r.phase_inverted
            )
        })
        .collect();
    receives.sort();
    out.push(format!("receives: {receives:?}"));
    let mut ext_state: Vec<String> = p
        .track_ext_state
        .iter()
        .map(|(k, v)| format!("{k:?}={v}"))
        .collect();
    ext_state.sort();
    out.push(format!("ext: {ext_state:?}"));
    let mut project_ext: Vec<String> = p
        .project_ext_state
        .iter()
        .map(|(k, v)| format!("{k:?}={v}"))
        .collect();
    project_ext.sort();
    out.push(format!("project ext: {project_ext:?}"));
    out.push(format!(
        "master: {} {} {} override {:?} bay {:?} rec {:?} metronome {}",
        p.master_volume,
        p.master_pan,
        p.master_muted,
        p.global_automation_override,
        p.bay_state
            .as_ref()
            .map(daw_standalone::media_bay::BayState::serialize),
        p.transport.record_mode,
        p.transport.metronome,
    ));
    out.push(format!("ruler: {:?}", p.ruler_lanes));
    let takes = &p.takes["{TAKES}"];
    out.push(format!(
        "takes: {} active {} names {:?}",
        takes.takes.len(),
        takes.active_idx,
        takes
            .takes
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
    ));
    out.join("\n")
}

/// The model as the bridge shows it now — what the engine reads as.
fn shown(me: &Bridge) -> SessionModel {
    me.shown().clone()
}

/// `label`'s edit, made just before, is one step of the doc's, and
/// undoing it puts the project back exactly — then a step that changes
/// nothing is undone as nothing, not as the step before it.
async fn undoes(me: &mut Bridge, before: &SessionModel, label: &str) -> eyre::Result<()> {
    assert!(me.local_step(label).await?, "{label}: recorded");
    assert_ne!(&shown(me), before, "{label}: changed something");
    assert_eq!(me.next_undo().as_deref(), Some(label));
    assert_eq!(me.undo().await?.as_deref(), Some(label));
    assert_eq!(&shown(me), before, "{label}: undone exactly");
    // The engine reads as the doc: a step now finds nothing to record —
    // and is undone first, as nothing.
    assert!(
        !me.local_step("Nothing").await?,
        "{label}: engine as before"
    );
    assert_eq!(me.undo().await?.as_deref(), Some("Nothing"));
    assert_eq!(&shown(me), before, "{label}: nothing undone as nothing");
    Ok(())
}

async fn undo_each_kind(
    standalone: &Standalone,
    a: &daw_control::Project,
    me: &mut Bridge,
    ids: &Ids,
) -> eyre::Result<()> {
    let kick = a.tracks().by_guid("{KICK}").await?.expect("the kick");

    let before = shown(me);
    kick.set_comp_areas(Vec::new()).await?;
    undoes(me, &before, "Clear comp").await?;

    let before = shown(me);
    a.items()
        .by_guid("{TAKES}")
        .await?
        .expect("the takes")
        .takes()
        .by_index(0)
        .await?
        .expect("the first")
        .make_active()
        .await?;
    undoes(me, &before, "Play take 1").await?;

    let before = shown(me);
    a.items()
        .by_guid("{TAKES}")
        .await?
        .expect("the takes")
        .takes()
        .add()
        .await?;
    undoes(me, &before, "Add take").await?;

    let before = shown(me);
    kick.input_fx_chain().add("De-esser").await?;
    undoes(me, &before, "Add input FX").await?;

    let before = shown(me);
    daw_proto::Tracks::set_num_channels(
        standalone,
        ProjectContext::Project("a".into()),
        TrackRef::Guid("{KICK}".into()),
        6,
    )?;
    undoes(me, &before, "Channels").await?;

    let before = shown(me);
    kick.set_record_input(RecordInput::Midi {
        device_id: None,
        channel: Some(2),
    })
    .await?;
    undoes(me, &before, "Record input").await?;

    let before = shown(me);
    kick.set_tcp_height(140).await?;
    undoes(me, &before, "Height").await?;

    let groove = ids.groove.clone();
    let before = shown(me);
    edit(standalone, |p| {
        let take = p.takes[&groove].takes[0].guid.clone();
        if let Some(ccs) = p.midi_ccs.get_mut(&take) {
            ccs[1].value = 64;
        }
    });
    undoes(me, &before, "Edit CC").await?;

    let before = shown(me);
    edit(standalone, |p| {
        let take = p.takes[&groove].takes[0].guid.clone();
        p.midi_sysex.remove(&take);
    });
    undoes(me, &before, "Remove sysex").await?;

    let before = shown(me);
    edit(standalone, |p| p.master_volume = 0.5);
    undoes(me, &before, "Master volume").await?;

    let before = shown(me);
    edit(standalone, |p| {
        p.project_ext_state
            .insert(("FTS".into(), "key".into()), "E".into());
    });
    undoes(me, &before, "Project ext").await?;

    // A track's own key (no section): what the window keeps on a track —
    // its markers and regions, a leveled vocal's lanes.
    let before = shown(me);
    edit(standalone, |p| {
        p.track_ext_state.insert(
            ("{KICK}".into(), String::new(), "fts.marks".into()),
            "{\"regions\":[]}".into(),
        );
    });
    undoes(me, &before, "Track ext").await?;

    let before = shown(me);
    edit(standalone, |p| {
        if let Some(lane) = p.ruler_lanes.get_mut(&1) {
            lane.name = "Chords".into();
        }
    });
    undoes(me, &before, "Rename ruler lane").await?;

    let before = shown(me);
    edit(standalone, |p| {
        if let Some(volume) = p.envelopes.get_mut(&(
            "{KICK}".to_owned(),
            EnvelopeKey::Track(EnvelopeType::Volume),
        )) {
            volume.automation_items[0].0.amplitude = 0.5;
        }
    });
    undoes(me, &before, "Automation item").await?;

    let before = shown(me);
    kick.envelopes()
        .of(EnvelopeRef::Send {
            send_index: 0,
            kind: SendEnvelopeKind::Pan,
        })
        .add_point(at(3.0), 0.5, EnvelopeShape::Linear)
        .await?;
    undoes(me, &before, "Send pan").await?;

    let before = shown(me);
    let input_fx = ids.input_fx.clone();
    edit(standalone, |p| {
        p.fx_preset_index.insert(input_fx.clone(), 5);
        p.fx_named_config
            .insert((input_fx.clone(), "mode".into()), "minimum".into());
        if let Some(e) = p
            .fx_chains
            .get_mut(&FxChainKey::Input("{KICK}".into()))
            .and_then(|c| c.iter_mut().find(|e| e.fx.guid == input_fx))
        {
            e.params.insert(1, 0.9);
        }
    });
    undoes(me, &before, "FX preset and params").await?;

    let before = shown(me);
    edit(standalone, |p| {
        let second = p.takes["{TAKES}"].takes[1].guid.clone();
        p.stretch_modes
            .insert(second, daw_proto::StretchMode::Tonal);
    });
    undoes(me, &before, "Stretch mode").await?;

    let before = shown(me);
    a.transport().set_metronome(false).await?;
    undoes(me, &before, "Metronome").await?;

    let before = shown(me);
    a.monitoring_fx().add("Talkback").await?;
    undoes(me, &before, "Add monitoring FX").await?;

    let before = shown(me);
    kick.hardware_outputs().add(3).await?;
    undoes(me, &before, "Add output").await?;
    Ok(())
}
