//! The top bar, the chord box, and the status bar.
//!
//! The top bar carries only what changes *what a gesture does* — tool,
//! dimension, curve shape, history, view. Everything that is a setting
//! rather than a mode (grid, key, tuning, bend range, lane strip) moved
//! to the status bar, because those get set once a session and were
//! crowding the controls that get touched constantly.
//!
//! One consequence worth keeping: the top bar now fits on a single row
//! at plugin width, so the canvas no longer loses a wrapped second row.

use dioxus::prelude::*;
use expression_editor_core::doc::Dimension;
use expression_editor_core::{Editor, Shape, StripLane, Tool, chord};

use crate::drawer::ModDrawer;
use crate::interaction::{self, Drag};
use crate::theme;

fn tool_glyph(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "⬚",
        Tool::Pen => "✎",
        Tool::Curve => "∿",
        Tool::Eraser => "⌫",
        Tool::NoteDraw => "▤",
        Tool::NoteErase => "✕",
        Tool::Razor => "⌗",
        Tool::Velocity => "⇅",
        Tool::Zoom => "⌕",
    }
}

fn shape_glyph(shape: Shape) -> &'static str {
    match shape {
        Shape::Linear => "╱",
        Shape::EaseIn => "◟",
        Shape::EaseOut => "◜",
        Shape::EaseInOut => "∫",
        Shape::Exponential => "⌐",
        Shape::SCurve => "∽",
    }
}

/// A run of buttons that reads as one control.
///
/// Segmenting is what makes a dense bar scannable: the eye finds a
/// group before it finds a button, so related choices share one outline
/// instead of floating as separate pills.
#[component]
fn Segment(children: Element) -> Element {
    rsx! {
        div {
            // `flex: 0 0 auto` is load-bearing. A segment allowed to
            // shrink does not drop cells or ellipsize — it compresses
            // each one until the icon overlaps its own label, which
            // reads as a rendering bug rather than as a full toolbar.
            style: "display: flex; flex: 0 0 auto; align-items: stretch; \
                    border: 1px solid {theme::PANEL_BORDER}; border-radius: 6px; \
                    overflow: hidden; background: {theme::SURFACE_BAR};",
            {children}
        }
    }
}

/// One cell inside a [`Segment`].
#[component]
fn Seg(
    active: bool,
    title: String,
    #[props(default = false)] accent: bool,
    #[props(default)] color: Option<String>,
    /// A handle for tests, on the button itself.
    ///
    /// It has to be here rather than on whatever the caller puts inside:
    /// the button is the element with the click handler, and a testid on
    /// a plain child names a node the harness cannot dispatch to
    /// ("Expected element to have a Dioxus ID").
    #[props(default)]
    testid: Option<String>,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    let fg = color.unwrap_or_else(|| {
        if active {
            theme::SELECTED.to_string()
        } else {
            theme::TEXT.to_string()
        }
    });
    let bg = if active {
        if accent {
            theme::CONTROL_ACTIVE
        } else {
            theme::CONTROL_HOVER
        }
    } else {
        "transparent"
    };
    rsx! {
        button {
            style: "display: flex; align-items: center; justify-content: center; \
                    min-width: 24px; height: 24px; padding: 0 7px; border: none; \
                    border-right: 1px solid {theme::PANEL_BORDER}; \
                    background: {bg}; color: {fg}; font-size: 11px; \
                    line-height: 1; cursor: pointer; user-select: none; \
                    white-space: nowrap;",
            title: "{title}",
            "data-testid": testid,
            // The highlight is a background colour, and a test that
            // asserts on one is really asserting on the theme. This
            // says the same thing in a form that survives a repaint.
            "data-active": "{active}",
            onclick: move |e| onclick.call(e),
            {children}
        }
    }
}

fn divider() -> Element {
    rsx! {
        div {
            style: "width: 1px; height: 18px; background: {theme::PANEL_BORDER}; \
                    margin: 0 3px; flex: 0 0 auto;"
        }
    }
}

// ── top bar ──────────────────────────────────────────────────────────

