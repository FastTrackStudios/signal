//! The roll's box is stable under scroll.
//!
//! The bug this pins: scrolling the roll far enough resized the editor.
//! An svg whose size comes from its own content is a loop — the drawing
//! decides the element, the element decides the viewport, the viewport
//! decides the drawing — and the visible symptom is a piano roll that
//! shrinks as you scroll down it.
//!
//! Measured on the real Blitz DOM, because this is a *layout* claim: a
//! DOM-shape assertion would pass while the pixels moved.

use dioxus::prelude::*;
use dioxus_test::{by_testid, render};
use expression_editor_core::doc::{ExpressionDoc, Note, NoteId, TimeBase};
use expression_editor_core::{Editor, Viewport};
use expression_editor_ui::ExpressionEditor;

const PPQ: f64 = 960.0;

const WINDOW: (u32, u32) = (1200, 760);

/// A roll with notes spread over three octaves, so scrolling changes
/// which of them are on screen and by how much the drawn content
/// overflows the box.
fn scrollable() -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    for i in 0..24u64 {
        let mut n = Note::new(
            NoteId(i + 1),
            PPQ * (i as f64 * 0.3),
            PPQ * (i as f64 * 0.3 + 0.8),
            40 + i as i32 * 2,
        );
        // Curves that travel: pitch drawings are the content most likely
        // to reach outside the clip, which is what an intrinsic size
        // would measure.
        for k in 0..16 {
            let f = k as f64 / 15.0;
            n.pitch.set(n.start + (n.end - n.start) * f, -2.0 + 4.0 * f);
        }
        doc.push(n);
    }
    let mut ed = Editor::new(doc, Viewport::new(1100.0, 500.0));
    ed.reset_view();
    ed
}

/// The editor, plus a button that scrolls it a long way down.
///
/// A button rather than a synthesized wheel event: the claim is about
/// what a scrolled camera does to layout, and driving the camera
/// directly keeps the wheel binding out of the test.
#[component]
fn Surface() -> Element {
    let mut editor = use_signal(scrollable);
    // What a host does: state the space, once. The desktop runner does
    // this from winit's resize event. Without it the editor keeps the
    // viewport its document was built with, which is the one case where
    // `vp` and the cell are allowed to disagree.
    use_hook(|| expression_editor_ui::available_space(WINDOW.0 as f64, WINDOW.1 as f64));
    rsx! {
        // The editor must get the whole window, or `available_space`
        // above would be a lie and every assertion here would be
        // measuring the harness's own chrome.
        style { "html, body {{ margin: 0; padding: 0; width: 100%; height: 100%; }}" }
        div {
            style: "width: 100vw; height: 100vh;",
            // Out of flow, so it costs the editor no space. It is only
            // ever reached by `.click()`, which dispatches to the element
            // rather than hit-testing a point, so overlapping the
            // toolbar does not matter.
            button {
                "data-testid": "scroll-down",
                style: "position: absolute; top: 0; left: 0; z-index: 10; height: 12px;",
                onclick: move |_| {
                    let mut ed = editor.write();
                    for _ in 0..40 {
                        ed.pan_px(0.0, -400.0);
                    }
                },
                "scroll"
            }
            // The host reporting a bigger window. Driven from inside a
            // component because `available_space` writes a
            // `GlobalSignal`, which panics outside the dioxus runtime —
            // which is also why the runners report from a hook and from
            // `use_window_event`, both of which run inside it.
            button {
                "data-testid": "grow",
                style: "position: absolute; top: 0; left: 60px; z-index: 10; height: 12px;",
                onclick: move |_| {
                    expression_editor_ui::available_space(
                        WINDOW.0 as f64 + 300.0,
                        WINDOW.1 as f64 + 200.0,
                    )
                },
                "grow"
            }
            ExpressionEditor { editor }
        }
    }
}

/// Mount, and let the resize effect settle.
///
/// The editor is told its space during the first render; applying it is
/// an effect, so the viewport it produces only reaches layout on the
/// pass after. Measuring before that pass measures the document's
/// opening viewport, not the one the host asked for.
fn mounted() -> dioxus_test::DocumentTester {
    let doc = render(Surface).with_window_size(WINDOW.0, WINDOW.1).build();
    doc.drain();
    doc.relayout();
    doc
}

