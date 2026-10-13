//! A vocal sung 18 dB quieter in the verse than in the chorus, leveled:
//! the verse brought up to sit with the chorus, the four lanes kept with
//! the track for each item, and their sum written as each item's volume.

#![cfg(feature = "native")]

use daw::service::{Items as _, ProjectContext, Takes as _, Tracks};
use daw_proto::automation::Automation as _;
use daw_proto::{EnvelopeLocation, EnvelopeRef, ItemRef, TakeEnvelopeKind, TrackRef};

/// Six lines a second and a half long, half a second apart, of a 220 Hz
/// tone at `db` dBFS — over a noise floor 70 dB under it.
fn sung(path: &std::path::Path, db: f64) {
    let rate = 48_000_u32;
    let amp = 10f64.powf(db / 20.0) * std::f64::consts::SQRT_2;
    let mut seed = 0x9e37_79b9_u32;
    let samples: Vec<i16> = (0..rate * 12)
        .map(|n| {
            let t = f64::from(n) / f64::from(rate);
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = (f64::from(seed) / f64::from(u32::MAX) - 0.5) * amp * 0.001;
            let line = (t % 2.0) < 1.5;
            let tone = if line {
                (t * 220.0 * std::f64::consts::TAU).sin() * amp
            } else {
                0.0
            };
            num_traits::ToPrimitive::to_i16(&((tone + noise) * 32_767.0).round()).unwrap_or(0)
        })
        .collect();
    let data = u32::try_from(samples.len() * 2).unwrap_or(0);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("a wav");
}

const SONG: &str = r#"<REAPER_PROJECT 0.1 "7.75/linux-x86_64" 1790015830 0
  TEMPO 120 4 4 0
  SAMPLERATE 48000 0 0
  MARKER 1 0 "VS" 1
  MARKER 1 12 "" 1
  MARKER 2 12 "CH" 1
  MARKER 2 24 "" 1
  <TRACK {B0000000-0000-0000-0000-000000000001}
    NAME Vocals
    ISBUS 1 1
  >
  <TRACK {B0000000-0000-0000-0000-000000000002}
    NAME "Lead Vocal"
    ISBUS 2 -1
    <ITEM
      POSITION 0
      LENGTH 12
      IGUID {C0000000-0000-0000-0000-000000000001}
      <SOURCE WAVE
        FILE "VERSE_WAV"
      >
    >
    <ITEM
      POSITION 12
      LENGTH 12
      IGUID {C0000000-0000-0000-0000-000000000002}
      <SOURCE WAVE
        FILE "CHORUS_WAV"
      >
    >
  >
>
"#;

