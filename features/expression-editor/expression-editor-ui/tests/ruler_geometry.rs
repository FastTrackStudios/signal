//! The ruler's height and the mouse agree.
//!
//! The ruler used to be a constant, and half the view took its offset
//! from that constant: the lane translate, the playhead, and the
//! pointer maths that turns a click into a lane. Then the ruler grew a
//! shelf per (ruler lane, kind) of timeline chrome, so a project with
//! regions *and* markers has a taller ruler than one with neither.
//!
//! That is a quiet failure if it goes wrong. Nothing looks broken — the
//! lanes draw correctly, the ruler draws correctly — but the hit-testing
//! is reading from a different coordinate system than the drawing, and
//! clicks land on the lane above or below the one under the cursor. A
//! screenshot cannot catch it. Only a real click can, which is what
//! these do: mount the actual editor, press a real pointer at a real
//! pixel, and ask which track it selected.

use dioxus::prelude::*;
use dioxus_test::keyboard_types::Modifiers;
use dioxus_test::{by_testid, render};
use expression_editor_core::doc::{Marker, Region};
use expression_editor_core::tracks::Track;
use expression_editor_core::{Editor, ExpressionDoc, Mode, Note, NoteId, TimeBase, Viewport};
use expression_editor_ui::{ExpressionEditor, stack};
use std::cell::RefCell;

const RATE: f64 = 100.0;

/// How far inside a lane's top edge to probe.
///
/// Small on purpose. The error this file exists to catch is a whole
/// ruler shelf — 15px — and a lane is an order of magnitude taller than
/// that, so a probe anywhere near a lane's middle absorbs the drift and
/// passes with the bug present. Only a probe within a few pixels of a
/// boundary turns that drift into a different answer.
const EDGE: f64 = 3.0;

/// The lane geometry the stacked view actually lays out with.
///
/// Must be the component's own parameters, not plausible-looking ones:
/// the active lane is boosted, so a probe computed at boost 1.0 sits in
/// the wrong lane entirely and the test fails against correct code.
fn laid_out(ed: &Editor) -> Vec<expression_editor_ui::stack::LaneView> {
    stack::lanes(ed, Editor::ACTIVE_BOOST, ed.lane_floor().max(22.0))
}

thread_local! {
    static STAGED: RefCell<Option<Editor>> = const { RefCell::new(None) };
}

fn stage(ed: Editor) {
    STAGED.with(|s| *s.borrow_mut() = Some(ed));
}

thread_local! {
    static FILLS: RefCell<Vec<(f64, f64)>> = const { RefCell::new(Vec::new()) };
}

#[component]
fn Surface() -> Element {
    let editor = use_signal(|| STAGED.with(|s| s.borrow_mut().take()).expect("staged"));
    let ed = editor.read();
    let readout = format!("active={}", ed.tracks.active());
    let span = ed.camera.time_span(ed.viewport);
    let ups = ed.doc.time_base.units_per_second(ed.bpm).max(1e-9);
    let view0 = format!("{}", span.0);
    let view_secs = format!("{} {} {}", span.0 / ups, span.1 / ups, ed.viewport.w);
    drop(ed);
    rsx! {
        div { "data-testid": "readout", "{readout}" }
        div { "data-testid": "view0", "{view0}" }
        div { "data-testid": "view-secs", "{view_secs}" }
        ExpressionEditor { editor, fills: FILLS.with(|f| f.borrow().clone()) }
    }
}

fn active(html: &str) -> usize {
    html.split_whitespace()
        .find_map(|kv| kv.strip_prefix("active=")?.parse().ok())
        .unwrap_or_else(|| panic!("no `active` in readout: {html}"))
}

fn doc_with_a_note(row: i32) -> ExpressionDoc {
    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate: RATE }, 0.0, RATE * 4.0);
    doc.push(Note::new(NoteId(1), RATE, RATE * 1.5, row));
    doc
}

