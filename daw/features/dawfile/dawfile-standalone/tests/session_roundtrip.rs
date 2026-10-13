//! What a **session** needs to survive the trip, not just what an `.rpp`
//! parser happens to model.
//!
//! The session domain opens an `.RPP`, runs a prepare pass over it —
//! folders, busses and sends, generated click / count / guide tracks,
//! SONG and SECTIONS marker lanes, the tempo map from the chart, colours
//! — saves the result as `.session`, and later exports back to `.rpp` so
//! REAPER can open the same arrangement. Every one of those has to come
//! back, and each of the tests here pins one of them.
//!
//! The material is a real session when the machine has one:
//! `DAW_SESSION_RPP` (or the checked-in path used by
//! [`real_session`]) points at `Always On Time.RPP`, 21 stereo stems.
//! The assertions that do not need it run on a fixture, because a test
//! that silently skips is a test that silently stops working.

use dawfile_standalone::{
    DawProject, DocumentEdit, DocumentQuery, EntityId, ReceiveNode, SourceRef,
};
use std::path::PathBuf;

/// The real session, when this machine has it.
///
/// Returned rather than asserted so the suite still runs on a fresh
/// checkout; every test that uses it also has a fixture-backed sibling
/// that always runs.
fn real_session() -> Option<PathBuf> {
    let path = std::env::var("DAW_SESSION_RPP")
        .map(PathBuf::from)
        .ok()
        .or(Some(PathBuf::from(
            "/Volumes/build-disk/development/sessions/Always On Time/Always On Time.RPP",
        )))?;
    path.is_file().then_some(path)
}

/// A small project with the shape a prepare pass starts from: a handful
/// of flat stem tracks and nothing else.
const STEMS: &str = r#"<REAPER_PROJECT 0.1 "7.75/test" 1790015830 0
  SAMPLERATE 48000 0 0
  TEMPO 120 4 4
  <TRACK {AAAAAAAA-0001-0000-0000-000000000000}
    NAME Kick
    VOLPAN 1 0 -1 -1 1
    MUTESOLO 0 0 0
    IPHASE 0
    ISBUS 0 0
    SEL 0
    TRACKID {AAAAAAAA-0001-0000-0000-000000000000}
    NCHAN 2
    MAINSEND 1 0
  >
  <TRACK {AAAAAAAA-0002-0000-0000-000000000000}
    NAME Snare
    VOLPAN 1 0 -1 -1 1
    MUTESOLO 0 0 0
    IPHASE 0
    ISBUS 0 0
    SEL 0
    TRACKID {AAAAAAAA-0002-0000-0000-000000000000}
    NCHAN 2
    MAINSEND 1 0
  >
  <TRACK {AAAAAAAA-0003-0000-0000-000000000000}
    NAME Vox
    VOLPAN 1 0 -1 -1 1
    MUTESOLO 0 0 0
    IPHASE 0
    ISBUS 0 0
    SEL 0
    TRACKID {AAAAAAAA-0003-0000-0000-000000000000}
    NCHAN 2
    MAINSEND 1 0
  >
>
"#;

/// A REAPER MIDI source chunk — what a generated click or guide track
/// carries, and what the format holds opaquely as an object.
const MIDI_SOURCE: &str =
    "<SOURCE MIDI\n  HASDATA 1 960 QN\n  E 0 90 24 60\n  E 960 80 24 00\n  E 960 b0 7b 00\n>\n";

fn reimport(project: &DawProject) -> DawProject {
    let exported = project.to_rpp().expect("export");
    DawProject::import_rpp(&exported, "reimported")
        .expect("the exported .rpp re-imports")
        .0
}

// ──────────────────────────────────────────────────────────────
// Folder structure
// ──────────────────────────────────────────────────────────────

