//! A keyflow chart builds its song structure into the project the Session
//! window has open — tempo, markers, section regions and the Keyflow
//! folder — on `daw-standalone`, through `session::keyflow::from_chart`.

use std::path::Path;

use daw::service::{Markers, ProjectContext, Regions, TempoMap, Tracks};
use session::keyflow::from_chart::{build_from_chart, rebuild_from_chart};

/// A project with nothing in it yet — the state a chart is built into.
const EMPTY: &str = "<REAPER_PROJECT 0.1 \"7.0/test\" 0\n  TEMPO 120 4 4\n>\n";

/// Two bars of count-in, then a short song — at 90, so a bar is 8/3 s.
const CHART: &str = "\
Test Song - Nobody
90bpm 4/4 #F
Count 2
In 4
1 5 6 4
vs 8
1 5 6 4 x2
ch 8
4 1 5 6 x2
";

/// Opening a project stands up the window's process-wide engine (the
/// facade, the "current" project), so two tests opening at once step on
/// each other. One at a time.
static ENGINE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn build(
    project_file: &Path,
    chart: &str,
) -> (
    daw::standalone::Standalone,
    ProjectContext,
    session::keyflow::from_chart::ChartBuilt,
) {
    let opened = session_daw::open::open_silent(project_file).expect("open");
    // The transport engine spawns on the window's runtime; so must we.
    let _runtime = session_daw::open::runtime()
        .expect("engine runtime")
        .enter();
    let project = ProjectContext::Project(opened.project_guid.clone());
    let built = build_from_chart(&opened.daw, &project, chart).expect("build from chart");
    (opened.daw, project, built)
}

#[test]
fn a_chart_builds_tempo_markers_regions_and_the_keyflow_folder() {
    let _engine = ENGINE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("empty.rpp");
    std::fs::write(&file, EMPTY).expect("write project");
    let (daw, project, built) = build(&file, CHART);

    assert!((built.tempo_bpm - 90.0).abs() < 1e-9);
    assert_eq!(built.time_sig, (4, 4));
    assert!((TempoMap::get_tempo_at(&daw, project.clone(), 0.0) - 90.0).abs() < 1e-6);

    // Two bars of count-in at 90 in 4/4 is 16/3 s; the song starts there.
    let bar = 4.0 * 60.0 / 90.0;
    assert!((built.song_start_seconds - 2.0 * bar).abs() < 1e-6);

    let markers: Vec<String> = Markers::all(&daw, project.clone())
        .into_iter()
        .map(|m| m.name)
        .collect();
    for want in ["COUNT-IN", "SONGSTART", "SONGEND", "=END"] {
        assert!(
            markers.iter().any(|m| m.eq_ignore_ascii_case(want)),
            "missing {want} marker; have {markers:?}"
        );
    }

    let regions = Regions::all(&daw, project.clone());
    assert!(
        regions.iter().any(|r| r.name == "Test Song"),
        "the SONG-lane region carries the title"
    );
    // Intro, verse and chorus: at least one region each besides the song's.
    assert!(
        regions.len() > built.sections.min(3),
        "section regions: {regions:?}"
    );

    let names: Vec<String> = Tracks::all(&daw, project.clone())
        .into_iter()
        .map(|t| t.name)
        .collect();
    let at = names
        .iter()
        .position(|n| n == "Keyflow")
        .expect("Keyflow folder");
    assert_eq!(
        &names[at + 1..at + 6],
        ["KEY", "CHORD", "LINES", "Lyrics", "HITS"]
    );

    // KEY: one key item at the start, read back as the chart's key.
    let changes = session::key::key_changes(&daw, &project);
    assert_eq!(changes.len(), 1, "one key item: {changes:?}");
    assert!(changes[0].seconds.abs() < 1e-6, "at the start");
    assert!(
        session::key::format_key(&changes[0].key).starts_with('F'),
        "in F: {}",
        session::key::format_key(&changes[0].key)
    );

    // CHORD: one MIDI item per chord, after the two count-in bars, named
    // as written and holding notes. The verse's first chord (1 = F) is at
    // bar 6 (2 count + 4 intro) — 16 s at 90.
    let chord_track = Tracks::all(&daw, project.clone())
        .into_iter()
        .find(|t| t.name == "CHORD")
        .expect("CHORD");
    let items = daw::service::Items::get_items(
        &daw,
        project.clone(),
        daw::service::TrackRef::Guid(chord_track.guid),
    );
    // intro 4 bars + verse 4 bars x2 + chorus 4 bars x2, one chord a bar.
    assert_eq!(items.len(), 20, "a chord item per chord");
    let first = items
        .iter()
        .min_by(|a, b| a.position.as_seconds().total_cmp(&b.position.as_seconds()))
        .expect("items");
    assert!(
        (first.position.as_seconds() - 2.0 * bar).abs() < 1e-6,
        "after the count-in"
    );
    let notes = daw::service::Midi::notes(
        &daw,
        daw::service::MidiTakeLocation::new(
            project.clone(),
            daw::service::ItemRef::Guid(first.guid.clone()),
            daw::service::TakeRef::Active,
        ),
    );
    assert_eq!(notes.len(), 3, "a triad");
    assert_eq!(
        first.label.as_deref(),
        Some("1"),
        "named as the chart writes it"
    );

    // A second build is refused rather than stamping everything twice.
    let again = build_from_chart(&daw, &project, CHART);
    assert!(
        again.is_err(),
        "a project with its structure built refuses a rebuild"
    );
}

