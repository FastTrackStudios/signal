//! `viz.comp`: a compressor's picture — input and gain-reduction history,
//! the transfer curve with its threshold, and the signal riding it.

use document_proto::{NodeId, NodeKind, Stroke, StrokeAlign, TextAlign, TextData};
use surface_proto::{
    Axis, Binding, Control, ControlKind, CurveInput, HandleAxis, PlotRange, PlotStyle, SlotRole,
    ValueFormat,
};
use surface_widgets::Builder;

use super::eq::HANDLE_BOX;
use super::simple::{db_labels, db_lines, gr_readout, gr_trace};
use super::{LABEL, Look, area, dot, grid, label, ns, opt_f, panel, trace};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// The dB span both graphs show.
const FLOOR: f64 = -60.0;

fn build(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let ns = ns(p);
    let w = p.size.unwrap_or(600.0).max(260.0);
    let h = opt_f(p, "h", (w * 0.42).round());
    let addr = |k: &str| format!("{ns}/{k}");
    let input = p.opt("in").map_or_else(|| addr("in"), ToString::to_string);
    // Histories: `History` feeds, or any reading (the runtime keeps a ring).
    let in_history = p
        .opt("in_history")
        .map_or_else(|| addr("in_history"), ToString::to_string);
    let gr = p
        .opt("gr_history")
        .map_or_else(|| addr("gr_history"), ToString::to_string);
    let len = opt_f(p, "len", 300.0) as u32;
    let root = panel(b, parent, &p.name, w, h);

    // ── the transfer graph, a square on the right ────────────────────────
    let s = h - 12.0 - 26.0;
    let (tx, ty) = (w - 30.0 - s, 12.0);
    let t = area(b, root, tx, ty, s, s);
    let axis = Axis::Linear {
        min: FLOOR,
        max: 0.0,
    };
    let range = PlotRange::linear(FLOOR, 0.0);
    let lines = db_lines(&range, 12.0);
    let xs: Vec<(f64, bool)> = lines.iter().map(|(u, _)| (*u, false)).collect();
    grid(b, &t, &xs, &lines);
    db_labels(b, root, &t, (tx, ty), &range, 12.0, false);
    for (u, _) in &lines {
        let db = axis.from_unit(*u);
        label(
            b,
            root,
            &format!("{db:.0}"),
            (tx + u * s, ty + s + 10.0),
            TextAlign::Center,
            LABEL,
        );
    }
    label(b, t.area, "IN", (s - 4.0, s - 9.0), TextAlign::Right, LABEL);
    label(b, t.area, "OUT", (4.0, 9.0), TextAlign::Left, LABEL);

    let param_or = |k: &str, fallback: f64| {
        if p.flag(&format!("no_{k}")) {
            CurveInput::constant(k, fallback)
        } else {
            CurveInput::param(k, addr(k))
        }
    };
    let inputs = vec![
        CurveInput::param("threshold", addr("threshold")),
        CurveInput::param("ratio", addr("ratio")),
        param_or("knee", 0.0),
        param_or("makeup", 0.0),
        CurveInput::stream("marker", input.clone()),
    ];
    let curve = |series: &str, style| ControlKind::Curve {
        provider: "comp.transfer".into(),
        inputs: inputs.clone(),
        series: Some(series.into()),
        style,
        x: axis.clone(),
        y: range.clone(),
    };
    let (node, slots) = trace(
        b,
        &t,
        "unity",
        Look {
            line: Some(("#ffffff2e", 1.0)),
            fill: None,
            glow: None,
            dash: true,
        },
    );
    b.controls.insert(
        node,
        with(Control::new(curve("unity", PlotStyle::Line)), slots),
    );
    let look = Look {
        line: Some(("#e7f5ff", 2.2)),
        fill: Some(("#4dabf740", "#4dabf705")),
        glow: Some(("#74c0fc", 0.6, 7.0)),
        dash: false,
    };
    let (node, mut slots) = trace(b, &t, "transfer", look);
    // The signal: a ball on the curve at the input level.
    let ball = dot(b, node, "input", 20.0, 11.0, "#ffb347", "", &t);
    slots.push((SlotRole::Marker, ball));
    b.controls.insert(
        node,
        with(Control::new(curve("transfer", PlotStyle::Line)), slots),
    );

    // The threshold: a line across the graph with a chip to grab; the
    // wheel over it turns the ratio.
    let handle = b.add(
        t.area,
        draw::frame("threshold", (s - HANDLE_BOX) / 2.0, 0.0, HANDLE_BOX, s),
    );
    let mut line = draw::path(
        "line",
        0.0,
        0.0,
        HANDLE_BOX,
        s,
        format!("M {:.1} 0 V {s:.1}", HANDLE_BOX / 2.0),
        draw::solid("#00000000"),
    );
    line.fills.clear();
    line.stroke = Stroke {
        paints: vec![draw::solid("#ffd43b99")],
        weight: 1.0,
        align: StrokeAlign::Center,
        dash_pattern: vec![4.0, 3.0],
        ..Stroke::default()
    };
    b.add(handle, line);
    let mut chip = draw::frame("chip", 3.0, s - 20.0, HANDLE_BOX - 6.0, 15.0);
    chip.fills = vec![draw::solid("#ffd43b")];
    let chip = draw::emissive(draw::rounded(chip, 7.5), draw::hex("#ffd43b"), 0.45, 8.0);
    let chip = b.add(handle, chip);
    let mut thr = draw::text(
        "legend",
        "THR",
        0.0,
        2.0,
        HANDLE_BOX - 6.0,
        8.0,
        "#1f1a00",
        TextAlign::Center,
    );
    if let NodeKind::Text(TextData {
        font_weight,
        letter_spacing,
        ..
    }) = &mut thr.kind
    {
        *font_weight = Some(800.0);
        *letter_spacing = 0.8;
    }
    b.add(chip, thr);
    let kind = ControlKind::Handle {
        x: Some(Binding::param(addr("threshold"))),
        y: None,
        wheel: Some(Binding::param(addr("ratio"))),
        x_map: HandleAxis::new(axis.clone()),
        y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
    };
    b.controls.insert(handle, Control::new(kind));

    // ── the history on the left ──────────────────────────────────────────
    let (hx, hy) = (12.0, 12.0);
    let hw = tx - 34.0 - hx;
    let hist = area(b, root, hx, hy, hw, s);
    let cols: Vec<(f64, bool)> = (1..10).map(|i| (f64::from(i) / 10.0, false)).collect();
    grid(b, &hist, &cols, &lines);
    db_labels(b, root, &hist, (hx, hy), &range, 12.0, false);
    let look = Look {
        line: Some(("#c5d0de99", 1.0)),
        fill: Some(("#a5b4c838", "#a5b4c805")),
        glow: None,
        dash: false,
    };
    let (node, slots) = trace(b, &hist, "input level", look);
    let c = Control::new(ControlKind::Plot {
        style: PlotStyle::Line,
        x: Axis::Time { len },
        y: range.clone(),
    })
    .bound(Binding::stream(&in_history));
    b.controls.insert(node, with(c, slots));
    let (node, mut slots) = gr_trace(b, &hist, "#ff6b6b");
    slots.push((SlotRole::Value, gr_readout(b, &hist, node)));
    let gr_range = PlotRange::linear(-opt_f(p, "gr_range", 24.0), 0.0);
    let c = Control::new(ControlKind::Plot {
        style: PlotStyle::Area { baseline: 0.0 },
        x: Axis::Time { len },
        y: gr_range,
    })
    .bound(Binding::stream(&gr))
    .formatted(ValueFormat::decimals(1));
    b.controls.insert(node, with(c, slots));
    label(
        b,
        hist.area,
        "GAIN REDUCTION",
        (6.0, 9.0),
        TextAlign::Left,
        "#ff8787",
    );
    root
}

fn with(mut c: Control, slots: Vec<(SlotRole, NodeId)>) -> Control {
    for (role, id) in slots {
        c = c.slot(role, id.0);
    }
    c
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "viz.comp",
        family: Family::Viz,
        title: "Compressor",
        description: "A compressor visualizer: a scrolling history of the input level (grey fill) with the gain reduction hanging from the top in glowing red, and beside it the transfer curve (soft knee, ratio, makeup) with a dashed threshold line and chip to drag (wheel = ratio) and an orange ball riding the curve at the input level. Binds `<ns>/{threshold,ratio,knee,makeup}` and feeds `<ns>/in` (the ball), `<ns>/in_history`, `<ns>/gr_history` (`ns=`, `in=`, `in_history=`, `gr_history=`, `no_knee`, `no_makeup`, `gr_range=`, `len=`, `h=`).",
        variants: &[],
        default_size: 600.0,
        binds: "params by namespace",
        build,
    }]
}
