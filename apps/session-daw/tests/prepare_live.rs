//! A multitrack session prepared end to end on daw-standalone, the way the
//! Session window will: organize it, build the song from its chart, and
//! generate the click and guide.
//!
//! The multitrack brings its own `Click` and `Guide` audio stems. They
//! must survive as muted references in the Guide folder — never written
//! over — while the generated Click / Count / Guide tracks are MIDI,
//! played by the guide instrument, and are what is heard.

use daw::service::{ItemRef, Items, ProjectContext, Takes, TrackRef, Tracks};
use dynamic_template::apply::{DawTarget, organize};
use session::guide::{Guide, GuideScope};
use session::keyflow::from_chart::build_from_chart;
use session::song::SongBuilder;

/// Three stems, two of them a multitrack's own click and guide.
const RPP: &str = r#"<REAPER_PROJECT 0.1 "7.0/test" 0
  TEMPO 90 4 4
  <TRACK {11111111-1111-1111-1111-111111111111}
    NAME "Bass"
    <ITEM
      POSITION 0
      LENGTH 40
      <SOURCE WAVE
        FILE "Media/Bass.wav"
      >
    >
  >
  <TRACK {22222222-2222-2222-2222-222222222222}
    NAME "Click"
    <ITEM
      POSITION 0
      LENGTH 40
      <SOURCE WAVE
        FILE "Media/Click.wav"
      >
    >
  >
  <TRACK {33333333-3333-3333-3333-333333333333}
    NAME "Guide"
    <ITEM
      POSITION 0
      LENGTH 40
      <SOURCE WAVE
        FILE "Media/Guide.wav"
      >
    >
  >
>
"#;

const CHART: &str = "\
Test Song - Nobody
90bpm 4/4 #F
Count 2
In 4
1 5 6 4
vs 4
1 5 6 4
ch 4
4 1 5 6
";

