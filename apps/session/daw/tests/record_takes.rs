//! A four-bar part recorded five times through the Action List, as the
//! Original Composition spec's step 4.1 does it: `TRACK_ARM`, then
//! `TRANSPORT_RECORD` on and off five times over bars 3–7 at 68 bpm — the
//! takes on lanes `1` to `5`, each where the pass began and as long as it,
//! the newest one playing.
//!
//! No hardware: the input plays a generated tone in place of the device
//! (`Standalone::set_test_input`), take n a sine at 110·(n + 1) Hz, and the
//! span is rendered offline through a `ProjectRenderer` while the
//! transport records — the real capture, the real `finish_recording`.
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.

#![cfg(feature = "native")]

use daw::service::{ItemRef, Items, ProjectContext, Takes, TrackRef, Tracks};
use daw_proto::transport::service::Transport as TransportService;
use num_traits::ToPrimitive;
use session_daw::actions::list;

const RATE: u32 = 48_000;
const BLOCK: usize = 512;
const BPM: f64 = 68.0;

/// Bars 3–7 (from 1): the intro, after a two-bar count-in.
fn intro() -> (f64, f64) {
    let bar = 240.0 / BPM;
    (2.0 * bar, 6.0 * bar)
}

/// A sine at `freq` Hz, −18 dBFS, `seconds` long.
fn sine(freq: f64, seconds: f64) -> Vec<f32> {
    let amp = 10f64.powf(-18.0 / 20.0);
    let n = (seconds * f64::from(RATE)).ceil().to_usize().unwrap_or(0);
    (0..n)
        .map(|i| {
            let t = i.to_f64().unwrap_or(0.0) / f64::from(RATE);
            ((t * freq * std::f64::consts::TAU).sin() * amp)
                .to_f32()
                .unwrap_or(0.0)
        })
        .collect()
}

/// A mono float WAV's samples, as the recorder writes them.
fn wav_samples(path: &std::path::Path) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("the take's file");
    assert_eq!(bytes.get(0..4), Some(b"RIFF".as_slice()));
    bytes
        .get(44..)
        .unwrap_or_default()
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

const SONG: &str = r#"<REAPER_PROJECT 0.1 "7.75/linux-x86_64" 1790015830 0
  TEMPO 68 4 4 0
  SAMPLERATE 48000 0 0
  <TRACK {D0000000-0000-0000-0000-000000000001}
    NAME "Guitar Clean"
    TRACKID {D0000000-0000-0000-0000-000000000001}
  >
  <TRACK {D0000000-0000-0000-0000-000000000002}
    NAME Bass
    TRACKID {D0000000-0000-0000-0000-000000000002}
  >
>
"#;

/// One record pass over `span`: the input playing `tone`, the transport
/// recording while the span renders, then stopped.
fn record_pass(span: (f64, f64), tone: Vec<f32>) {
    let (daw, project) = session_daw::actions::song().expect("the song is open");
    let ProjectContext::Project(guid) = project.clone() else {
        panic!("a song, by guid");
    };
    daw.set_test_input(0, tone);
    TransportService::set_position(daw, project, span.0).expect("located");
    list()
        .run_json("TRANSPORT_RECORD", br#"{"on":true}"#)
        .expect("recording");
    assert!(daw.is_capturing(&guid), "the armed guitar is captured");
    let renderer = daw::standalone::audio_engine::render::ProjectRenderer::new(daw, &guid, RATE);
    let from = (span.0 * f64::from(RATE)).round().to_u64().unwrap_or(0);
    let frames = ((span.1 - span.0) * f64::from(RATE))
        .round()
        .to_usize()
        .unwrap_or(0);
    let mut done = 0_usize;
    while done < frames {
        let n = BLOCK.min(frames.saturating_sub(done));
        renderer.render_block(from.saturating_add(done.to_u64().unwrap_or(0)), n);
        done = done.saturating_add(n);
    }
    list()
        .run_json("TRANSPORT_RECORD", br#"{"on":false}"#)
        .expect("stopped");
    assert!(!daw.is_capturing(&guid));
}

/// Step 4.1: five passes over the intro, through `TRACK_ARM` and
/// `TRANSPORT_RECORD`.
#[test]
fn five_takes_of_a_four_bar_part_land_on_lanes_one_to_five() {
    let dir = std::env::temp_dir().join(format!("record-takes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    let path = dir.join("song.RPP");
    std::fs::write(&path, SONG).expect("the song");
    session_daw::open::open_silent(&path).expect("the song opens");
    // The transport moves spawn on the engine's runtime, as in the window.
    let _inside = session_daw::open::runtime()
        .expect("the engine's runtime")
        .enter();
    let (daw, project) = session_daw::actions::song().expect("the song is open");
    let ProjectContext::Project(song) = project.clone() else {
        panic!("a song, by guid");
    };
    let media = daw.record_dir(&song);
    assert!(
        media.starts_with(&dir) && media.ends_with("Media"),
        "takes go in the song's Media folder: {}",
        media.display()
    );

    list()
        .run_json(
            "TRACK_ARM",
            br#"{"track":"Guitar Clean","on":true,"input":1}"#,
        )
        .expect("armed");
    let tracks = Tracks::all(daw, project.clone());
    let guitar = tracks
        .iter()
        .find(|t| t.name == "Guitar Clean")
        .expect("the guitar")
        .guid
        .clone();
    assert_eq!(
        tracks
            .iter()
            .filter(|t| t.armed)
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Guitar Clean"],
        "the guitar is armed, nothing else"
    );

    let span = intro();
    let mut tones = Vec::new();
    for n in 1_u32..=5 {
        let tone = sine(
            110.0 * f64::from(n.saturating_add(1)),
            span.1 - span.0 + 1.0,
        );
        tones.push(tone.clone());
        record_pass(span, tone);
    }

    let track =
        Tracks::get(daw, project.clone(), TrackRef::Guid(guitar.clone())).expect("the guitar");
    assert_eq!(track.lane_count, 5);
    let names: Vec<String> = (0..5_usize)
        .map(|i| track.lane_names.get(i).cloned().unwrap_or_default())
        .collect();
    assert_eq!(names, ["1", "2", "3", "4", "5"]);
    assert_eq!(
        track.lane_play_mask,
        1 << 4,
        "the newest take, lane 5, plays"
    );

    let mut items = Items::get_items(daw, project.clone(), TrackRef::Guid(guitar));
    items.sort_by_key(|i| i.fixed_lane);
    assert_eq!(items.len(), 5, "one item a pass");
    for (lane, (item, tone)) in items.iter().zip(&tones).enumerate() {
        let n = lane.saturating_add(1);
        assert_eq!(item.fixed_lane, lane.to_u32(), "take {n} on lane {n}");
        assert!(
            (item.position.as_seconds() - span.0).abs() < 1e-3,
            "take {n} at the intro: {}",
            item.position.as_seconds()
        );
        assert!(
            (item.length.as_seconds() - (span.1 - span.0)).abs() < 1e-3,
            "take {n} four bars long: {}",
            item.length.as_seconds()
        );
        let source = Takes::get_active_take(daw, project.clone(), ItemRef::Guid(item.guid.clone()))
            .and_then(|t| t.source_file_path)
            .expect("its recording");
        let source = std::path::PathBuf::from(source);
        assert_eq!(
            source,
            media.join(format!("Guitar Clean-{n:02}.wav")),
            "take {n}'s file, in the song's Media folder"
        );
        let samples = wav_samples(&source);
        let recorded = samples.len();
        assert_eq!(
            samples.as_slice(),
            tone.get(..recorded).expect("the tone covers the pass"),
            "take {n} is its own tone"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}