/// The same song, edited: faster, in G, a longer chorus, different chords.
const EDITED: &str = "\
Test Song - Nobody
100bpm 4/4 #G
Count 2
In 4
1 5 6 4
vs 8
1 5 6 4 x2
ch 16
4 1 5 6 x4
";

/// A changed chart replaces what the chart owns — tempo, the structural
/// markers, the song and section regions, the key and the chords — in
/// place, once each; leaves what a person put there; and says what span
/// the old song covered. Text that does not parse changes nothing.
#[test]
fn a_changed_chart_rebuilds_in_place() {
    let _engine = ENGINE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("empty.rpp");
    std::fs::write(&file, EMPTY).expect("write project");
    let (daw, project, first) = build(&file, CHART);
    // A marker someone placed by hand.
    Markers::add(&daw, project.clone(), 3.0, "Lyrics").expect("hand marker");

    let rebuilt = rebuild_from_chart(&daw, &project, EDITED).expect("rebuild");
    let (old_start, old_end) = rebuilt.replaced.expect("there was a song to replace");
    assert!(
        old_start.abs() < 1e-6,
        "the old song began at its count-in, at zero"
    );
    assert!(
        old_end >= first.song_end_seconds - 1e-6,
        "{old_end} covers the old song"
    );

    assert!((TempoMap::get_tempo_at(&daw, project.clone(), 0.0) - 100.0).abs() < 1e-6);
    let markers: Vec<String> = Markers::all(&daw, project.clone())
        .into_iter()
        .map(|m| m.name)
        .collect();
    for want in ["COUNT-IN", "SONGSTART", "SONGEND", "=END"] {
        let n = markers
            .iter()
            .filter(|m| m.eq_ignore_ascii_case(want))
            .count();
        assert_eq!(n, 1, "{want} once, not stamped twice: {markers:?}");
    }
    assert!(
        markers.iter().any(|m| m == "Lyrics"),
        "the hand-placed marker stays"
    );
    let songs = Regions::all(&daw, project.clone())
        .into_iter()
        .filter(|r| r.name == "Test Song")
        .count();
    assert_eq!(songs, 1, "one SONG-lane region");

    let tracks = Tracks::all(&daw, project.clone());
    assert_eq!(
        tracks.iter().filter(|t| t.name == "Keyflow").count(),
        1,
        "one Keyflow folder"
    );
    let changes = session::key::key_changes(&daw, &project);
    assert_eq!(changes.len(), 1, "one key item: {changes:?}");
    assert!(
        session::key::format_key(&changes[0].key).starts_with('G'),
        "now in G"
    );
    let chord_track = tracks.iter().find(|t| t.name == "CHORD").expect("CHORD");
    let items = daw::service::Items::get_items(
        &daw,
        project.clone(),
        daw::service::TrackRef::Guid(chord_track.guid.clone()),
    );
    // intro 4 + verse 8 + chorus 16, one chord a bar.
    assert_eq!(items.len(), 28, "the new chords, and none of the old");

    // Half-typed: nothing moves.
    let regions_before = Regions::all(&daw, project.clone()).len();
    assert!(
        rebuild_from_chart(
            &daw,
            &project,
            "Test Song\n100bpm 4/4 #G\nvs 8\n1 5 6 4 x9\n"
        )
        .is_err()
    );
    assert_eq!(Regions::all(&daw, project.clone()).len(), regions_before);
    assert!((TempoMap::get_tempo_at(&daw, project.clone(), 0.0) - 100.0).abs() < 1e-6);
}

/// The real song, when a path to it is given — for looking at the result,
/// not for CI: `FTS_CHART_PROJECT=song.rpp FTS_CHART=song.kf`.
#[test]
fn a_real_chart_when_one_is_given() {
    let _engine = ENGINE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (Some(project_file), Some(chart)) = (
        std::env::var_os("FTS_CHART_PROJECT"),
        std::env::var_os("FTS_CHART"),
    ) else {
        return;
    };
    let text = std::fs::read_to_string(&chart).expect("chart");
    let (daw, project, built) = build(Path::new(&project_file), &text);
    eprintln!("{built:#?}");
    let mut regions = Regions::all(&daw, project.clone());
    regions.sort_by(|a, b| a.start_seconds().total_cmp(&b.start_seconds()));
    for r in regions {
        eprintln!(
            "  region {:>8.3} – {:>8.3}  lane {:?} color {:?}  {}",
            r.start_seconds(),
            r.end_seconds(),
            r.lane,
            r.color.map(|c| format!("{c:#08x}")),
            r.name
        );
    }
    let mut markers = Markers::all(&daw, project);
    markers.sort_by(|a, b| a.position_seconds().total_cmp(&b.position_seconds()));
    for m in markers {
        eprintln!("  marker {:>8.3}  {}", m.position_seconds(), m.name);
    }
}

