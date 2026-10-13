//! The modulation drawer — programmatic vibrato and swells.
//!
//! Opens on `B` against a captured target (the selected note, or its
//! active Q zone) and **locks** that target: while the drawer is open,
//! selection, drawing, and note edits are blocked, but transport and
//! all navigation stay live so the result can be auditioned in context.
//!
//! Changes write to the take as an audible preview. Apply converts the
//! preview into one undoable edit; Cancel restores the exact captured
//! curve, byte-for-byte.

use dioxus::prelude::*;
use expression_editor_core::doc::{Dimension, NoteId, Point};
use expression_editor_core::edit::Edit;
use expression_editor_core::modulation::{CurveTarget, Row, Stack, Wave};
use expression_editor_core::{Editor, Shape};

use crate::theme;
use crate::widgets::Slider;

/// Drawer state, owned by the editor component.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModDrawer {
    pub open: bool,
    pub stack: Stack,
    pub selected_row: Option<usize>,
    /// Captured target: note, dimension, span.
    target: Option<(NoteId, Dimension, f64, f64)>,
    /// The dimension's exact points before the drawer touched anything.
    captured: Vec<Point>,
    pub taper: f64,
}

impl ModDrawer {
    /// Capture the current target and open. Returns false when there is
    /// nothing selected to modulate.
    pub fn open_on(&mut self, ed: &Editor) -> bool {
        let Some(&id) = ed.selection.notes.first() else {
            return false;
        };
        let Some(n) = ed.doc.note(id) else {
            return false;
        };
        let (t0, t1) = n.target_span();
        self.target = Some((id, ed.dimension, t0, t1));
        self.captured = n.curve(ed.dimension).points().to_vec();
        self.stack = Stack::growing_vibrato();
        self.selected_row = Some(0);
        self.taper = 0.08;
        self.open = true;
        true
    }

    pub fn target(&self) -> Option<(NoteId, Dimension, f64, f64)> {
        self.target
    }

    /// Rewrite the preview from the captured curve — restore first, then
    /// modulate, so successive control moves never compound.
    pub fn preview(&self, ed: &mut Editor) {
        let Some((note, dimension, t0, t1)) = self.target else {
            return;
        };
        ed.apply_live(&Edit::RestoreDimension {
            note,
            dimension,
            t0,
            t1,
            points: self.captured.clone(),
        });
        ed.apply_live(&Edit::ApplyModulation {
            note,
            dimension,
            t0,
            t1,
            stack: self.stack.clone(),
            taper: self.taper,
            samples: 192,
        });
    }

    /// Commit as one undo step.
    pub fn apply(&mut self, ed: &mut Editor) {
        let Some((note, dimension, t0, t1)) = self.target else {
            return;
        };
        // Rewind to captured *without* history, then apply once through
        // it — so undo lands on the pre-drawer curve, not on a preview.
        ed.apply_live(&Edit::RestoreDimension {
            note,
            dimension,
            t0,
            t1,
            points: self.captured.clone(),
        });
        ed.apply(&Edit::ApplyModulation {
            note,
            dimension,
            t0,
            t1,
            stack: self.stack.clone(),
            taper: self.taper,
            samples: 192,
        });
        self.close();
    }

    pub fn cancel(&mut self, ed: &mut Editor) {
        if let Some((note, dimension, t0, t1)) = self.target {
            ed.apply_live(&Edit::RestoreDimension {
                note,
                dimension,
                t0,
                t1,
                points: self.captured.clone(),
            });
        }
        self.close();
    }

    fn close(&mut self) {
        self.open = false;
        self.target = None;
        self.captured.clear();
        self.selected_row = None;
    }
}

/// Mutate the stack and re-preview — every control is these two steps,
/// and the preview always rebuilds from the captured curve so
/// successive moves never compound.
fn edit_stack(
    mut editor: Signal<Editor>,
    mut drawer: Signal<ModDrawer>,
    f: impl FnOnce(&mut Stack),
) {
    {
        let mut dw = drawer.write();
        f(&mut dw.stack);
    }
    let dw = drawer.read().clone();
    dw.preview(&mut editor.write());
}

fn wave_glyph(w: Wave) -> &'static str {
    match w {
        Wave::Sine => "∿",
        Wave::Square => "⊓",
        Wave::Saw => "◺",
        Wave::Triangle => "△",
    }
}

/// An accurate preview of one row's own contribution.
fn row_preview(row: &Row, w: f64, h: f64) -> String {
    let n = 72;
    (0..n)
        .map(|i| {
            let x = i as f64 / (n - 1) as f64;
            let v = match *row {
                Row::Oscillator {
                    wave,
                    amplitude,
                    rate,
                } => wave.sample(rate * x) * amplitude.clamp(0.0, 1.0),
                Row::Curve {
                    shape, depth, up, ..
                } => {
                    let s = shape.amount(x);
                    let s = if up { s } else { 1.0 - s };
                    (s * depth).clamp(0.0, 1.0) * 2.0 - 1.0
                }
            };
            format!("{:.1},{:.1} ", x * w, h * 0.5 - v * h * 0.42)
        })
        .collect()
}