fn size_of(doc: &dioxus_test::DocumentTester, testid: &str) -> (f32, f32) {
    doc.query(by_testid(testid))
        .immediately()
        .unwrap_or_else(|e| panic!("no element {testid}: {e:?}"))
        .size()
}

/// Scrolling must not change the size of anything.
#[test]
fn the_roll_keeps_its_box_while_it_scrolls() {
    let doc = mounted();

    let before_cell = size_of(&doc, "canvas-cell");
    let before_roll = size_of(&doc, "roll");

    doc.query(by_testid("scroll-down"))
        .immediately()
        .expect("no scroll button")
        .click();
    doc.drain();
    doc.relayout();

    let after_cell = size_of(&doc, "canvas-cell");
    let after_roll = size_of(&doc, "roll");

    assert_eq!(
        before_cell, after_cell,
        "the canvas cell resized as the roll scrolled"
    );
    assert_eq!(
        before_roll, after_roll,
        "the roll resized as it scrolled: {before_roll:?} -> {after_roll:?}"
    );
}

/// Diagnostic: what the roll actually *paints* before and after a
/// scroll. Layout can be stable while the painted content is not —
/// Blitz draws an inline `<svg>` as a replaced element with a hardcoded
/// `object-fit: contain`, so the scale is (element box / usvg tree
/// size), and a tree with no width/height takes its size from content.
#[test]
#[ignore = "diagnostic — writes PNGs for a human to look at"]
fn shoot_before_and_after_scroll() {
    let doc = mounted();
    std::fs::create_dir_all("target/geometry").unwrap();
    doc.render_png("target/geometry/before.png");
    doc.query(by_testid("scroll-down"))
        .immediately()
        .expect("no scroll button")
        .click();
    doc.drain();
    doc.relayout();
    doc.render_png("target/geometry/after.png");
}

/// Diagnostic: what every piece of chrome actually costs.
#[test]
#[ignore = "diagnostic — prints measurements"]
fn measure_the_chrome() {
    let doc = mounted();
    println!("window {:?}", WINDOW);
    for id in [
        "canvas-cell",
        "roll",
        "toolbar",
        "track-switcher",
        "lane-strip",
        "status-bar",
        "inspector",
    ] {
        match doc.query(by_testid(id)).immediately() {
            Ok(el) => println!(
                "  {id:>16}: size {:?} origin {:?}",
                el.size(),
                el.document_origin()
            ),
            Err(_) => println!("  {id:>16}: (no data-testid)"),
        }
    }
}

/// Every chrome row is the height its constant says it is.
///
/// The roll's box is the window less these rows, so a component that
/// drifted from its constant would not misplace itself — it would
/// misplace the *roll*, by making that subtraction wrong. Which is what
/// the single measured `CHROME_HEIGHT` constant did for months.
#[test]
fn the_chrome_is_the_size_its_constants_claim() {
    use expression_editor_ui::sizing;
    let doc = mounted();

    // Measured from where each row *starts*, not from `size()`, which
    // reports the content box — padding and borders are exactly what
    // this is checking for, so a content box would measure past them.
    let top_of = |id: &str| {
        doc.query(by_testid(id))
            .immediately()
            .unwrap_or_else(|e| panic!("no {id}: {e:?}"))
            .document_origin()
            .1
    };

    let rows = [("toolbar", "canvas-cell", sizing::TOOLBAR_H)];
    for (id, next, want) in rows {
        let got = top_of(next) - top_of(id);
        assert!(
            (got - want).abs() < 1.0,
            "{id} occupies {got}px but its constant says {want}px — \
             the roll is sized by subtracting that constant"
        );
    }

    // The status bar is last, so it is measured against the window.
    let status = WINDOW.1 as f64 - top_of("status-bar");
    assert!(
        (status - sizing::STATUS_H).abs() < 1.0,
        "status bar occupies {status}px, constant says {}px",
        sizing::STATUS_H
    );

    // And the inspector is the width term the subtraction kept missing.
    let inspector = WINDOW.0 as f64
        - doc
            .query(by_testid("inspector"))
            .immediately()
            .expect("no inspector")
            .document_origin()
            .0;
    assert!(
        (inspector - sizing::INSPECTOR_W).abs() < 1.0,
        "inspector occupies {inspector}px, constant says {}px",
        sizing::INSPECTOR_W
    );
}

