//! An `<ITEM>` must survive a parse/serialize round trip with each of its
//! take fields written exactly ONCE.
//!
//! The serializer used to write the item-level take fields (which ARE
//! take #0's — that is how the RPP format stores the first take) and then
//! *also* iterate `Item::takes`, emitting take #0 a second time, with a
//! `LANE` line wedged between the two copies. On a real 1.8 MB project
//! that took NAME from 196 to 359 occurrences, VOLPAN 196 -> 350, SOFFS
//! 154 -> 304, PLAYRATE 155 -> 308, and introduced 154 `LANE` tokens —
//! which REAPER rejects outright ("Project tokens not recognized: LANE").
//! `CHANMODE`, `YPOS` and `<EXT>` were dropped on the same pass.

use dawfile_reaper::primitives::block::parse_block;
use dawfile_reaper::types::serialize::RppSerialize;
use dawfile_reaper::{Item, SourceType};

/// Verbatim from a real project — the item that exposed the bug.
const ITEM: &str = r#"<ITEM
  POSITION 3
  SNAPOFFS 0
  LENGTH 25.5
  LOOP 1
  ALLTAKES 0
  FADEIN 1 0 0 1 0 0 0
  FADEOUT 1 0.01 0 1 0 0 0
  MUTE 0 0
  SEL 0
  YPOS 0 1 2
  IGUID {606F9861-B401-AF49-B000-3EE403EEC38A}
  IID 352
  NAME "Bass - It knows my name.wav"
  VOLPAN 1 0 1 -1
  SOFFS 4.4692408724771
  PLAYRATE 1.0317 1 0 -1 0 0.0025
  CHANMODE 0
  GUID {35ACC821-3163-2E45-9377-59CAC0311723}
  <SOURCE WAVE
    FILE "Media/Bass - It knows my name.wav"
  >
  <EXT
    ORIGINAL_FILENAME "/Volumes/Josh-SSD/Crescendum Scratch Tracks/Songs/It Knows my Name/Bass - It knows my name.wav"
  >
>"#;

/// Number of lines in `rpp` whose first whitespace-separated token is `token`.
fn count_token(rpp: &str, token: &str) -> usize {
    rpp.lines()
        .filter(|line| line.split_whitespace().next() == Some(token))
        .count()
}

fn assert_written_once(rpp: &str, route: &str) {
    for token in ["NAME", "VOLPAN", "SOFFS", "PLAYRATE", "GUID"] {
        assert_eq!(
            count_token(rpp, token),
            1,
            "{route}: expected exactly one {token} line, got:\n{rpp}"
        );
    }
}

fn assert_no_lane(rpp: &str, route: &str) {
    assert_eq!(
        count_token(rpp, "LANE"),
        0,
        "{route}: REAPER has no ITEM-level LANE token; got:\n{rpp}"
    );
}

fn assert_content_survives(rpp: &str, route: &str) {
    assert_eq!(
        count_token(rpp, "CHANMODE"),
        1,
        "{route}: CHANMODE must survive, exactly once:\n{rpp}"
    );
    assert!(
        rpp.contains("YPOS 0 1 2"),
        "{route}: YPOS must survive as YPOS, not LANE:\n{rpp}"
    );
    assert!(
        rpp.contains("<EXT"),
        "{route}: the <EXT> block must survive:\n{rpp}"
    );
    assert!(
        rpp.contains(
            "ORIGINAL_FILENAME \"/Volumes/Josh-SSD/Crescendum Scratch Tracks/Songs/It Knows my Name/Bass - It knows my name.wav\""
        ),
        "{route}: EXT contents must survive verbatim:\n{rpp}"
    );
    assert!(
        rpp.contains("FILE \"Media/Bass - It knows my name.wav\""),
        "{route}: the source path must survive:\n{rpp}"
    );
    // Item-level identity is untouched by the take de-duplication.
    assert!(rpp.contains("IGUID {606F9861-B401-AF49-B000-3EE403EEC38A}"));
    assert!(rpp.contains("GUID {35ACC821-3163-2E45-9377-59CAC0311723}"));
    assert!(rpp.contains("IID 352"));
    assert!(rpp.contains("POSITION 3"));
    assert!(rpp.contains("LENGTH 25.5"));
}

