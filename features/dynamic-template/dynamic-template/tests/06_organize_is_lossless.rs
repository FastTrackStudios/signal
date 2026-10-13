//! Organizing a project must only *add* to it.
//!
//! The regression this pins down shipped for months and was invisible until a
//! real album hit it. `--apply-buses` used to read a project into
//! `ReaperProject`, apply the template, and write it back with
//! `to_rpp_string()` — a typed serializer that can only emit the fields the
//! type models. Everything else in the file was dropped: the master track,
//! `<NOTES>`, `<RECORD_CFG>`, all twelve `RENDER_*` settings, `<METRONOME>`,
//! `<PROJBAY>`, `<EXTENSIONS>`, per-item `CHANMODE` and `YPOS`, and every
//! `<EXT>` block carrying the original capture filename. On top of that it
//! wrote each take twice around a `LANE` token REAPER has never had. REAPER
//! opened the result with "11160 elements in the project were not understood".
//!
//! Nothing about that was caught by a test, because every existing test built
//! its project *through* the same typed model — so the fields the model
//! couldn't represent were never there to lose.
//!
//! These tests work the other way round: start from project text containing
//! things the template knows nothing about, organize, and require them back
//! verbatim.

use dynamic_template::apply::chunk::RChunkTarget;
use dynamic_template::apply::{apply_buses, apply_colors, apply_routing};
use dynamic_template::buses::all_buses;

/// A small project carrying a representative sample of what a real session
/// holds and the template has no model for.
const PROJECT: &str = r#"<REAPER_PROJECT 0.1 "7.65/macOS-arm64" 1786573501 0
  <NOTES 0 2
    take one was the good one
  >
  RIPPLE 0 0
  AUTOXFADE 129
  PEAKGAIN 28.10243685
  <RECORD_CFG
    ZXZhdxgAAA==
  >
  <RENDER_CFG
    ZXZhdxgAAA==
  >
  RENDER_FILE ""
  RENDER_PATTERN "$project"
  RENDER_FMT 0 2 0
  <METRONOME 6 2
    VOL 0.25 0.125
  >
  <TRACK {76C5CF39-EE77-234F-B94C-1A51752BA9B6}
    NAME "Kick In"
    PEAKCOL 16576
    IPHASE 0
    PANLAWFLAGS 3
    ISBUS 0 0
    NCHAN 2
    <ITEM
      POSITION 3
      LENGTH 25.5
      YPOS 0 1 2
      NAME "Kick In.wav"
      CHANMODE 0
      GUID {35ACC821-3163-2E45-9377-59CAC0311723}
      <SOURCE WAVE
        FILE "Media/Kick In.wav"
      >
      <EXT
        ORIGINAL_FILENAME "/Volumes/SSD/Songs/Kick In.wav"
      >
    >
  >
  <TRACK {F669FE35-70B5-AF42-9F2D-058FD4372C72}
    NAME "Bass DI"
    PEAKCOL 16576
    IPHASE 0
    ISBUS 0 0
    NCHAN 2
  >
  <EXTENSIONS
    <DRIVEN_BY_MOSS
    >
  >
>
"#;

/// Lines the template is entitled to change: a track's own template-owned
/// properties. Everything else must survive untouched.
const TEMPLATE_OWNED: &[&str] = &["PEAKCOL", "ISBUS", "BUSCOMP", "MAINSEND", "NCHAN"];

fn organize(text: &str) -> String {
    let mut project = dawfile_reaper::read_rpp_chunk(text).expect("parse");
    let mut target = RChunkTarget::new(&mut project);
    let applied = apply_buses(&mut target, &all_buses()).expect("buses");
    apply_colors(&mut target).expect("colors");
    apply_routing(&mut target, &applied).expect("routing");
    dawfile_reaper::stringify_rpp_node(&dawfile_reaper::RNodeTree::Chunk(project))
}

fn key_of(line: &str) -> &str {
    line.trim().split_whitespace().next().unwrap_or("")
}

#[test]
fn every_line_the_template_does_not_own_survives_verbatim() {
    let out = organize(PROJECT);

    let mut missing: Vec<&str> = Vec::new();
    for line in PROJECT.lines().filter(|l| !l.trim().is_empty()) {
        if TEMPLATE_OWNED.contains(&key_of(line)) {
            continue;
        }
        if !out.lines().any(|l| l == line) {
            missing.push(line);
        }
    }

    assert!(
        missing.is_empty(),
        "organizing dropped {} line(s) it does not own:\n{}",
        missing.len(),
        missing.join("\n")
    );
}

#[test]
fn the_project_body_is_never_re_serialized() {
    // Named individually so a failure says which part of a real session would
    // have been lost, rather than just "a line went missing".
    let out = organize(PROJECT);
    for needle in [
        "take one was the good one", // <NOTES> content
        "<RECORD_CFG",
        "<RENDER_CFG",
        r#"RENDER_PATTERN "$project""#,
        "<METRONOME 6 2",
        "PEAKGAIN 28.10243685",
        "AUTOXFADE 129",
        "PANLAWFLAGS 3", // per-track, unmodelled
        "YPOS 0 1 2",    // per-item lane
        "CHANMODE 0",    // per-take
        "<EXT",          // item extension block
        r#"ORIGINAL_FILENAME "/Volumes/SSD/Songs/Kick In.wav""#,
        "<DRIVEN_BY_MOSS",
    ] {
        assert!(out.contains(needle), "organizing lost {needle:?}");
    }
}

#[test]
fn no_take_is_written_twice_and_no_lane_token_is_invented() {
    let out = organize(PROJECT);
    // `LANE` is not an RPP token at any REAPER version — the item's lane is
    // `YPOS`. Emitting one made REAPER reject the whole item.
    assert_eq!(
        out.lines().filter(|l| key_of(l) == "LANE").count(),
        0,
        "invented a LANE token"
    );
    // The one item has exactly one take, so exactly one of each take field.
    for key in ["SOFFS", "CHANMODE", "YPOS"] {
        assert!(
            out.lines().filter(|l| key_of(l) == key).count() <= 1,
            "{key} was written more than once — the take is duplicated"
        );
    }
}

#[test]
fn organizing_twice_changes_nothing_the_second_time() {
    // Idempotence is what makes the pipeline safe to re-run over an album, and
    // it is only observable now that the output is stable line for line.
    let once = organize(PROJECT);
    let twice = organize(&once);
    assert_eq!(once, twice, "a second organize pass was not a no-op");
}
