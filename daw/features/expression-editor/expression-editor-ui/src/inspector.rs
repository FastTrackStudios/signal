//! The right-hand inspector.
//!
//! Everything about *the current selection* that does not need to be
//! one click away. The selection row above the roll keeps only the
//! chord and the two blend controls — the things you reach for while
//! shaping a phrase — and the rest lives here, where there is room to
//! label it.
//!
//! Also the home for the controller lanes, because pinning a CC and
//! choosing which one to edit is a per-session decision that wants a
//! list, not a cycling button.

use dioxus::prelude::*;
use expression_editor_core::cc::{CC_COLORS, standard_name};
use expression_editor_core::doc::NoteId;
use expression_editor_core::flam::FlamStep;
use expression_editor_core::rows::Articulation;
use expression_editor_core::rows::Hand;
use expression_editor_core::{Edit, Editor, RowSpace, blob, chord, tuning};

use crate::theme;
use crate::widgets::{CenterSlider, Slider};

fn section(title: &str) -> Element {
    rsx! {
        div {
            style: "font-size: 9px; letter-spacing: 0.09em; text-transform: uppercase; \
                    color: {theme::TEXT_DIM}; padding: 10px 10px 4px;",
            "{title}"
        }
    }
}

fn row(label: &str, value: String) -> Element {
    rsx! {
        div {
            style: "display: flex; justify-content: space-between; align-items: center; \
                    padding: 2px 10px; font-size: 10px; color: {theme::TEXT_DIM};",
            span { "{label}" }
            span {
                style: "font-family: ui-monospace, monospace; color: {theme::TEXT};",
                "{value}"
            }
        }
    }
}