#[test]
fn a_hierarchy_built_in_the_document_reaches_the_exported_rpp() {
    // The single most common thing an organisation pass does: take flat
    // stems and put them under busses. `parent` is the document's truth;
    // `ISBUS` is what REAPER reads, and without the encoding being
    // rebuilt from the parent links the project exports back flat.
    let (mut project, _) = DawProject::import_rpp(STEMS, "Stems").expect("import");
    let kick = project.document().tracks[0].id.clone();
    let snare = project.document().tracks[1].id.clone();
    let vox = project.document().tracks[2].id.clone();

    let (drums, all) = project.edit(|document| {
        let drums = document.add_track("DRUM BUS");
        let all = document.add_track("MIX BUS");
        // Arrange order has to be the depth-first order REAPER reads:
        // bus, then its children.
        document.move_track_before(&all, Some(&kick)).expect("move");
        document
            .move_track_before(&drums, Some(&kick))
            .expect("move");
        document
            .set_parent(&drums, Some(all.clone()))
            .expect("nest");
        document
            .set_parent(&kick, Some(drums.clone()))
            .expect("nest");
        document
            .set_parent(&snare, Some(drums.clone()))
            .expect("nest");
        document.set_parent(&vox, Some(all.clone())).expect("nest");
        (drums, all)
    });

    let document = reimport(&project);
    let document = document.document();
    assert!(document.check_invariants().is_empty());

    assert_eq!(
        document.track(&kick).expect("kick").parent.as_ref(),
        Some(&drums)
    );
    assert_eq!(
        document.track(&snare).expect("snare").parent.as_ref(),
        Some(&drums)
    );
    assert_eq!(
        document.track(&vox).expect("vox").parent.as_ref(),
        Some(&all)
    );
    assert_eq!(
        document.track(&drums).expect("drum bus").parent.as_ref(),
        Some(&all)
    );
    assert_eq!(document.track(&all).expect("mix bus").parent, None);

    assert!(document.track(&all).expect("mix bus").track.is_folder);
    assert!(document.track(&drums).expect("drum bus").track.is_folder);
    assert!(!document.track(&vox).expect("vox").track.is_folder);
}

#[test]
fn a_real_session_keeps_its_folder_tree_through_a_round_trip() {
    let Some(path) = real_session() else {
        return;
    };
    // The organised sibling is the prepare pass's own output — the exact
    // thing the session domain will be asking this format to hold.
    let organised = path.with_file_name(format!(
        "{}.organized.RPP",
        path.file_stem().expect("stem").to_string_lossy()
    ));
    let Ok(text) = std::fs::read_to_string(&organised) else {
        return;
    };

    let (project, _) = DawProject::import_rpp(&text, "organised").expect("import");
    let before: Vec<(String, Option<String>, bool)> = project
        .document()
        .tracks
        .iter()
        .map(|node| {
            (
                node.id.to_string(),
                node.parent.as_ref().map(ToString::to_string),
                node.track.is_folder,
            )
        })
        .collect();
    assert!(
        before.iter().any(|(_, parent, _)| parent.is_some()),
        "the organised session should have a folder tree to preserve"
    );

    // Touch one unrelated value so the export takes the patch path rather
    // than handing back the original bytes.
    let mut project = project;
    let first = project.document().tracks[0].id.clone();
    project.edit(|document| document.track_mut(&first).expect("track").track.selected = true);

    let after: Vec<(String, Option<String>, bool)> = reimport(&project)
        .document()
        .tracks
        .iter()
        .map(|node| {
            (
                node.id.to_string(),
                node.parent.as_ref().map(ToString::to_string),
                node.track.is_folder,
            )
        })
        .collect();
    assert_eq!(before, after, "the folder tree changed on the round trip");
}

// ──────────────────────────────────────────────────────────────
// Routing
// ──────────────────────────────────────────────────────────────

#[test]
fn sends_drawn_in_the_document_reach_the_exported_rpp() {
    let (mut project, _) = DawProject::import_rpp(STEMS, "Stems").expect("import");
    let kick = project.document().tracks[0].id.clone();
    let snare = project.document().tracks[1].id.clone();

    let bus = project.edit(|document| {
        let bus = document.add_track("DRUM BUS");
        document.add_send(&kick, &bus).expect("send");
        document.add_send(&snare, &bus).expect("send");
        bus
    });

    let document = reimport(&project);
    let document = document.document();
    let sources: Vec<String> = document
        .track(&bus)
        .expect("bus")
        .receives
        .iter()
        .map(|receive| receive.source.to_string())
        .collect();
    assert_eq!(sources, vec![kick.to_string(), snare.to_string()]);
    assert!(document.check_invariants().is_empty());
}