/// Three tracks, so there are lanes above and below the one probed —
/// an off-by-a-shelf error has somewhere wrong to land in either
/// direction.
fn three_tracks() -> Editor {
    let mut ed = Editor::new(doc_with_a_note(60), Viewport::new(1200.0, 600.0));
    ed.tracks.track_mut(0).unwrap().name = "One".into();
    for name in ["Two", "Three"] {
        ed.tracks
            .push(Track::in_mode(name, doc_with_a_note(62), Mode::Midi));
    }
    // The stacked view is what has a ruler with shelves; the single-track
    // roll never shows one.
    ed.stacked = true;
    ed
}

/// Section chrome on two different ruler lanes: a region on lane 1 and
/// a marker on lane 3. Two shelves, so the ruler is taller than the
/// bare case by exactly one shelf.
fn with_two_shelves(ed: &mut Editor) {
    ed.doc.regions = vec![Region {
        start: 0.0,
        end: RATE * 2.0,
        label: "VS 1".into(),
        color: Some("#3d5a8a".into()),
        lane: Some((1, "SONG".into())),
    }];
    ed.doc.markers = vec![Marker {
        t: RATE,
        label: Some("IN".into()),
        color: Some("#8a5a3d".into()),
        lane: Some((3, "MARKS".into())),
    }];
}

// ── the height itself ────────────────────────────────────────────────

#[test]
fn a_project_with_no_chrome_keeps_the_ruler_it_always_had() {
    // The shelf height was chosen to match the band the ruler used
    // before shelves existed, so the common case is untouched. If this
    // drifts, every committed screenshot silently reflows.
    let ed = three_tracks();
    assert!(stack::chrome_shelves(&ed).is_empty());
    assert_eq!(stack::ruler_height(&ed), 28.0);
}

#[test]
fn each_kind_on_each_lane_gets_its_own_shelf() {
    let mut ed = three_tracks();
    with_two_shelves(&mut ed);
    let shelves = stack::chrome_shelves(&ed);
    assert_eq!(shelves.len(), 2, "a region shelf and a marker shelf");
    // Ordered by lane, and the region (lane 1) sorts above the marker
    // (lane 3).
    assert_eq!(shelves[0].0, Some(1));
    assert!(shelves[0].1, "regions first");
    assert_eq!(shelves[1].0, Some(3));
    assert!(!shelves[1].1);
    assert_eq!(stack::ruler_height(&ed), 28.0 + 15.0);
}

#[test]
fn a_region_and_a_marker_sharing_a_lane_still_get_two_shelves() {
    // REAPER allows it and these projects do it: `The ballad` files 15
    // regions and 2 markers on `SONG`. On one shelf a marker tick lands
    // inside a region band and the two fight for the same pixels.
    let mut ed = three_tracks();
    with_two_shelves(&mut ed);
    ed.doc.markers[0].lane = Some((1, "SONG".into()));
    let shelves = stack::chrome_shelves(&ed);
    assert_eq!(shelves.len(), 2, "same lane, different kinds, two shelves");
    assert_eq!(shelves[0], (Some(1), true, "SONG".into()));
    assert_eq!(shelves[1], (Some(1), false, "SONG".into()));
}

// ── fill bands ───────────────────────────────────────────────────────