#[component]
pub fn Inspector(
    editor: Signal<Editor>,
    open: Signal<bool>,
    /// Anything the host wants at the top of the panel.
    ///
    /// A slot rather than a fixed control, because what goes here is the
    /// host's business: the standalone runner puts its file chooser
    /// there, and a DAW panel would put nothing. The alternative was a
    /// bar of its own across the top of the window, which is a band of
    /// vertical space spent on something the roll wants more.
    #[props(default)]
    panel_top: Option<Element>,
) -> Element {
    let mut editor = editor;
    let mut open = open;

    if !open() {
        // A tab, not a hidden panel: the way back has to be visible.
        return rsx! {
            div {
                "data-testid": "inspector",
                style: "flex: 0 0 auto; width: {crate::sizing::INSPECTOR_TAB_W}px; \
                        display: flex; align-items: center; \
                        justify-content: center; background: {theme::PANEL}; \
                        border-left: 1px solid {theme::PANEL_BORDER}; cursor: pointer; \
                        color: {theme::TEXT_DIM}; font-size: 10px;",
                onclick: move |_| open.set(true),
                "‹"
            }
        };
    }

    let ed = editor.read();
    let sel: Vec<NoteId> = ed.selection.notes.clone();
    let single = sel.first().copied();
    let note = single.and_then(|id| ed.doc.note(id)).cloned();
    let space = ed.row_space.clone();
    let ups = ed.doc.time_base.units_per_second(ed.bpm);
    let chord_name = ed.current_chord().map(|c| chord::name(&c));
    let lanes = ed.doc.cc.lanes.clone();
    let cc_edit = ed.cc_edit;
    let is_strings = matches!(space, RowSpace::Strings(_));
    // Derived, not stored: the fret is the pitch less the string's open
    // pitch, so it cannot disagree with the note it labels.
    let is_bands = matches!(space, RowSpace::Bands(_));
    let bands = match &space {
        RowSpace::Bands(b) => b.clone(),
        _ => Default::default(),
    };
    let editing_lyric = ed.editing_lyric;
    let lyric = note
        .as_ref()
        .and_then(|n| n.text.clone())
        .unwrap_or_default();
    // The field's own text while it is being typed. Held here rather
    // than written through on each keystroke so the input is not
    // fighting a trimmed value, and so one syllable is one undo step.
    let mut draft_lyric = use_signal(String::new);
    use_effect(move || {
        let current = lyric.clone();
        if editor.read().editing_lyric.is_none() {
            draft_lyric.set(current);
        }
    });
    let selected_id = single;
    let fret_now: Option<u8> = match (&space, &note) {
        (RowSpace::Strings(t), Some(n)) => {
            expression_editor_core::rows::fret_of(n, t).and_then(|f| u8::try_from(f).ok())
        }
        _ => None,
    };
    let fret_label: String = match (&space, &note) {
        (RowSpace::Strings(t), Some(n)) => expression_editor_core::rows::fret_of(n, t)
            .map(|f| f.to_string())
            .unwrap_or_else(|| "—".into()),
        _ => "—".into(),
    };
    let is_drums = matches!(space, RowSpace::Drums(_));
    let color_by_string = ed.color_by_string;
    // What `f` will do to the selected hit, so the button can say it
    // rather than the user finding out by pressing.
    let flam_step = ed.selection.notes.first().and_then(|id| ed.flam_step(*id));
    let hand = ed
        .selection
        .notes
        .first()
        .and_then(|id| ed.hand_of_note(*id));
    drop(ed);

    let decomposition = note
        .as_ref()
        .map(|n| blob::decompose(&n.pitch, n.start, n.end, 64, ups, 0.0));

    rsx! {
        div {
            "data-testid": "inspector",
            // `border-box`: the 1px left border is part of the width the
            // roll is sized against, not an extra pixel on top of it.
            style: "flex: 0 0 auto; box-sizing: border-box; \
                    width: {crate::sizing::INSPECTOR_W}px; \
                    display: flex; flex-direction: column; \
                    background: {theme::PANEL}; overflow-y: auto; \
                    border-left: 1px solid {theme::PANEL_BORDER}; \
                    color: {theme::TEXT}; font-family: system-ui, sans-serif;",

            div {
                style: "display: flex; align-items: center; justify-content: space-between; \
                        padding: 7px 10px; border-bottom: 1px solid {theme::PANEL_BORDER};",
                span {
                    style: "font-size: 10px; letter-spacing: 0.08em; \
                            text-transform: uppercase; color: {theme::TEXT_DIM};",
                    "Inspector"
                }
                button {
                    style: "background: none; border: none; color: {theme::TEXT_DIM}; \
                            cursor: pointer; font-size: 11px;",
                    onclick: move |_| open.set(false),
                    "›"
                }
            }

            // ── the host's own controls ──────────────────────────────
            if let Some(top) = panel_top.clone() {
                {top}
            }

            // ── which surface ────────────────────────────────────────
            //
            // First, because it is the widest-scoped thing in the panel:
            // everything below is about the selection, this is about the
            // whole editor. It was a row across the top of the window
            // until the roll needed the height more than the modes
            // needed the prominence.
            crate::mode_picker::ModePicker { editor }

            // ── selection ────────────────────────────────────────────
            if let Some(n) = note.clone() {
                {section("Note")}
                {row("Pitch", space.row_label(n.row))}
                if let Some(label) = space.note_label(&n) {
                    {row("Label", label)}
                }
                {row("Sounding", tuning::note_name(space.pitch_of(&n)))}
                if let Some(ch) = n.channel {
                    {row("Channel", ch.to_string())}
                }
                if sel.len() > 1 {
                    {row("Selected", format!("{} notes", sel.len()))}
                }
                if let Some(name) = chord_name.clone() {
                    {row("Chord", name)}
                }

                div { style: "padding: 4px 10px 8px;",
                    Slider {
                        label: "Vel".to_string(),
                        value: n.velocity,
                        min: 0.0,
                        max: 1.0,
                        width: 130.0,
                        readout: format!("{:.0}", n.velocity * 127.0),
                        on_change: move |v: f64| {
                            let notes = editor.read().selection.notes.clone();
                            editor.write().apply(&Edit::SetVelocity { notes, velocity: v });
                        },
                    }
                    Slider {
                        label: "Rel".to_string(),
                        value: n.off_velocity,
                        min: 0.0,
                        max: 1.0,
                        width: 130.0,
                        readout: format!("{:.0}", n.off_velocity * 127.0),
                        on_change: move |v: f64| {
                            let notes = editor.read().selection.notes.clone();
                            editor.write().apply(&Edit::SetOffVelocity { notes, velocity: v });
                        },
                    }
                }

                // ── pitch shape ──────────────────────────────────────
                {section("Pitch shape")}
                if let Some(d) = decomposition {
                    {row("Vibrato", format!("{:.0}¢", d.modulation_depth() * 100.0))}
                    {row("Drift", format!("{:+.0}¢", d.drift_extent() * 100.0))}
                }
                div { style: "padding: 4px 10px 8px;",
                    Blend { editor, label: "Drift".to_string(), drift: true }
                    Blend { editor, label: "Vibrato".to_string(), drift: false }
                    button {
                        style: "margin-top: 6px; width: 100%; height: 22px; \
                                background: {theme::CONTROL}; border: 1px solid {theme::PANEL_BORDER}; \
                                border-radius: 5px; color: {theme::TEXT}; font-size: 10px; \
                                cursor: pointer;",
                        title: "Flatten drift and vibrato",
                        onclick: move |_| {
                            let Some(id) = single else { return };
                            let (t0, t1) = {
                                let ed = editor.read();
                                let Some(n) = ed.doc.note(id) else { return };
                                (n.start, n.end)
                            };
                            editor.write().apply(&Edit::ReblendPitch {
                                note: id,
                                t0,
                                t1,
                                drift_amount: 0.0,
                                modulation_amount: 0.0,
                            });
                        },
                        "Robot — flatten both"
                    }
                }

                // ── lyric ────────────────────────────────────────────
                // A syllable *is* the note's identity in a vocal
                // editor, so this is the one field that has to be
                // typable rather than picked.
                if selected_id.is_some() {
                    {section("Lyric")}
                    div {
                        style: "padding: 0 10px 8px;",
                        input {
                            style: format!(
                                "width: 100%; height: 22px; font-size: 11px; \
                                 border-radius: 4px; padding: 0 6px; \
                                 border: 1px solid {}; background: {}; color: {};",
                                if editing_lyric.is_some() { theme::ACCENT } else { theme::PANEL_BORDER },
                                theme::SURFACE_INSET,
                                theme::TEXT,
                            ),
                            value: "{draft_lyric}",
                            placeholder: "syllable",
                            onfocus: move |_| {
                                if let Some(id) = selected_id {
                                    editor.write().editing_lyric = Some(id);
                                }
                            },
                            // Committed on blur and on Enter, not on
                            // every keystroke. Per-keystroke looked
                            // friendlier and was two bugs: each
                            // character pushed an undo snapshot, and
                            // with a history limit of ten an
                            // eleven-letter syllable evicted every
                            // other undo step in the session; and
                            // committing a trimmed value into a
                            // controlled input ate the space, so
                            // multi-word text could not be typed.
                            onkeydown: move |e| {
                                if e.key() == Key::Enter
                                    && let Some(id) = selected_id
                                {
                                    let text = draft_lyric.read().clone();
                                    editor.write().set_lyric(id, &text);
                                }
                            },
                            oninput: move |e| draft_lyric.set(e.value()),
                            onblur: move |_| {
                                if let Some(id) = selected_id {
                                    let text = draft_lyric.read().clone();
                                    editor.write().set_lyric(id, &text);
                                }
                            },
                        }
                    }
                }

                // ── band splits ──────────────────────────────────────
                // Moving a split re-sorts every slice from the centroid
                // it already carries — lossless, and no analysis reruns.
                if is_bands && !bands.splits.is_empty() {
                    {section("Band splits")}
                    for (i, hz) in bands.splits.iter().copied().enumerate() {
                        div {
                            style: "display: flex; align-items: center; gap: 4px; \
                                    padding: 0 10px 6px;",
                            span {
                                "data-testid": "split-{i}-hz",
                                style: format!("font-size: 10px; width: 54px; color: {};", theme::TEXT_DIM),
                                "{hz:.0} Hz"
                            }
                            for (label, factor, dir) in [("−", 0.917f64, "down"), ("+", 1.09, "up")] {
                                button {
                                    // Addressed by row, because the bug
                                    // this guards is the row and the
                                    // split disagreeing after a move.
                                    "data-testid": "split-{i}-{dir}",
                                    style: "flex: 1; height: 20px; font-size: 10px; \
                                            border-radius: 4px; cursor: pointer;",
                                    // Proportional, not additive: pitch
                                    // is logarithmic, so a fixed 100 Hz
                                    // step is a semitone up top and an
                                    // octave down the bottom.
                                    onclick: move |_| {
                                        editor.write().move_band_split(i, (hz * factor).clamp(20.0, 20_000.0));
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                }

                // ── guitar colour ────────────────────────────────────
                if is_strings {
                    {section("Colour")}
                    div {
                        style: "padding: 0 10px 8px;",
                        button {
                            style: format!(
                                "height: 22px; width: 100%; font-size: 10px; \
                                 border-radius: 4px; cursor: pointer; \
                                 border: 1px solid {}; background: {}; color: {};",
                                if color_by_string { theme::ACCENT } else { theme::PANEL_BORDER },
                                if color_by_string { theme::CONTROL_ACTIVE } else { theme::SURFACE_INSET },
                                theme::TEXT,
                            ),
                            title: "On: colour shows which string a run is on. \
                                    Off: colour shows pitch class, so you read harmony.",
                            onclick: move |_| {
                                let now = editor.read().color_by_string;
                                editor.write().color_by_string = !now;
                            },
                            if color_by_string { "By string" } else { "By pitch class" }
                        }
                    }
                }

                // ── sticking ─────────────────────────────────────────
                if is_drums {
                    {section("Hand")}
                    div {
                        style: "display: flex; gap: 4px; padding: 0 10px 8px;",
                        for h in [Hand::Left, Hand::Right] {
                            button {
                                key: "hand{h:?}",
                                style: format!(
                                    "flex: 1; height: 22px; font-size: 10px; \
                                     border-radius: 4px; \
                                     border: 1px solid {}; background: {}; color: {}; \
                                     cursor: {};",
                                    if hand == Some(h) { theme::ACCENT } else { theme::PANEL_BORDER },
                                    if hand == Some(h) { theme::CONTROL_ACTIVE } else { theme::SURFACE_INSET },
                                    if hand.is_some() { theme::TEXT } else { theme::TEXT_DIM },
                                    if hand.is_some() { "pointer" } else { "default" },
                                ),
                                disabled: hand.is_none(),
                                title: if hand.is_some() {
                                    "Which hand plays this hit — moves it to that row"
                                } else {
                                    "This piece is played with one hand"
                                },
                                onclick: move |_| {
                                    editor.write().set_hand_of_selection(h);
                                },
                                {if h == Hand::Left { "L" } else { "R" }}
                            }
                        }
                    }

                    // ── flam ─────────────────────────────────────────
                    {section("Flam")}
                    div {
                        style: "padding: 0 10px 8px;",
                        button {
                            style: format!(
                                "height: 22px; width: 100%; font-size: 10px; \
                                 border-radius: 4px; \
                                 border: 1px solid {}; background: {}; color: {}; \
                                 cursor: {};",
                                theme::PANEL_BORDER,
                                theme::SURFACE_INSET,
                                if flam_step.is_some() { theme::TEXT } else { theme::TEXT_DIM },
                                if flam_step.is_some() { "pointer" } else { "default" },
                            ),
                            disabled: flam_step.is_none(),
                            title: "F — cycles none, before, after, none",
                            onclick: move |_| {
                                editor.write().flam_selection();
                            },
                            {match flam_step {
                                Some(FlamStep::Add(_)) => "Add flam  ·  F",
                                Some(FlamStep::Move { .. }) => "Move after  ·  F",
                                Some(FlamStep::Remove(_)) => "Remove flam  ·  F",
                                // A hi-hat has no other hand to flam
                                // with, and saying so beats a dead
                                // button with no explanation.
                                None => "One-handed piece",
                            }}
                        }
                    }
                }

                // ── technique ────────────────────────────────────────
                if is_strings {
                    {section("Technique")}
                    {row("String", n.string.map(|s| (s + 1).to_string()).unwrap_or_else(|| "—".into()))}
                    {row("Fret", fret_label.clone())}
                    // Setting a fret *transposes* — the string is kept
                    // and the pitch moves, which is what sliding the
                    // shape up the neck does.
                    div {
                        style: "display: flex; gap: 4px; padding: 0 10px 8px;",
                        for (label, delta) in [("−12", -12i32), ("−1", -1), ("+1", 1), ("+12", 12)] {
                            button {
                                style: "flex: 1; height: 20px; font-size: 10px; \
                                        border-radius: 4px; cursor: pointer;",
                                disabled: fret_now.is_none(),
                                onclick: move |_| {
                                    let Some(f) = fret_now else { return };
                                    let next = (f as i32 + delta).clamp(0, 24) as u8;
                                    editor.write().set_fret_of_selection(next);
                                },
                                "{label}"
                            }
                        }
                    }
                    {row("Legato", if n.legato { "yes".into() } else { "no".into() })}
                    div {
                        style: "display: flex; flex-wrap: wrap; gap: 3px; padding: 4px 10px 8px;",
                        for a in Articulation::ALL {
                            button {
                                key: "art{a:?}",
                                style: format!(
                                    "height: 20px; padding: 0 6px; font-size: 9px; \
                                     border-radius: 4px; cursor: pointer; \
                                     border: 1px solid {}; background: {}; color: {};",
                                    if n.articulation == Some(a) { theme::ACCENT } else { theme::PANEL_BORDER },
                                    if n.articulation == Some(a) { theme::CONTROL_ACTIVE } else { theme::SURFACE_INSET },
                                    theme::TEXT,
                                ),
                                title: "{a.label()}",
                                onclick: move |_| {
                                    let notes = editor.read().selection.notes.clone();
                                    let cur = editor
                                        .read()
                                        .doc
                                        .note(notes[0])
                                        .and_then(|n| n.articulation);
                                    // Clicking the active technique
                                    // clears it — otherwise there is no
                                    // way back to plain sustain.
                                    let next = if cur == Some(a) { None } else { Some(a) };
                                    editor.write().apply(&Edit::SetArticulation {
                                        notes,
                                        articulation: next,
                                    });
                                },
                                "{a.label()}"
                            }
                        }
                    }
                }

                // ── zones ────────────────────────────────────────────
                if n.zone_count() > 1 {
                    {section("Q zones")}
                    {row("Count", n.zone_count().to_string())}
                    {row(
                        "Target",
                        match n.target {
                            expression_editor_core::Target::WholeNote => "whole note".into(),
                            expression_editor_core::Target::Zone(i) => format!("zone {}", i + 1),
                        },
                    )}
                }
            } else {
                div {
                    style: "padding: 16px 10px; font-size: 11px; color: {theme::TEXT_DIM}; \
                            text-align: center; line-height: 1.7;",
                    "Nothing selected"
                    br {}
                    span { style: "font-size: 10px;", "Click a note to inspect it" }
                }
            }

            // ── controller lanes ─────────────────────────────────────
            {section("Controller lanes")}
            div {
                style: "display: flex; flex-direction: column; gap: 3px; padding: 0 10px 8px;",
                for dimension in lanes.iter() {
                    div {
                        key: "cc{dimension.number}",
                        style: format!(
                            "display: flex; align-items: center; gap: 6px; \
                             border: 1px solid {}; border-radius: 5px; padding: 4px 6px; \
                             background: {};",
                            if cc_edit == Some(dimension.number) { theme::ACCENT } else { theme::PANEL_BORDER },
                            if cc_edit == Some(dimension.number) { theme::CONTROL_SELECTED } else { theme::SURFACE_INSET },
                        ),
                        // The colour swatch is how a background curve is
                        // matched to its dimension, so it has to be here.
                        div {
                            style: format!(
                                "width: 10px; height: 10px; border-radius: 2px; \
                                 background: {}; flex: 0 0 auto;",
                                CC_COLORS[dimension.color % CC_COLORS.len()],
                            ),
                        }
                        span {
                            style: "flex: 1; font-size: 10px;",
                            "CC{dimension.number} {dimension.name}"
                        }
                        button {
                            style: format!(
                                "background: none; border: none; cursor: pointer; \
                                 font-size: 11px; color: {};",
                                if dimension.pinned { theme::ACCENT } else { theme::BORDER_STRONG },
                            ),
                            title: "Pin behind the roll",
                            onclick: {
                                let number = dimension.number;
                                move |_| { editor.write().doc.cc.toggle_pin(number); }
                            },
                            if dimension.pinned { "◉" } else { "○" }
                        }
                        button {
                            style: format!(
                                "background: none; border: none; cursor: pointer; \
                                 font-size: 10px; color: {};",
                                if cc_edit == Some(dimension.number) { theme::ACCENT } else { theme::TEXT_DIM },
                            ),
                            title: "Edit this controller on the roll",
                            onclick: {
                                let number = dimension.number;
                                move |_| {
                                    let mut e = editor;
                                    if e.read().cc_edit == Some(number) {
                                        e.write().exit_cc_edit();
                                    } else {
                                        e.write().edit_cc(number);
                                    }
                                }
                            },
                            "✎"
                        }
                    }
                }
            }
            // The controllers an orchestral part actually rides.
            div {
                style: "display: flex; flex-wrap: wrap; gap: 3px; padding: 0 10px 12px;",
                for number in [1u8, 11, 2, 7, 10, 64] {
                    button {
                        key: "add{number}",
                        style: "height: 20px; padding: 0 6px; font-size: 9px; \
                                border: 1px solid {theme::PANEL_BORDER}; border-radius: 4px; \
                                background: {theme::SURFACE_INSET}; color: {theme::TEXT_DIM}; cursor: pointer;",
                        title: "Add CC{number} ({standard_name(number)})",
                        onclick: move |_| { editor.write().doc.cc.ensure(number); },
                        "+CC{number}"
                    }
                }
            }
        }
    }
}

/// One of the drift/vibrato sliders.
///
/// Parked at 1.0 rather than tracking a stored amount: each change
/// re-decomposes the *current* curve, so the control is a relative
/// scale on what is there now and never fights an earlier edit.
#[component]
fn Blend(editor: Signal<Editor>, label: String, drift: bool) -> Element {
    let mut editor = editor;
    let mut amount = use_signal(|| 1.0f64);
    rsx! {
        CenterSlider {
            label: label.clone(),
            value: amount(),
            min: 0.0,
            max: 1.5,
            center: 1.0,
            width: 130.0,
            readout: format!("{:.0}%", amount() * 100.0),
            on_change: move |v: f64| {
                amount.set(v);
                let Some(id) = editor.read().selection.notes.first().copied() else {
                    return;
                };
                let (t0, t1) = {
                    let ed = editor.read();
                    let Some(n) = ed.doc.note(id) else { return };
                    (n.start, n.end)
                };
                editor.write().apply(&Edit::ReblendPitch {
                    note: id,
                    t0,
                    t1,
                    drift_amount: if drift { v } else { 1.0 },
                    modulation_amount: if drift { 1.0 } else { v },
                });
            },
        }
    }
}