/// Opening a project stands up the window's process-wide engine (the
/// facade, the "current" project), so two tests opening at once step on
/// each other. One at a time.
static ENGINE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn a_multitrack_is_organized_built_and_guided() {
    let _engine = ENGINE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("song.rpp");
    std::fs::write(&file, RPP).expect("write");
    let opened = session_daw::open::open_silent(&file).expect("open");
    let _runtime = session_daw::open::runtime().expect("runtime").enter();
    let daw = opened.daw.clone();
    let project = ProjectContext::Project(opened.project_guid.clone());

    organize(&mut DawTarget::on(daw.clone(), project.clone())).expect("organize");
    build_from_chart(&daw, &project, CHART).expect("chart");

    // The song the guide will be built for: named, from the count-in.
    let songs = SongBuilder::build_on(&daw, ProjectContext::Current).expect("song");
    assert_eq!(songs.len(), 1, "{songs:?}");
    assert_eq!(songs[0].name, "Test Song");

    Guide::new(daw.clone())
        .with_instrument(session_daw::guide_instrument::IDENT)
        .generate(GuideScope::All)
        .expect("generate guide");

    let tracks = Tracks::all(&daw, project.clone());
    if std::env::var_os("FTS_SHOW_TRACKS").is_some() {
        for t in &tracks {
            eprintln!(
                "{:>2} {:>2} {} {}",
                t.index,
                t.folder_depth,
                if t.muted { "M" } else { " " },
                t.name
            );
        }
    }
    let has_audio = |guid: &str| {
        Items::get_items(&daw, project.clone(), TrackRef::Guid(guid.into()))
            .iter()
            .any(|i| {
                Takes::get_active_take(&daw, project.clone(), ItemRef::Guid(i.guid.clone()))
                    .is_some_and(|t| !t.is_midi)
            })
    };
    let item_count =
        |guid: &str| Items::get_items(&daw, project.clone(), TrackRef::Guid(guid.into())).len();

    // Where each track sits: the running folder depth before it.
    let mut depth = 0i32;
    let mut inside_click_guide: Vec<&str> = Vec::new();
    let mut open_at: Option<i32> = None;
    for t in &tracks {
        if let Some(level) = open_at {
            if depth > level {
                inside_click_guide.push(t.name.as_str());
            } else {
                open_at = None;
            }
        }
        if t.name == "Guide" && t.folder_depth > 0 {
            open_at = Some(depth);
        }
        depth += t.folder_depth;
    }

    for stem in ["Click Audio", "Guide Audio"] {
        let t = tracks
            .iter()
            .find(|t| t.name == stem && has_audio(&t.guid))
            .unwrap_or_else(|| panic!("the {stem} stem is still there, with its audio"));
        assert!(t.muted, "the {stem} stem is muted as a reference");
    }
    for role in ["Click", "Count", "Guide"] {
        let generated = tracks
            .iter()
            .find(|t| t.name == role && t.folder_depth <= 0 && !has_audio(&t.guid))
            .unwrap_or_else(|| panic!("a generated {role} track"));
        assert!(
            item_count(&generated.guid) > 0,
            "the generated {role} has MIDI in it"
        );
        assert!(!generated.muted, "the generated {role} plays");
        let chain = daw::service::FxChainContext::Track(generated.guid.clone());
        assert!(
            daw::service::Effects::list(&daw, project.clone(), chain)
                .iter()
                .any(|f| f.name
                    == format!(
                        "{}:{}",
                        session_daw::guide_instrument::IDENT,
                        role.to_lowercase()
                    )),
            "the generated {role} is played by the guide instrument"
        );
    }
    // Every generated note a sixteenth — a quarter of a quarter note —
    // so the MIDI shows the space between clicks rather than a bar.
    for role in ["Click", "Count"] {
        let generated = tracks
            .iter()
            .find(|t| t.name == role && t.folder_depth <= 0 && !has_audio(&t.guid))
            .expect("generated");
        let notes: Vec<_> = Items::get_items(
            &daw,
            project.clone(),
            TrackRef::Guid(generated.guid.clone()),
        )
        .iter()
        .flat_map(|item| {
            daw::service::Midi::notes(
                &daw,
                daw::service::MidiTakeLocation::new(
                    project.clone(),
                    ItemRef::Guid(item.guid.clone()),
                    daw::service::TakeRef::Active,
                ),
            )
        })
        .collect();
        assert!(!notes.is_empty());
        for n in &notes {
            assert!(
                n.length_ppq <= 0.25 + 1e-6 && n.length_ppq > 0.0,
                "a {role} note {} quarter notes long",
                n.length_ppq
            );
        }
        for pair in notes.windows(2) {
            if let [a, b] = pair {
                assert!(
                    a.start_ppq + a.length_ppq < b.start_ppq,
                    "{role} notes touch: {a:?} {b:?}"
                );
            }
        }
    }
    assert_eq!(
        inside_click_guide,
        [
            "Click",
            "Shaker",
            "Count",
            "Guide",
            "Click Audio",
            "Guide Audio"
        ],
        "the Guide folder, in order, and it closes after them"
    );
    let total: i32 = tracks.iter().map(|t| t.folder_depth).sum();
    assert_eq!(total, 0, "every folder in the session closes");

    // And it is heard: render the first bars — the stems are muted and
    // their media is missing here, so any sound is the guide's.
    let renderer = daw::standalone::audio_engine::render::ProjectRenderer::new(
        &daw,
        &opened.project_guid,
        48_000,
    );
    let mut peak = 0.0f32;
    for block in 0..(48_000 * 4 / 512) {
        let out = renderer.render_block(block * 512, 512);
        peak = out.samples.iter().fold(peak, |m, s| m.max(s.abs()));
    }
    assert!(peak > 0.01, "the click and count are audible (peak {peak})");

    // Generating again rewrites the generated tracks, not the stems.
    Guide::new(daw.clone())
        .with_instrument(session_daw::guide_instrument::IDENT)
        .generate(GuideScope::All)
        .expect("regenerate");
    let again = Tracks::all(&daw, project.clone());
    assert_eq!(again.len(), tracks.len(), "no tracks added the second time");
}

/// The real song, prepared the way the window prepares it — for looking
/// at: `FTS_CHART_PROJECT=song.rpp FTS_CHART=song.kf`.
#[test]
fn a_real_session_prepared_when_one_is_given() {
    let _engine = ENGINE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (Some(project_file), Some(chart)) = (
        std::env::var_os("FTS_CHART_PROJECT"),
        std::env::var_os("FTS_CHART"),
    ) else {
        return;
    };
    let opened = session_daw::open::open_silent(std::path::Path::new(&project_file)).expect("open");
    session_daw::prepare::Prepare {
        organize: true,
        chart: Some(chart.into()),
        guide: true,
    }
    .run(&opened)
    .expect("prepare");
    let project = ProjectContext::Project(opened.project_guid.clone());
    let mut depth = 0i32;
    for t in Tracks::all(&opened.daw, project) {
        eprintln!(
            "{}{}{}{}",
            "    ".repeat(usize::try_from(depth.max(0)).unwrap_or(0)),
            t.name,
            if t.folder_depth > 0 { "/" } else { "" },
            match (t.muted, t.visible_in_tcp) {
                (true, false) => "  [muted, hidden]",
                (true, true) => "  [muted]",
                (false, false) => "  [hidden]",
                (false, true) => "",
            }
        );
        depth += t.folder_depth;
    }
    assert_eq!(depth, 0, "every folder closes");

    // A slow song clicks in eighths: count the Click track's notes in the
    // first bar after the count-in.
    let project = ProjectContext::Project(opened.project_guid.clone());
    let click = Tracks::all(&opened.daw, project.clone())
        .into_iter()
        .find(|t| t.name == "Click" && t.folder_depth <= 0 && !t.muted)
        .expect("generated click");
    let bpm = daw::service::TempoMap::get_tempo_at(&opened.daw, project.clone(), 0.0);
    let notes: usize = Items::get_items(&opened.daw, project.clone(), TrackRef::Guid(click.guid))
        .iter()
        .map(|item| {
            daw::service::Midi::notes(
                &opened.daw,
                daw::service::MidiTakeLocation::new(
                    project.clone(),
                    ItemRef::Guid(item.guid.clone()),
                    daw::service::TakeRef::Active,
                ),
            )
            .len()
        })
        .sum();
    eprintln!("{bpm} bpm: {notes} click notes");
}