#[test]
fn removing_a_track_repoints_sends_rather_than_shifting_them() {
    // `AUXRECV` names its source by track **index**, so deleting a track
    // above a send silently re-points it at whatever moves into that row.
    // The document names the source by id, and the exporter resolves the
    // index against the track list it is actually writing — this is the
    // test that the resolution happens at the right moment.
    let (mut project, _) = DawProject::import_rpp(STEMS, "Stems").expect("import");
    let kick = project.document().tracks[0].id.clone();
    let snare = project.document().tracks[1].id.clone();
    let vox = project.document().tracks[2].id.clone();

    let bus = project.edit(|document| {
        let bus = document.add_track("MIX BUS");
        document.add_send(&vox, &bus).expect("send");
        bus
    });
    // Delete the two tracks *above* the send's source. Under positional
    // routing the bus would now be fed by whatever landed on index 2.
    project.edit(|document| {
        document.remove_track(&kick).expect("remove");
        document.remove_track(&snare).expect("remove");
    });

    let exported = project.to_rpp().expect("export");
    let (reimported, _) = DawProject::import_rpp(&exported, "reimported").expect("re-import");
    let document = reimported.document();

    let receives = &document.track(&bus).expect("bus").receives;
    assert_eq!(receives.len(), 1, "the send survived exactly once");
    assert_eq!(
        receives[0].source, vox,
        "the send must still come from Vox, not from whoever now sits at its old index"
    );
}

#[test]
fn a_send_into_a_removed_track_is_dropped_rather_than_left_dangling() {
    let (mut project, _) = DawProject::import_rpp(STEMS, "Stems").expect("import");
    let kick = project.document().tracks[0].id.clone();

    let bus = project.edit(|document| {
        let bus = document.add_track("DRUM BUS");
        document.add_send(&kick, &bus).expect("send");
        bus
    });
    project.edit(|document| {
        document.remove_track(&kick).expect("remove");
    });

    assert!(
        project
            .document()
            .track(&bus)
            .expect("bus")
            .receives
            .is_empty(),
        "a send out of a deleted track is not a send"
    );
    assert!(project.document().check_invariants().is_empty());
}

#[test]
fn a_send_to_a_track_that_is_not_in_the_document_is_a_named_violation() {
    let (mut project, _) = DawProject::import_rpp(STEMS, "Stems").expect("import");
    let bus = project.edit(|document| {
        let bus = document.add_track("DRUM BUS");
        let absent = EntityId::adopt("{NOBODY-HOME}");
        document
            .track_mut(&bus)
            .expect("bus")
            .receives
            .push(ReceiveNode::new(absent));
        bus
    });

    let problems = project.document().check_invariants();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("NOBODY-HOME")),
        "a dangling send must be named, not silently dropped: {problems:?}"
    );
    assert!(project.document().track(&bus).is_some());
}