#[component]
pub fn Toolbar(
    editor: Signal<Editor>,
    drag: Signal<Drag>,
    drawer: Signal<ModDrawer>,
    /// The quantize drawer's visibility. Owned by the host component —
    /// the toolbar only toggles it.
    quantize_open: Signal<bool>,
    /// Save, when the host has somewhere to save to. Standalone writes
    /// a *new* `.rpp` beside the original; absent (REAPER hosts, demo
    /// scenes) the button is not drawn at all — a dead Save button
    /// reads as a bug.
    #[props(default)]
    on_save: Option<EventHandler<()>>,
) -> Element {
    let mut editor = editor;
    let mut drawer = drawer;
    let mut quantize_open = quantize_open;

    let ed = editor.read();
    // What the modifiers say, falling back to what is armed. Holding
    // Ctrl lights the razor and Alt lights note-draw, the same way
    // holding `z` lights zoom — the surface's other previews all promise
    // this and the tool buttons were the ones staying quiet.
    let tool = ed.shown_tool();
    let dimension = ed.dimension;
    let overlays = ed.overlays.clone();
    let shape = ed.shape;
    let can_undo = ed.can_undo();
    let can_redo = ed.can_redo();
    let mod_open = drawer.read().open;
    let mode = ed.mode;
    let stacked = ed.stacked;
    let bend = ed.doc.bend_range;
    drop(ed);

    rsx! {
        div {
            "data-testid": "toolbar",
            // One row, and a fixed height that scrolls rather than wraps.
            //
            // A wrapping bar's height is a function of the window
            // *width*, and the roll is sized by subtracting this bar — so
            // a narrow window silently stole a row from the music. It was
            // briefly two rows, because pinning the height with `nowrap`
            // meant the `margin-left: auto` group had no free space to be
            // pushed into and fell off the right edge, taking undo, Mod,
            // Zoom, Fit and the frame meter with it.
            //
            // With the modes moved to the panel there is only one row of
            // material left, and the group after the verbs is pinned
            // rather than pushed, so neither failure is reachable.
            //
            // `border-box`, so the constant is the bar's *total* height.
            style: "display: flex; flex-direction: column; flex: 0 0 auto; \
                    box-sizing: border-box; height: {crate::sizing::TOOLBAR_H}px; \
                    background: {theme::PANEL}; \
                    border-bottom: 1px solid {theme::PANEL_BORDER}; \
                    font-family: system-ui, sans-serif;",

            // ── how you are editing it ───────────────────────────────
            div {
                style: "display: flex; flex: 0 0 auto; align-items: center; \
                        gap: 5px; height: 100%; padding: 0 8px; \
                        overflow: hidden;",

            // The verbs scroll; the group after them does not.
            //
            // Two containers rather than one row, because `margin-left:
            // auto` needs free space to push into and there is none once
            // the tools carry their names. Giving the left half its own
            // `overflow-x` means a narrow window costs you a scroll
            // through the tools, never the disappearance of undo.
            div {
                style: "display: flex; flex: 1 1 auto; min-width: 0; \
                        align-items: center; gap: 5px; \
                        overflow-x: auto; overflow-y: hidden;",

            // The tools carry their names.
            //
            // A `title` is not an affordance here: Blitz draws no
            // tooltips, so six unlabelled glyphs were six things you
            // could only learn by clicking them and watching what
            // happened. These are the verbs of the whole surface — what
            // a drag will *do* — and the one control that most needs to
            // say so.
            Segment {
                for t in Tool::ALL {
                    Seg {
                        key: "{t:?}",
                        // `format!` rather than an rsx literal: rsx
                        // interpolates `"tool-{t:?}"` only when it is
                        // the whole value, and `.to_lowercase()` on the
                        // end makes it an ordinary method call on an
                        // ordinary string — which shipped the braces.
                        testid: format!("tool-{t:?}").to_lowercase(),
                        active: tool == t,
                        accent: true,
                        title: t.label().to_string(),
                        onclick: move |_| editor.write().tool = t,
                        span {
                            style: "display: flex; align-items: center; gap: 5px;",
                            span { "{tool_glyph(t)}" }
                            span { style: "font-size: 10px;", "{t.label()}" }
                        }
                    }
                }
            }

            {divider()}

            // Dimension selection and overlay visibility, joined: the name
            // picks the dimension to edit, the dot toggles it as an overlay.
            // Hidden outside MPE and Audio — plain MIDI cannot carry
            // per-note pressure, so offering the control would promise
            // an edit the format will drop.
            if mode.has_expression_lanes() {
            Segment {
                for l in Dimension::ALL {
                    // One wrapper per dimension so the key lands on a single
                    // node; Dioxus only honours a key on the first node
                    // in a block, and this loop emits a pair.
                    div {
                        key: "dimension{l:?}",
                        style: "display: flex; align-items: stretch;",
                    Seg {
                        active: dimension == l,
                        color: theme::lane_color(l).to_string(),
                        title: format!("Edit {}", theme::lane_label(l)),
                        onclick: move |_| editor.write().dimension = l,
                        "{theme::lane_label(l)}"
                    }
                    Seg {
                        active: overlays.contains(&l),
                        title: format!("Show {} as overlay", theme::lane_label(l)),
                        onclick: move |_| {
                            let mut ed = editor.write();
                            match ed.overlays.iter().position(|&x| x == l) {
                                Some(i) => { ed.overlays.remove(i); }
                                None => ed.overlays.push(l),
                            }
                        },
                        if overlays.contains(&l) { "●" } else { "○" }
                    }
                    }
                }
            }
            }

            // MPE channel management, only where channels are the
            // mechanism that makes per-note expression possible.
            if mode.has_mpe_channels() {
                Segment {
                    Seg {
                        active: false,
                        title: "Spread selected notes across member channels (R)".to_string(),
                        onclick: move |_| {
                            let notes = editor.read().selection.notes.clone();
                            if !notes.is_empty() {
                                editor.write().apply(
                                    &expression_editor_core::Edit::AssignChannels {
                                        notes,
                                        seed: 0x5EED,
                                    },
                                );
                            }
                        },
                        "Spread ch"
                    }
                    Seg {
                        active: false,
                        title: "Channel down".to_string(),
                        onclick: move |_| {
                            let notes = editor.read().selection.notes.clone();
                            editor.write().apply(
                                &expression_editor_core::Edit::NudgeChannel { notes, delta: -1 },
                            );
                        },
                        "ch−"
                    }
                    Seg {
                        active: false,
                        title: "Channel up".to_string(),
                        onclick: move |_| {
                            let notes = editor.read().selection.notes.clone();
                            editor.write().apply(
                                &expression_editor_core::Edit::NudgeChannel { notes, delta: 1 },
                            );
                        },
                        "ch+"
                    }
                    Seg {
                        active: false,
                        title: "Pitch-bend range — must match the instrument".to_string(),
                        onclick: move |_| {
                            // The values instruments actually use.
                            const RANGES: [f64; 4] = [2.0, 12.0, 24.0, 48.0];
                            let mut ed = editor.write();
                            let cur = ed.doc.bend_range;
                            let i = RANGES.iter().position(|r| *r == cur).unwrap_or(3);
                            ed.doc.bend_range = RANGES[(i + 1) % RANGES.len()];
                        },
                        span { style: "min-width: 42px;", "±{bend:.0}" }
                    }
                }

                {divider()}
            }

            {divider()}

            // The shapes keep their glyphs, and gain a caption.
            //
            // Unlike the tools these icons are not arbitrary — each one
            // *is* the curve it applies, which is a better label than the
            // word "Exponential" would be. What was missing is what the
            // row of them is for, so the group says it once instead of
            // six times.
            span {
                style: "font-size: 9px; letter-spacing: 0.08em; \
                        text-transform: uppercase; color: {theme::TEXT_DIM}; \
                        flex: 0 0 auto; padding-right: 2px;",
                "Shape"
            }
            Segment {
                for s in Shape::ALL {
                    Seg {
                        key: "{s:?}",
                        active: shape == s,
                        title: s.label().to_string(),
                        onclick: move |_| {
                            let d = drag.read().clone();
                            interaction::apply_shape(&mut editor.write(), &d, s);
                        },
                        "{shape_glyph(s)}"
                    }
                }
            }

            }

            // History, and things that act on the view rather than the
            // material. Pinned, never pushed.
            div {
                style: "flex: 0 0 auto; display: flex; align-items: center; gap: 5px; \
                        padding-left: 6px;",

                // Stack: every track on one timeline. It sat beside the
                // mode buttons, which was right when those were here —
                // it answers the same question from the other side. With
                // the modes moved to the panel it belongs with the other
                // view toggles instead, because that is what it is.
                // Hidden with one track, where a stack of one is the roll
                // with less room.
                if editor.read().tracks.len() > 1 {
                    Segment {
                        Seg {
                            active: stacked,
                            accent: true,
                            title: "Show every track on one timeline".to_string(),
                            onclick: move |_| {
                                let now = editor.read().stacked;
                                editor.write().stacked = !now;
                            },
                            svg {
                                view_box: "0 0 16 16",
                                style: "width: 13px; height: 13px; flex: 0 0 auto;",
                                path {
                                    // Three stacked lanes.
                                    d: "M2 4h12 M2 8h12 M2 12h12",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "1.3",
                                    stroke_linecap: "round",
                                }
                            }
                        }
                    }
                }
                Segment {
                    Seg {
                        active: false,
                        title: "Undo".to_string(),
                        onclick: move |_| { editor.write().undo(); },
                        span { style: if can_undo { "" } else { "opacity: 0.3;" }, "↶" }
                    }
                    Seg {
                        active: false,
                        title: "Redo".to_string(),
                        onclick: move |_| { editor.write().redo(); },
                        span { style: if can_redo { "" } else { "opacity: 0.3;" }, "↷" }
                    }
                }
                // The quantize drawer, where the mode edits audio hits
                // on a waveform. A tool on the toolbar rather than a
                // status-bar setting because opening it changes what the
                // surface is *for* — the engine was complete and there
                // was no way to reach it.
                // r[impl drums.quantize.panel]
                if mode.draws_slices() {
                    Segment {
                        Seg {
                            active: quantize_open(),
                            accent: true,
                            testid: "tool-quantize".to_string(),
                            title: "Quantize — detect hits and put them on the grid".to_string(),
                            onclick: move |_| {
                                let open = quantize_open();
                                quantize_open.set(!open);
                            },
                            "Quantize"
                        }
                    }
                }
                // r[impl drums.save.new-file]
                if let Some(save) = on_save {
                    Segment {
                        Seg {
                            active: false,
                            accent: false,
                            testid: "tool-save".to_string(),
                            title: "Save as a new .rpp beside the original — never over it"
                                .to_string(),
                            onclick: move |_| save.call(()),
                            "Save"
                        }
                    }
                }
                Segment {
                    Seg {
                        active: mod_open,
                        accent: true,
                        title: "Modulation (B)".to_string(),
                        onclick: move |_| {
                            let mut dw = drawer.write();
                            if dw.open {
                                dw.cancel(&mut editor.write());
                            } else if dw.open_on(&editor.read()) {
                                dw.preview(&mut editor.write());
                            }
                        },
                        "Mod"
                    }
                }
                Segment {
                    Seg {
                        active: false,
                        title: "Zoom to the passage under the cursor (F)".to_string(),
                        onclick: move |_| {
                            let (t, row) = {
                                let ed = editor.read();
                                (
                                    ed.playhead
                                        .unwrap_or_else(|| ed.camera.t_at(ed.viewport.w * 0.5)),
                                    ed.camera.vertical.center,
                                )
                            };
                            editor.write().smart_zoom(
                                expression_editor_core::ZoomModes::NOTE_AREA,
                                t,
                                row,
                            );
                        },
                        "Zoom"
                    }
                    Seg {
                        active: false,
                        title: "Reset view (V)".to_string(),
                        onclick: move |_| editor.write().reset_view(),
                        "Fit"
                    }
                }
                Fps { editor }
            }
            }
        }
    }
}