/// Editing the chart live: each change rebuilds the song and regenerates
/// the click, count and cues to match — one item per role, the new song's,
/// with nothing of a longer earlier version left behind.
#[test]
fn an_edited_chart_regenerates_the_guide_to_match() {
    let _engine = ENGINE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("empty.rpp");
    std::fs::write(&file, EMPTY).expect("write project");
    let opened = session_daw::open::open_silent(&file).expect("open");
    let _runtime = session_daw::open::runtime()
        .expect("engine runtime")
        .enter();
    let guid = opened.project_guid.clone();
    let project = ProjectContext::Project(guid.clone());

    // (role, start, end) of every generated item.
    let generated = |daw: &daw::standalone::Standalone| -> Vec<(String, f64, f64)> {
        Tracks::all(daw, project.clone())
            .into_iter()
            .filter(|t| {
                ["Click", "Count", "Guide"].contains(&t.name.as_str()) && t.folder_depth <= 0
            })
            .flat_map(|t| {
                let name = t.name.clone();
                daw::service::Items::get_items(
                    daw,
                    project.clone(),
                    daw::service::TrackRef::Guid(t.guid),
                )
                .into_iter()
                .map(move |i| {
                    let at = i.position.as_seconds();
                    (name.clone(), at, at + i.length.as_seconds())
                })
            })
            .collect()
    };

    let long = session_daw::prepare::apply_chart(&opened.daw, &guid, EDITED, true)
        .expect("the long chart");
    let items = generated(&opened.daw);
    assert_eq!(
        items.len(),
        3,
        "one item each for click, count and cues: {items:?}"
    );
    assert!(
        items
            .iter()
            .all(|(_, _, end)| *end >= long.built.song_end_seconds - 1e-3)
    );
    let long_end = items.iter().map(|(_, _, end)| *end).fold(0.0_f64, f64::max);

    let short = session_daw::prepare::apply_chart(&opened.daw, &guid, CHART, true)
        .expect("the short chart");
    assert!(short.built.song_end_seconds < long.built.song_end_seconds);
    let items = generated(&opened.daw);
    assert_eq!(items.len(), 3, "the long song's items are gone: {items:?}");
    // The shorter song's items, which end with it — not the long song's.
    assert!(
        items
            .iter()
            .all(|(_, _, end)| *end >= short.built.song_end_seconds - 1e-3 && *end < long_end - 1.0),
        "the shorter song's guide: {items:?} (the long one ran to {long_end})"
    );
    assert!((TempoMap::get_tempo_at(&opened.daw, project.clone(), 0.0) - 90.0).abs() < 1e-6);
}

/// A one-bar meter change in the chart is one in the song: the tempo map
/// changes to 2/4 at the bar and back to 4/4 after it — once, however many
/// times the chart is laid over the song again.
#[test]
fn a_bar_of_two_four_reaches_the_tempo_map() {
    let _engine = ENGINE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("empty.rpp");
    std::fs::write(&file, EMPTY).expect("write project");
    let opened = session_daw::open::open_silent(&file).expect("open");
    let _runtime = session_daw::open::runtime()
        .expect("engine runtime")
        .enter();
    let project = ProjectContext::Project(opened.project_guid.clone());
    let chart = "Song\n60bpm 4/4 #D\n\nCount 1\nCH 2\nBreakdown 1\n!T2/4\nVS 2\n";

    for _ in 0..2 {
        session::keyflow::from_chart::rebuild_from_chart(&opened.daw, &project, chart)
            .expect("rebuild");
    }
    let meters: Vec<(i64, Option<(u32, u32)>)> =
        TempoMap::get_tempo_points(&opened.daw, project.clone())
            .into_iter()
            .map(|p| {
                (
                    (p.position.seconds().unwrap_or(-1.0) * 1000.0).round() as i64,
                    p.time_signature
                        .map(|ts| (ts.numerator(), ts.denominator())),
                )
            })
            .collect();
    assert_eq!(
        meters,
        vec![(12_000, Some((2, 4))), (14_000, Some((4, 4)))],
        "{meters:?}"
    );

    // The verse after the breakdown starts two beats earlier than 4/4 would.
    let verse = Regions::all(&opened.daw, project.clone())
        .into_iter()
        .find(|r| r.name.starts_with("VS"))
        .expect("a verse region");
    assert!(
        (verse.time_range.start_seconds() - 14.0).abs() < 1e-6,
        "{verse:?}"
    );
}
