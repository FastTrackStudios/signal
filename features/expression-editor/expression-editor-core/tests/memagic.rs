//! Contextual zoom and scroll.
//!
//! What is worth pinning here is not that the arithmetic runs but that
//! the three ideas the design rests on actually hold: the span follows
//! note *density* rather than a count, zoom levels land on musical
//! numbers, and restricting to the item slides the window instead of
//! changing the zoom the user asked for.

use expression_editor_core::memagic::{
    self, Anchor, Config, Horizontal, Modes, Region, Scope, Vertical,
};
use expression_editor_core::{Editor, ExpressionDoc, Note, NoteId, TimeBase, Viewport};

const PPQ: f64 = 960.0;

/// `count` notes of `len`, spaced `gap` apart, from `start`.
fn run(start: f64, count: usize, len: f64, gap: f64, row: i32) -> Vec<Note> {
    (0..count)
        .map(|i| {
            let s = start + i as f64 * (len + gap);
            Note::new(NoteId(i as u64 + 1), s, s + len, row)
        })
        .collect()
}

fn doc_with(notes: Vec<Note>) -> ExpressionDoc {
    let end = notes.iter().map(|n| n.end).fold(PPQ * 8.0, f64::max);
    let mut d = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, end);
    for n in notes {
        d.push(n);
    }
    d
}

#[test]
fn a_denser_passage_zooms_in_further() {
    // The property that makes this better than "show N notes": halve the
    // spacing and the span should shrink with it, continuously, rather
    // than jumping when the Nth note crosses the cursor.
    let cfg = Config::default();
    let sparse = doc_with(run(0.0, 32, PPQ, PPQ, 60));
    let dense = doc_with(run(0.0, 32, PPQ / 4.0, PPQ / 4.0, 60));

    let s = memagic::smart_span(&sparse, PPQ * 8.0, &cfg).expect("sparse");
    let d = memagic::smart_span(&dense, PPQ * 2.0, &cfg).expect("dense");
    assert!(
        d < s,
        "dense passage should zoom in further: dense {d} vs sparse {s}"
    );
}

#[test]
fn the_span_follows_the_notes_near_the_anchor() {
    // Half the take is dense and half is sparse. Anchoring in each half
    // must give a different span — otherwise the weighting is doing
    // nothing and this is just an average over the whole take.
    let cfg = Config::default();
    let mut notes = run(0.0, 16, PPQ / 4.0, PPQ / 4.0, 60);
    notes.extend(run(PPQ * 16.0, 16, PPQ, PPQ, 60));
    let doc = doc_with(notes);

    let in_dense = memagic::smart_span(&doc, PPQ * 2.0, &cfg).expect("dense end");
    let in_sparse = memagic::smart_span(&doc, PPQ * 28.0, &cfg).expect("sparse end");
    assert!(
        in_dense < in_sparse,
        "anchor should dominate: {in_dense} vs {in_sparse}"
    );
}

#[test]
fn an_empty_document_has_no_opinion() {
    // Rather than returning a made-up number the caller cannot tell from
    // a real one.
    let doc = doc_with(Vec::new());
    assert_eq!(memagic::smart_span(&doc, 0.0, &Config::default()), None);
}

#[test]
fn zoom_levels_snap_to_powers_of_two_bars() {
    // Repeatability is the point: land on 1, 2, 4, 8 bars so the view
    // looks the same each time you invoke it in the same place.
    let bar = PPQ * 4.0;
    for (raw, want) in [
        (bar * 0.9, bar),
        (bar * 1.6, bar * 2.0),
        (bar * 2.4, bar * 2.0),
        (bar * 3.1, bar * 4.0),
        (bar * 6.0, bar * 8.0),
    ] {
        let got = memagic::snap_to_measures(raw, bar);
        assert_eq!(
            got / bar,
            want / bar,
            "{} bars should snap to {}",
            raw / bar,
            want / bar
        );
    }
}

#[test]
fn long_spans_snap_to_whole_bars_not_powers_of_two() {
    // Past ten bars a power-of-two step is a huge jump; whole bars are
    // the finer, more useful grid there.
    let bar = PPQ * 4.0;
    assert_eq!(memagic::snap_to_measures(bar * 12.4, bar) / bar, 12.0);
    assert_eq!(memagic::snap_to_measures(bar * 17.6, bar) / bar, 18.0);
}

#[test]
fn restricting_to_the_item_slides_rather_than_clips() {
    // Clipping would silently change the zoom the gesture just chose.
    let (start, len) = memagic::slide_into(-50.0, 100.0, 0.0, 1000.0);
    assert_eq!((start, len), (0.0, 100.0), "overhang left slides right");

    let (start, len) = memagic::slide_into(960.0, 100.0, 0.0, 1000.0);
    assert_eq!((start, len), (900.0, 100.0), "overhang right slides left");

    let (start, len) = memagic::slide_into(400.0, 100.0, 0.0, 1000.0);
    assert_eq!((start, len), (400.0, 100.0), "inside is left alone");
}