// ── chord box ────────────────────────────────────────────────────────

/// The selection row: what is selected, what chord it makes, and the
/// two Melodyne blend controls for it.
///
/// Reads the selection, or the notes under the playhead when nothing is
/// selected — so it says something useful while you navigate rather
/// than going blank the moment you deselect.
///
/// **In the status bar, not a row of its own.** It had a full 30px band
/// under the toolbar, and spent most of it repeating the inspector: the
/// note name, channel, zone count, vibrato and drift it showed on the
/// right are the inspector's NOTE and PITCH SHAPE sections verbatim. The
/// chord *name* and its member pitches are the only things here the
/// inspector cannot say, and they fit beside "3 selected" — so the roll
/// gets the row back and no fact has two homes.
#[component]
pub fn ChordBox(editor: Signal<Editor>) -> Element {
    let ed = editor.read();
    let pitches = ed.chord_pitches();
    let name = ed.current_chord().map(|c| chord::name(&c));
    let note_names: Vec<String> = pitches
        .iter()
        .map(|&p| expression_editor_core::tuning::note_name(p))
        .collect();
    drop(ed);

    // Nothing to analyse, nothing to show. In its own row this had to
    // draw *something* or leave a visibly empty band; in the status bar
    // it can simply not be there.
    if pitches.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            "data-testid": "chord-box",
            style: "display: flex; align-items: center; gap: 6px; flex: 0 0 auto; \
                    overflow: hidden; font-family: system-ui, sans-serif;",

            span {
                style: "font-size: 9px; letter-spacing: 0.08em; text-transform: uppercase; \
                        color: {theme::TEXT_DIM}; flex: 0 0 auto;",
                "Chord"
            }
            if let Some(name) = name {
                span {
                    style: "font-size: 12px; font-weight: 600; color: {theme::GOLD}; \
                            flex: 0 0 auto;",
                    "{name}"
                }
            } else {
                span {
                    style: "font-size: 10px; color: {theme::TEXT_FAINT}; flex: 0 0 auto;",
                    "single note"
                }
            }
            // The constituent notes, so a surprising name can be checked
            // against what is actually sounding.
            div {
                style: "display: flex; gap: 3px; align-items: center; overflow: hidden;",
                for (i, n) in note_names.iter().enumerate() {
                    span {
                        key: "cn{i}",
                        style: "font-size: 9px; font-family: ui-monospace, monospace; \
                                color: {theme::TEXT}; background: {theme::CONTROL}; \
                                border: 1px solid {theme::PANEL_BORDER}; \
                                border-radius: 3px; padding: 0 4px; flex: 0 0 auto;",
                        "{n}"
                    }
                }
            }
        }
    }
}