/// The whole stack's output, which is what actually lands on the curve.
fn stack_preview(stack: &Stack, w: f64, h: f64) -> String {
    let values = stack.render(160);
    let peak = values.iter().fold(0.05f64, |a, b| a.max(b.abs()));
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = i as f64 / (values.len() - 1) as f64 * w;
            format!("{:.1},{:.1} ", x, h * 0.5 - v / peak * h * 0.42)
        })
        .collect()
}

#[component]
pub fn ModulationDrawer(editor: Signal<Editor>, drawer: Signal<ModDrawer>) -> Element {
    let mut editor = editor;
    let mut drawer = drawer;

    let d = drawer.read().clone();
    if !d.open {
        return rsx! {};
    }
    let selected = d.selected_row;
    let rows = d.stack.rows.clone();

    rsx! {
        div {
            style: "position: absolute; top: 0; right: 0; bottom: 0; width: 268px; \
                    z-index: 20; display: flex; flex-direction: column; \
                    background: {theme::PANEL}; border-left: 1px solid {theme::PANEL_BORDER}; \
                    color: {theme::TEXT}; font-family: system-ui, sans-serif; \
                    font-size: 11px; overflow-y: auto;",

            div {
                style: "display: flex; align-items: center; justify-content: space-between; \
                        padding: 7px 10px; border-bottom: 1px solid {theme::PANEL_BORDER};",
                span { style: "font-size: 10px; letter-spacing: 0.08em; \
                               text-transform: uppercase; color: {theme::TEXT_DIM};",
                    "Modulation"
                }
                span { style: "color: {theme::TEXT_DIM}; font-size: 10px;",
                    "target locked"
                }
            }

            // The summed result — the thing that lands on the curve.
            div {
                style: "padding: 8px 10px; border-bottom: 1px solid {theme::PANEL_BORDER};",
                svg {
                    view_box: "0 0 240 54",
                    style: "width: 100%; height: 54px; background: {theme::BG}; \
                            border: 1px solid {theme::PANEL_BORDER}; border-radius: 4px;",
                    line {
                        x1: "0", y1: "27", x2: "240", y2: "27",
                        stroke: theme::PANEL_BORDER, stroke_width: "1",
                    }
                    polyline {
                        points: "{stack_preview(&d.stack, 240.0, 54.0)}",
                        fill: "none",
                        stroke: theme::ACCENT,
                        stroke_width: "1.5",
                    }
                }
            }

            // Rows, in order. A curve row placed after an oscillator
            // envelopes it — the order is the meaning.
            div {
                style: "display: flex; flex-direction: column; gap: 6px; padding: 8px 10px;",
                for (i, row) in rows.iter().enumerate() {
                    div {
                        key: "row{i}",
                        style: format!(
                            "border: 1px solid {}; border-radius: 5px; padding: 6px; \
                             background: {}; cursor: pointer;",
                            if selected == Some(i) { theme::ACCENT } else { theme::PANEL_BORDER },
                            if selected == Some(i) { theme::CONTROL_SELECTED } else { theme::SURFACE_INSET },
                        ),
                        onclick: move |_| drawer.write().selected_row = Some(i),

                        div {
                            style: "display: flex; align-items: center; gap: 6px;",
                            svg {
                                view_box: "0 0 60 20",
                                style: "width: 60px; height: 20px; flex: 0 0 auto;",
                                polyline {
                                    points: "{row_preview(row, 60.0, 20.0)}",
                                    fill: "none",
                                    stroke: theme::lane_color(Dimension::Pitch),
                                    stroke_width: "1.2",
                                }
                            }
                            span {
                                style: "flex: 1;",
                                match row {
                                    Row::Oscillator { wave, .. } => wave.label(),
                                    Row::Curve { up: true, .. } => "Curve Up",
                                    Row::Curve { up: false, .. } => "Curve Down",
                                }
                            }
                            button {
                                style: theme::button_style(false),
                                title: "Remove row",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    edit_stack(editor, drawer, |s: &mut Stack| {
                                        if i < s.rows.len() {
                                            s.rows.remove(i);
                                        }
                                    });
                                },
                                "✕"
                            }
                        }

                        match row {
                            Row::Oscillator { amplitude, rate, .. } => rsx! {
                                Slider {
                                    label: "Amp".to_string(),
                                    value: *amplitude,
                                    min: 0.0,
                                    max: 1.0,
                                    width: 112.0,
                                    readout: format!("{:.2}", amplitude),
                                    on_change: move |v: f64| edit_stack(editor, drawer, |s: &mut Stack| {
                                        if let Some(Row::Oscillator { amplitude, .. }) = s.rows.get_mut(i) {
                                            *amplitude = v;
                                        }
                                    }),
                                }
                                // Rate is logarithmic: musically useful
                                // vibrato lives in a narrow band that a
                                // linear control makes impossible to hit.
                                Slider {
                                    label: "Rate".to_string(),
                                    value: (rate.max(0.5)).log2(),
                                    min: -1.0,
                                    max: 6.0,
                                    width: 112.0,
                                    readout: format!("{rate:.1}×"),
                                    on_change: move |v: f64| edit_stack(editor, drawer, |s: &mut Stack| {
                                        if let Some(Row::Oscillator { rate, .. }) = s.rows.get_mut(i) {
                                            *rate = 2f64.powf(v);
                                        }
                                    }),
                                }
                            },
                            Row::Curve { depth, target, .. } => rsx! {
                                Slider {
                                    label: "Depth".to_string(),
                                    value: *depth,
                                    min: 0.0,
                                    max: 1.0,
                                    width: 112.0,
                                    readout: format!("{:.2}", depth),
                                    on_change: move |v: f64| edit_stack(editor, drawer, |s: &mut Stack| {
                                        if let Some(Row::Curve { depth, .. }) = s.rows.get_mut(i) {
                                            *depth = v;
                                        }
                                    }),
                                }
                                button {
                                    style: theme::button_style(*target == CurveTarget::Rate),
                                    title: "Envelope amplitude or rate",
                                    onclick: move |e: MouseEvent| {
                                        e.stop_propagation();
                                        edit_stack(editor, drawer, |s: &mut Stack| {
                                            if let Some(Row::Curve { target, .. }) = s.rows.get_mut(i) {
                                                *target = match target {
                                                    CurveTarget::Amplitude => CurveTarget::Rate,
                                                    CurveTarget::Rate => CurveTarget::Amplitude,
                                                };
                                            }
                                        });
                                    },
                                    match target {
                                        CurveTarget::Amplitude => "→ Amplitude",
                                        CurveTarget::Rate => "→ Rate",
                                    }
                                }
                            },
                        }
                    }
                }
            }

            // Add rows.
            div {
                style: "display: flex; flex-wrap: wrap; gap: 4px; padding: 0 10px 8px;",
                for w in Wave::ALL {
                    button {
                        key: "add{w:?}",
                        style: theme::button_style(false),
                        title: "Add {w.label()}",
                        onclick: move |_| edit_stack(editor, drawer, |s: &mut Stack| {
                            s.rows.push(Row::Oscillator {
                                wave: w,
                                amplitude: 0.3,
                                rate: 10.0,
                            });
                        }),
                        "{wave_glyph(w)}"
                    }
                }
                button {
                    style: theme::button_style(false),
                    title: "Add an envelope that rises across the selection",
                    onclick: move |_| edit_stack(editor, drawer, |s: &mut Stack| {
                        s.rows.push(Row::Curve {
                            shape: Shape::EaseIn,
                            depth: 1.0,
                            up: true,
                            target: CurveTarget::Amplitude,
                        });
                    }),
                    "Curve ↑"
                }
                button {
                    style: theme::button_style(false),
                    title: "Add an envelope that falls across the selection",
                    onclick: move |_| edit_stack(editor, drawer, |s: &mut Stack| {
                        s.rows.push(Row::Curve {
                            shape: Shape::EaseOut,
                            depth: 1.0,
                            up: false,
                            target: CurveTarget::Amplitude,
                        });
                    }),
                    "Curve ↓"
                }
            }

            // Starting points for the two gestures people actually want.
            div {
                style: "display: flex; gap: 4px; padding: 0 10px 10px;",
                button {
                    style: theme::button_style(false),
                    onclick: move |_| edit_stack(editor, drawer, |s: &mut Stack| {
                        *s = Stack::growing_vibrato();
                    }),
                    "Vibrato grows"
                }
                button {
                    style: theme::button_style(false),
                    onclick: move |_| edit_stack(editor, drawer, |s: &mut Stack| {
                        *s = Stack::receding_vibrato();
                    }),
                    "Vibrato recedes"
                }
            }

            div {
                style: "margin-top: auto; display: flex; gap: 6px; padding: 10px; \
                        border-top: 1px solid {theme::PANEL_BORDER};",
                button {
                    style: format!("{} flex: 1;", theme::button_style(true)),
                    onclick: move |_| {
                        let mut dw = drawer.write();
                        dw.apply(&mut editor.write());
                    },
                    "Apply"
                }
                button {
                    style: format!("{} flex: 1;", theme::button_style(false)),
                    onclick: move |_| {
                        let mut dw = drawer.write();
                        dw.cancel(&mut editor.write());
                    },
                    "Cancel"
                }
            }
        }
    }
}
