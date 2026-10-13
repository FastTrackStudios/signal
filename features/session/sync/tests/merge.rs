//! Two people editing one session at the same time.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use session_sync::{
    Change, EnvelopePointState, EnvelopeState, FxState, GainPoint, ItemField, ItemState, LaneState,
    MarkerState, NoteState, ORIGIN_LOCAL, SendState, SessionDoc, SessionModel, StretchPoint,
    TakeState, TempoPoint, TrackField, TrackState, diff,
};

fn track(guid: &str, parent: Option<&str>, name: &str) -> TrackState {
    TrackState {
        guid: guid.into(),
        parent: parent.map(Into::into),
        name: name.into(),
        volume: 1.0,
        parent_send: true,
        visible_in_tcp: true,
        visible_in_mixer: true,
        ..TrackState::default()
    }
}

fn item(track: &str, position: f64) -> ItemState {
    ItemState {
        track: track.into(),
        position,
        length: 4.0,
        volume: 1.0,
        fade_in_shape: "linear".into(),
        fade_out_shape: "linear".into(),
        take: TakeState {
            name: "Kick".into(),
            source: Some("Media/Kick.wav".into()),
            start_offset: 0.0,
            playrate: 1.0,
            ..TakeState::default()
        },
        ..ItemState::default()
    }
}

fn song() -> SessionModel {
    let mut m = SessionModel {
        tracks: vec![
            track("drums", None, "Drums"),
            track("kick", Some("drums"), "Kick"),
            track("snare", Some("drums"), "Snare"),
            track("bass", None, "Bass"),
        ],
        tempo: vec![TempoPoint {
            at: 0.0,
            bpm: 72.0,
            beats_per_bar: 4,
            beat_unit: 4,
        }],
        chart: "Song\n72bpm 4/4 #E\n\nVS 4\n1 4 6m 5\n".into(),
        ..SessionModel::default()
    };
    m.items.insert("i-kick".into(), item("kick", 0.0));
    m.items.insert("i-bass".into(), item("bass", 2.0));
    // Schema 2: a kick with lanes, a group, an FX chain, a send and its
    // automation; an edited slice and a MIDI take.
    let kick = &mut m.tracks[1];
    kick.lanes = LaneState {
        count: 3,
        names: vec!["COMP".into(), "C1".into(), "EDIT".into()],
        play_mask: 1 << 2,
    };
    kick.grouping.insert("recarm_lead".into(), 1);
    kick.grouping
        .insert("media_edit_follow".into(), 1 | (1 << 100));
    kick.fx = vec![
        FxState {
            guid: "fx-gate".into(),
            plugin: "FTS Gate".into(),
            name: "Gate".into(),
            enabled: true,
            offline: false,
            state: Some("ab12".into()),
            ..FxState::default()
        },
        FxState {
            guid: "fx-comp".into(),
            plugin: "FTS Comp".into(),
            name: "Comp".into(),
            enabled: false,
            offline: false,
            state: None,
            ..FxState::default()
        },
    ];
    kick.sends = vec![SendState {
        dest: "bass".into(),
        mode: "post_fx".into(),
        volume: 0.5,
        pan: 0.0,
        muted: false,
        from: 2,
        to: 0,
        ..SendState::default()
    }];
    kick.envelopes.insert(
        "fx:fx-gate:3".into(),
        EnvelopeState {
            mode: "read".into(),
            visible: true,
            armed: false,
            points: vec![
                EnvelopePointState {
                    at: 1.0,
                    value: 0.25,
                    shape: 0,
                    tension: 0.0,
                },
                EnvelopePointState {
                    at: 2.5,
                    value: 0.75,
                    shape: 1,
                    tension: 0.0,
                },
            ],
            ..EnvelopeState::default()
        },
    );
    let slice = m.items.get_mut("i-kick").unwrap();
    slice.fixed_lane = Some(2);
    slice.take.stretch = vec![StretchPoint {
        at: 0.5,
        source: 0.48,
        slope: 0.0,
    }];
    // A leveled kick: its take's volume curve is the song's too.
    slice.take.volume = vec![
        GainPoint {
            at: 0.995,
            value: 1.0,
        },
        GainPoint {
            at: 0.996,
            value: 1.41,
        },
    ];
    let mut midi = item("bass", 8.0);
    midi.take.source = None;
    midi.loop_source = true;
    midi.take.notes = vec![
        NoteState {
            start: 0.0,
            length: 0.25,
            pitch: 36,
            velocity: 110,
            channel: 9,
            muted: false,
        },
        NoteState {
            start: 1.0,
            length: 0.25,
            pitch: 38,
            velocity: 90,
            channel: 9,
            muted: true,
        },
    ];
    m.items.insert("i-midi".into(), midi);
    m.markers.insert(
        "m1".into(),
        MarkerState {
            at: 8.0,
            name: "Count".into(),
            color: None,
            lane: Some(1),
        },
    );
    m
}

