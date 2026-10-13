//! `viz.gate`: a noise gate's picture — the level it keys from scrolling
//! past its threshold, the attenuation hanging from the top while it is
//! shut, and the threshold as a line across to drag.

use crate::document::proto::{NodeId, NodeKind, Stroke, StrokeAlign, TextAlign, TextData};
use crate::surface::proto::{
    Axis, Binding, ButtonMode, Control, ControlKind, HandleAxis, PlotRange, PlotStyle, SlotRole,
    ValueFormat,
};
use crate::surface::widgets::Builder;

use super::eq::HANDLE_BOX;
use super::simple::{db_labels, db_lines, gr_trace};
use super::{LABEL, Look, area, grid, label, ns, opt_f, panel, trace};
use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

fn build(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let ns = ns(p);
    let w = p.size.unwrap_or(300.0).max(140.0);
    let h = opt_f(p, "h", (w * 1.4).round());
    let floor = opt_f(p, "floor", -90.0);
    let addr = |k: &str| format!("{ns}/{k}");
    let in_history = p
        .opt("in_history")
        .map_or_else(|| addr("in_history"), ToString::to_string);
    let shut = p
        .opt("gr_history")
        .map_or_else(|| addr("gr_history"), ToString::to_string);
    let len = opt_f(p, "len", 240.0) as u32;
    let accent = p
        .opt("color")
        .filter(|c| c.starts_with('#'))
        .unwrap_or("#69db7c")
        .to_string();
    let root = panel(b, parent, &p.name, w, h);

    // ── the head: name, the threshold read out, the power button ────────
    let head_h = 34.0;
    let mut title = draw::text(
        "title",
        "GATE",
        12.0,
        9.0,
        80.0,
        13.0,
        "#e6ebf2",
        TextAlign::Left,
    );
    if let NodeKind::Text(TextData {
        font_weight,
        letter_spacing,
        ..
    }) = &mut title.kind
    {
        *font_weight = Some(700.0);
        *letter_spacing = 2.0;
    }
    b.add(root, title);
    let mut value = draw::text(
        "threshold",
        "-60 dB",
        w - 12.0 - 40.0 - 90.0,
        10.0,
        90.0,
        12.0,
        &accent,
        TextAlign::Right,
    );
    if let NodeKind::Text(TextData { font_weight, .. }) = &mut value.kind {
        *font_weight = Some(600.0);
    }
    let value = b.add(root, value);
    let readout = Control::new(ControlKind::Readout)
        .slot(SlotRole::Value, value.0)
        .bound(Binding::param(addr("threshold")))
        .formatted(ValueFormat::decimals(0).unit("dB"));
    b.controls.insert(value, readout);
    // Power: a ring that lights in the accent while the gate is in.
    let power = b.add(root, draw::frame("power", w - 12.0 - 30.0, 3.0, 30.0, 28.0));
    b.add(
        power,
        draw::stroked(
            draw::circle("ring", 15.0, 14.0, 9.0, draw::solid("#0c0f14")),
            "#3a4250",
            1.4,
        ),
    );
    let lit = draw::emissive(
        draw::stroked(
            draw::circle(
                "lit",
                15.0,
                14.0,
                9.0,
                draw::solid(&super::alpha(&accent, 0.25)),
            ),
            &accent,
            1.6,
        ),
        draw::hex(&accent),
        0.6,
        8.0,
    );
    let lit = b.add(power, lit);
    let c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::Led, lit.0)
    .bound(Binding::param(addr("on")));
    b.controls.insert(power, c);

    // ── the history ─────────────────────────────────────────────────────
    let (ax, ay) = (30.0, head_h + 4.0);
    let (aw, ah) = (w - ax - 10.0, h - ay - 12.0);
    let a = area(b, root, ax, ay, aw, ah);
    let range = PlotRange::linear(floor, 0.0);
    let step = if floor < -60.0 { 18.0 } else { 12.0 };
    let cols: Vec<(f64, bool)> = (1..6).map(|i| (f64::from(i) / 6.0, false)).collect();
    grid(b, &a, &cols, &db_lines(&range, step));
    db_labels(b, root, &a, (ax, ay), &range, step, true);
    // The level: a soft region in the accent, its edge bright.
    let fill_top = super::alpha(&accent, 0.32);
    let fill_bottom = super::alpha(&accent, 0.03);
    let edge = super::alpha(&accent, 0.9);
    let look = Look {
        line: Some((&edge, 1.4)),
        fill: Some((&fill_top, &fill_bottom)),
        glow: Some((&accent, 0.4, 5.0)),
        dash: false,
    };
    let (node, slots) = trace(b, &a, "input level", look);
    let c = Control::new(ControlKind::Plot {
        style: PlotStyle::Line,
        x: Axis::Time { len },
        y: range.clone(),
    })
    .bound(Binding::stream(&in_history));
    b.controls.insert(node, with(c, slots));
    // Shut: the attenuation, hanging from the top in red.
    let (node, slots) = gr_trace(b, &a, "#ff6b6b");
    let c = Control::new(ControlKind::Plot {
        style: PlotStyle::Area { baseline: 0.0 },
        x: Axis::Time { len },
        y: PlotRange::linear(-opt_f(p, "gr_range", 60.0), 0.0),
    })
    .bound(Binding::stream(&shut));
    b.controls.insert(node, with(c, slots));

    // The threshold: a dashed line across with a chip to grab at its end.
    let handle = b.add(
        a.area,
        draw::frame("threshold", 0.0, (ah - HANDLE_BOX) / 2.0, aw, HANDLE_BOX),
    );
    let mid = HANDLE_BOX / 2.0;
    let mut line = draw::path(
        "line",
        0.0,
        0.0,
        aw,
        HANDLE_BOX,
        format!("M 0 {mid:.1} H {aw:.1}"),
        draw::solid("#00000000"),
    );
    line.fills.clear();
    line.stroke = Stroke {
        paints: vec![draw::solid("#ffd43bcc")],
        weight: 1.4,
        align: StrokeAlign::Center,
        dash_pattern: vec![5.0, 4.0],
        ..Stroke::default()
    };
    b.add(handle, line);
    let mut chip = draw::frame("chip", aw - 40.0, mid - 8.0, 36.0, 16.0);
    chip.fills = vec![draw::solid("#ffd43b")];
    let chip = b.add(
        handle,
        draw::emissive(draw::rounded(chip, 8.0), draw::hex("#ffd43b"), 0.45, 8.0),
    );
    let mut thr = draw::text(
        "legend",
        "THR",
        0.0,
        3.0,
        36.0,
        8.5,
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
        x: None,
        y: Some(Binding::param(addr("threshold"))),
        wheel: Some(Binding::param(addr("threshold"))),
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear {
            min: floor,
            max: 0.0,
        }),
    };
    b.controls.insert(handle, Control::new(kind));
    label(b, a.area, "SHUT", (6.0, ah - 9.0), TextAlign::Left, LABEL);
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
        id: "viz.gate",
        family: Family::Viz,
        title: "Gate",
        description: "A noise gate visualizer, portrait: the level it keys from scrolling past as a soft region in the accent, the attenuation hanging from the top in red while it is shut, and the threshold as a dashed line across with a chip to drag (wheel = fine). A head carries the name, the threshold in dB and a power ring. Binds `<ns>/{threshold,on}` and feeds `<ns>/in_history`, `<ns>/gr_history` (`ns=`, `in_history=`, `gr_history=`, `floor=` (dB, default −90), `gr_range=`, `len=`, `color=`, `h=`).",
        variants: &[],
        default_size: 300.0,
        binds: "params by namespace",
        build,
    }]
}