// r[verify drums.fills.draw]
#[tokio::test]
async fn a_fill_draws_a_band_where_it_happened() -> dioxus_test::Result<()> {
    // Rendered geometry, not a signal round trip: the band has to land
    // over the part of the take the fill covers, and the arithmetic that
    // maps seconds to pixels is the part that can be wrong while
    // everything still compiles.
    let mut ed = three_tracks();
    ed.doc.end = RATE * 40.0;
    // A fill over the second quarter of a 40-second take.
    FILLS.with(|f| *f.borrow_mut() = vec![(10.0, 20.0)]);
    ed.zoom_to_box(0.0, RATE * 40.0, 0.0, 127.0);
    stage(ed);

    let tester = render(Surface).with_window_size(1400, 700).build();
    let cell = tester.query(by_testid("stack-cell")).immediately()?;
    FILLS.with(|f| f.borrow_mut().clear());

    // The band is painted into the stack's scene, not emitted as
    // markup — e23f0f0 moved the whole drawing off the DOM, and a
    // custom widget's scene never reaches `render_png` either. So the
    // pane says where its bands are, the way `data-view` says where
    // its camera is: the same numbers the painter is handed.
    let bands = cell
        .attribute("data-fills")
        .unwrap_or_else(|| panic!("the stack cell carries no data-fills"));
    let band = bands
        .split(',')
        .find(|b| !b.is_empty())
        .unwrap_or_else(|| panic!("no fill band in the rendered stack"));
    let (x0, x1) = band
        .split_once(':')
        .and_then(|(a, b)| Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?)))
        .unwrap_or_else(|| panic!("malformed fill band {band:?}"));
    let (x, width) = (x0, x1 - x0);
    // Against the span and viewport the editor *actually* rendered,
    // read back from the mounted component. Two traps here, both hit:
    // the editor resizes to its frame on mount, so the span it was
    // built with is not the one it drew; and these coordinates are SVG
    // user units, not element pixels, so measuring the element's width
    // compares two different spaces and fails on correct code.
    let secs = tester
        .query(by_testid("view-secs"))
        .immediately()?
        .inner_html();
    let mut it = secs
        .split_whitespace()
        .filter_map(|v| v.parse::<f64>().ok());
    let (v0, v1) = (it.next().unwrap_or(0.0), it.next().unwrap_or(1.0));
    let lane_w = it.next().unwrap_or(1.0);
    let want_x = (10.0 - v0) / (v1 - v0) * lane_w;
    let want_w = 10.0 / (v1 - v0) * lane_w;
    assert!(
        (x - want_x).abs() < 0.05 * lane_w,
        "band at x={x:.0} over a view of {v0:.1}s..{v1:.1}s; the fill at \
         10s belongs at x={want_x:.0}"
    );
    assert!(
        (width - want_w).abs() < 0.05 * lane_w,
        "band is {width:.0}px for a 10s fill in a {:.1}s view; wanted {want_w:.0}px",
        v1 - v0
    );
    Ok(())
}

// ── paging the view ──────────────────────────────────────────────────

// r[verify drums.view.page-bars]
#[tokio::test]
async fn the_bracket_key_pages_the_view_by_four_bars() -> dioxus_test::Result<()> {
    // The binding, not the arithmetic — `page_bars` has its own tests
    // in core. What this pins is that the key reaches it at all, on the
    // stacked surface, without being swallowed by another handler.
    let mut ed = three_tracks();
    let bar = RATE * 2.0;
    ed.doc.bars = (0..=32).map(|i| i as f64 * bar).collect();
    // The document has to be as long as the bars claim. The camera
    // clamps to the document, so a four-second doc with a sixty-second
    // grid pages one bar and then stops against the end — correctly,
    // and confusingly.
    ed.doc.end = *ed.doc.bars.last().unwrap();
    ed.frame_bars(4);
    let before = ed.camera.time_span(ed.viewport).0;

    stage(ed);
    let tester = render(Surface).with_window_size(1400, 700).build();
    let cell = tester.query(by_testid("stack-cell")).immediately()?;
    let (ox, oy) = cell.document_origin();
    // Focus the surface first: keys go to whatever holds focus, and an
    // unfocused test types into nothing and passes for the wrong reason.
    tester.pointer_down_mods(ox + 400.0, oy + 200.0, Modifiers::empty());
    tester.pointer_up_mods(ox + 400.0, oy + 200.0, Modifiers::empty());
    tester.drain();
    tester.key_down(
        dioxus_test::keyboard_types::Key::Character("]".into()),
        Modifiers::empty(),
    );
    let _ = tester.pump().await;

    let after = tester
        .query(by_testid("view0"))
        .immediately()?
        .inner_html()
        .trim()
        .parse::<f64>()
        .unwrap_or(f64::NAN);
    assert!(
        (after - before - 4.0 * bar).abs() < 1.0,
        "`]` moved the view from {before} to {after}, not four bars on"
    );
    Ok(())
}

// ── the height and the mouse agree ───────────────────────────────────