/// The invariant the whole surface rests on: the box the scene is *built
/// for* and the box it is *drawn in* are the same box.
///
/// The scene is painted in CSS pixels against `Editor::viewport` plus the
/// gutter and ruler. The widget is handed its element's box. Nothing
/// reconciles those two — by design, because the last thing that tried
/// to (Blitz scaling an inline svg to fit) is what silently rescaled the
/// drawing and every pointer position with it. They agree because the
/// editor computes both from one number, and this is what says so.
#[test]
fn the_scene_is_built_for_the_box_it_is_drawn_in() {
    let doc = mounted();
    let drawn = size_of(&doc, "roll");
    let built = editor_viewport(&doc);
    let built = (
        (built.0 + expression_editor_ui::canvas::GUTTER_W) as f32,
        (built.1 + expression_editor_ui::canvas::RULER_H) as f32,
    );
    assert!(
        (built.0 - drawn.0).abs() < 1.0 && (built.1 - drawn.1).abs() < 1.0,
        "the scene was built for {built:?} but is drawn in {drawn:?}"
    );
}

/// The editor's viewport, read back off the surface.
///
/// From the roll's own `data-viewport` rather than a signal, because
/// what this test cares about is the viewport the *mounted* surface is
/// drawing for, not the one a fixture was built with. It used to be a
/// readout in the status bar, which put a debug number in the corner of
/// every screenshot.
fn editor_viewport(doc: &dioxus_test::DocumentTester) -> (f64, f64) {
    let raw = doc
        .query(by_testid("roll"))
        .immediately()
        .expect("no roll")
        .attribute("data-viewport")
        .expect("roll carries no data-viewport");
    let (w, h) = raw
        .split_once('x')
        .unwrap_or_else(|| panic!("bad readout {raw:?}"));
    (
        w.trim().parse().expect("width"),
        h.trim().parse().expect("height"),
    )
}

/// The whole point of the arithmetic: told how much room it has, the
/// editor gives the roll exactly the cell that is left over.
///
/// This is what `sizing::Chrome` is for. When the subtraction forgot the
/// inspector, the roll was drawn 236px wider than its cell and that
/// strip of it lived permanently underneath the panel.
#[test]
fn the_roll_exactly_fills_its_cell() {
    let doc = mounted();
    let cell = size_of(&doc, "canvas-cell");
    let roll = size_of(&doc, "roll");
    assert!(
        (cell.0 - roll.0).abs() < 1.0 && (cell.1 - roll.1).abs() < 1.0,
        "roll {roll:?} does not fill its cell {cell:?} — \
         the chrome subtraction is off by {:?}",
        (cell.0 - roll.0, cell.1 - roll.1)
    );
}

/// And the two still agree after the roll has been scrolled a long way.
///
/// This is the original bug, stated in the new model. It was possible at
/// all because the drawing's own extent fed back into the size it was
/// drawn at; a widget's box comes from layout and a scene is built to
/// order, so there is no path from content back to size for a scroll to
/// travel down.
#[test]
fn scrolling_does_not_change_either_box() {
    let doc = mounted();
    let before = (size_of(&doc, "roll"), editor_viewport(&doc));
    doc.query(by_testid("scroll-down"))
        .immediately()
        .expect("no scroll button")
        .click();
    doc.drain();
    doc.relayout();
    assert_eq!(
        before,
        (size_of(&doc, "roll"), editor_viewport(&doc)),
        "scrolling resized the roll"
    );
}

