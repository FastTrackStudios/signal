//! `.daw` <-> DAWproject, both ways (#174).

use dawfile_standalone::dawproject::{from_dawproject, to_dawproject};
use dawfile_standalone::document::{DawDocument, TrackNode};
use dawfile_standalone::id::EntityId;

fn track(name: &str, volume: f64, pan: f64) -> TrackNode {
    let id = EntityId::new();
    let mut t = daw_proto::track::Track::default();
    t.guid = id.to_string();
    t.name = name.into();
    t.volume = volume;
    t.pan = pan;
    TrackNode {
        id,
        track: t,
        parent: None,
        envelopes: Vec::new(),
        items: Vec::new(),
        fx_chain: None,
        input_fx_chain: None,
        comping: daw_proto::track::LaneComping::default(),
        receives: Vec::new(),
    }
}

fn doc() -> DawDocument {
    let mut d = DawDocument::new("Belief");
    d.tempo_map = vec![daw_proto::tempo_map::TempoPoint {
        position: Default::default(),
        bpm: 138.0,
        time_signature: Some(daw_proto::TimeSignature {
            numerator: 6,
            denominator: 8,
        }),
        shape: None,
        bezier_tension: None,
        selected: None,
        linear: None,
    }];
    d.tracks.push(track("Lead Vox", 0.8, -0.25));
    d.tracks.push(track("Kit", 1.0, 0.0));
    d
}

#[test]
fn transport_crosses_both_ways() {
    let out = to_dawproject(&doc());
    assert_eq!(out.transport.tempo, 138.0);
    assert_eq!(out.transport.numerator, 6);
    assert_eq!(out.transport.denominator, 8);

    let back = from_dawproject(&out, "Belief").unwrap();
    assert_eq!(back.tempo_map[0].bpm, 138.0);
    let ts = back.tempo_map[0].time_signature.as_ref().unwrap();
    assert_eq!((ts.numerator, ts.denominator), (6, 8));
}

#[test]
fn the_track_hierarchy_and_mixer_state_cross_both_ways() {
    let out = to_dawproject(&doc());
    assert_eq!(out.tracks.len(), 2);
    assert_eq!(out.tracks[0].name, "Lead Vox");
    let ch = out.tracks[0].channel.as_ref().unwrap();
    assert_eq!(ch.volume, 0.8);
    assert_eq!(ch.pan, -0.25);

    let back = from_dawproject(&out, "Belief").unwrap();
    assert_eq!(back.tracks.len(), 2);
    assert_eq!(back.tracks[0].track.name, "Lead Vox");
    assert_eq!(back.tracks[0].track.volume, 0.8);
    assert_eq!(back.tracks[0].track.pan, -0.25);
}

#[test]
fn a_full_round_trip_preserves_the_modelled_set() {
    let original = doc();
    let back = from_dawproject(&to_dawproject(&original), "Belief").unwrap();

    assert_eq!(back.tempo_map[0].bpm, original.tempo_map[0].bpm);
    assert_eq!(back.tracks.len(), original.tracks.len());
    for (a, b) in back.tracks.iter().zip(&original.tracks) {
        assert_eq!(a.track.name, b.track.name);
        assert_eq!(a.track.volume, b.track.volume);
        assert_eq!(a.track.pan, b.track.pan);
    }
}

#[test]
fn imported_entities_get_fresh_ids() {
    // DAWproject ids are XML cross-reference strings scoped to one file,
    // not stable identities. Adopting them would make two imports of the
    // same file collide.
    let out = to_dawproject(&doc());
    let a = from_dawproject(&out, "A").unwrap();
    let b = from_dawproject(&out, "B").unwrap();
    assert_ne!(a.tracks[0].id, b.tracks[0].id);
    assert_eq!(
        a.tracks[0].track.guid,
        a.tracks[0].id.to_string(),
        "and the facade guid still matches its node id"
    );
}

#[test]
fn colour_survives_the_representation_change() {
    // Ours is a packed integer, theirs a hex string.
    let mut d = doc();
    d.tracks[0].track.color = Some(0xFF8800);
    let out = to_dawproject(&d);
    assert_eq!(out.tracks[0].color.as_deref(), Some("#FF8800"));

    let back = from_dawproject(&out, "Belief").unwrap();
    assert_eq!(back.tracks[0].track.color, Some(0xFF8800));
}

#[test]
fn an_unparseable_colour_is_dropped_rather_than_guessed() {
    let mut out = to_dawproject(&doc());
    out.tracks[0].color = Some("rebeccapurple".into());
    let back = from_dawproject(&out, "Belief").unwrap();
    assert_eq!(back.tracks[0].track.color, None);
}

#[test]
fn every_shipped_fixture_converts_without_panicking() {
    // The corpus `dawfile-dawproject` ships. The bar here is that the
    // bridge is total over real files, not that every field survives —
    // DAWproject models things we do not.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../dawfile-dawproject/tests/fixtures");
    if !dir.exists() {
        return;
    }
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("dawproject") {
            continue;
        }
        let Ok(project) = dawfile_dawproject::read_project(&path) else {
            continue;
        };
        let doc = from_dawproject(&project, "fixture").expect("import");
        let back = to_dawproject(&doc);
        assert_eq!(
            back.tracks.len(),
            doc.tracks.len(),
            "track count changed for {}",
            path.display()
        );
        seen += 1;
    }
    assert!(seen > 0, "no fixtures were exercised");
}
