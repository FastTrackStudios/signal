//! Paging the view a phrase at a time.
//!
//! Editing drums is done four bars at a time: zoom in, fix them, move
//! on. The claim under test is that the page lands on a *bar line* and
//! keeps its zoom, because a view that drifts off the grid puts the
//! downbeat somewhere different on every page and the thing you
//! navigate by becomes the thing that moves.

use expression_editor_core::{Editor, ExpressionDoc, Note, NoteId, TimeBase, Viewport};

const RATE: f64 = 100.0;

/// A take of `n` bars whose lengths are given in seconds — uneven on
/// purpose, since a real take changes meter.
fn editor_with_bars(lengths: &[f64]) -> Editor {
    let total: f64 = lengths.iter().sum();
    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate: RATE }, 0.0, total * RATE);
    doc.push(Note::new(NoteId(1), 0.0, RATE * 0.1, 60));
    let mut t = 0.0;
    let mut bars = vec![0.0];
    for len in lengths {
        t += len * RATE;
        bars.push(t);
    }
    doc.bars = bars;
    let mut ed = Editor::new(doc, Viewport::new(1000.0, 400.0));
    // Start framed on the first four bars, which is the working state.
    ed.frame_bars(4);
    ed
}

fn even(n: usize) -> Editor {
    editor_with_bars(&vec![2.0; n])
}

fn view(ed: &Editor) -> (f64, f64) {
    ed.camera.time_span(ed.viewport)
}

// r[verify drums.view.page-bars]
#[test]
fn a_page_lands_on_a_bar_line() {
    let mut ed = even(32);
    assert!(ed.page_bars(4, 1));
    let (t0, _) = view(&ed);
    assert!(
        ed.doc.bars.iter().any(|b| (b - t0).abs() < 1e-6),
        "the view started at {t0}, which is not a bar line"
    );
}

// r[verify drums.view.page-bars]
#[test]
fn a_page_moves_exactly_four_bars() {
    let mut ed = even(32);
    let (before, _) = view(&ed);
    ed.page_bars(4, 1);
    let (after, _) = view(&ed);
    // Four bars of two seconds, in document units.
    assert!(
        (after - before - 4.0 * 2.0 * RATE).abs() < 1e-6,
        "moved {} units, wanted {}",
        after - before,
        4.0 * 2.0 * RATE
    );
}

// r[verify drums.view.page-bars]
#[test]
fn paging_keeps_the_zoom() {
    // Otherwise "next page" doubles as a zoom control, and the amount
    // of music on screen changes under the user without being asked.
    let mut ed = even(32);
    let (a0, a1) = view(&ed);
    ed.page_bars(4, 1);
    let (b0, b1) = view(&ed);
    assert!(
        ((b1 - b0) - (a1 - a0)).abs() < 1e-6,
        "span changed from {} to {}",
        a1 - a0,
        b1 - b0
    );
}

// r[verify drums.view.page-bars]
#[test]
fn paging_back_returns_to_where_it_started() {
    let mut ed = even(32);
    let start = view(&ed);
    ed.page_bars(4, 1);
    ed.page_bars(4, -1);
    let back = view(&ed);
    assert!(
        (back.0 - start.0).abs() < 1e-6,
        "went out to {} and came back to {}, not {}",
        start.0,
        back.0,
        start.0
    );
}

// r[verify drums.view.page-bars]
#[test]
fn uneven_bars_page_by_bars_not_by_seconds() {
    // The reason bar lines are supplied rather than computed. In 7/4
    // and 6/8 the bars are different lengths, and a page of a fixed
    // number of seconds would land mid-bar and stay there.
    let mut ed = editor_with_bars(&[2.0, 2.0, 2.0, 2.0, 3.5, 3.5, 3.5, 3.5, 2.0, 2.0, 2.0, 2.0]);
    assert!(ed.page_bars(4, 1));
    let (t0, _) = view(&ed);
    assert!(
        (t0 - 4.0 * 2.0 * RATE).abs() < 1e-6,
        "expected the fifth bar at {}, got {t0}",
        4.0 * 2.0 * RATE
    );
    // The next page crosses the long bars, so it must move further in
    // seconds than the previous one did.
    assert!(ed.page_bars(4, 1));
    let (t1, _) = view(&ed);
    assert!(
        (t1 - t0 - 4.0 * 3.5 * RATE).abs() < 1e-6,
        "the long bars were paged as if they were short ones"
    );
}

// r[verify drums.view.page-bars]
#[test]
fn paging_stops_at_the_end_rather_than_leaving_the_take() {
    // Landing past the last bar shows an empty screen, which reads as
    // the editor having lost the project rather than as the end.
    let mut ed = even(10);
    for _ in 0..20 {
        ed.page_bars(4, 1);
    }
    let (t0, _) = view(&ed);
    let last = *ed.doc.bars.last().unwrap();
    assert!(
        t0 < last,
        "the view starts at {t0}, past the last bar {last}"
    );
    assert!(!ed.page_bars(4, 1), "kept paging past the end");
}

// r[verify drums.view.page-bars]
#[test]
fn a_view_nudged_off_the_grid_re_aligns() {
    // Paging from wherever the view happens to be would carry a small
    // scroll error forward into every subsequent page.
    let mut ed = even(32);
    ed.pan_px(7.0, 0.0);
    let (drifted, _) = view(&ed);
    assert!(ed.doc.bars.iter().all(|b| (b - drifted).abs() > 1e-6));
    ed.page_bars(4, 1);
    let (t0, _) = view(&ed);
    assert!(
        ed.doc.bars.iter().any(|b| (b - t0).abs() < 1e-6),
        "the drift was carried forward: {t0}"
    );
}

#[test]
fn without_a_bar_grid_nothing_moves() {
    // A host with no tempo map supplies no bars, and guessing a grid
    // would page to positions that mean nothing.
    let mut ed = even(32);
    ed.doc.bars.clear();
    let before = view(&ed);
    assert!(!ed.page_bars(4, 1));
    assert_eq!(view(&ed).0, before.0);
}

// r[verify drums.view.page-bars]
#[test]
fn framing_shows_exactly_the_requested_bars() {
    let mut ed = even(32);
    ed.page_bars(4, 2);
    assert!(ed.frame_bars(4));
    let (t0, t1) = view(&ed);
    assert!(
        (t1 - t0 - 4.0 * 2.0 * RATE).abs() < 1e-6,
        "framed {}",
        t1 - t0
    );
    assert!(ed.doc.bars.iter().any(|b| (b - t0).abs() < 1e-6));
}