/// The token route (`Item::from_block`) is what a real project parse
/// uses, and it is the route that double-wrote every take.
#[test]
fn token_route_writes_each_take_field_once() {
    let (_, block) = parse_block(ITEM).expect("fixture parses as a block");
    let item = Item::from_block(&block).expect("item parses");

    assert_eq!(item.takes.len(), 1, "one inline take, no phantom take #0");
    let rpp = item.to_rpp_string();

    assert_written_once(&rpp, "from_block");
    assert_no_lane(&rpp, "from_block");
    assert_content_survives(&rpp, "from_block");
}

/// The string route keeps `raw_content` and echoes it, so clear it to put
/// the same item through the field-by-field serializer.
#[test]
fn string_route_writes_each_take_field_once() {
    let mut item = Item::from_rpp_block(ITEM).expect("item parses");
    assert_eq!(item.takes.len(), 1);
    item.raw_content.clear();

    let rpp = item.to_rpp_string();

    assert_written_once(&rpp, "from_rpp_block");
    assert_no_lane(&rpp, "from_rpp_block");
    assert_content_survives(&rpp, "from_rpp_block");
}

/// Both routes must agree on the take contents they extracted.
#[test]
fn both_routes_capture_the_same_take() {
    let (_, block) = parse_block(ITEM).expect("fixture parses as a block");
    let from_tokens = Item::from_block(&block).expect("token route");
    let from_text = Item::from_rpp_block(ITEM).expect("string route");

    for item in [&from_tokens, &from_text] {
        let take = item.takes.first().expect("a first take");
        assert_eq!(take.name, "Bass - It knows my name.wav");
        assert_eq!(
            take.take_guid.as_deref(),
            Some("{35ACC821-3163-2E45-9377-59CAC0311723}")
        );
        let source = take.source.as_ref().expect("a source");
        assert_eq!(source.source_type, SourceType::Wave);
        assert_eq!(source.file_path, "Media/Bass - It knows my name.wav");
        assert_eq!(
            take.extra_blocks.len(),
            1,
            "the <EXT> block is kept on the take"
        );
        assert!(take.extra_blocks[0].contains("ORIGINAL_FILENAME"));
        let ypos = item.y_pos.expect("YPOS parsed");
        assert_eq!((ypos.y, ypos.height, ypos.mode), (0.0, 1.0, 2));
    }
}

/// A second take gets a `TAKE` marker; the first never does. Regression
/// guard on the de-duplication: it must not swing the other way and drop
/// or double a genuine multi-take item.
#[test]
fn multi_take_item_writes_one_marker_per_extra_take() {
    let rpp_in = r#"<ITEM
  POSITION 0
  LENGTH 10
  NAME "take-one.wav"
  VOLPAN 1 0 1 -1
  SOFFS 0
  CHANMODE 0
  GUID {AAAAAAAA-0000-0000-0000-000000000001}
  <SOURCE WAVE
    FILE "Media/take-one.wav"
  >
  TAKE SEL
  NAME "take-two.wav"
  VOLPAN 1 0 1 -1
  SOFFS 0
  CHANMODE 0
  GUID {AAAAAAAA-0000-0000-0000-000000000002}
  <SOURCE WAVE
    FILE "Media/take-two.wav"
  >
>"#;

    let (_, block) = parse_block(rpp_in).expect("block parses");
    let item = Item::from_block(&block).expect("item parses");
    assert_eq!(item.takes.len(), 2);

    let out = item.to_rpp_string();
    assert_eq!(
        count_token(&out, "TAKE"),
        1,
        "one marker for take #1:\n{out}"
    );
    assert_eq!(count_token(&out, "NAME"), 2, "one NAME per take:\n{out}");
    assert_eq!(count_token(&out, "GUID"), 2, "one GUID per take:\n{out}");
    assert_no_lane(&out, "multi-take");
    assert!(out.contains("FILE \"Media/take-one.wav\""));
    assert!(out.contains("FILE \"Media/take-two.wav\""));
}
