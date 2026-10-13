//! The Multi Tool overlay.
//!
//! Zones light up over the selection's bounding box, each labelled.
//! Where the drag starts picks the transform — which is the whole idea:
//! a dozen operations become spatial instead of memorised.
//!
//! While a gesture runs, the curve shape and steepness are adjustable,
//! and the readout says what is happening. Without that feedback the
//! zones are just a colourful rectangle.

use dioxus::prelude::*;
use expression_editor_core::Point;
use expression_editor_core::multitool::{self, Bend, Capture, Drag as MtDrag, Pt, Steepness, Zone};
use expression_editor_core::{Dimension, Editor, NoteId};

use crate::theme;

/// Live Multi Tool state.
#[derive(Clone, Debug, PartialEq)]
pub struct MultiTool {
    pub armed: bool,
    /// Bounding box in document space: `(t0, t1, row_lo, row_hi)`.
    pub bounds: Option<(f64, f64, i32, i32)>,
    pub hover: Option<Zone>,
    /// The running gesture.
    pub active: Option<Zone>,
    pub bend: Bend,
    pub steep: Steepness,
    pub symmetric: bool,
    pub amount: f64,
    /// Captured points per note, so every frame recomputes from the
    /// same input instead of compounding.
    captures: Vec<(NoteId, Dimension, Capture)>,
    origin: (f64, f64),
}

impl Default for MultiTool {
    fn default() -> Self {
        Self {
            armed: false,
            bounds: None,
            hover: None,
            active: None,
            bend: Bend::Sine,
            steep: Steepness::default(),
            symmetric: false,
            amount: 0.0,
            captures: Vec::new(),
            origin: (0.0, 0.0),
        }
    }
}

impl MultiTool {
    /// Arm over the current selection, computing its bounding box.
    pub fn arm(&mut self, ed: &Editor) -> bool {
        let notes: Vec<_> = ed
            .selection
            .notes
            .iter()
            .filter_map(|id| ed.doc.note(*id))
            .collect();
        if notes.is_empty() {
            return false;
        }
        let mut t0 = f64::MAX;
        let mut t1 = f64::MIN;
        let mut lo = i32::MAX;
        let mut hi = i32::MIN;
        for n in &notes {
            t0 = t0.min(n.start);
            t1 = t1.max(n.end);
            lo = lo.min(n.row);
            hi = hi.max(n.row);
        }
        self.bounds = Some((t0, t1, lo, hi));
        self.armed = true;
        true
    }

    pub fn disarm(&mut self) {
        *self = Self {
            bend: self.bend,
            steep: self.steep,
            ..Self::default()
        };
    }

    /// Begin a gesture in `zone`, capturing the dimension's current state.
    pub fn begin(&mut self, ed: &mut Editor, zone: Zone, x: f64, y: f64) {
        let Some((t0, t1, ..)) = self.bounds else {
            return;
        };
        let dimension = ed.dimension;
        self.captures = ed
            .selection
            .notes
            .iter()
            .filter_map(|id| {
                let n = ed.doc.note(*id)?;
                let pts: Vec<Pt> = n
                    .curve(dimension)
                    .points()
                    .iter()
                    .map(|p| Pt {
                        t: p.t,
                        value: p.value,
                    })
                    .collect();
                Capture::new(pts).map(|c| (*id, dimension, c))
            })
            .collect();
        let _ = (t0, t1);
        ed.begin_gesture();
        self.active = Some(zone);
        self.origin = (x, y);
        self.amount = 0.0;
    }

    /// Re-run the active transform from the capture.
    pub fn update(&mut self, ed: &mut Editor, x: f64, y: f64) {
        let Some(zone) = self.active else { return };
        // Positional zones read horizontal travel, value zones
        // vertical — matching the axis the transform actually works on.
        let raw = if zone.is_positional() {
            (x - self.origin.0) / 220.0
        } else {
            (self.origin.1 - y) / 180.0
        };
        self.amount = raw.clamp(-1.0, 1.0);
        self.rerun(ed);
    }

    /// Apply the wheel alternative for the active or hovered zone.
    pub fn wheel_alternative(&mut self, ed: &mut Editor) {
        let Some(zone) = self.active.or(self.hover) else {
            return;
        };
        let captures = self.snapshot(ed);
        ed.begin_gesture();
        for (id, dimension, cap) in captures {
            let out = multitool::apply_wheel(zone, &cap);
            write_back(ed, id, dimension, &cap, &out);
        }
    }

