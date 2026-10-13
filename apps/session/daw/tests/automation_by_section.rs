//! A mix move made section by section: a song with a quiet sung verse, two
//! loud sung choruses and a loud instrumental between them, and the kick's
//! top end set up in the choruses and further in the instrumental — the
//! sections found by what is in them, the envelope written so it plays
//! that way, each change ramping in before its section.

#![cfg(feature = "native")]

use std::sync::Arc;

use daw::service::{ProjectContext, TrackRef};
use daw_proto::automation::{Automation as _, EnvelopeLocation, EnvelopeRef};
use daw_proto::primitives::PositionInSeconds;
use session_daw::automation_actions::{BySection, Value, analyse, automate_by_section};

const RATE: u32 = 48_000;

/// `seconds` of a 220 Hz tone at `db` dBFS (silence where `db` is `None`),
/// span by span, as a mono WAV.
fn wav(path: &std::path::Path, spans: &[(f64, Option<f64>)]) {
    let mut samples: Vec<i16> = Vec::new();
    for &(seconds, db) in spans {
        let n = num_traits::ToPrimitive::to_usize(&(seconds * f64::from(RATE))).unwrap_or(0);
        let amp = db.map_or(0.0, |db| 10f64.powf(db / 20.0) * std::f64::consts::SQRT_2);
        for i in 0..n {
            let t = num_traits::ToPrimitive::to_f64(&i).unwrap_or(0.0) / f64::from(RATE);
            let x = (t * 220.0 * std::f64::consts::TAU).sin() * amp;
            samples.push(num_traits::ToPrimitive::to_i16(&(x * 32_767.0).round()).unwrap_or(0));
        }
    }
    let data = u32::try_from(samples.len() * 2).unwrap_or(0);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("a wav");
}

/// Four sections of 8 s at 120 bpm: VS 1, CH 1, INST, CH 2.
const SONG: &str = r#"<REAPER_PROJECT 0.1 "7.75/linux-x86_64" 1790015830 0
  TEMPO 120 4 4 0
  SAMPLERATE 48000 0 0
  MARKER 1 0 "VS 1" 1
  MARKER 1 8 "" 1
  MARKER 2 8 "CH 1" 1
  MARKER 2 16 "" 1
  MARKER 3 16 "INST" 1
  MARKER 3 24 "" 1
  MARKER 4 24 "CH 2" 1
  MARKER 4 32 "" 1
  <TRACK {B0000000-0000-0000-0000-000000000001}
    NAME Vocals
    ISBUS 1 1
  >
  <TRACK {B0000000-0000-0000-0000-000000000002}
    NAME Lead
    ISBUS 2 -1
    <ITEM
      POSITION 0
      LENGTH 32
      IGUID {C0000000-0000-0000-0000-000000000001}
      <SOURCE WAVE
        FILE "VOCAL_WAV"
      >
    >
  >
  <TRACK {B0000000-0000-0000-0000-000000000003}
    NAME Guitar
    <ITEM
      POSITION 0
      LENGTH 32
      IGUID {C0000000-0000-0000-0000-000000000002}
      <SOURCE WAVE
        FILE "GUITAR_WAV"
      >
    >
  >
  <TRACK {B0000000-0000-0000-0000-000000000004}
    NAME Kick
  >
>
"#;

