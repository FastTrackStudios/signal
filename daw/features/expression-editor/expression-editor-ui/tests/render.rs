//! SSR structure tests — the surface renders, and what it draws
//! follows the document.
//!
//! Plain `VirtualDom` + `dioxus-ssr`, no GPU and no browser, so this
//! runs anywhere. Event dispatch is covered by the core's own suite;
//! this asserts the view actually reaches the screen.

use dioxus::prelude::*;
use expression_editor_core::doc::{Dimension, ExpressionDoc, Note, NoteId, TimeBase};
use expression_editor_core::{Editor, Viewport};
use expression_editor_ui::ExpressionEditor;

const PPQ: f64 = 960.0;

fn demo_editor(microtonal: bool, with_zones: bool) -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    for i in 0..4 {
        let mut n = Note::new(
            NoteId(i + 1),
            PPQ * i as f64,
            PPQ * (i as f64 + 0.9),
            60 + (i as i32) * 2,
        );
        n.channel = Some(2 + i as u8);
        // A scoop into the note plus a little vibrato.
        for k in 0..24 {
            let f = k as f64 / 23.0;
            let t = n.start + (n.end - n.start) * f;
            let scoop = -1.5 * (1.0 - f).powi(3);
            let vib = 0.15 * (f * 18.0).sin() * f;
            n.pitch.set(t, scoop + vib);
        }
        if with_zones && i == 1 {
            n.add_split(n.start + (n.end - n.start) * 0.5);
        }
        doc.push(n);
    }
    doc.mark_ambiguity();

    let mut ed = Editor::new(doc, Viewport::new(900.0, 480.0));
    // MPE: the mode that has every control on screen, so the layout
    // tests see the full bar.
    ed.set_mode(expression_editor_core::Mode::Mpe);
    ed.selection.set_single(NoteId(2));
    if microtonal {
        ed.tuning.temperament = expression_editor_core::tuning::RAST.clone();
    }
    ed
}