    /// Re-run with the current amount, bend and steepness.
    fn rerun(&self, ed: &mut Editor) {
        let Some(zone) = self.active else { return };
        let drag = MtDrag {
            amount: self.amount,
            symmetric: self.symmetric,
        };
        for (id, dimension, cap) in &self.captures {
            let out = multitool::apply(zone, cap, drag, self.bend, self.steep);
            write_back(ed, *id, *dimension, cap, &out);
        }
    }

    fn snapshot(&self, ed: &Editor) -> Vec<(NoteId, Dimension, Capture)> {
        let dimension = ed.dimension;
        ed.selection
            .notes
            .iter()
            .filter_map(|id| {
                let n = ed.doc.note(*id)?;
                let pts: Vec<Pt> = n
                    .curve(dimension)
                    .points()
                    .iter()
                    .map(|p| Pt {
                        t: p.t,
                        value: p.value,
                    })
                    .collect();
                Capture::new(pts).map(|c| (*id, dimension, c))
            })
            .collect()
    }

    pub fn end(&mut self) {
        self.active = None;
        self.captures.clear();
    }

    /// Mid-gesture tweaks.
    pub fn toggle_bend(&mut self, ed: &mut Editor) {
        self.bend = self.bend.toggled();
        self.rerun(ed);
    }

    pub fn nudge_steep(&mut self, ed: &mut Editor, delta: f64) {
        self.steep = self.steep.nudge(delta);
        self.rerun(ed);
    }

    pub fn toggle_symmetric(&mut self, ed: &mut Editor) {
        self.symmetric = !self.symmetric;
        self.rerun(ed);
    }
}

/// Splice a transformed capture back into its note's dimension.
fn write_back(ed: &mut Editor, id: NoteId, dimension: Dimension, cap: &Capture, out: &[Pt]) {
    let points: Vec<expression_editor_core::Point> = out
        .iter()
        .map(|p| expression_editor_core::Point {
            t: p.t,
            value: dimension.clamp(p.value),
            ..Point::default()
        })
        .collect();
    // Splice the union of the original and transformed spans, so a
    // stretch that moved points outside the capture still clears where
    // they came from.
    let t0 = cap.t0.min(out.iter().map(|p| p.t).fold(f64::MAX, f64::min));
    let t1 = cap.t1.max(out.iter().map(|p| p.t).fold(f64::MIN, f64::max));
    ed.apply_live(&expression_editor_core::Edit::DrawDimension {
        note: id,
        dimension,
        t0,
        t1,
        points,
    });
}