/// Two replicas of `song()`, as two collaborators who both opened it.
fn two_peers() -> (SessionDoc, SessionDoc) {
    let a = SessionDoc::new();
    a.write(&song(), ORIGIN_LOCAL).unwrap();
    let b = SessionDoc::new();
    b.import(&a.snapshot().unwrap(), "remote").unwrap();
    (a, b)
}

/// Exchange everything both ways.
fn sync(a: &SessionDoc, b: &SessionDoc) {
    let to_b = a.updates_since(&b.version()).unwrap();
    let to_a = b.updates_since(&a.version()).unwrap();
    b.import(&to_b, "remote").unwrap();
    a.import(&to_a, "remote").unwrap();
}

#[test]
fn a_model_reads_back_as_written() {
    let doc = SessionDoc::new();
    doc.write(&song(), ORIGIN_LOCAL).unwrap();
    assert_eq!(doc.read(), song());
}

#[test]
fn writing_the_same_model_again_records_nothing() {
    let doc = SessionDoc::new();
    doc.write(&song(), ORIGIN_LOCAL).unwrap();
    let before = doc.version();
    doc.write(&song(), ORIGIN_LOCAL).unwrap();
    assert_eq!(doc.version(), before);
}

#[test]
fn one_moves_an_item_while_the_other_turns_it_down() {
    let (a, b) = two_peers();
    let mut ma = a.read();
    ma.items.get_mut("i-kick").unwrap().position = 1.5;
    a.write(&ma, ORIGIN_LOCAL).unwrap();
    let mut mb = b.read();
    mb.items.get_mut("i-kick").unwrap().volume = 0.5;
    b.write(&mb, ORIGIN_LOCAL).unwrap();

    sync(&a, &b);
    let merged = a.read();
    assert_eq!(merged, b.read());
    assert_eq!(merged.items["i-kick"].position, 1.5);
    assert_eq!(merged.items["i-kick"].volume, 0.5);
}

#[test]
fn one_renames_a_track_while_the_other_moves_it_into_a_folder() {
    let (a, b) = two_peers();
    let mut ma = a.read();
    ma.tracks
        .iter_mut()
        .find(|t| t.guid == "bass")
        .unwrap()
        .name = "Bass DI".into();
    a.write(&ma, ORIGIN_LOCAL).unwrap();

    let mut mb = b.read();
    let bass = mb.tracks.pop().unwrap();
    mb.tracks.insert(
        1,
        TrackState {
            parent: Some("drums".into()),
            ..bass
        },
    );
    b.write(&mb, ORIGIN_LOCAL).unwrap();

    sync(&a, &b);
    let merged = a.read();
    assert_eq!(merged, b.read());
    let names: Vec<(&str, Option<&str>)> = merged
        .tracks
        .iter()
        .map(|t| (t.name.as_str(), t.parent.as_deref()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Drums", None),
            ("Bass DI", Some("drums")),
            ("Kick", Some("drums")),
            ("Snare", Some("drums"))
        ]
    );
}