// r[verify automation.by-section]
#[test]
fn the_kicks_top_end_comes_up_in_the_loud_sections() {
    let dir = std::env::temp_dir().join(format!("by-section-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    let (vocal, guitar) = (dir.join("vocal.wav"), dir.join("guitar.wav"));
    // Sung through the verse and the choruses, not the instrumental.
    wav(
        &vocal,
        &[(16.0, Some(-20.0)), (8.0, None), (8.0, Some(-20.0))],
    );
    // Quiet under the verse, loud from the first chorus on.
    wav(&guitar, &[(8.0, Some(-36.0)), (24.0, Some(-8.0))]);
    let song = SONG
        .replace("VOCAL_WAV", &vocal.to_string_lossy())
        .replace("GUITAR_WAV", &guitar.to_string_lossy());
    let daw = daw::standalone::Standalone::new();
    daw.set_fx_factory(Arc::new(builtin_fx::BuiltinFx));
    let loaded =
        daw::standalone::project_loader::load_rpp_text(&daw, "song", "/tmp/song.RPP", &song)
            .expect("the song");
    let project = ProjectContext::Project(loaded.project_guid);
    let kick = daw::service::Tracks::all(&daw, project.clone())
        .into_iter()
        .find(|t| t.name == "Kick")
        .expect("the kick");
    let fx = daw_proto::fx::Effects::add(
        &daw,
        project.clone(),
        daw_proto::fx::FxChainContext::Track(kick.guid.clone()),
        "FTS EQ",
    )
    .expect("an EQ");

    // What is in each section.
    let sections = analyse(&daw, &project);
    let section = |name: &str| {
        sections
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("{name}: {sections:?}"))
    };
    let (vs, ch, inst) = (section("VS 1"), section("CH 1"), section("INST"));
    assert!(vs.vocal && !vs.loud, "a quiet sung verse: {vs:?}");
    assert!(ch.vocal && ch.loud, "a loud sung chorus: {ch:?}");
    assert!(!inst.vocal && inst.loud, "a loud instrumental: {inst:?}");

    let text = |t: &str| Value {
        value: None,
        text: Some(t.to_owned()),
    };
    let set = automate_by_section(
        &daw,
        &project,
        &BySection {
            track: "Kick".into(),
            envelope: "FTS EQ/SSL HF Gain".into(),
            values: vec![
                ("loud instrumental".into(), text("+3 dB")),
                ("CH".into(), text("+2 dB")),
            ],
            rest: Some(text("0 dB")),
            ramp_beats: 1.0,
            pooled: false,
        },
        false,
    )
    .expect("automated");
    let picked: Vec<(&str, &str)> = set
        .iter()
        .map(|s| (s.section.as_str(), s.picked_by.as_str()))
        .collect();
    assert_eq!(
        picked,
        [
            ("VS 1", ""),
            ("CH 1", "CH"),
            ("INST", "loud instrumental"),
            ("CH 2", "CH")
        ]
    );

    let (index, _) = builtin_fx::setting("FTS EQ", "SSL HF Gain", "0 dB").expect("the parameter");
    let level = |t: &str| {
        builtin_fx::setting("FTS EQ", "SSL HF Gain", t)
            .map(|(_, v)| v)
            .expect("a level")
    };
    let at = EnvelopeLocation::new(
        TrackRef::Guid(kick.guid.clone()),
        EnvelopeRef::FxParam {
            fx_guid: fx,
            param_index: index,
        },
    );
    let value = |t: f64| {
        daw.value_at(
            project.clone(),
            at.clone(),
            PositionInSeconds::from_seconds(t),
        )
    };
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
    assert!(near(value(4.0), level("0 dB")), "the verse: {}", value(4.0));
    assert!(
        near(value(12.0), level("+2 dB")),
        "a chorus: {}",
        value(12.0)
    );
    assert!(
        near(value(20.0), level("+3 dB")),
        "the instrumental: {}",
        value(20.0)
    );
    assert!(
        near(value(28.0), level("+2 dB")),
        "the other chorus: {}",
        value(28.0)
    );
    // The change into the chorus ramps over the beat before it (half a
    // second at 120): under way just before, not before the beat.
    let into = value(7.75);
    assert!(
        into > level("0 dB") && into < level("+2 dB"),
        "ramping: {into}"
    );
    assert!(
        near(value(7.4), level("0 dB")),
        "still the verse: {}",
        value(7.4)
    );
    // And the envelope shows in a lane.
    let shown = daw
        .envelope(project.clone(), at.clone())
        .is_some_and(|e| e.visible);
    assert!(shown, "shown in a lane");

    // Pooled: each picked kind of section one pool, the choruses sharing it.
    automate_by_section(
        &daw,
        &project,
        &BySection {
            track: "Kick".into(),
            envelope: "FTS EQ/SSL HF Gain".into(),
            values: vec![
                ("loud instrumental".into(), text("+3 dB")),
                ("CH".into(), text("+2 dB")),
            ],
            rest: Some(text("0 dB")),
            ramp_beats: 1.0,
            pooled: true,
        },
        false,
    )
    .expect("automated, pooled");
    let items = daw.automation_items(project.clone(), at.clone());
    assert_eq!(items.len(), 3, "{items:?}");
    let pool_of = |start: f64| {
        items
            .iter()
            .find(|i| (i.position.as_seconds() - start).abs() < 1e-6)
            .map(|i| i.pool_id)
    };
    assert_eq!(pool_of(8.0), pool_of(24.0), "the choruses one pool");
    assert_ne!(pool_of(8.0), pool_of(16.0));
    assert!(near(value(12.0), level("+2 dB")) && near(value(20.0), level("+3 dB")));
    assert!(near(value(4.0), level("0 dB")));
}

/// The bass rig's scene switched by section: the choruses on Drive, the rest
/// on Clean — named, as its plugin names its scenes; stepping at each
/// chorus's downbeat rather than ramping through Crunch.
#[cfg(feature = "signal")]
#[test]
fn the_bass_rigs_scene_follows_the_sections() {
    let dir = std::env::temp_dir().join(format!("by-section-rig-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    // SAFETY: before anything of this process reads it.
    unsafe { std::env::set_var("FTS_BASS_RIG_DIR", dir.join("bass-rig")) };
    let (vocal, guitar) = (dir.join("vocal.wav"), dir.join("guitar.wav"));
    wav(
        &vocal,
        &[(16.0, Some(-20.0)), (8.0, None), (8.0, Some(-20.0))],
    );
    wav(&guitar, &[(8.0, Some(-36.0)), (24.0, Some(-8.0))]);
    let song = SONG
        .replace("VOCAL_WAV", &vocal.to_string_lossy())
        .replace("GUITAR_WAV", &guitar.to_string_lossy());
    let daw = daw::standalone::Standalone::new();
    daw.set_fx_factory(Arc::new(signal_fx::SignalFx));
    let loaded =
        daw::standalone::project_loader::load_rpp_text(&daw, "song", "/tmp/song.RPP", &song)
            .expect("the song");
    let project = ProjectContext::Project(loaded.project_guid);
    let bass = daw::service::Tracks::all(&daw, project.clone())
        .into_iter()
        .find(|t| t.name == "Kick")
        .expect("a track for the rig");
    let fx = daw_proto::fx::Effects::add(
        &daw,
        project.clone(),
        daw_proto::fx::FxChainContext::Track(bass.guid.clone()),
        signal_fx::BASS_RIG,
    )
    .expect("the rig");
    daw.apply_plugin_state(&fx, br#"{"profile": "Metal", "scene": "Clean"}"#)
        .expect("a profile");

    let text = |t: &str| Value {
        value: None,
        text: Some(t.to_owned()),
    };
    let set = automate_by_section(
        &daw,
        &project,
        &BySection {
            track: "Kick".into(),
            envelope: "FTS Bass Rig/Scene".into(),
            values: vec![("CH".into(), text("Drive"))],
            rest: Some(text("Clean")),
            ramp_beats: 1.0,
            pooled: false,
        },
        false,
    )
    .expect("automated");
    let said: Vec<(&str, &str)> = set
        .iter()
        .map(|s| (s.section.as_str(), s.value.as_str()))
        .collect();
    assert_eq!(
        said,
        [
            ("VS 1", "Clean"),
            ("CH 1", "Drive"),
            ("INST", "Clean"),
            ("CH 2", "Drive")
        ]
    );

    let at = EnvelopeLocation::new(
        TrackRef::Guid(bass.guid.clone()),
        EnvelopeRef::FxParam {
            fx_guid: fx,
            param_index: signal_fx::rig::SCENE,
        },
    );
    let value = |t: f64| {
        daw.value_at(
            project.clone(),
            at.clone(),
            PositionInSeconds::from_seconds(t),
        )
    };
    // Clean is the first scene (0), Drive the last (1): no step between.
    assert!(
        value(7.99).abs() < 1e-9,
        "Clean up to the chorus: {}",
        value(7.99)
    );
    assert!(
        (value(8.0) - 1.0).abs() < 1e-9,
        "Drive on its downbeat: {}",
        value(8.0)
    );
    assert!(value(20.0).abs() < 1e-9 && (value(28.0) - 1.0).abs() < 1e-9);
}