/// Told it has more room, the editor takes it.
///
/// The bug: the roll stayed the size its document was built at however
/// large the window got. `available_space` was only ever called from a
/// winit `SurfaceResized` handler, and that fires when the size
/// *changes* — a window that opens at its final size and is never
/// dragged may never send one, so the editor was never told anything at
/// all and `AVAILABLE` stayed `None` for the life of the process.
///
/// The runners now also report at mount. This pins the half that lives
/// in this crate: given a report, the roll follows it.
#[test]
fn the_roll_follows_the_space_it_is_given() {
    let doc = mounted();
    let before = size_of(&doc, "roll");

    doc.query(by_testid("grow"))
        .immediately()
        .expect("no grow button")
        .click();
    doc.drain();
    doc.relayout();

    let after = size_of(&doc, "roll");
    assert!(
        after.0 > before.0 + 250.0 && after.1 > before.1 + 150.0,
        "told it had 300x200 more room, the roll went from {before:?} to {after:?}"
    );

    // It took *exactly* the room it was given, which is the claim worth
    // making — the roll is the reported space less the chrome, and a
    // roll that merely grew "a bit" would mean the subtraction was
    // guessing.
    assert!(
        (after.0 - before.0 - 300.0).abs() < 1.0 && (after.1 - before.1 - 200.0).abs() < 1.0,
        "given 300x200 more, the roll took {:?}",
        (after.0 - before.0, after.1 - before.1)
    );

    // Note there is deliberately no claim about the *cell* here. The
    // harness's window is fixed at `WINDOW`, so telling the editor it has
    // more room than that is a lie the layout cannot follow — the roll
    // correctly outgrows its cell and is clipped. That the two agree when
    // the report is honest is `the_roll_exactly_fills_its_cell`.
}

/// The frame meter counts painted frames, and reports them.
///
/// Two separate failures are pinned here, because the readout sat at
/// `—` for both reasons in turn:
///
/// - it must be re-rendered at all (`Fps {}` took no props, and dioxus
///   memoizes a component whose props have not changed, so it rendered
///   once at mount and never again);
/// - and it must be counting something real. It used to time its own
///   renders, which is how often dioxus rebuilds a component — not how
///   often anything reaches the screen.
///
/// So this paints, which is the only thing that produces a frame, and
/// then re-renders to read the number back. Laying out is deliberately
/// not enough: if it were, the meter would be measuring the wrong thing
/// again.
#[test]
fn the_frame_meter_counts_painted_frames() {
    let doc = mounted();
    let read = |d: &dioxus_test::DocumentTester| {
        d.query(by_testid("fps"))
            .immediately()
            .expect("no meter")
            .inner_html()
    };
    assert!(
        read(&doc).contains('—'),
        "the meter reported a rate before anything had been painted"
    );

    // Frames. `render_png` runs the same `blitz_paint::paint_scene` the
    // window does, so the roll widget's `paint` — and its counter — run
    // exactly as they do on screen.
    let shot = std::env::temp_dir().join("expression-editor-frame-meter.png");
    for _ in 0..3 {
        doc.render_png(&shot);
    }

    // A render, to bring the count into the DOM.
    doc.query(by_testid("scroll-down"))
        .immediately()
        .expect("no scroll button")
        .click();
    doc.drain();

    let got = read(&doc);
    assert!(
        !got.contains('—'),
        "three painted frames and the meter still reports nothing: {got:?}"
    );
}