// ── status bar ───────────────────────────────────────────────────────

/// Settings, not modes: grid, key, tuning, the lane strip, and readouts.
#[component]
pub fn StatusBar(editor: Signal<Editor>) -> Element {
    let mut editor = editor;
    let ed = editor.read();
    let grid_label = ed.grid.label();
    let grid_on = ed.grid.enabled;
    let triplet = ed.grid.triplet;
    let temperament = ed.tuning.temperament.name;
    let key_pc = ed.tuning.key_pc;
    let snap_12tet = ed.tuning.snap_12tet;
    let bend = ed.doc.bend_range;
    let strip = ed.strip_lane;
    let strip_on = ed.lane_strip_h > 0.0;
    let count = ed.selection.notes.len();
    let ambiguous = ed.doc.notes.iter().filter(|n| n.ambiguous).count();
    // The sticky razor modes, as the letters that set them.
    let razor_modes = {
        use expression_editor_core::razor::RazorAxis;
        let mut s = String::new();
        if ed.razor_insert {
            s.push_str(" I");
        }
        match ed.razor_axis {
            Some(RazorAxis::Horizontal) => s.push_str(" H"),
            Some(RazorAxis::Vertical) => s.push_str(" L"),
            None => {}
        }
        s
    };
    let razors = ed.razor.areas.len();
    let mouse_preset = ed.mouse.name;
    let density = ed.grid.adaptive.density;
    let adaptive_on = ed.grid.adaptive.is_adaptive();
    let coarsened = ed.grid.is_coarsened();
    // What was asked for, as opposed to what the zoom is allowing.
    let ceiling_label = ed.grid.ceiling_label();
    // The setting, spelled out, so the readout can show what is in use
    // while the tooltip says what was asked for — the two differ exactly
    // when the zoom is holding the grid back.
    let grid_title = if coarsened {
        format!(
            "Grid 1/{:.0} — held at 1/{:.0} by the zoom",
            1.0 / ed.grid.division,
            1.0 / ed.grid.effective(),
        )
    } else {
        "Grid division".to_string()
    };
    drop(ed);

    let density_short = match density {
        adaptive_grid::Density::Widest => "WIDE+",
        adaptive_grid::Density::Wide => "WIDE",
        adaptive_grid::Density::Medium => "MED",
        adaptive_grid::Density::Narrow => "NARR",
        adaptive_grid::Density::Narrowest => "NARR-",
        adaptive_grid::Density::Custom(_) => "CUST",
        adaptive_grid::Density::Fixed => "AUTO",
    };

    rsx! {
        div {
            "data-testid": "status-bar",
            style: "display: flex; flex: 0 0 auto; align-items: center; gap: 5px; \
                    box-sizing: border-box; height: {crate::sizing::STATUS_H}px; \
                    padding: 0 8px; background: {theme::PANEL}; \
                    border-top: 1px solid {theme::PANEL_BORDER}; \
                    color: {theme::TEXT_DIM}; font-size: 10px; \
                    font-family: system-ui, sans-serif; overflow: hidden;",

            Segment {
                Seg {
                    active: grid_on,
                    accent: true,
                    title: "Snap to the editor's own grid".to_string(),
                    onclick: move |_| {
                        let on = editor.read().grid.enabled;
                        editor.write().grid.enabled = !on;
                    },
                    "⊞"
                }
                Seg {
                    active: false,
                    title: "Coarser (1)".to_string(),
                    onclick: move |_| editor.write().grid_coarser(),
                    "−"
                }
                Seg {
                    active: false,
                    title: grid_title,
                    onclick: move |_| {},
                    span {
                        "data-testid": "grid-division",
                        // Dimmed while the zoom is holding it coarser
                        // than the setting, so the number always reads as
                        // what notes will actually snap to — and it is
                        // visible that the difference is the zoom's doing
                        // rather than the setting having changed.
                        style: format!(
                            "min-width: 38px; font-family: ui-monospace, monospace; color: {};",
                            if coarsened { theme::GOLD } else { theme::TEXT },
                        ),
                        "{grid_label}"
                    }
                    // The ceiling, spelled out beside it while the zoom
                    // is holding the grid back.
                    //
                    // This used to live only in the `title`, and Blitz
                    // draws no tooltips — so the one moment the readout
                    // is not the setting was also the one moment nothing
                    // said so, and the number looked like it had changed
                    // itself. Two numbers is the whole explanation:
                    // what you get, and what you asked for.
                    if coarsened {
                        span {
                            "data-testid": "grid-ceiling",
                            style: format!(
                                "font-family: ui-monospace, monospace; font-size: 9px; \
                                 color: {}; margin-left: 3px;",
                                theme::TEXT_DIM,
                            ),
                            "≤{ceiling_label}"
                        }
                    }
                }
                Seg {
                    active: false,
                    title: "Finer (2)".to_string(),
                    onclick: move |_| editor.write().grid_finer(),
                    "+"
                }
                // Adaptive: the setting becomes a ceiling, and the zoom
                // coarsens away from it. Cycles through the densities
                // rather than being a plain checkbox, because "how much
                // room to leave" is the second half of the decision and
                // hiding it in a menu makes it undiscoverable.
                Seg {
                    active: adaptive_on,
                    testid: "grid-adaptive".to_string(),
                    title: format!(
                        "Adaptive grid — {} (the division above is the finest it will use)",
                        density.label()
                    ),
                    onclick: move |_| {
                        let next = next_density(editor.read().grid.adaptive.density);
                        editor.write().set_grid_density(next);
                    },
                    // Says both halves: that the grid can follow the zoom
                    // at all, and what it is currently set to. A bare
                    // "AUTO" said the first and hid the second, and a
                    // bare "WIDE" said the second without ever
                    // explaining itself.
                    span {
                        style: "font-size: 9px; display: flex; align-items: center; gap: 4px;",
                        span { style: "color: {theme::TEXT_DIM};", "AUTO" }
                        span { if adaptive_on { "{density_short}" } else { "OFF" } }
                    }
                }
                Seg {
                    active: triplet,
                    title: "Triplet grid (T)".to_string(),
                    onclick: move |_| {
                        let on = editor.read().grid.triplet;
                        editor.write().grid.triplet = !on;
                    },
                    "T"
                }
            }

            Segment {
                Seg {
                    active: false,
                    title: "Key — click to cycle".to_string(),
                    onclick: move |_| {
                        let mut ed = editor.write();
                        ed.tuning.key_pc = (ed.tuning.key_pc + 1).rem_euclid(12);
                    },
                    span {
                        style: "min-width: 20px; color: {theme::TEXT}; font-weight: 600;",
                        "{expression_editor_core::tuning::pitch_class_name(key_pc)}"
                    }
                }
                Seg {
                    active: false,
                    title: "Temperament — click to cycle".to_string(),
                    onclick: move |_| {
                        let presets = expression_editor_core::tuning::PRESETS;
                        let mut ed = editor.write();
                        let i = presets
                            .iter()
                            .position(|t| t.name == ed.tuning.temperament.name)
                            .unwrap_or(0);
                        ed.tuning.temperament = presets[(i + 1) % presets.len()].clone();
                    },
                    span { style: "min-width: 92px;", "{temperament}" }
                }
                Seg {
                    active: snap_12tet,
                    title: "Also offer ordinary semitone centres".to_string(),
                    onclick: move |_| {
                        let on = editor.read().tuning.snap_12tet;
                        editor.write().tuning.snap_12tet = !on;
                    },
                    "12TET"
                }
            }

            Segment {
                Seg {
                    active: strip_on,
                    accent: true,
                    title: "Show the velocity / CC dimension".to_string(),
                    onclick: move |_| {
                        let h = editor.read().lane_strip_h;
                        editor.write().lane_strip_h = if h > 0.0 { 0.0 } else { 96.0 };
                    },
                    "▁"
                }
                Seg {
                    active: false,
                    title: "Which dimension the strip shows".to_string(),
                    onclick: move |_| {
                        let cur = editor.read().strip_lane;
                        let all = StripLane::ALL;
                        let i = all.iter().position(|s| *s == cur).unwrap_or(0);
                        editor.write().strip_lane = all[(i + 1) % all.len()];
                    },
                    span { style: "min-width: 54px; color: {theme::TEXT};", "{strip.label()}" }
                }
            }

            span { "Bend {bend:.0}" }
            span { "{mouse_preset}" }

            div {
                style: "margin-left: auto; display: flex; align-items: center; gap: 12px; \
                        font-family: ui-monospace, monospace; flex: 0 0 auto;",
                ChordBox { editor }
                // The razor's own state, spelled out.
                //
                // `I`, `H` and `L` are sticky, and sticky state that
                // nothing displays is the mode you forget you are in.
                // The area count alone did not say why the next drag was
                // about to behave differently from the last one.
                if razors > 0 {
                    span {
                        style: "color: {theme::RAZOR};",
                        "razor {razors}{razor_modes}"
                    }
                }
                // Names the mark, not just the count.
                //
                // The roll flags these notes and this line explains
                // them, and for a long time neither said so — you got a
                // red something on a note and a red something in the
                // corner and no reason to connect them. "Red bar" is the
                // whole fix: it is the one word that turns two unrelated
                // warnings into one.
                if ambiguous > 0 {
                    span {
                        style: "color: {theme::ZONE};",
                        "⚠ red bar: {ambiguous} overlap on one MIDI channel — per-note expression can't be told apart"
                    }
                }
                span { "{count} selected" }
            }
        }
    }
}

