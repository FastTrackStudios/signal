//! `viz.spectrum`, `viz.gr_history`, `viz.scope`: one stream, one plot.

use document_proto::{NodeId, TextAlign};
use surface_proto::{Axis, ControlKind, PlotRange, PlotStyle, SlotRole, ValueFormat};
use surface_widgets::Builder;

use super::{Area, LABEL, Look, area, grid, label, opt_f, panel, trace};
use crate::registry::{ComponentDef, Family, Place};

/// Frequencies the grid marks (major = a decade).
const FREQ_LINES: [f64; 27] = [
    20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 200.0, 300.0, 400.0, 500.0, 600.0, 700.0, 800.0, 900.0, 1000.0, 2000.0, 3000.0,
    4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0, 10_000.0,
];
const FREQ_LABELS: [(f64, &str); 10] =
    [(20.0, "20"), (50.0, "50"), (100.0, "100"), (200.0, "200"), (500.0, "500"), (1000.0, "1k"), (2000.0, "2k"), (5000.0, "5k"), (10_000.0, "10k"), (20_000.0, "20k")];

/// A log-frequency grid over `f_min..f_max` with its labels printed under
/// the area (in `panel`), and dB lines every `step` over `lo..hi` labelled
/// at the area's left (`labels_left`) or right.
#[allow(clippy::too_many_arguments)]
pub(crate) fn freq_grid(b: &mut Builder<'_>, panel: NodeId, a: &Area, (ax, ay): (f64, f64), f: (f64, f64), db: (f64, f64, f64), labels_left: bool) {
    let x = Axis::Log { f_min: f.0, f_max: f.1 };
    let y = PlotRange::linear(db.0, db.1);
    let xs: Vec<(f64, bool)> = FREQ_LINES
        .iter()
        .filter(|v| **v > f.0 && **v < f.1)
        .map(|v| (x.to_unit(*v), [100.0, 1000.0, 10_000.0].contains(v)))
        .collect();
    let ys = db_lines(&y, db.2);
    grid(b, a, &xs, &ys);
    for (hz, s) in FREQ_LABELS.iter().filter(|(v, _)| *v >= f.0 && *v <= f.1) {
        let u = x.to_unit(*hz);
        let align = if u < 0.02 {
            TextAlign::Left
        } else if u > 0.98 {
            TextAlign::Right
        } else {
            TextAlign::Center
        };
        label(b, panel, s, (ax + u * a.w, ay + a.h + 10.0), align, LABEL);
    }
    db_labels(b, panel, a, (ax, ay), &y, db.2, labels_left);
}

/// Horizontal lines every `step` dB (0 dB major).
pub(crate) fn db_lines(y: &PlotRange, step: f64) -> Vec<(f64, bool)> {
    let mut out = Vec::new();
    let mut v = (y.min / step).ceil() * step;
    while v <= y.max + 1e-9 {
        if v > y.min + 1e-9 && v < y.max - 1e-9 {
            out.push((y.to_unit(v), v.abs() < 1e-9));
        }
        v += step;
    }
    out
}

/// dB numerals beside the lines of [`db_lines`].
pub(crate) fn db_labels(b: &mut Builder<'_>, panel: NodeId, a: &Area, (ax, ay): (f64, f64), y: &PlotRange, step: f64, left: bool) {
    for (v, _) in db_lines(y, step) {
        let db = y.from_unit(v);
        let s = if db > 0.5 { format!("+{db:.0}") } else { format!("{db:.0}") };
        let cy = ay + (1.0 - v) * a.h;
        if left {
            label(b, panel, &s, (ax - 5.0, cy), TextAlign::Right, LABEL);
        } else {
            label(b, panel, &s, (ax + a.w + 5.0, cy), TextAlign::Left, LABEL);
        }
    }
}

fn spectrum(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p.size.unwrap_or(420.0).max(160.0);
    let h = opt_f(p, "h", w * 0.45);
    let root = panel(b, parent, &p.name, w, h);
    let (ax, ay) = (34.0, 12.0);
    let a = area(b, root, ax, ay, w - ax - 12.0, h - ay - 26.0);
    let (lo, hi) = (opt_f(p, "floor", -90.0), opt_f(p, "ceiling", 0.0));
    let f = (opt_f(p, "f_min", 20.0), opt_f(p, "f_max", 20_000.0));
    freq_grid(b, root, &a, (ax, ay), f, (lo, hi, 12.0), true);
    let color = p.lamp_color(0).unwrap_or_else(|| "#748ffc".into());
    let (fill_top, fill_bottom, line) = (super::alpha(&color, 0.42), super::alpha(&color, 0.03), super::alpha(&color, 0.9));
    let (node, slots) = trace(b, &a, "spectrum", Look { line: Some((&line, 1.2)), fill: Some((&fill_top, &fill_bottom)), glow: Some((&color, 0.35, 4.0)), dash: false });
    let mut c = p.control(ControlKind::Plot { style: PlotStyle::Line, x: Axis::Log { f_min: f.0, f_max: f.1 }, y: PlotRange::linear(lo, hi) });
    for (role, id) in slots {
        c = c.slot(role, id.0);
    }
    b.controls.insert(node, c);
    root
}