/// Where a spoken cue lands on a count note, the cue takes its place in
/// the audio: "Intro, 2, 3, 4". Mute the Guide track and the "1" is back.
#[test]
fn a_cue_takes_the_counts_one_unless_the_guide_is_muted() {
    let _engine = ENGINE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !session_daw::guide_instrument::samples_dir()
        .join("Guide")
        .is_dir()
    {
        return; // cues are silent without the library
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("song.rpp");
    std::fs::write(&file, RPP).expect("write");
    let opened = session_daw::open::open_silent(&file).expect("open");
    session_daw::prepare::Prepare {
        organize: true,
        chart: Some({
            let chart = dir.path().join("song.kf");
            std::fs::write(&chart, CHART).expect("chart");
            chart
        }),
        guide: true,
    }
    .run(&opened)
    .expect("prepare");
    let _rt = session_daw::open::runtime().expect("runtime").enter();
    let daw = opened.daw.clone();
    let project = ProjectContext::Project(opened.project_guid.clone());
    let role = |name: &str| {
        Tracks::all(&daw, project.clone())
            .into_iter()
            .find(|t| {
                t.name == name
                    && t.folder_depth <= 0
                    && Items::get_items(&daw, project.clone(), TrackRef::Guid(t.guid.clone()))
                        .iter()
                        .all(|i| {
                            Takes::get_active_take(
                                &daw,
                                project.clone(),
                                ItemRef::Guid(i.guid.clone()),
                            )
                            .is_some_and(|t| t.is_midi)
                        })
            })
            .map(|t| t.guid)
            .unwrap_or_else(|| panic!("generated {name}"))
    };
    let (click, count, guide) = (role("Click"), role("Count"), role("Guide"));
    let mute = |guid: &str, on: bool| {
        Tracks::set_muted(&daw, project.clone(), TrackRef::Guid(guid.to_owned()), on)
            .expect("mute");
    };
    // Only the count and the cues: everything with audio off.
    mute(&click, true);
    for t in Tracks::all(&daw, project.clone()) {
        let audio = Items::get_items(&daw, project.clone(), TrackRef::Guid(t.guid.clone()))
            .iter()
            .any(|i| {
                Takes::get_active_take(&daw, project.clone(), ItemRef::Guid(i.guid.clone()))
                    .is_some_and(|t| !t.is_midi)
            });
        if audio {
            mute(&t.guid, true);
        }
    }

    // The Intro follows two bars of count-in at 90: its cue is on the
    // downbeat of the second bar, 8/3 s in. Render that beat.
    const RATE: u32 = 48_000;
    let beat_at = |seconds: f64| (seconds * f64::from(RATE)) as u64;
    let (from, to) = (beat_at(8.0 / 3.0), beat_at(8.0 / 3.0 + 60.0 / 90.0));
    let render = || {
        let renderer = daw::standalone::audio_engine::render::ProjectRenderer::new(
            &daw,
            &opened.project_guid,
            RATE,
        );
        // Exactly the frames [from, to), interleaved stereo.
        let mut out = Vec::new();
        let mut at = 0u64;
        while at < to {
            let block = renderer.render_block(at, 512);
            for f in 0..512u64 {
                let frame = at + f;
                if (from..to).contains(&frame) {
                    let i = usize::try_from(f).unwrap_or(0) * 2;
                    out.extend_from_slice(&block.samples[i..i + 2]);
                }
            }
            at += 512;
        }
        out
    };
    let both = render();
    mute(&count, true);
    let cue_only = render();
    mute(&count, false);
    mute(&guide, true);
    let count_only = render();

    let loud = |x: &[f32]| x.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let diff = both
        .iter()
        .zip(&cue_only)
        .fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(loud(&cue_only) > 0.01, "the Intro cue sounds");
    assert!(
        diff < 1e-4,
        "with both playing it is the cue alone — the count's 1 gave way (diff {diff})"
    );
    assert!(
        loud(&count_only) > 0.01,
        "with the Guide muted, the count's 1 is back"
    );
}