// r[verify level.vocal]
#[test]
fn a_quiet_verse_comes_up_to_the_chorus() {
    let dir = std::env::temp_dir().join(format!("vocal-level-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    let (verse, chorus) = (dir.join("verse.wav"), dir.join("chorus.wav"));
    sung(&verse, -38.0);
    sung(&chorus, -20.0);
    let song = SONG
        .replace("VERSE_WAV", &verse.to_string_lossy())
        .replace("CHORUS_WAV", &chorus.to_string_lossy());
    let daw = daw::standalone::Standalone::new();
    let loaded =
        daw::standalone::project_loader::load_rpp_text(&daw, "song", "/tmp/song.RPP", &song)
            .expect("the song");
    let project = ProjectContext::Project(loaded.project_guid);
    let lead = Tracks::all(&daw, project.clone())
        .into_iter()
        .find(|t| t.name == "Lead Vocal")
        .expect("the lead");

    let knobs = session_daw::vocal_level::Knobs::default();
    let said =
        session_daw::vocal_level::level(&daw, &project, &lead, (&knobs, false)).expect("leveled");
    assert_eq!(said.items, 2);
    let section = |name: &str| {
        said.sections
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("{name}: {:?}", said.sections))
    };
    let (vs, ch) = (section("VS"), section("CH"));
    assert!(ch.before_db - vs.before_db > 15.0, "{said:?}");
    assert!(
        (ch.after_db - vs.after_db).abs() < 3.0,
        "the verse with the chorus: {vs:?} {ch:?}"
    );
    // The makeup: finished where the chorus was sung.
    assert!((ch.after_db - ch.before_db).abs() < 3.0, "{ch:?}");

    for item in daw.get_items(project.clone(), TrackRef::Guid(lead.guid.clone())) {
        let take = daw
            .get_active_take(project.clone(), ItemRef::Guid(item.guid.clone()))
            .expect("a take");
        let points = daw.points(
            project.clone(),
            EnvelopeLocation {
                track: TrackRef::Index(0),
                envelope: EnvelopeRef::Take {
                    item_guid: item.guid.clone(),
                    take_guid: take.guid,
                    kind: TakeEnvelopeKind::Volume,
                },
            },
        );
        assert!(
            points.len() > 2,
            "{}: its volume is the lanes' sum",
            item.guid
        );
        let lanes = session_daw::vocal_level::kept(&daw, &project, &lead.guid, &item.guid)
            .expect("its four lanes kept");
        assert!(!lanes.ride.is_empty() && !lanes.gate.is_empty());
    }
}

/// The song, loaded with its two recordings written to a folder of
/// `name`'s own (tests run side by side), and its lead vocal leveled.
fn leveled_song(
    name: &str,
) -> (
    daw::standalone::Standalone,
    ProjectContext,
    daw_proto::Track,
) {
    let dir = std::env::temp_dir().join(format!("vocal-level-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    let (verse, chorus) = (dir.join("verse.wav"), dir.join("chorus.wav"));
    sung(&verse, -38.0);
    sung(&chorus, -20.0);
    let song = SONG
        .replace("VERSE_WAV", &verse.to_string_lossy())
        .replace("CHORUS_WAV", &chorus.to_string_lossy());
    let daw = daw::standalone::Standalone::new();
    let loaded =
        daw::standalone::project_loader::load_rpp_text(&daw, "song", "/tmp/song.RPP", &song)
            .expect("the song");
    let project = ProjectContext::Project(loaded.project_guid);
    let lead = Tracks::all(&daw, project.clone())
        .into_iter()
        .find(|t| t.name == "Lead Vocal")
        .expect("the lead");
    session_daw::vocal_level::level(
        &daw,
        &project,
        &lead,
        (&session_daw::vocal_level::Knobs::default(), false),
    )
    .expect("leveled");
    (daw, project, lead)
}

/// The take volume envelope the engine plays for `item`: `(seconds into
/// the item, dB)`.
fn played(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    item: &str,
) -> Vec<(f64, f64)> {
    let take = daw
        .get_active_take(project.clone(), ItemRef::Guid(item.to_owned()))
        .expect("a take");
    daw.points(
        project.clone(),
        EnvelopeLocation {
            track: TrackRef::Index(0),
            envelope: EnvelopeRef::Take {
                item_guid: item.to_owned(),
                take_guid: take.guid,
                kind: TakeEnvelopeKind::Volume,
            },
        },
    )
    .into_iter()
    .map(|p| (p.time.as_seconds(), 20.0 * p.value.max(1e-9).log10()))
    .collect()
}

/// Every point the engine plays for each leveled item is where the sum
/// drawn on the item says, at the same moment of the timeline: what is
/// drawn, what plays and what the waveform is drawn through agree.
fn drawn_as_played(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    leveled: &[session_daw::leveler::Leveled],
) {
    for l in leveled {
        let points = played(daw, project, &l.item);
        assert!(points.len() > 2, "{}: plays its sum", l.item);
        for (t, db) in points.iter().filter(|(t, _)| *t >= 0.0 && *t <= l.length) {
            let drawn = session_daw::automation::db_at(&l.composite, l.lane_time(l.position + t));
            assert!(
                (drawn - db).abs() < 1e-3,
                "{} at {t}: drawn {drawn} dB, plays {db} dB",
                l.item
            );
        }
    }
}

#[test]
fn the_sum_on_each_item_is_what_it_plays_and_opens_into_four_lanes() {
    use session_daw::automation::{Kind, Stage};
    let (daw, project, lead) = leveled_song("lanes");
    let tracks = Tracks::all(&daw, project.clone());
    let by_track = session_daw::automation::read_leveled(&daw, &project, &tracks);
    let leveled = by_track.get(&lead.guid).expect("the lead is leveled");
    assert_eq!(leveled.len(), 2);
    drawn_as_played(&daw, &project, leveled);

    // Folded, by default: the sum on the items, no lanes under the track.
    let lanes = session_daw::automation::read(&daw, &project, &tracks);
    assert!(
        lanes
            .get(&lead.guid)
            .is_none_or(|l| l.iter().all(|l| !l.kind.is_leveler())),
        "folded until asked"
    );
    // Expanded: the four, a lane each, in their order.
    session_daw::vocal_level::show_lane(&daw, &project, &lead.guid, true);
    let lanes = session_daw::automation::read(&daw, &project, &tracks);
    let kinds: Vec<Kind> = lanes
        .get(&lead.guid)
        .expect("lanes under the lead")
        .iter()
        .map(|l| l.kind)
        .filter(|k| k.is_leveler())
        .collect();
    assert_eq!(
        kinds,
        Stage::ALL.map(Kind::Leveler).to_vec(),
        "De-Ess, De-Breath, Ride, Gate"
    );
    // The expand key is not a leveled item's.
    assert!(session_daw::vocal_level::has_lanes(
        &daw, &project, &lead.guid
    ));
    session_daw::vocal_level::show_lane(&daw, &project, &lead.guid, false);
    let lanes = session_daw::automation::read(&daw, &project, &tracks);
    assert!(
        lanes
            .get(&lead.guid)
            .is_none_or(|l| l.iter().all(|l| !l.kind.is_leveler())),
        "folded again"
    );
}

#[test]
fn a_slip_and_a_split_keep_the_curves_on_their_audio() {
    let (daw, project, lead) = leveled_song("split");
    let verse = |leveled: &[session_daw::leveler::Leveled]| {
        leveled
            .iter()
            .find(|l| l.item == "{C0000000-0000-0000-0000-000000000001}")
            .cloned()
            .expect("the verse")
    };
    let before = verse(&session_daw::vocal_level::leveled(&daw, &project, &lead));
    let gain_at = |l: &session_daw::leveler::Leveled, t: f64| {
        session_daw::automation::db_at(&l.composite, l.lane_time(t))
    };

    // Cut at 6 s, as the engine cuts: the left half shortened, its copy
    // moved to the cut and slipped into its recording to it.
    let left = ItemRef::Guid(before.item.clone());
    let right =
        daw_proto::Items::duplicate_item(&daw, project.clone(), left.clone()).expect("a copy");
    let seconds = daw_proto::primitives::Duration::from_seconds;
    daw_proto::Items::set_length(&daw, project.clone(), left.clone(), seconds(6.0))
        .expect("the left half");
    daw_proto::Items::set_position(
        &daw,
        project.clone(),
        ItemRef::Guid(right.clone()),
        daw_proto::PositionInSeconds::from_seconds(6.0),
    )
    .expect("the right half moved");
    daw_proto::Items::set_length(
        &daw,
        project.clone(),
        ItemRef::Guid(right.clone()),
        seconds(6.0),
    )
    .expect("the right half");
    daw.set_start_offset(
        project.clone(),
        ItemRef::Guid(right.clone()),
        daw_proto::TakeRef::Active,
        seconds(6.0),
    )
    .expect("slipped to the cut");

    let after = session_daw::vocal_level::leveled(&daw, &project, &lead);
    let half = after
        .iter()
        .find(|l| l.item == right)
        .expect("the new half is drawn with the lanes it was cut from");
    for t in [6.3, 7.1, 8.25, 9.6, 11.4] {
        assert!(
            (gain_at(half, t) - gain_at(&before, t)).abs() < 1e-6,
            "the right half at {t}"
        );
    }
    drawn_as_played(&daw, &project, &after);

    // The left half's left edge trimmed a second in: it starts later, and
    // later in its recording; the curve stays where the audio is.
    daw_proto::Items::set_position(
        &daw,
        project.clone(),
        left.clone(),
        daw_proto::PositionInSeconds::from_seconds(1.0),
    )
    .expect("trimmed");
    daw_proto::Items::set_length(&daw, project.clone(), left.clone(), seconds(5.0))
        .expect("trimmed");
    daw.set_start_offset(
        project.clone(),
        left,
        daw_proto::TakeRef::Active,
        seconds(1.0),
    )
    .expect("slipped");
    let trimmed = verse(&session_daw::vocal_level::leveled(&daw, &project, &lead));
    for t in [1.2, 2.5, 3.75, 5.9] {
        assert!(
            (gain_at(&trimmed, t) - gain_at(&before, t)).abs() < 1e-6,
            "the trimmed left half at {t}"
        );
    }
    drawn_as_played(
        &daw,
        &project,
        &session_daw::vocal_level::leveled(&daw, &project, &lead),
    );
}

#[test]
fn the_dials_are_kept_and_change_what_plays() {
    use session_daw::leveler::{Dial, Stage, Stages};
    let (daw, project, lead) = leveled_song("dials");
    let mut dials = Stages::default();
    dials.set(
        Stage::Gate,
        Dial {
            on: false,
            amount: 1.0,
        },
    );
    dials.set(
        Stage::DeEss,
        Dial {
            on: true,
            amount: 0.0,
        },
    );
    dials.set(
        Stage::DeBreath,
        Dial {
            on: true,
            amount: 1.5,
        },
    );
    let written = session_daw::vocal_level::set_stages(&daw, &project, &lead.guid, &dials);
    assert_eq!(written, 2, "every leveled item written again");
    assert_eq!(
        session_daw::vocal_level::stages(&daw, &project, &lead.guid),
        dials,
        "kept with the song"
    );
    let leveled = session_daw::vocal_level::leveled(&daw, &project, &lead);
    drawn_as_played(&daw, &project, &leveled);
    for l in &leveled {
        assert!(l.curves.gate.is_empty(), "the gate off is flat");
        assert!(
            l.curves.de_ess.iter().all(|p| p.db.abs() < 1e-9),
            "the de-ess at nothing is flat"
        );
        // What is left in the sum: the ride and the breaths, deeper.
        let made =
            session_daw::vocal_level::kept(&daw, &project, &lead.guid, &l.item).expect("its lanes");
        assert!(
            l.curves
                .de_breath
                .iter()
                .zip(&made.breath)
                .all(|(now, was)| (now.db - 1.5 * was.db).abs() < 1e-9),
            "the breaths half as deep again"
        );
        for k in 1..40 {
            let t = f64::from(k) * l.length / 40.0;
            let sum = session_daw::automation::db_at(&l.curves.ride, t)
                + session_daw::automation::db_at(&l.curves.de_breath, t);
            let drawn = session_daw::automation::db_at(&l.composite, t);
            assert!((drawn - sum).abs() < 1e-6, "{} at {t}", l.item);
        }
    }
}