fn render(ed: Editor) -> String {
    #[component]
    fn Harness(seed: Editor) -> Element {
        let editor = use_signal(|| seed.clone());
        rsx! { ExpressionEditor { editor } }
    }
    let mut dom = VirtualDom::new_with_props(Harness, HarnessProps { seed: ed });
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[test]
fn the_editor_renders_its_toolbar_canvas_and_status_bar() {
    let html = render(demo_editor(false, false));
    assert!(html.contains("<svg"), "the canvas must render");
    // The top bar carries modes; the status bar carries settings.
    assert!(html.contains("Undo"), "history controls");
    assert!(html.contains("Reset view (V)"), "view controls");
    assert!(html.contains("12TET"), "tuning moved to the status bar");
    assert!(html.contains("Chord"), "the chord box renders");
    // Dimension controls.
    for dimension in Dimension::ALL {
        let label = expression_editor_ui::theme::lane_label(dimension);
        assert!(html.contains(label), "missing dimension control: {label}");
    }
    assert!(
        html.contains("1/16"),
        "the grid readout, now in the status bar"
    );
}

#[test]
fn the_top_bar_follows_the_mode() {
    use expression_editor_core::Mode;

    let mut mpe = demo_editor(false, false);
    mpe.set_mode(Mode::Mpe);
    let mpe_html = render(mpe);
    assert!(mpe_html.contains("Spread ch"), "MPE shows channel controls");
    assert!(mpe_html.contains("Pressure"), "and the expression lanes");

    let mut midi = demo_editor(false, false);
    midi.set_mode(Mode::Midi);
    let midi_html = render(midi);
    // Plain MIDI cannot carry per-note pressure, so offering the
    // control would promise an edit the format drops.
    assert!(
        !midi_html.contains("Spread ch"),
        "plain MIDI must not show MPE channel controls"
    );
    assert!(
        !midi_html.contains(">Pressure<"),
        "nor the per-note expression lanes"
    );
    // The mode switcher names the mode you are in.
    //
    // Only that one: the picker is a line in the panel that opens a list
    // when asked, so the other six are not in the markup until it does.
    // That the list offers all of them, grouped by family, is
    // `tests/geometry.rs` — it needs a click, which SSR cannot give.
    assert!(
        midi_html.contains(Mode::Midi.label()),
        "the picker must name the current mode"
    );
}

/// Every note reaches the drawing.
///
/// Asserted against the scene rather than the markup, because the roll
/// is painted now: there is no `<rect fill=…>` in the DOM to count, and
/// counting one would have been testing the emission rather than the
/// picture anyway. A scene is a plain list of draw commands, so this
/// asks the real question — does the number of things drawn follow the
/// number of notes?
#[test]
fn every_note_reaches_the_canvas() {
    use expression_editor_ui::paint;

    let ed = demo_editor(false, false);
    let notes = ed.doc.notes.len();
    let mut labels = expression_editor_ui::text::Labeller::new();

    let with_notes = paint::roll_scene(&ed, 900.0, 480.0, &paint::Overlay::default(), &mut labels)
        .commands
        .len();

    // The same view with nothing in it, so the comparison is against
    // this roll's own chrome rather than a guessed constant: rows, grid,
    // keyboard and ruler are drawn either way.
    let mut empty = demo_editor(false, false);
    empty.doc.notes.clear();
    empty.selection.notes.clear();
    let without = paint::roll_scene(
        &empty,
        900.0,
        480.0,
        &paint::Overlay::default(),
        &mut labels,
    )
    .commands
    .len();

    assert!(
        with_notes >= without + notes,
        "{notes} notes added only {} draw commands — some did not reach the scene",
        with_notes.saturating_sub(without)
    );
}

#[test]
fn a_non_equal_tuning_is_visibly_flagged() {
    // The badge, not the toolbar's preset list — every preset name
    // appears in the dropdown either way.
    const BADGE: &str = "background: #422006";
    let plain = render(demo_editor(false, false));
    assert!(!plain.contains(BADGE), "no badge in 12-TET");

    let tuned = render(demo_editor(true, false));
    assert!(
        tuned.contains(BADGE),
        "a non-equal tuning must always be called out"
    );
    assert!(
        tuned.contains(expression_editor_ui::theme::GOLD),
        "microtonal centers draw in gold"
    );
}

/// A Q split adds structure to the drawing, and says so in the panel.
///
/// Asserted against the scene rather than the markup: the roll is
/// painted, so its zone dividers are draw commands and there is no
/// coloured element in the DOM left to count. The panel half is what
/// survives in the html, and between them they cover the claim — the
/// split is visible, and its count is legible.
#[test]
fn zone_structure_is_drawn_and_reported() {
    use expression_editor_ui::paint;

    let mut labels = expression_editor_ui::text::Labeller::new();
    let mut commands = |ed| {
        paint::roll_scene(&ed, 900.0, 480.0, &paint::Overlay::default(), &mut labels)
            .commands
            .len()
    };
    let plain = commands(demo_editor(false, false));
    let zoned = commands(demo_editor(false, true));
    assert!(
        zoned > plain,
        "a Q split must add structure to the drawing: {zoned} commands vs {plain}"
    );

    let html = render(demo_editor(false, true));
    assert!(html.contains("Q zones"), "the panel must report the split");
}

/// The selected note's analysis reaches the screen — in the panel.
///
/// It used to be read off the chord row, which repeated the inspector
/// and cost the roll 30px for the privilege. The row is gone; these are
/// the inspector's own labels, which is now the single place any of it
/// is said.
#[test]
fn the_selected_notes_analysis_is_reported() {
    let html = render(demo_editor(false, false));
    assert!(html.contains("1 selected"), "the status bar's count");
    assert!(html.contains("Vibrato"), "the vibrato readout");
    assert!(html.contains("Drift"), "the drift readout");
    assert!(html.contains("Robot"), "the flatten button");
    assert!(html.contains("Channel"), "the MPE member channel");
}

#[test]
fn an_empty_document_still_renders() {
    let doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 4.0);
    let html = render(Editor::new(doc, Viewport::new(900.0, 480.0)));
    assert!(html.contains("<svg"), "no notes must not mean no canvas");
    assert!(html.contains("0 selected"));
}