fn gr_history(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p.size.unwrap_or(360.0).max(120.0);
    let h = opt_f(p, "h", w * 0.4);
    let root = panel(b, parent, &p.name, w, h);
    let (ax, ay) = (12.0, 12.0);
    let a = area(b, root, ax, ay, w - ax - 34.0, h - 24.0);
    let range = PlotRange::linear(-opt_f(p, "range", 18.0), 0.0);
    grid(b, &a, &(1..8).map(|i| (f64::from(i) / 8.0, false)).collect::<Vec<_>>(), &db_lines(&range, 3.0));
    db_labels(b, root, &a, (ax, ay), &range, 6.0, false);
    let len = opt_f(p, "len", 240.0) as u32;
    let (node, slots) = gr_trace(b, &a, p.lamp_color(0).as_deref().unwrap_or("#ff6b6b"));
    let mut c = p.control(ControlKind::Plot { style: PlotStyle::Area { baseline: 0.0 }, x: Axis::Time { len }, y: range });
    for (role, id) in slots {
        c = c.slot(role, id.0);
    }
    c = c.slot(SlotRole::Value, gr_readout(b, &a, node).0);
    c.format.get_or_insert_with(|| ValueFormat::decimals(1));
    b.controls.insert(node, c);
    root
}

/// A gain-reduction trace: red, hanging from the top, lit.
pub(crate) fn gr_trace(b: &mut Builder<'_>, a: &Area, color: &str) -> (NodeId, Vec<(SlotRole, NodeId)>) {
    let (top, bottom, line) = (super::alpha(color, 0.36), super::alpha(color, 0.04), super::alpha(color, 1.0));
    trace(b, a, "gain reduction", Look { line: Some((&line, 1.6)), fill: Some((&top, &bottom)), glow: Some((color, 0.6, 6.0)), dash: false })
}

/// The latest reduction, printed top right inside the trace frame.
pub(crate) fn gr_readout(b: &mut Builder<'_>, a: &Area, frame: NodeId) -> NodeId {
    let mut t = crate::draw::text("reduction", "0.0 dB", a.w - 76.0, 6.0, 70.0, 11.0, "#ff8787", TextAlign::Right);
    if let document_proto::NodeKind::Text(td) = &mut t.kind {
        td.font_weight = Some(600.0);
    }
    b.add(frame, t)
}

fn scope(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p.size.unwrap_or(320.0).max(120.0);
    let h = opt_f(p, "h", w * 0.5);
    let root = panel(b, parent, &p.name, w, h);
    let a = area(b, root, 12.0, 12.0, w - 24.0, h - 24.0);
    let xs: Vec<(f64, bool)> = (1..10).map(|i| (f64::from(i) / 10.0, i == 5)).collect();
    let ys: Vec<(f64, bool)> = (1..8).map(|i| (f64::from(i) / 8.0, i == 4)).collect();
    grid(b, &a, &xs, &ys);
    let color = p.lamp_color(0).unwrap_or_else(|| "#51cf66".into());
    let (node, slots) = trace(b, &a, "trace", Look { line: Some((&color, 1.4)), fill: None, glow: Some((&color, 0.9, 7.0)), dash: false });
    let mut c = p.control(ControlKind::Plot { style: PlotStyle::Line, x: Axis::Linear { min: 0.0, max: 1.0 }, y: PlotRange::linear(-1.0, 1.0) });
    for (role, id) in slots {
        c = c.slot(role, id.0);
    }
    b.controls.insert(node, c);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "viz.spectrum",
            family: Family::Viz,
            title: "Spectrum",
            description: "A dark analyser panel: a log-frequency grid (20 Hz–20 kHz, labelled) and dB lines, the bound spectrum stream drawn as a glowing line over a gradient fill. Options: `floor=`, `ceiling=` (dB), `f_min=`, `f_max=`, `h=`, `color=#hex`.",
            variants: &[],
            default_size: 420.0,
            binds: "spectrum stream",
            build: spectrum,
        },
        ComponentDef {
            id: "viz.gr_history",
            family: Family::Viz,
            title: "GR history",
            description: "A scrolling gain-reduction history: the bound gain-reduction stream as a red trace hanging from the top, newest at the right, with the latest reduction printed. Options: `range=` (dB, default 18), `len=` (samples kept, default 240), `h=`.",
            variants: &[],
            default_size: 360.0,
            binds: "gain-reduction stream",
            build: gr_history,
        },
        ComponentDef {
            id: "viz.scope",
            family: Family::Viz,
            title: "Scope",
            description: "A phosphor-green oscilloscope: the bound stream's latest frame of samples (−1..1) drawn across a graticule. Options: `h=`, `color=#hex`.",
            variants: &[],
            default_size: 320.0,
            binds: "waveform stream",
            build: scope,
        },
    ]
}