/// Click `into_lane` pixels below the top of the lane area and report
/// which track the editor made active.
async fn click_below_the_ruler(ed: Editor, into_lane: f64) -> dioxus_test::Result<usize> {
    let ruler_h = stack::ruler_height(&ed);
    stage(ed);
    let tester = render(Surface).with_window_size(1400, 700).build();
    // Anchor to the stack surface itself, not to the readout above it:
    // the readout's own box is not the editor's top, and guessing at
    // the gap is how a geometry test ends up measuring its own
    // arithmetic instead of the code's.
    let cell = tester.query(by_testid("stack-cell")).immediately()?;
    let (ox, oy) = cell.document_origin();
    let x = ox + expression_editor_ui::canvas::GUTTER_W + 300.0;
    let y = oy + ruler_h + into_lane;
    tester.pointer_down_mods(x, y, Modifiers::empty());
    tester.drain();
    tester.pointer_up_mods(x, y, Modifiers::empty());
    let _ = tester.pump().await;
    let html = tester
        .query(by_testid("readout"))
        .immediately()?
        .inner_html();
    Ok(active(&html))
}

#[tokio::test]
async fn a_click_lands_on_the_same_lane_however_tall_the_ruler_is() -> dioxus_test::Result<()> {
    // The invariant: which lane a click hits depends on how far below
    // the ruler it is, never on where the ruler happens to end. Adding
    // a shelf moves every lane down by 15px; if the hit-testing missed
    // that, the same offset would resolve to a different lane.
    //
    // Probed just inside the *top* of the second lane, not its middle.
    // A lane here is nearly 200px tall, so a shelf's worth of error
    // lands harmlessly mid-lane and the test would pass with the bug
    // present — verified by putting the bug back. At the boundary the
    // same error crosses into the lane above, which is the whole point.
    // The lower edge of the middle lane — a real boundary, and the side
    // that catches drift from hit-testing against too short a ruler.
    let lanes = laid_out(&three_tracks());
    let probe = lanes[1].y + lanes[1].h - EDGE;

    let bare = click_below_the_ruler(three_tracks(), probe).await?;

    let mut tall = three_tracks();
    with_two_shelves(&mut tall);
    let shelved = click_below_the_ruler(tall, probe).await?;

    assert_eq!(
        bare, shelved,
        "the same offset below the ruler picked track {bare} with no chrome \
         and track {shelved} with two shelves — the lanes and the mouse are \
         in different coordinate systems"
    );
    Ok(())
}

#[tokio::test]
async fn clicking_a_lane_selects_that_lane_not_its_neighbour() -> dioxus_test::Result<()> {
    // The above would pass if both cases were wrong in the same way, so
    // this pins the absolute answer: a click in the middle of lane N
    // selects lane N's track.
    let mut ed = three_tracks();
    with_two_shelves(&mut ed);
    let lanes = laid_out(&ed);
    assert!(
        lanes.len() >= 3,
        "expected three lanes, got {}",
        lanes.len()
    );

    // Lanes 1 and 2 only: lane 0 is already active, and the handler
    // deliberately ignores a click on the lane you are in, so it would
    // report success without resolving anything.
    //
    // Both edges of each lane, because the drift has a sign. Hit-testing
    // against a ruler shorter than the one drawn makes the computed
    // offset too *large*, pushing a click down into the next lane — so a
    // top-edge probe absorbs it and only a bottom-edge probe catches it.
    // The reverse error is caught only at the top. Testing one edge left
    // half the failure invisible, which is how this test first passed
    // with the bug deliberately put back.
    let last = lanes.len() - 1;
    for (i, lane) in lanes.iter().enumerate().take(3).skip(1) {
        // Every boundary *between* lanes, from both sides. The final
        // lane's lower edge is skipped: it is the bottom of the
        // viewport rather than a boundary, and the editor resizes to its
        // mounted frame, so that pixel is not dependably inside the
        // surface at all.
        let mut probes = vec![("top", lane.y + EDGE)];
        if i != last {
            probes.push(("bottom", lane.y + lane.h - EDGE));
        }
        for (edge, y) in probes {
            let mut probe_ed = three_tracks();
            with_two_shelves(&mut probe_ed);
            let got = click_below_the_ruler(probe_ed, y).await?;
            assert_eq!(
                got, i,
                "a click just inside the {edge} of lane {i} ({}) selected \
                 track {got} — a ruler shelf's worth of drift lands here",
                lane.name
            );
        }
    }
    Ok(())
}
