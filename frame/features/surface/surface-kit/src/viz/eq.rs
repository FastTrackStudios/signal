//! `viz.adsr`: an envelope curve of params with a handle per point you can
//! grab.

use document_proto::{NodeId, NodeKind, TextAlign, TextData};
use surface_proto::{
    Axis, Binding, Control, ControlKind, CurveInput, HandleAxis, PlotRange, PlotStyle, SlotRole, ValueFormat,
};
use surface_widgets::Builder;

use super::{Area, LABEL, Look, area, dot, grid, label, ns, opt_f, panel, trace};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// Touch box of a handle, px (its disc is smaller; the box is what you hit).
pub(crate) const HANDLE_BOX: f64 = 44.0;

fn curve_control(provider: &str, inputs: Vec<CurveInput>, series: Option<String>, style: PlotStyle, x: Axis, y: PlotRange) -> Control {
    Control::new(ControlKind::Curve { provider: provider.into(), inputs, series, style, x, y })
}

fn with_slots(mut c: Control, slots: Vec<(SlotRole, NodeId)>) -> Control {
    for (role, id) in slots {
        c = c.slot(role, id.0);
    }
    c
}

/// A legend over a value, `w` wide, the value a `Readout` of `addr`.
fn readout(b: &mut Builder<'_>, parent: NodeId, (x, y, w): (f64, f64, f64), legend: &str, addr: &str, color: &str) -> NodeId {
    let root = b.add(parent, draw::frame(&format!("{legend} readout"), x, y, w, 30.0));
    let mut l = draw::text("legend", legend, 0.0, 0.0, w, 8.0, LABEL, TextAlign::Center);
    if let NodeKind::Text(TextData { font_weight, letter_spacing, .. }) = &mut l.kind {
        *font_weight = Some(600.0);
        *letter_spacing = 1.2;
    }
    b.add(root, l);
    let mut v = draw::text("value", "", 0.0, 12.0, w, 12.0, color, TextAlign::Center);
    if let NodeKind::Text(TextData { font_weight, .. }) = &mut v.kind {
        *font_weight = Some(600.0);
    }
    let v = b.add(root, v);
    let c = Control::new(ControlKind::Readout).bound(Binding::param(addr)).slot(SlotRole::Value, v.0).formatted(ValueFormat::default());
    b.controls.insert(root, c);
    root
}