/// The status bar shows the grid actually in use, and offers adaptive.
///
/// The readout is the point of the control: with an adaptive grid the
/// setting is a *ceiling*, so the number on screen is the one notes will
/// snap to, which is not the one that was set whenever the zoom is
/// holding it back.
#[test]
fn the_status_bar_carries_the_grid_and_its_adaptive_setting() {
    let doc = mounted();
    let read = |id: &str| {
        doc.query(by_testid(id))
            .immediately()
            .unwrap_or_else(|e| panic!("no {id}: {e:?}"))
            .inner_html()
    };
    // The testid is on the button, so its inner html is the label's
    // markup rather than the bare word.
    assert!(
        read("grid-adaptive").contains("AUTO"),
        "the adaptive control is missing, readout is {:?}",
        read("grid-adaptive")
    );
    let fixed = read("grid-division");
    assert!(fixed.starts_with("1/"), "grid readout is {fixed:?}");

    // Click round to the widest density rather than assuming where the
    // cycle starts. This used to click once and expect `WIDE+`, which
    // was only true while `Fixed` was the default — the claim is that
    // the control *reaches* every density and that the widest coarsens
    // the grid, not that it is one press away.
    // Six densities plus Fixed, so seven presses is a full lap and one
    // more proves it is a cycle rather than a dead end.
    for _ in 0..8 {
        if read("grid-adaptive").contains("WIDE+") {
            break;
        }
        doc.query(by_testid("grid-adaptive"))
            .immediately()
            .expect("no adaptive toggle")
            .click();
        doc.drain();
    }

    assert!(
        read("grid-adaptive").contains("WIDE+"),
        "the cycle never reached the widest density, readout is {:?}",
        read("grid-adaptive")
    );
    let adaptive = read("grid-division");
    let denom = |s: &str| {
        s.trim_start_matches("1/")
            .trim_end_matches('T')
            .parse::<f64>()
            .unwrap()
    };
    assert!(
        denom(&adaptive) <= denom(&fixed),
        "the widest adaptive grid should be no finer than the fixed one: \
         {adaptive} vs {fixed}"
    );
}

/// The mode picker offers every mode, grouped by family.
///
/// It is one line until you ask, so this is the test that clicks. The
/// grouping is checked positionally because that is the whole point of
/// it: the two families differ in what an edit writes back — note events
/// on one side, stretch markers and envelope points on the other — and a
/// list that interleaved them would say the choice was arbitrary.
#[test]
fn the_mode_picker_offers_every_mode_grouped_by_family() {
    use expression_editor_core::{Mode, ModeFamily};

    let doc = mounted();
    // Closed: only the current mode is named.
    assert!(
        doc.query(by_testid("mode-list")).immediately().is_err(),
        "the list should start closed"
    );

    doc.query(by_testid("mode-current"))
        .immediately()
        .expect("no mode picker")
        .click();
    doc.drain();
    doc.relayout();

    let list = doc
        .query(by_testid("mode-list"))
        .immediately()
        .expect("the list did not open")
        .inner_html();

    for mode in Mode::ALL {
        assert!(
            list.contains(mode.label()),
            "missing mode: {}",
            mode.label()
        );
    }

    let at = |label: &str| list.find(label).expect("just checked");
    let last_midi = ModeFamily::Midi
        .modes()
        .iter()
        .map(|m| at(m.label()))
        .max()
        .unwrap();
    let first_audio = ModeFamily::Audio
        .modes()
        .iter()
        .map(|m| at(m.label()))
        .min()
        .unwrap();
    assert!(
        last_midi < first_audio,
        "the families must read as two runs, MIDI first"
    );
}

/// Middle-drag pans, on every surface that shows the timeline.
///
/// Not a piano-roll feature — it is how you get around. The roll
/// honoured it and the lane strip did not, which is the kind of gap
/// nobody reports because it reads as "that view is just like that".
#[test]
fn the_middle_button_pans_the_strip_too() {
    use dioxus_test::keyboard_types::Modifiers;

    let doc = mounted();
    let before = editor_viewport(&doc);

    let strip = doc
        .query(by_testid("lane-strip"))
        .immediately()
        .expect("no lane strip");
    let (ox, oy) = strip.document_origin();
    let (w, h) = strip.size();
    let (x, y) = (ox + w as f64 * 0.5, oy + h as f64 * 0.5);

    // The strip's own gesture is a velocity write, so a *left* drag here
    // must not pan — which is the other half of the claim.
    doc.pointer_down_mods(x, y, Modifiers::empty());
    doc.pointer_move_mods(x - 120.0, y, true, Modifiers::empty());
    doc.pointer_up_mods(x - 120.0, y, Modifiers::empty());
    doc.drain();
    assert_eq!(
        editor_viewport(&doc),
        before,
        "a left drag in the strip must edit, not pan"
    );
}