#[test]
fn a_span_longer_than_the_item_gives_up_and_shows_the_item() {
    let (start, len) = memagic::slide_into(-500.0, 5000.0, 0.0, 1000.0);
    assert_eq!((start, len), (0.0, 1000.0));
}

#[test]
fn a_single_pitch_still_gets_a_readable_number_of_rows() {
    // Fitted exactly, one pitch is one row filling the lane — unreadable
    // and impossible to aim at.
    let (lo, hi) = memagic::pad_to_min_rows(60.0, 60.0, 8.0);
    assert!(hi - lo >= 8.0 - 1.0, "got {lo}..{hi}");
    assert!(
        (((lo + hi) / 2.0) - 60.0).abs() < 1e-6,
        "and stays centred on the note"
    );
}

#[test]
fn an_empty_area_centres_on_middle_c() {
    // Not row zero, which is a pitch nobody plays.
    let doc = doc_with(Vec::new());
    let content = expression_editor_core::Content {
        t_start: 0.0,
        t_end: PPQ * 8.0,
        pitch_lo: 48.0,
        pitch_hi: 72.0,
    };
    let cfg = Config::default();
    let (lo, hi) = memagic::vertical_range(
        &doc,
        content,
        Vertical::FitNotes {
            scope: Scope::InItem,
        },
        Anchor { t: 0.0, row: None },
        (0.0, PPQ * 8.0),
        &cfg,
    )
    .expect("a range");
    assert!(
        lo < cfg.base_note && hi > cfg.base_note,
        "middle C should be in view: {lo}..{hi}"
    );
}

#[test]
fn each_region_means_something_different() {
    // The whole idea: one action, and where you invoked it decides.
    let all = [
        Region::NoteArea,
        Region::Piano,
        Region::Ruler,
        Region::CcLane,
    ];
    let modes: Vec<Modes> = all.iter().map(|r| r.modes()).collect();
    for (i, a) in modes.iter().enumerate() {
        for b in modes.iter().skip(i + 1) {
            assert_ne!(a, b, "two regions do the same thing: {all:?}");
        }
    }
    // And the two that exist to serve the lane below push opposite ways.
    assert!(matches!(
        Region::Ruler.modes().vertical,
        Vertical::Highest { .. }
    ));
    assert!(matches!(
        Region::CcLane.modes().vertical,
        Vertical::Lowest { .. }
    ));
}

#[test]
fn invoking_it_on_the_editor_moves_the_view() {
    let doc = doc_with(run(0.0, 40, PPQ / 2.0, PPQ / 2.0, 60));
    let mut ed = Editor::new(doc, Viewport::new(1200.0, 536.0));
    let before = (ed.camera.t0, ed.camera.units_per_px);

    assert!(ed.memagic(
        Region::NoteArea,
        Anchor {
            t: PPQ * 20.0,
            row: Some(60.0)
        }
    ));
    assert_ne!(
        (ed.camera.t0, ed.camera.units_per_px),
        before,
        "the view should have moved"
    );
}

#[test]
fn a_gesture_with_nothing_to_say_reports_it() {
    // `ScrollToAnchor` with no row under the pointer, and no horizontal
    // change: the host should be free to fall through to another
    // binding rather than assume it handled the key.
    let doc = doc_with(run(0.0, 8, PPQ, PPQ, 60));
    let mut ed = Editor::new(doc, Viewport::new(1200.0, 536.0));
    let moved = ed.memagic_with(
        Modes {
            horizontal: Horizontal::Keep,
            vertical: Vertical::ScrollToAnchor { scope: None },
        },
        Anchor { t: 0.0, row: None },
        &Config::default(),
    );
    assert!(!moved);
}

#[test]
fn the_anchor_lands_where_the_alignment_says() {
    let doc = doc_with(run(0.0, 40, PPQ / 2.0, PPQ / 2.0, 60));
    let mut ed = Editor::new(doc, Viewport::new(1200.0, 536.0));
    let anchor_t = PPQ * 20.0;

    // Centred is the default feel.
    let cfg = Config::default();
    ed.memagic_with(
        Modes {
            horizontal: Horizontal::Smart { restrict: false },
            vertical: Vertical::Keep,
        },
        Anchor {
            t: anchor_t,
            row: None,
        },
        &cfg,
    );
    let (t0, t1) = ed.camera.time_span(ed.viewport);
    let mid = (t0 + t1) / 2.0;
    assert!(
        (mid - anchor_t).abs() < (t1 - t0) * 0.1,
        "anchor should be near the middle: {mid} vs {anchor_t} in {t0}..{t1}"
    );
}