#[test]
fn a_real_session_keeps_its_routing_through_a_round_trip() {
    let Some(path) = real_session() else {
        return;
    };
    let organised = path.with_file_name(format!(
        "{}.organized.RPP",
        path.file_stem().expect("stem").to_string_lossy()
    ));
    let Ok(text) = std::fs::read_to_string(&organised) else {
        return;
    };

    let (mut project, _) = DawProject::import_rpp(&text, "organised").expect("import");
    let before: Vec<(String, Vec<String>)> = project
        .document()
        .tracks
        .iter()
        .map(|node| {
            (
                node.id.to_string(),
                node.receives
                    .iter()
                    .map(|receive| receive.source.to_string())
                    .collect(),
            )
        })
        .collect();
    let total: usize = before.iter().map(|(_, sends)| sends.len()).sum();
    assert!(
        total > 0,
        "the organised session should be mixed through busses"
    );

    let first = project.document().tracks[0].id.clone();
    project.edit(|document| document.track_mut(&first).expect("track").track.selected = true);

    let after: Vec<(String, Vec<String>)> = reimport(&project)
        .document()
        .tracks
        .iter()
        .map(|node| {
            (
                node.id.to_string(),
                node.receives
                    .iter()
                    .map(|receive| receive.source.to_string())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        before, after,
        "{total} send(s) did not survive the round trip"
    );
}

// ──────────────────────────────────────────────────────────────
// Generated MIDI: click, count-in and guide
// ──────────────────────────────────────────────────────────────

#[test]
fn a_generated_midi_track_survives_the_round_trip() {
    // The click, count and guide tracks the session generates are MIDI
    // items. Their event data is carried as an object rather than
    // modelled (#162), so this pins the thing that actually matters: the
    // bytes come back, on the right take, byte for byte.
    let (mut project, _) = DawProject::import_rpp(STEMS, "Stems").expect("import");
    let object = project.put_object(MIDI_SOURCE.as_bytes().to_vec());

    let (click, item) = project.edit(|document| {
        let click = document.add_track("Click");
        let item = document.add_item(&click, 0.0, 8.0).expect("item");
        document
            .add_take(
                &item,
                SourceRef::Object {
                    object: object.clone(),
                    kind: "MIDI".into(),
                },
            )
            .expect("take");
        (click, item)
    });

    let exported = project.to_rpp().expect("export");
    assert!(
        exported.contains("HASDATA 1 960 QN") && exported.contains("E 960 b0 7b 00"),
        "the MIDI events did not reach the .rpp:\n{exported}"
    );

    let (reimported, _) = DawProject::import_rpp(&exported, "reimported").expect("re-import");
    let document = reimported.document();
    let track = document
        .track(&click)
        .expect("the click track reached the .rpp");
    assert_eq!(track.track.name, "Click");
    let node = document.item(&item).expect("the item reached the .rpp");
    assert_eq!(node.item.length.as_seconds(), 8.0);

    let take = &node.takes[0];
    assert!(take.take.is_midi, "the take should come back as MIDI");
    let SourceRef::Object { object: back, kind } = &take.source else {
        panic!(
            "a MIDI take must come back as an object source, got {:?}",
            take.source
        );
    };
    assert_eq!(kind, "MIDI");
    // Compared with the trailing newline trimmed: the chunk tree
    // normalises the line ending on the way out, which is the same
    // normalisation `dawfile-reaper`'s own round-trip test documents.
    let stored = reimported.objects().get(back).expect("the MIDI bytes");
    assert_eq!(
        String::from_utf8_lossy(stored).trim_end(),
        MIDI_SOURCE.trim_end(),
        "the MIDI source data changed on the round trip"
    );
}

// ──────────────────────────────────────────────────────────────
// Markers and regions — the SONG and SECTIONS lanes
// ──────────────────────────────────────────────────────────────

#[test]
fn markers_and_regions_added_in_the_document_reach_the_exported_rpp() {
    let (mut project, _) = DawProject::import_rpp(STEMS, "Stems").expect("import");

    let (song_start, verse, chorus) = project.edit(|document| {
        let song_start = document.add_marker(4.0, "SONGSTART");
        let verse = document.add_region(4.0, 20.0, "Verse 1");
        let chorus = document.add_region(20.0, 36.0, "Chorus");
        // The ruler lanes the session uses: SONG on lane 0, SECTIONS on
        // lane 1. Lanes are data, not a view preference.
        document.markers[1].marker.lane = Some(1);
        document.markers[2].marker.lane = Some(1);
        document.markers[2].marker.color = Some(16576);
        (song_start, verse, chorus)
    });

    let reimported = reimport(&project);
    let document = reimported.document();
    assert_eq!(document.markers.len(), 3);

    let find = |id: &EntityId| {
        document
            .markers
            .iter()
            .find(|node| &node.id == id)
            .unwrap_or_else(|| panic!("marker {id} did not survive"))
    };

    let start = find(&song_start);
    assert_eq!(start.marker.name, "SONGSTART");
    assert_eq!(start.marker.position_seconds(), 4.0);
    assert_eq!(start.region_end_seconds, None, "a point marker has no end");

    let verse = find(&verse);
    assert_eq!(verse.marker.name, "Verse 1");
    assert_eq!(verse.region_end_seconds, Some(20.0));
    assert_eq!(
        verse.marker.lane,
        Some(1),
        "the SECTIONS lane is part of the data"
    );

    let chorus = find(&chorus);
    assert_eq!(chorus.marker.position_seconds(), 20.0);
    assert_eq!(chorus.region_end_seconds, Some(36.0));
    assert_eq!(chorus.marker.color, Some(16576));
}

#[test]
fn a_marker_removed_in_the_document_loses_its_line() {
    let (mut project, _) = DawProject::import_rpp(STEMS, "Stems").expect("import");
    let kept = project.edit(|document| {
        document.add_marker(1.0, "Keep");
        let doomed = document.add_region(2.0, 6.0, "Drop");
        let kept = document.markers[0].id.clone();
        document.markers.retain(|node| node.id != doomed);
        kept
    });

    let exported = project.to_rpp().expect("export");
    assert!(
        !exported.contains("Drop"),
        "a removed region kept its line:\n{exported}"
    );

    let (reimported, _) = DawProject::import_rpp(&exported, "reimported").expect("re-import");
    assert_eq!(reimported.document().markers.len(), 1);
    assert_eq!(reimported.document().markers[0].id, kept);
}

// ──────────────────────────────────────────────────────────────
// Tempo map
// ──────────────────────────────────────────────────────────────

#[test]
fn a_tempo_map_set_on_the_document_reaches_the_exported_rpp() {
    use daw_proto::primitives::{Position, PositionInSeconds, TimeSignature};
    use daw_proto::tempo_map::TempoPoint;

    // The chart's tempo map, applied to a project that had none — which
    // is exactly what happens when a keyflow song is laid over stems.
    let (mut project, _) = DawProject::import_rpp(STEMS, "Stems").expect("import");
    project.edit(|document| {
        document.tempo_map = vec![
            TempoPoint {
                position: Position::from_time(PositionInSeconds::from_seconds(0.0)),
                bpm: 68.0,
                time_signature: Some(TimeSignature::new(4, 4)),
                shape: Some(1),
                bezier_tension: None,
                selected: Some(false),
                linear: Some(false),
            },
            TempoPoint {
                position: Position::from_time(PositionInSeconds::from_seconds(32.0)),
                bpm: 72.0,
                time_signature: Some(TimeSignature::new(6, 8)),
                shape: Some(1),
                bezier_tension: None,
                selected: Some(false),
                linear: Some(false),
            },
        ];
    });

    let exported = project.to_rpp().expect("export");
    assert!(
        exported.contains("<TEMPOENVEX"),
        "a project with a tempo map needs the chunk:\n{exported}"
    );
    assert!(
        exported.contains("TEMPO 68 4 4"),
        "the transport tempo must follow the first point:\n{exported}"
    );

    let (reimported, _) = DawProject::import_rpp(&exported, "reimported").expect("re-import");
    let map = &reimported.document().tempo_map;
    assert_eq!(map.len(), 2);
    assert_eq!(map[0].bpm, 68.0);
    assert_eq!(map[0].time_signature, Some(TimeSignature::new(4, 4)));
    assert_eq!(map[1].position_seconds(), 32.0);
    assert_eq!(map[1].bpm, 72.0);
    assert_eq!(
        map[1].time_signature,
        Some(TimeSignature::new(6, 8)),
        "a signature change must survive the packed field"
    );
}

// ──────────────────────────────────────────────────────────────
// Mixer state
// ──────────────────────────────────────────────────────────────

#[test]
fn colour_level_and_mute_solo_survive_the_round_trip() {
    let (mut project, _) = DawProject::import_rpp(STEMS, "Stems").expect("import");
    let kick = project.document().tracks[0].id.clone();
    let snare = project.document().tracks[1].id.clone();
    // A track the file never coloured: the colour has nowhere to go
    // unless the exporter inserts the `PEAKCOL` line.
    project.edit(|document| {
        let node = document.track_mut(&kick).expect("kick");
        node.track.color = Some(0x00_20_40_60);
        node.track.volume = 0.375;
        node.track.pan = -0.5;
        node.track.muted = true;

        let node = document.track_mut(&snare).expect("snare");
        node.track.soloed = true;
        node.track.phase_inverted = true;
    });

    let reimported = reimport(&project);
    let document = reimported.document();

    let node = document.track(&kick).expect("kick");
    assert_eq!(node.track.color, Some(0x00_20_40_60));
    assert_eq!(node.track.volume, 0.375);
    assert_eq!(node.track.pan, -0.5);
    assert!(node.track.muted);

    let node = document.track(&snare).expect("snare");
    assert!(node.track.soloed);
    assert!(node.track.phase_inverted);
}

// ──────────────────────────────────────────────────────────────
// Reopening
// ──────────────────────────────────────────────────────────────

#[test]
fn a_session_saved_and_reopened_still_exports_its_edits() {
    // The whole point of the format: edit in the app, save as ours,
    // export for REAPER *later*. An in-memory "modified" flag is cleared
    // by saving, so a reopened project would take the verbatim shortcut
    // and hand REAPER the bytes it was imported from — throwing away
    // every edit, silently, with no error anywhere.
    let (mut project, _) = DawProject::import_rpp(STEMS, "Stems").expect("import");
    let kick = project.document().tracks[0].id.clone();
    project.edit(|document| {
        document.track_mut(&kick).expect("kick").track.name = "Kick In".into();
        document.add_marker(8.0, "SONGSTART");
    });

    let dir = std::env::temp_dir().join(format!("session-reopen-{}", uuid::Uuid::new_v4()));
    project.save(&dir).expect("save");
    let reopened = DawProject::load(&dir).expect("load");

    let exported = reopened.to_rpp().expect("export");
    assert!(
        exported.contains("Kick In"),
        "the reopened session exported its original .rpp, not its edits:\n{exported}"
    );
    assert!(
        exported.contains("SONGSTART"),
        "the marker was lost on reopen"
    );

    let (back, _) = DawProject::import_rpp(&exported, "reimported").expect("re-import");
    assert_eq!(
        back.document().track(&kick).expect("kick").track.name,
        "Kick In"
    );
    assert_eq!(back.document().markers.len(), 1);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_unedited_session_still_exports_byte_for_byte_after_a_reopen() {
    // The other half of the same rule: nothing above may cost the
    // byte-faithfulness an untouched project has.
    let (mut project, _) = DawProject::import_rpp(STEMS, "Stems").expect("import");
    let dir = std::env::temp_dir().join(format!("session-pristine-{}", uuid::Uuid::new_v4()));
    project.save(&dir).expect("save");

    let reopened = DawProject::load(&dir).expect("load");
    assert_eq!(reopened.to_rpp().expect("export"), STEMS);

    std::fs::remove_dir_all(&dir).ok();
}

// ──────────────────────────────────────────────────────────────
// The whole trip, on the real session
// ──────────────────────────────────────────────────────────────

#[test]
fn a_prepared_session_survives_rpp_to_session_to_rpp() {
    // Everything above, in one pass, in the order the session domain does
    // it: open the `.RPP`, organise it, generate the click, lay the chart
    // over it, save as `.session`, reopen, export for REAPER.
    let text = match real_session() {
        Some(path) => std::fs::read_to_string(path).expect("read the session"),
        None => STEMS.to_string(),
    };
    let (mut project, _) = DawProject::import_rpp(&text, "Always On Time").expect("import");

    let stems: Vec<EntityId> = project
        .document()
        .tracks
        .iter()
        .filter(|node| !node.track.is_folder)
        .map(|node| node.id.clone())
        .take(4)
        .collect();
    assert!(
        !stems.is_empty(),
        "the session should have stems to organise"
    );

    let midi = project.put_object(MIDI_SOURCE.as_bytes().to_vec());

    let (bus, click, click_item, song_start, verse) = project.edit(|document| {
        // 1. A generated click track carrying MIDI, at the top level.
        let click = document.add_track("Click");
        document.track_mut(&click).expect("click").track.color = Some(0x00_FF_00_00);
        let click_item = document.add_item(&click, 0.0, 64.0).expect("item");
        document
            .add_take(
                &click_item,
                SourceRef::Object {
                    object: midi.clone(),
                    kind: "MIDI".into(),
                },
            )
            .expect("take");

        // 2. A bus, with the stems nested under it and sending into it.
        //
        // The stems are *moved* as well as re-parented, because `.rpp`
        // can only spell a hierarchy that matches arrange order: a folder
        // is "the rows below me, until the depth closes". A prepare pass
        // that only re-parented would describe a tree REAPER cannot read.
        let bus = document.add_track("MIX BUS");
        document.track_mut(&bus).expect("bus").track.color = Some(0x00_33_66_99);
        for stem in &stems {
            document.set_parent(stem, Some(bus.clone())).expect("nest");
            document.move_track_before(stem, None).expect("gather");
            document.add_send(stem, &bus).expect("send");
        }

        // 3. The SONG and SECTIONS lanes.
        let song_start = document.add_marker(4.0, "SONGSTART");
        let verse = document.add_region(4.0, 20.0, "Verse 1");
        if let Some(node) = document.markers.iter_mut().find(|node| node.id == verse) {
            node.marker.lane = Some(1);
        }

        // 4. The chart's tempo.
        document.tempo_map = vec![daw_proto::tempo_map::TempoPoint {
            position: daw_proto::primitives::Position::from_time(
                daw_proto::primitives::PositionInSeconds::from_seconds(0.0),
            ),
            bpm: 68.0,
            time_signature: Some(daw_proto::primitives::TimeSignature::new(4, 4)),
            shape: Some(1),
            bezier_tension: None,
            selected: Some(false),
            linear: Some(false),
        }];

        (bus, click, click_item, song_start, verse)
    });

    assert!(
        project.document().check_invariants().is_empty(),
        "{:?}",
        project.document().check_invariants()
    );

    // Save as our own format and reopen it — the app's actual path.
    let dir = std::env::temp_dir().join(format!("session-roundtrip-{}", uuid::Uuid::new_v4()));
    project.save(&dir).expect("save");
    assert!(
        dir.join("Always On Time.session").is_file(),
        "the manifest should be written as .session"
    );
    let reopened = DawProject::load(&dir).expect("load");

    // Nothing was lost on the way through the text form.
    let stored = reopened.document();
    assert_eq!(stored.tempo_map.len(), 1);
    assert_eq!(stored.markers.len(), 2);
    assert_eq!(
        stored.track(&bus).expect("bus").receives.len(),
        stems.len(),
        "the sends into the bus did not survive the .session file"
    );

    // And back out to REAPER.
    let exported = reopened.to_rpp().expect("export");
    let (back, _) = DawProject::import_rpp(&exported, "reimported").expect("re-import");
    let document = back.document();
    assert!(document.check_invariants().is_empty());

    for stem in &stems {
        assert_eq!(
            document.track(stem).expect("stem").parent.as_ref(),
            Some(&bus),
            "a stem lost its folder"
        );
    }
    let sources: Vec<EntityId> = document
        .track(&bus)
        .expect("bus")
        .receives
        .iter()
        .map(|receive| receive.source.clone())
        .collect();
    assert_eq!(sources, stems, "the bus lost its sends");
    assert_eq!(
        document.track(&bus).expect("bus").track.color,
        Some(0x00_33_66_99)
    );
    assert!(document.track(&bus).expect("bus").track.is_folder);

    let click = document.track(&click).expect("the click track");
    assert_eq!(click.track.name, "Click");
    assert_eq!(click.track.color, Some(0x00_FF_00_00));
    let item = document.item(&click_item).expect("the click item");
    assert!(item.takes[0].take.is_midi);

    assert_eq!(
        document
            .markers
            .iter()
            .find(|node| node.id == song_start)
            .expect("SONGSTART")
            .marker
            .name,
        "SONGSTART"
    );
    let verse = document
        .markers
        .iter()
        .find(|node| node.id == verse)
        .expect("the Verse 1 region");
    assert_eq!(verse.region_end_seconds, Some(20.0));
    assert_eq!(verse.marker.lane, Some(1));
    assert_eq!(document.tempo_map.len(), 1);
    assert_eq!(document.tempo_map[0].bpm, 68.0);

    std::fs::remove_dir_all(&dir).ok();
}