/// Off, then coarsest to finest, then off again.
///
/// A cycle rather than a menu because there are only six states and the
/// interesting move is "a bit more/less room", which a menu makes into
/// two clicks and a decision.
fn next_density(current: adaptive_grid::Density) -> adaptive_grid::Density {
    use adaptive_grid::Density as D;
    match current {
        D::Fixed => D::Widest,
        D::Widest => D::Wide,
        D::Wide => D::Medium,
        D::Medium => D::Narrow,
        D::Narrow => D::Narrowest,
        D::Narrowest | D::Custom(_) => D::Fixed,
    }
}

/// The frame rate, top right.
///
/// Two things had to be true before this could report anything.
///
/// It has to be *re-rendered*: `Fps {}` took no props, and dioxus
/// memoizes a component whose props have not changed, so it rendered
/// once at mount and never again — the readout sat at `—` no matter how
/// hard the surface was working. Reading `editor` fixes that, because a
/// subscription re-runs a component whether or not its props changed.
///
/// And it has to have something honest to count. This used to time its
/// own renders, which is how often dioxus rebuilds a component, not how
/// often anything reaches the screen.
///
/// It reports the **renderer's** frame rate, counted in the roll
/// widget's `paint` — which the renderer calls once per frame. That
/// became possible only when the roll stopped being an svg subtree and
/// became a custom widget; before that there was no per-frame hook
/// anywhere in the surface, and this reported editor updates instead.
///
/// Reading it still needs a render, so what is on screen is the frame
/// rate as of the last editor update. That is the right time to look:
/// the number matters while something is moving.
///
/// Averaged over a short window rather than shown per-frame. A raw
/// reciprocal jitters far too much to read, and a number nobody can read
/// is not a diagnostic.
#[component]
fn Fps(editor: Signal<Editor>) -> Element {
    // The subscription. Reading any field is enough — the component is
    // then re-run on every write to the editor, which is the event being
    // counted. Cheap on purpose: a clone of the document here would make
    // the meter itself the slow thing it is measuring.
    let _tick = editor.read().doc.notes.len();

    // Counted where frames actually happen — the roll widget's `paint`.
    //
    // Deliberately **read**, never written here. A signal written during
    // render schedules another render, which writes it again: the
    // surface spins at 100% forever and never settles. That is not a
    // subtle failure — it hung the headless suite outright, every
    // gesture test timing out at thirty seconds while this component
    // re-rendered underneath them. A meter must be a passive observer of
    // drawing and never a cause of it.
    //
    // `try_consume_context` rather than `use_context`: the panels are
    // also mounted outside `ExpressionEditor` by their own tests, where
    // there is no roll and so no frame counter to find.
    let frames = try_consume_context::<crate::roll_widget::Frames>();
    let fps = frames.as_ref().and_then(|f| f.fps());
    // How much of that frame was the roll. Shown beside the rate because
    // the two answer different questions: a blown budget with this at
    // almost nothing is being spent in the renderer or the rest of the
    // DOM, and knowing which is the difference between a day of
    // profiling and a one-line fix.
    let paint_ms = frames.as_ref().and_then(|f| f.paint_ms());
    // Renders per second. Higher than the frame rate means the surface
    // is rebuilding for events the screen never shows — which is what a
    // drag driven by a high-polling-rate mouse does.
    let ups = frames.as_ref().and_then(|f| f.builds_per_second());

    // Quiet while it is healthy, and only asks for attention when it is
    // not: dim above a screen refresh, gold where a drag starts to feel
    // it, and the razor colour below the rate at which motion reads as
    // motion. A readout that is loud when nothing is wrong gets ignored
    // when something is.
    let color = match fps {
        Some(f) if f >= 50.0 => theme::TEXT_DIM,
        Some(f) if f >= 24.0 => theme::GOLD,
        Some(_) => theme::RAZOR,
        None => theme::TEXT_DIM,
    };
    rsx! {
        div {
            "data-testid": "fps",
            title: "Frames painted per second, and how much of a frame the roll itself takes",
            style: format!(
                "min-width: 180px; text-align: right; font-size: 10px; \
                 white-space: nowrap; \
                 font-variant-numeric: tabular-nums; color: {color};",
            ),
            // Three numbers, because a stuttering drag is invisible in
            // any one of them: how fast frames arrive, how much of a
            // frame the roll costs, and how often the surface rebuilds.
            // Rebuilds far above the frame rate means work no frame ever
            // showed.
            match (fps, paint_ms, ups) {
                (Some(f), Some(ms), Some(u)) => {
                    format!("{f:.0} fps · roll {ms:.2}ms · {u:.0}/s")
                }
                (Some(f), Some(ms), None) => format!("{f:.0} fps · roll {ms:.2}ms"),
                (Some(f), _, _) => format!("{f:.0} fps"),
                _ => "— fps".to_string(),
            }
        }
    }
}