#[test]
fn both_add_tracks_and_neither_is_lost() {
    let (a, b) = two_peers();
    let mut ma = a.read();
    ma.tracks.push(track("keys", None, "Keys"));
    a.write(&ma, ORIGIN_LOCAL).unwrap();
    let mut mb = b.read();
    mb.tracks.push(track("vox", None, "Vox"));
    b.write(&mb, ORIGIN_LOCAL).unwrap();

    sync(&a, &b);
    let merged = a.read();
    assert_eq!(merged, b.read());
    assert!(merged.track("keys").is_some() && merged.track("vox").is_some());
    assert_eq!(merged.tracks.len(), 6);
}

#[test]
fn concurrent_chart_edits_both_land() {
    let (a, b) = two_peers();
    let mut ma = a.read();
    ma.chart = ma.chart.replace("VS 4\n", "IN 2\n1 5\n\nVS 4\n");
    a.write(&ma, ORIGIN_LOCAL).unwrap();
    let mut mb = b.read();
    mb.chart.push_str("\nCH 4\n4 1 5 6m\n");
    b.write(&mb, ORIGIN_LOCAL).unwrap();

    sync(&a, &b);
    let chart = a.read().chart;
    assert_eq!(chart, b.read().chart);
    assert!(chart.contains("IN 2\n1 5\n"), "{chart}");
    assert!(chart.contains("CH 4\n4 1 5 6m\n"), "{chart}");
}

#[test]
fn a_deleted_track_stays_deleted_and_its_folder_keeps_the_rest() {
    let (a, b) = two_peers();
    let mut ma = a.read();
    ma.tracks.retain(|t| t.guid != "snare");
    a.write(&ma, ORIGIN_LOCAL).unwrap();
    let mut mb = b.read();
    mb.tracks
        .iter_mut()
        .find(|t| t.guid == "kick")
        .unwrap()
        .muted = true;
    b.write(&mb, ORIGIN_LOCAL).unwrap();

    sync(&a, &b);
    let merged = a.read();
    assert_eq!(merged, b.read());
    assert!(merged.track("snare").is_none());
    assert!(merged.track("kick").unwrap().muted);
}

#[test]
fn history_keeps_every_version() {
    let doc = SessionDoc::new();
    doc.write(&song(), ORIGIN_LOCAL).unwrap();
    let first = doc.loro().state_frontiers();
    let mut m = doc.read();
    m.items.get_mut("i-bass").unwrap().position = 10.0;
    doc.write(&m, ORIGIN_LOCAL).unwrap();

    // Open the session as it was before the move.
    let then = SessionDoc::from_loro(doc.loro().fork_at(&first).unwrap());
    assert_eq!(then.read().items["i-bass"].position, 2.0);
    assert_eq!(doc.read().items["i-bass"].position, 10.0);
}

#[test]
fn a_snapshot_reopens_with_its_history() {
    let doc = SessionDoc::new();
    doc.write(&song(), ORIGIN_LOCAL).unwrap();
    let reopened = SessionDoc::new();
    reopened.import(&doc.snapshot().unwrap(), "file").unwrap();
    assert_eq!(reopened.read(), song());
    assert_eq!(reopened.version(), doc.version());
}

#[test]
fn diff_names_what_the_engine_must_do() {
    let old = song();
    let mut new = song();
    new.tracks
        .iter_mut()
        .find(|t| t.guid == "bass")
        .unwrap()
        .volume = 0.7;
    new.tracks.push(track("keys", None, "Keys"));
    new.items.get_mut("i-kick").unwrap().position = 3.0;
    new.items.insert("i-keys".into(), item("keys", 0.0));
    new.tracks.retain(|t| t.guid != "snare");
    new.markers.clear();

    let changes = diff(&old, &new);
    assert!(changes.contains(&Change::TrackAdded {
        track: track("keys", None, "Keys"),
        index: 2
    }));
    assert!(changes.iter().any(|c| matches!(c, Change::TrackChanged { track, fields } if track.guid == "bass" && fields == &[TrackField::Volume])));
    assert!(changes.iter().any(|c| matches!(c, Change::ItemChanged { guid, fields, .. } if guid == "i-kick" && fields == &[ItemField::Position])));
    assert!(changes.contains(&Change::MarkerRemoved { guid: "m1".into() }));
    // Tracks are added before items go on them, and removed last.
    let added = changes
        .iter()
        .position(|c| matches!(c, Change::TrackAdded { .. }))
        .unwrap();
    let item_added = changes
        .iter()
        .position(|c| matches!(c, Change::ItemAdded { .. }))
        .unwrap();
    assert!(added < item_added);
    assert!(matches!(changes.last(), Some(Change::TrackRemoved { guid }) if guid == "snare"));
    assert!(diff(&new, &new).is_empty());
}