fn adsr(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let ns = ns(p);
    let w = p.size.unwrap_or(480.0).max(200.0);
    let h = opt_f(p, "h", (w * 0.46).round());
    let span = opt_f(p, "time", 4.0).max(0.1);
    let hold = opt_f(p, "hold", 0.6);
    let root = panel(b, parent, &p.name, w, h);
    let a: Area = area(b, root, 12.0, 12.0, w - 24.0, h - 12.0 - 46.0);
    let x = Axis::Linear { min: 0.0, max: span };
    let y = PlotRange::linear(0.0, 1.0);
    let step = if span > 6.0 { 1.0 } else { 0.5 };
    let xs: Vec<(f64, bool)> =
        (1..).map(|i| f64::from(i) * step).take_while(|t| *t < span - 1e-9).map(|t| (x.to_unit(t), (t % 1.0).abs() < 1e-9)).collect();
    let ys: Vec<(f64, bool)> = (1..4).map(|i| (f64::from(i) / 4.0, false)).collect();
    grid(b, &a, &xs, &ys);
    for (u, major) in &xs {
        if *major {
            let t = x.from_unit(*u);
            label(b, a.area, &format!("{t:.0}s"), (u * a.w + 3.0, a.h - 8.0), TextAlign::Left, LABEL);
        }
    }

    let addr = |k: &str| format!("{ns}/{k}");
    let inputs = vec![
        CurveInput::param("attack", addr("attack")),
        CurveInput::param("decay", addr("decay")),
        CurveInput::param("sustain", addr("sustain")),
        CurveInput::param("release", addr("release")),
        CurveInput::constant("hold", hold),
        CurveInput::constant("curve", opt_f(p, "curve", 0.5)),
    ];
    let look = Look { line: Some(("#b2f2e5", 2.2)), fill: Some(("#38d9a959", "#38d9a905")), glow: Some(("#38d9a9", 0.6, 7.0)), dash: false };
    let (node, slots) = trace(b, &a, "envelope", look);
    // Playheads from an `Envelope` feed: a lit dot per sounding voice,
    // riding the curve.
    if let Some(feed) = p.opt("playheads") {
        let voices = opt_f(p, "voices", 4.0).clamp(1.0, 32.0) as u8;
        let mut heads = inputs.clone();
        for v in 0..voices {
            let ch = |k: u8| surface_proto::InputSource::Stream { id: surface_proto::EntryId::new(feed), channel: Some(v * 3 + k) };
            heads.push(CurveInput { name: format!("v{v}_stage"), source: ch(0) });
            heads.push(CurveInput { name: format!("v{v}_pos"), source: ch(1) });
        }
        let frame = b.add(a.clip, draw::frame("playheads", 0.0, 0.0, a.w, a.h));
        let mut dots = document_proto::Node::new(NodeKind::Vector, "dots");
        dots.size = document_proto::Size { width: a.w, height: a.h };
        dots.fills = vec![draw::solid("#ffffff")];
        dots.stroke = document_proto::Stroke { paints: vec![draw::solid("#e6fcf5")], weight: 3.0, ..document_proto::Stroke::default() };
        let dots = b.add(frame, draw::emissive(dots, draw::hex("#63e6be"), 0.9, 8.0));
        let c = curve_control("adsr.envelope", heads, Some("playheads".into()), PlotStyle::Dots, x.clone(), y.clone()).slot(SlotRole::Trace, dots.0);
        b.controls.insert(frame, c);
    }
    b.controls.insert(node, with_slots(curve_control("adsr.envelope", inputs, Some("envelope".into()), PlotStyle::Line, x.clone(), y), slots));

    // A: time across, pinned to the top. D: decay across (after the
    // attack), sustain up. S: the sustain's end, level up. R: release
    // across (after everything), on the floor.
    let level = Axis::Linear { min: 0.0, max: 1.0 };
    let top = HandleAxis::new(level.clone()).after(&[], 1.0);
    // (label, x param, x axis, y param, y axis, colour)
    type Stage<'a> = (&'a str, Option<&'a str>, HandleAxis, Option<&'a str>, HandleAxis, &'a str);
    let handles: [Stage<'_>; 4] = [
        ("A", Some("attack"), HandleAxis::new(x.clone()), None, top, "#ffa94d"),
        ("D", Some("decay"), HandleAxis::new(x.clone()).after(&[&addr("attack")], 0.0), Some("sustain"), HandleAxis::new(level.clone()), "#ffd43b"),
        ("S", None, HandleAxis::new(x.clone()).after(&[&addr("attack"), &addr("decay")], hold), Some("sustain"), HandleAxis::new(level.clone()), "#69db7c"),
        ("R", Some("release"), HandleAxis::new(x.clone()).after(&[&addr("attack"), &addr("decay")], hold), None, HandleAxis::new(level), "#4dabf7"),
    ];
    for (name, xp, x_map, yp, y_map, color) in handles {
        let node = dot(b, a.area, &format!("{name} handle"), HANDLE_BOX, 16.0, color, name, &a);
        if name == "R"
            && let Some(n) = b.scene.node_mut(node)
        {
            // On the floor: the release ends at zero.
            n.transform.ty = a.h - HANDLE_BOX / 2.0;
        }
        let kind = ControlKind::Handle { x: xp.map(|k| Binding::param(addr(k))), y: yp.map(|k| Binding::param(addr(k))), wheel: None, x_map, y_map };
        b.controls.insert(node, Control::new(kind));
    }

    let cw = (w - 24.0) / 4.0;
    for (i, (legend, k, color)) in
        [("ATTACK", "attack", "#ffa94d"), ("DECAY", "decay", "#ffd43b"), ("SUSTAIN", "sustain", "#69db7c"), ("RELEASE", "release", "#4dabf7")].into_iter().enumerate()
    {
        readout(b, root, (12.0 + cw * i as f64, h - 40.0, cw), legend, &addr(k), color);
    }
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "viz.adsr",
            family: Family::Viz,
            title: "ADSR envelope",
            description: "An envelope display: the attack / decay / sustain / release shape as a glowing curve over a timed grid, an A, D, S and R handle to drag (attack and decay time, sustain level, release time), and a readout per stage. Binds `<ns>/{attack,decay,sustain,release}` (`ns=`, `time=` seconds across, `hold=` drawn sustain, `curve=`, `h=`); `playheads=<envelope feed>` (and `voices=`) adds a lit dot per sounding voice.",
            variants: &[],
            default_size: 480.0,
            binds: "params by namespace",
            build: adsr,
        },
    ]
}