/// The zone overlay.
#[component]
pub fn MultiToolOverlay(editor: Signal<Editor>, tool: Signal<MultiTool>) -> Element {
    let mut editor = editor;
    let mut tool = tool;

    let mt = tool.read().clone();
    if !mt.armed {
        return rsx! {};
    }
    let Some((t0, t1, row_lo, row_hi)) = mt.bounds else {
        return rsx! {};
    };

    let ed = editor.read();
    let vp = ed.viewport;
    // Clamped to the roll: Blitz does not clip an absolutely-positioned
    // child against an `overflow: hidden` parent, so an overlay wider
    // than the canvas paints over the inspector.
    let x0 = ed.camera.x(t0).clamp(0.0, vp.w);
    let x1 = ed.camera.x(t1).clamp(0.0, vp.w);
    let y_top = ed
        .camera
        .y(row_hi as f64 + 0.5, ed.viewport)
        .clamp(0.0, vp.h);
    let y_bot = ed
        .camera
        .y(row_lo as f64 - 0.5, ed.viewport)
        .clamp(0.0, vp.h);
    drop(ed);

    // A floor on the box: a single short note would otherwise give
    // zones a few pixels across and nothing could be aimed at. Capped
    // by the remaining room so the floor cannot push it off-canvas.
    let w = (x1 - x0).max(160.0).min(vp.w - x0);
    let h = (y_bot - y_top).max(120.0).min(vp.h - y_top);

    rsx! {
        div {
            style: "position: absolute; left: {x0 + canvas_gutter():.1}px; \
                    top: {y_top + canvas_ruler():.1}px; width: {w:.1}px; height: {h:.1}px; \
                    z-index: 15; touch-action: none;",
            onpointerdown: move |e: PointerEvent| {
                let c = e.data().element_coordinates();
                let Some(zone) = multitool::zone_at(c.x / w, c.y / h) else {
                    return;
                };
                match zone {
                    Zone::Undo => {
                        editor.write().undo();
                    }
                    Zone::Redo => {
                        editor.write().redo();
                    }
                    _ => {
                        let mut t = tool.write();
                        t.begin(&mut editor.write(), zone, c.x, c.y);
                    }
                }
            },
            onpointermove: move |e: PointerEvent| {
                let c = e.data().element_coordinates();
                if tool.read().active.is_some() {
                    let mut t = tool.write();
                    t.update(&mut editor.write(), c.x, c.y);
                } else {
                    let z = multitool::zone_at(c.x / w, c.y / h);
                    if tool.read().hover != z {
                        tool.write().hover = z;
                    }
                }
            },
            onpointerup: move |_| tool.write().end(),
            onwheel: move |e: WheelEvent| {
                let dy = e.delta().strip_units().y;
                let mut t = tool.write();
                t.nudge_steep(&mut editor.write(), -dy / 400.0);
                e.prevent_default();
            },

            svg {
                style: "position: absolute; inset: 0; width: 100%; height: 100%; \
                        pointer-events: none;",
                view_box: "0 0 {w:.0} {h:.0}",
                preserve_aspect_ratio: "none",

                for z in Zone::ALL {
                    {
                        let (zx, zy, zw, zh) = multitool::layout(z);
                        let (px, py) = (zx * w, zy * h);
                        let (pw, ph) = (zw * w, zh * h);
                        let is_hot = mt.active == Some(z)
                            || (mt.active.is_none() && mt.hover == Some(z));
                        let color = zone_color(z);
                        rsx! {
                            g {
                                key: "mtz{z:?}",
                                rect {
                                    x: "{px:.1}",
                                    y: "{py:.1}",
                                    width: "{pw:.1}",
                                    height: "{ph:.1}",
                                    fill: color,
                                    fill_opacity: if is_hot { "0.42" } else { "0.14" },
                                    stroke: color,
                                    stroke_width: if is_hot { "2" } else { "1" },
                                    stroke_opacity: if is_hot { "0.95" } else { "0.4" },
                                }
                                // Only the hot zone is labelled: twelve
                                // labels at once is a wall of text over
                                // the material you are trying to see.
                                if is_hot && pw > 40.0 {
                                    text {
                                        x: "{px + pw * 0.5:.1}",
                                        y: "{py + ph * 0.5 + 3.0:.1}",
                                        text_anchor: "middle",
                                        fill: theme::SELECTED,
                                        font_size: "10",
                                        font_weight: "600",
                                        "{z.label()}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // The readout: what is running, and how it is shaped.
            div {
                style: "position: absolute; left: 0; top: -22px; display: flex; gap: 8px; \
                        align-items: center; background: {theme::SURFACE_SUNKEN}; \
                        border: 1px solid {theme::ACCENT}; border-radius: 4px; \
                        padding: 2px 8px; color: {theme::TEXT}; font-size: 10px; \
                        font-family: ui-monospace, monospace; white-space: nowrap;",
                span {
                    style: "color: {theme::ACCENT};",
                    if let Some(z) = mt.active.or(mt.hover) {
                        "{z.label()}"
                    } else {
                        "Multi Tool"
                    }
                }
                if mt.active.is_some() {
                    span { "{mt.amount * 100.0:+.0}%" }
                }
                span {
                    style: "color: {theme::TEXT_DIM};",
                    if mt.bend == Bend::Sine { "sine" } else { "power" }
                }
                span {
                    style: if mt.steep.is_neutral() {
                        "color: {theme::TEXT_FAINT};"
                    } else {
                        "color: {theme::GOLD};"
                    },
                    "curve {mt.steep.0:+.1}"
                }
                if mt.symmetric {
                    span { style: "color: {theme::GOLD};", "sym" }
                }
                span { style: "color: {theme::TEXT_DIM};", "wheel: curve · M: shape · S: sym" }
            }
        }
    }
}

fn zone_color(z: Zone) -> &'static str {
    match z {
        Zone::CompressTop | Zone::CompressBottom => theme::TOOL_COMPRESS,
        Zone::ScaleTop | Zone::ScaleBottom => theme::TOOL_SCALE,
        Zone::TiltLeft | Zone::TiltRight => theme::TOOL_TILT,
        Zone::StretchLeft | Zone::StretchRight => theme::TOOL_STRETCH,
        Zone::Warp => theme::TOOL_WARP,
        Zone::Move => theme::TOOL_MOVE,
        Zone::Undo | Zone::Redo => theme::TOOL_HISTORY,
    }
}

fn canvas_gutter() -> f64 {
    crate::canvas::GUTTER_W
}

fn canvas_ruler() -> f64 {
    crate::canvas::RULER_H
}