/// Tracks and items converge even when two replicas each wrote the same
/// song before syncing. The chart does NOT: two independent inserts of
/// the same text are two texts to a text CRDT. Which is why joining a
/// session always starts from the host's doc (`import`, then read), and
/// never from the joiner's own copy of the file.
#[test]
fn two_who_opened_their_own_copy_converge_on_one_set_of_tracks() {
    let a = SessionDoc::new();
    a.write(&song(), ORIGIN_LOCAL).unwrap();
    let b = SessionDoc::new();
    b.write(&song(), ORIGIN_LOCAL).unwrap();
    sync(&a, &b);
    // The next write on each side removes the duplicate nodes.
    a.write(&a.read(), ORIGIN_LOCAL).unwrap();
    b.write(&b.read(), ORIGIN_LOCAL).unwrap();
    sync(&a, &b);
    let (ma, mb) = (a.read(), b.read());
    assert_eq!(ma.tracks, song().tracks);
    assert_eq!(mb.tracks, song().tracks);
    assert_eq!(ma.items, song().items);
    assert_eq!(ma, mb);
}

#[test]
fn a_chart_caret_stays_on_its_character_while_others_type() {
    let (a, b) = two_peers();
    // B's caret on the "4" of "1 4 6m 5".
    let at = song().chart.find("4 6m").unwrap();
    let caret = b.chart_cursor(at).unwrap();
    // A types a line above it.
    let mut m = a.read();
    m.chart = m.chart.replace("VS 4\n", "IN 2\n1 5\n\nVS 4\n");
    a.write(&m, ORIGIN_LOCAL).unwrap();
    sync(&a, &b);
    let now = b.resolve_chart_cursor(&caret).unwrap();
    let chart = b.read().chart;
    assert_eq!(&chart[now..now + 4], "4 6m", "{chart}");
    assert!(b.has_session());
    assert!(!SessionDoc::new().has_session());
}

/// One bypasses a plugin while the other automates its gate: both land,
/// as each FX and each envelope is its own entry.
#[test]
fn one_bypasses_an_fx_while_the_other_automates_it() {
    let (a, b) = two_peers();
    let mut ma = a.read();
    ma.tracks[1].fx[0].enabled = false;
    a.write(&ma, ORIGIN_LOCAL).unwrap();
    let mut mb = b.read();
    mb.tracks[1]
        .envelopes
        .get_mut("fx:fx-gate:3")
        .unwrap()
        .points
        .push(EnvelopePointState {
            at: 4.0,
            value: 0.5,
            shape: 0,
            tension: 0.0,
        });
    b.write(&mb, ORIGIN_LOCAL).unwrap();

    sync(&a, &b);
    let merged = a.read();
    assert_eq!(merged, b.read(), "converged");
    let kick = &merged.tracks[1];
    assert!(!kick.fx[0].enabled, "the bypass landed");
    assert_eq!(
        kick.envelopes["fx:fx-gate:3"].points.len(),
        3,
        "the point landed"
    );
    let changes = diff(&song(), &merged);
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, Change::TrackChanged { fields, .. }
            if fields.contains(&TrackField::Fx) && fields.contains(&TrackField::Envelopes))),
        "the engine is told both: {changes:?}"
    );
}
