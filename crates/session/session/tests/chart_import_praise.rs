//! Offline test: the "Praise" keyflow chart lays out into a correct session
//! song skeleton (no audio, no REAPER). Golden case for the chart→sections
//! bridge behind the Praise demo song and live keyflow-text editing.

use session::keyflow::actions::SectionKind;
use session::setlist::chart_import::{ChartLayout, chart_to_layout};

const PRAISE_CHART: &str = "\
Praise - Elevation Worship
#A 127bpm 4/4

Count 2
In 4
Refrain 8
VS 8
VS
PRE 2
CH 8
VS
VS
PRE
CH
CH
Interlude \"Breakdown\" 8
BR \"Down\" 8
BR \"Build\"
CH
CH
CH
INST \"Guitar Lead\" 8
Refrain
Refrain";

fn layout() -> Result<ChartLayout, Box<dyn std::error::Error>> {
    Ok(chart_to_layout(PRAISE_CHART)?)
}

#[test]
fn praise_header_parses() {
    let l = layout().unwrap();
    assert!((l.tempo_bpm - 127.0).abs() < 1e-9);
    assert_eq!((l.time_sig_num, l.time_sig_den), (4, 4));
    assert_eq!(l.key.as_deref(), Some("A"));
    assert_eq!(l.title.as_deref(), Some("Praise"));
    assert_eq!(l.artist.as_deref(), Some("Elevation Worship"));
}

#[test]
fn praise_has_all_21_sections_in_order() {
    let l = layout().unwrap();
    assert_eq!(l.sections.len(), 21, "all 21 chart lines become sections");

    // Count-in first, then Intro, then the opening Refrain.
    assert_eq!(l.sections[0].kind, SectionKind::CountIn);
    assert_eq!(l.sections[0].measures, 2);
    assert_eq!(l.sections[1].kind, SectionKind::Intro);
    assert_eq!(l.sections[1].measures, 4);
    assert_eq!(l.sections[2].kind, SectionKind::Refrain);
    assert_eq!(l.sections[2].measures, 8);

    // The song opens and CLOSES on a Refrain (first-class, not dropped).
    assert_eq!(l.sections[19].kind, SectionKind::Refrain);
    assert_eq!(l.sections[20].kind, SectionKind::Refrain);
}

#[test]
fn praise_quoted_labels_and_measures_survive() {
    let l = layout().unwrap();
    // `TYPE "label" N` keeps BOTH the label and the bar count.
    let labeled: Vec<(SectionKind, &str, u32)> = l
        .sections
        .iter()
        .filter_map(|s| s.label.as_deref().map(|lbl| (s.kind, lbl, s.measures)))
        .collect();
    assert!(labeled.contains(&(SectionKind::Interlude, "Breakdown", 8)));
    assert!(labeled.contains(&(SectionKind::Bridge, "Down", 8)));
    // "Build" has no explicit count — inherits 8 from the previous Bridge.
    assert!(labeled.contains(&(SectionKind::Bridge, "Build", 8)));
    assert!(labeled.contains(&(SectionKind::Instrumental, "Guitar Lead", 8)));
}

#[test]
fn praise_timeline_is_contiguous_at_127bpm() {
    let l = layout().unwrap();
    let measure_secs = 4.0 * 60.0 / 127.0; // 1.88976 s

    // Count-in = 2 measures; the first musical downbeat follows it.
    assert!((2.0f64.mul_add(-measure_secs, l.count_in_seconds)).abs() < 1e-6);
    assert!((l.song_start_seconds - l.count_in_seconds).abs() < 1e-9);
    assert!(
        (l.sections[1].start_seconds - l.song_start_seconds).abs() < 1e-6,
        "Intro starts on the first downbeat"
    );

    // Sections are contiguous and the total is the measure sum (146 measures).
    let total_measures: u32 = l.sections.iter().map(|s| s.measures).sum();
    assert_eq!(total_measures, 146);
    assert!((f64::from(total_measures).mul_add(-measure_secs, l.song_end_seconds)).abs() < 1e-6);
    for pair in l.sections.windows(2) {
        assert!(
            (pair[0].end_seconds - pair[1].start_seconds).abs() < 1e-9,
            "no gaps between sections"
        );
    }
}
