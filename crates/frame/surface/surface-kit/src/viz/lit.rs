//! The lit visualizers — processor's panel shaders as frame components:
//! `viz.comp_scope`, `viz.delay_taps`, `viz.reverb_space`, `viz.mod_lfo`
//! and the `viz.eq_glow` layer.
//!
//! Each is a `light` frame carrying a built-in visualizer shader
//! (`viz.comp`, `viz.delay`, `viz.reverb`, `viz.modulation`, `eq.glow` —
//! render's `fx/wgsl/viz/`) whose inputs bind the component's params and
//! feeds. On a GPU renderer the shader draws the picture; everywhere else
//! the light's subtree — processor's vector fallback (`paint_comp`,
//! `paint_delay`, `paint_reverb`, `paint_mod`, the graph's own analyser),
//! rebuilt from `Plot` / `Curve` controls — draws instead. What processor
//! draws as geometry on top of its shader sits after the light, in both
//! paths (the compressor's threshold), or is drawn by the shader itself (a
//! time lane's beat grid, whose window differs between processor's two
//! painters).
//!
//! Params and feeds by namespace (`options: ns=<ns>`, default the place's
//! name lowercased); [`super::lit_catalog`] builds them.
//!
//! | component | params | feeds |
//! |---|---|---|
//! | `viz.comp_scope` | `<ns>/{threshold,ratio,knee}` (`on` optional) | `<ns>/in`, `<ns>/gr`, `<ns>/in_history`, `<ns>/gr_history` |
//! | `viz.delay_taps` | `<ns>/{time,feedback,mix,tap_div_l,tap_div_r,family,bpm,on}` | — |
//! | `viz.reverb_space` | `<ns>/{decay,predelay,density,damp,mix,family,bpm,on}` | — |
//! | `viz.mod_lfo` | `<ns>/{rate,depth,mix,engine,on}` | — |
//! | `viz.eq_glow` | `<ns>/b{n}_{freq,gain,on}` | `<ns>/spectrum` |
//!
//! `color=#hex` sets the lane colour (processor's defaults otherwise),
//! `h=` the height.

use document_proto::{Node, NodeId, NodeKind, Paint, ShaderFx, ShaderInput, Size, Stroke, StrokeAlign, StrokeCap, StrokeJoin};
use surface_proto::{Axis, Binding, Control, ControlKind, CurveInput, HandleAxis, PlotRange, PlotStyle, SlotRole};
use surface_widgets::Builder;

use super::{ns, opt_f};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// The lit components' ids.
pub const IDS: [&str; 5] = ["viz.comp_scope", "viz.delay_taps", "viz.reverb_space", "viz.mod_lfo", "viz.eq_glow"];

/// processor's rack ground, under every lane (`#080808`).
pub const RACK: &str = "#080808";
/// Trace resolution the shaders read (processor's `TRACE_LEN`).
pub const TRACE_LEN: u32 = 128;
/// Analyser bins the glow reads (processor's `MAX_BINS`).
pub const GLOW_BINS: u32 = 128;
/// Bands the glow can bind (processor's `MAX_NODES`).
pub const GLOW_BANDS: usize = 24;

/// Default lane colours: the compressor's input grey, delay blue, reverb
/// violet, modulation cyan.
pub const COMP_COLOR: [u8; 3] = [228, 228, 231];
pub const DELAY_COLOR: [u8; 3] = [59, 130, 246];
pub const REVERB_COLOR: [u8; 3] = [167, 139, 250];
pub const MOD_COLOR: [u8; 3] = [34, 211, 238];

// ── colour ───────────────────────────────────────────────────────────────

fn rgba([r, g, b]: [u8; 3], a: f64) -> String {
    format!("#{r:02x}{g:02x}{b:02x}{:02x}", (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// `#rrggbb` → bytes (`fallback` when it doesn't parse).
fn parse_rgb(s: Option<&str>, fallback: [u8; 3]) -> [u8; 3] {
    let Some(h) = s.map(|s| s.trim_start_matches('#')).filter(|h| h.len() >= 6) else { return fallback };
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    match (byte(0), byte(2), byte(4)) {
        (Some(r), Some(g), Some(b)) => [r, g, b],
        _ => fallback,
    }
}

/// processor's `lane::palette`: the lane colour and a highlight lifted
/// toward white by `lift` (0.55 for lanes, 0.58 for modulation).
fn palette(c: [u8; 3], lift: f64) -> ([u8; 3], [u8; 3]) {
    let up = |v: u8| (f64::from(v) * (1.0 - lift) + 255.0 * lift) as u8;
    (c, [up(c[0]), up(c[1]), up(c[2])])
}

/// The shader's `tint_r/g/b` params.
fn tint_params(c: [u8; 3]) -> std::collections::BTreeMap<String, f64> {
    [("tint_r", c[0]), ("tint_g", c[1]), ("tint_b", c[2])].into_iter().map(|(k, v)| (k.to_string(), f64::from(v) / 255.0)).collect()
}

// ── parts ────────────────────────────────────────────────────────────────

/// The component's root: `w × h`, clipped, on the rack's ground.
fn root(b: &mut Builder<'_>, parent: NodeId, name: &str, w: f64, h: f64, ground: &str) -> NodeId {
    let mut n = draw::frame(name, 0.0, 0.0, w, h);
    n.fills = vec![draw::solid(ground)];
    n.clips_content = true;
    b.add(parent, n)
}

/// The light: a `w × h` frame running `shader` over the vector fallback its
/// children draw.
fn light(b: &mut Builder<'_>, parent: NodeId, w: f64, h: f64, shader: &str, params: std::collections::BTreeMap<String, f64>, inputs: Vec<ShaderInput>) -> NodeId {
    let mut n = draw::frame("light", 0.0, 0.0, w, h);
    n.shader = Some(ShaderFx { shader: shader.into(), params, inputs });
    b.add(parent, n)
}

/// How a unit-space curve slot draws.
#[derive(Clone)]
enum Ink {
    /// A stroked line.
    Line { color: String, weight: f64, dash: bool },
    /// The region between the curve and `baseline` (unit `y`).
    Fill { paint: Paint, baseline: f64 },
    /// A filled dot of radius `r` per point.
    Dots { color: String, r: f64 },
}

/// A `w × h` frame drawing one series of a curve provider (unit axes) in
/// `ink`, registered as its `Curve` control.
#[allow(clippy::too_many_arguments)]
fn curve(b: &mut Builder<'_>, parent: NodeId, (w, h): (f64, f64), name: &str, provider: &str, inputs: &[CurveInput], series: &str, ink: Ink) -> NodeId {
    let frame = b.add(parent, draw::frame(name, 0.0, 0.0, w, h));
    let mut n = Node::new(NodeKind::Vector, "line");
    n.size = Size { width: w, height: h };
    let (style, role) = match &ink {
        Ink::Line { color, weight, dash } => {
            n.stroke = Stroke {
                paints: vec![draw::solid(color)],
                weight: *weight,
                cap: StrokeCap::None,
                join: StrokeJoin::Round,
                align: StrokeAlign::Center,
                dash_pattern: if *dash { vec![4.0, 3.0] } else { Vec::new() },
                ..Stroke::default()
            };
            (PlotStyle::Line, SlotRole::Trace)
        }
        Ink::Fill { paint, baseline } => {
            n.name = "fill".into();
            n.fills = vec![paint.clone()];
            (PlotStyle::Area { baseline: *baseline }, SlotRole::Area)
        }
        Ink::Dots { color, r } => {
            n.name = "dots".into();
            n.fills = vec![draw::solid(color)];
            // A dot's radius is the stroke weight × 1.2 (unpainted).
            n.stroke = Stroke { paints: Vec::new(), weight: r / 1.2, align: StrokeAlign::Center, ..Stroke::default() };
            (PlotStyle::Dots, SlotRole::Trace)
        }
    };
    let slot = b.add(frame, n);
    let kind = ControlKind::Curve {
        provider: provider.into(),
        inputs: inputs.to_vec(),
        series: Some(series.into()),
        style,
        x: Axis::Linear { min: 0.0, max: 1.0 },
        y: PlotRange::linear(0.0, 1.0),
    };
    b.controls.insert(frame, Control::new(kind).slot(role, slot.0));
    frame
}

/// A rule `x0,y0 → x1,y1` (node space) of `weight` in `color`.
fn rule(b: &mut Builder<'_>, parent: NodeId, (w, h): (f64, f64), name: &str, d: String, color: &str, weight: f64) -> NodeId {
    let mut n = draw::path(name, 0.0, 0.0, w, h, d, draw::solid("#00000000"));
    n.fills.clear();
    n.stroke = Stroke { paints: vec![draw::solid(color)], weight, align: StrokeAlign::Center, ..Stroke::default() };
    b.add(parent, n)
}

/// processor's `lane::beats`: a line per beat, the bar lines brighter, from
/// `provider`'s `beats` / `bars` series. (The lit shaders rule their own.)
fn beat_grid(b: &mut Builder<'_>, parent: NodeId, size: (f64, f64), provider: &str, inputs: &[CurveInput]) {
    let ink = |a: f64, weight: f64| Ink::Line { color: rgba([148, 163, 184], a), weight, dash: false };
    curve(b, parent, size, "beats", provider, inputs, "beats", ink(0.22, 1.0));
    curve(b, parent, size, "bars", provider, inputs, "bars", ink(0.46, 1.5));
}

fn param_input(ns: &str, name: &str, key: &str) -> CurveInput {
    CurveInput::param(name, format!("{ns}/{key}"))
}

fn has_opt_param(p: &Place, key: &str) -> bool {
    !p.flag(&format!("no_{key}"))
}

// ── viz.comp_scope ───────────────────────────────────────────────────────

fn comp_scope(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let ns = ns(p);
    let w = p.size.unwrap_or(620.0).max(120.0);
    let h = opt_f(p, "h", (w * 0.84).round());
    let range = opt_f(p, "range", 60.0);
    let color = parse_rgb(p.opt("color"), COMP_COLOR);
    let (signal, accent) = palette(color, 0.55);
    let addr = |k: &str| format!("{ns}/{k}");
    let feed = |k: &str| p.opt(k).map_or_else(|| addr(k), ToString::to_string);
    let root = root(b, parent, &p.name, w, h, RACK);

    let mut inputs = vec![
        ShaderInput::param("threshold", addr("threshold")),
        ShaderInput::param("ratio", addr("ratio")),
        ShaderInput::param("knee", addr("knee")),
        ShaderInput::stream("in_db", feed("in")),
        // Gain reduction arrives ≤ 0 dB; the shader reads it positive.
        ShaderInput::stream("gr_db", feed("gr")).map(0.0, -1.0),
        ShaderInput::stream("input", feed("in_history")).map(-range, 0.0).len(TRACE_LEN),
        ShaderInput::stream("gr", feed("gr_history")).map(0.0, -range).len(TRACE_LEN),
    ];
    if let Some(m) = inputs[4].map.as_mut() {
        m.unclamped = true;
    }
    if p.flag("on") {
        inputs.push(ShaderInput::param("on", addr("on")));
    }
    let mut params = tint_params(color);
    params.insert("p0".into(), range);
    let light = light(b, root, w, h, "viz.comp", params, inputs);

    // processor's `paint_comp`: the input rising from the floor, the gain
    // reduction hanging from the ceiling, the curve, the ball.
    let db = PlotRange::linear(-range, 0.0);
    let len = opt_f(p, "len", 300.0) as u32;
    let plot = |b: &mut Builder<'_>, name: &str, stream: String, baseline: f64, fill: String| {
        let frame = b.add(light, draw::frame(name, 0.0, 0.0, w, h));
        let mut n = Node::new(NodeKind::Vector, "fill");
        n.size = Size { width: w, height: h };
        n.fills = vec![draw::solid(&fill)];
        let slot = b.add(frame, n);
        let c = Control::new(ControlKind::Plot { style: PlotStyle::Area { baseline }, x: Axis::Time { len }, y: db.clone() })
            .bound(Binding::stream(stream))
            .slot(SlotRole::Area, slot.0);
        b.controls.insert(frame, c);
    };
    plot(b, "input", feed("in_history"), -range, rgba(signal, 0.22));
    plot(b, "gain reduction", feed("gr_history"), 0.0, rgba([239, 68, 68], 0.30));

    let transfer = vec![
        CurveInput::param("threshold", addr("threshold")),
        CurveInput::param("ratio", addr("ratio")),
        CurveInput::param("knee", addr("knee")),
        CurveInput::constant("makeup", 0.0),
        CurveInput::stream("marker", feed("in")),
    ];
    let frame = b.add(light, draw::frame("transfer", 0.0, 0.0, w, h));
    let mut line = Node::new(NodeKind::Vector, "line");
    line.size = Size { width: w, height: h };
    line.stroke = Stroke { paints: vec![draw::solid(&rgba(accent, 0.9))], weight: 1.5, join: StrokeJoin::Round, align: StrokeAlign::Center, ..Stroke::default() };
    let line = b.add(frame, line);
    let ball = b.add(frame, draw::frame("ball", w / 2.0 - 3.0, h / 2.0 - 3.0, 6.0, 6.0));
    b.add(ball, draw::circle("dot", 3.0, 3.0, 3.0, draw::solid(&rgba(accent, 0.95))));
    let axis = Axis::Linear { min: -range, max: 0.0 };
    let kind = ControlKind::Curve { provider: "comp.transfer".into(), inputs: transfer, series: Some("transfer".into()), style: PlotStyle::Line, x: axis, y: db };
    b.controls.insert(frame, Control::new(kind).slot(SlotRole::Trace, line.0).slot(SlotRole::Marker, ball.0));

    // The threshold: geometry in both paths, and what you drag.
    let touch = 44.0;
    let handle = b.add(root, draw::frame("threshold", 0.0, 0.0, w, touch));
    rule(b, handle, (w, touch), "line", format!("M 0 {0:.1} H {w:.1}", touch / 2.0), &rgba(signal, 0.55), 1.0);
    let kind = ControlKind::Handle {
        x: None,
        y: Some(Binding::param(addr("threshold"))),
        wheel: Some(Binding::param(addr("ratio"))),
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear { min: -range, max: 0.0 }),
    };
    b.controls.insert(handle, Control::new(kind));
    root
}

// ── viz.delay_taps ───────────────────────────────────────────────────────

fn delay_taps(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let ns = ns(p);
    let w = p.size.unwrap_or(760.0).max(120.0);
    let h = opt_f(p, "h", (w * 0.25).round());
    let color = parse_rgb(p.opt("color"), DELAY_COLOR);
    let (signal, accent) = palette(color, 0.55);
    let size = (w, h);
    let root = root(b, parent, &p.name, w, h, RACK);
    let taps: Vec<CurveInput> = vec![
        param_input(&ns, "delay_ms", "time"),
        param_input(&ns, "feedback", "feedback"),
        param_input(&ns, "mix", "mix"),
        param_input(&ns, "tap_div_l", "tap_div_l"),
        param_input(&ns, "tap_div_r", "tap_div_r"),
        param_input(&ns, "bpm", "bpm"),
    ]
    .into_iter()
    .chain(has_opt_param(p, "on").then(|| param_input(&ns, "on", "on")))
    .collect();
    let tap_input = |name: &str, series: &str| ShaderInput::curve(name, "delay.taps", taps.clone(), Some(series));
    let mut inputs = vec![tap_input("left", "left"), tap_input("right", "right"), tap_input("lane", "lane"), ShaderInput::param("family", format!("{ns}/family"))];
    if has_opt_param(p, "on") {
        inputs.push(ShaderInput::param("on", format!("{ns}/on")));
    }
    let light = light(b, root, w, h, "viz.delay", tint_params(color), inputs);

    // processor's `paint_delay`.
    let mut ground = draw::frame("ground", 0.0, 0.0, w, h);
    ground.fills = vec![draw::linear(90.0, &[(0.0, &rgba(signal, 0.10)), (1.0, "#00000000")])];
    b.add(light, ground);
    beat_grid(b, light, size, "delay.taps", &taps);
    let mid = h * 0.5;
    rule(b, light, size, "rule", format!("M 0 {mid:.2} H {w:.2}"), &rgba(signal, 0.22), 1.0);
    b.add(light, draw::rect("dry", 0.0, mid - h * 0.42, 2.0, h * 0.84, draw::solid(&rgba(signal, 0.9))));
    let env = draw::linear(90.0, &[(0.0, &rgba(accent, 0.34)), (1.0, &rgba(accent, 0.02))]);
    for series in ["env_up", "env_down"] {
        curve(b, light, size, series, "delay.taps", &taps, series, Ink::Fill { paint: env.clone(), baseline: 0.5 });
    }
    // Three loudness buckets: bar alpha 0.35 + 0.65·s, cap 0.5 + 0.5·s,
    // cap radius 1.5 + 2·s, at each bucket's middle s.
    curve(b, light, size, "blooms", "delay.taps", &taps, "blooms", Ink::Dots { color: rgba(accent, 0.18), r: 11.0 });
    for (i, s) in [1.0 / 6.0, 0.5, 5.0 / 6.0].into_iter().enumerate() {
        let ink = Ink::Line { color: rgba(signal, 0.35 + 0.65 * s), weight: 2.0, dash: false };
        curve(b, light, size, &format!("stems {i}"), "delay.taps", &taps, &format!("stems_{i}"), ink);
    }
    for (i, s) in [1.0 / 6.0, 0.5, 5.0 / 6.0].into_iter().enumerate() {
        let ink = Ink::Dots { color: rgba(signal, 0.5 + 0.5 * s), r: 1.5 + 2.0 * s };
        curve(b, light, size, &format!("caps {i}"), "delay.taps", &taps, &format!("caps_{i}"), ink);
    }
    // The sweep: its trail (a gradient, in slices) and its edge.
    for j in 0..6 {
        let a = 0.16 * (j as f64 + 0.5) / 6.0;
        let ink = Ink::Fill { paint: draw::solid(&rgba(signal, a)), baseline: 0.0 };
        curve(b, light, size, &format!("trail {j}"), "delay.taps", &taps, &format!("trail_{j}"), ink);
    }
    curve(b, light, size, "sweep", "delay.taps", &taps, "head", Ink::Line { color: rgba(signal, 0.5), weight: 1.0, dash: false });
    root
}

// ── viz.reverb_space ─────────────────────────────────────────────────────

fn reverb_space(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let ns = ns(p);
    let w = p.size.unwrap_or(760.0).max(120.0);
    let h = opt_f(p, "h", (w * 0.25).round());
    let color = parse_rgb(p.opt("color"), REVERB_COLOR);
    let (signal, accent) = palette(color, 0.55);
    let size = (w, h);
    let root = root(b, parent, &p.name, w, h, RACK);
    let tail: Vec<CurveInput> = ["decay", "predelay", "density", "bpm"]
        .into_iter()
        .map(|k| param_input(&ns, k, k))
        .chain(has_opt_param(p, "on").then(|| param_input(&ns, "on", "on")))
        .collect();
    let mut inputs: Vec<ShaderInput> =
        ["decay", "predelay", "density", "damp", "mix", "bpm", "family"].into_iter().map(|k| ShaderInput::param(k, format!("{ns}/{k}"))).collect();
    if has_opt_param(p, "on") {
        inputs.push(ShaderInput::param("on", format!("{ns}/on")));
    }
    let light = light(b, root, w, h, "viz.reverb", tint_params(color), inputs);

    // processor's `paint_reverb`.
    let mut ground = draw::frame("ground", 0.0, 0.0, w, h);
    ground.fills = vec![draw::linear(180.0, &[(0.0, &rgba(signal, 0.12)), (1.0, "#00000000")])];
    b.add(light, ground);
    beat_grid(b, light, size, "reverb.tail", &tail);
    let env = draw::linear(0.0, &[(0.0, &rgba(signal, 0.06)), (1.0, &rgba(signal, 0.46))]);
    curve(b, light, size, "tail", "reverb.tail", &tail, "env", Ink::Fill { paint: env, baseline: 0.0 });
    for (i, v) in [1.0 / 6.0, 0.5, 5.0 / 6.0].into_iter().enumerate() {
        let ink = Ink::Line { color: rgba(accent, 0.5 * v), weight: 1.0, dash: false };
        curve(b, light, size, &format!("reflections {i}"), "reverb.tail", &tail, &format!("refl_{i}"), ink);
    }
    // The shimmer: its band (a gradient behind the head, in slices) and the
    // head, by brightness (`lit` at each bucket's middle).
    for j in 0..4 {
        let a = 0.26 * 0.5 * (j as f64 + 0.5) / 4.0;
        let ink = Ink::Fill { paint: draw::solid(&rgba(signal, a)), baseline: 0.0 };
        curve(b, light, size, &format!("shimmer band {j}"), "reverb.tail", &tail, &format!("band_{j}"), ink);
    }
    for (i, lit) in [1.0 / 6.0, 0.5, 5.0 / 6.0].into_iter().enumerate() {
        let ink = Ink::Dots { color: rgba(accent, 0.55 * lit), r: 2.0 + 5.0 * lit };
        curve(b, light, size, &format!("shimmer {i}"), "reverb.tail", &tail, &format!("shimmer_{i}"), ink);
    }
    curve(b, light, size, "pre-delay", "reverb.tail", &tail, "pre", Ink::Fill { paint: draw::solid(&rgba(signal, 0.07)), baseline: 0.0 });
    root
}

// ── viz.mod_lfo ──────────────────────────────────────────────────────────

fn mod_lfo(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let ns = ns(p);
    let w = p.size.unwrap_or(760.0).max(120.0);
    let h = opt_f(p, "h", (w * 0.29).round());
    let color = parse_rgb(p.opt("color"), MOD_COLOR);
    let (base, bright) = palette(color, 0.58);
    let size = (w, h);
    let root = root(b, parent, &p.name, w, h, RACK);
    let lfo: Vec<CurveInput> = ["rate", "depth", "engine"]
        .into_iter()
        .map(|k| param_input(&ns, k, k))
        .chain(has_opt_param(p, "on").then(|| param_input(&ns, "on", "on")))
        .collect();
    let mut inputs: Vec<ShaderInput> = ["rate", "depth", "mix", "engine"].into_iter().map(|k| ShaderInput::param(k, format!("{ns}/{k}"))).collect();
    if has_opt_param(p, "on") {
        inputs.push(ShaderInput::param("on", format!("{ns}/on")));
    }
    let light = light(b, root, w, h, "viz.modulation", tint_params(color), inputs);

    // processor's `paint_mod`: the ground, then whichever engine it is.
    let mut ground = draw::frame("ground", 0.0, 0.0, w, h);
    ground.fills = vec![draw::linear(180.0, &[(0.0, &rgba(base, 0.10)), (1.0, &rgba(base, 0.01))])];
    b.add(light, ground);
    let fill = |c: [u8; 3], a: f64| Ink::Fill { paint: draw::solid(&rgba(c, a)), baseline: 0.5 };
    let bars = |c: [u8; 3], a: f64| Ink::Fill { paint: draw::solid(&rgba(c, a)), baseline: 0.0 };
    let slots: [(&str, Ink); 15] = [
        ("fill_up", fill(base, 0.16)),
        ("fill_down", fill(base, 0.16)),
        ("teeth_lo", bars(base, 0.35)),
        ("teeth_mid", bars(base, 0.6)),
        ("teeth_hi", bars(bright, 0.8)),
        ("grid", Ink::Line { color: rgba(base, 0.22), weight: 1.0, dash: false }),
        ("faint", Ink::Line { color: rgba(base, 0.42), weight: 1.2, dash: false }),
        ("bright", Ink::Line { color: rgba(bright, 0.88), weight: 1.8, dash: false }),
        ("thin", Ink::Line { color: rgba(bright, 0.85), weight: 1.4, dash: false }),
        ("mid", Ink::Line { color: rgba(bright, 0.9), weight: 1.6, dash: false }),
        ("dots_base", Ink::Dots { color: rgba(base, 0.5), r: 2.0 }),
        ("trail_0", Ink::Dots { color: rgba(base, 0.5 / 6.0), r: 2.0 }),
        ("trail_1", Ink::Dots { color: rgba(base, 0.25), r: 2.8 }),
        ("trail_2", Ink::Dots { color: rgba(base, 2.5 / 6.0), r: 3.4 }),
        ("dots_bright", Ink::Dots { color: rgba(bright, 0.95), r: 4.5 }),
    ];
    for (series, ink) in slots {
        curve(b, light, size, series, "mod.lfo", &lfo, series, ink);
    }
    root
}

// ── viz.eq_glow ──────────────────────────────────────────────────────────

/// The graph span an EQ glow lays its halos on.
pub(crate) enum GlowSpan {
    /// ± this many dB.
    Fixed(f64),
    /// A zoom param (a choice) picking from these spans.
    Param { addr: String, spans: Vec<f64> },
}

/// What an EQ's glow reads.
pub(crate) struct GlowSpec<'a> {
    pub ns: &'a str,
    pub bands: usize,
    /// Bind `b{n}_used` too (signal's wire family has it).
    pub used: bool,
    /// Bind `b{n}_on` (a band that can be bypassed).
    pub bypass: bool,
    pub spectrum: String,
    pub f: (f64, f64),
    pub span: GlowSpan,
    pub floor: f64,
    /// One colour (`#rrggbb`) for every band's bloom and the analyser, in
    /// place of the frequency sweep: the shader's `tint_r/g/b`.
    pub tint: Option<String>,
}

/// A `w × h` light running `eq.glow` fed by an EQ's spectrum feed and band
/// params: processor's EQ light. Its children are the vector fallback (an
/// EQ graph puts its analyser there — the glow draws the analyser as light,
/// as processor's graph does).
pub(crate) fn glow_light(b: &mut Builder<'_>, parent: NodeId, (w, h): (f64, f64), spec: &GlowSpec<'_>) -> NodeId {
    let keys: &[&str] = match (spec.used, spec.bypass) {
        (true, true) => &["freq", "gain", "on", "used"],
        (true, false) => &["freq", "gain", "used"],
        (false, true) => &["freq", "gain", "on"],
        (false, false) => &["freq", "gain"],
    };
    let band_inputs: Vec<CurveInput> =
        (1..=spec.bands.min(GLOW_BANDS)).flat_map(|n| keys.iter().map(move |k| CurveInput::param(format!("b{n}_{k}"), format!("{}/b{n}_{k}", spec.ns)))).collect();
    let mut inputs = vec![
        ShaderInput::stream("spectrum", spec.spectrum.clone()).len(GLOW_BINS),
        ShaderInput::curve("bands", "eq.bands", band_inputs, Some("bands")),
        ShaderInput::constant("floor", vec![spec.floor]),
    ];
    match &spec.span {
        GlowSpan::Fixed(r) => inputs.push(ShaderInput::constant("db_range", vec![*r])),
        GlowSpan::Param { addr, spans } => {
            inputs.push(ShaderInput::param("range", addr.clone()));
            inputs.push(ShaderInput::constant("ranges", spans.clone()));
        }
    }
    let mut params: std::collections::BTreeMap<String, f64> = [("p0", spec.f.0), ("p1", spec.f.1)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    if let Some(rgb) = spec.tint.as_deref().and_then(hex_rgb) {
        for (k, v) in ["tint_r", "tint_g", "tint_b"].into_iter().zip(rgb) {
            params.insert(k.to_string(), v);
        }
    }
    light(b, parent, w, h, "eq.glow", params, inputs)
}

/// `#rrggbb` as 0..1 channels.
fn hex_rgb(hex: &str) -> Option<[f64; 3]> {
    let h = hex.trim_start_matches('#');
    let c = |i: usize| h.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok()).map(|v| f64::from(v) / 255.0);
    Some([c(0)?, c(2)?, c(4)?])
}

fn eq_glow(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let ns = ns(p);
    let w = p.size.unwrap_or(800.0).max(120.0);
    let h = opt_f(p, "h", (w * 0.42).round());
    let bands = (opt_f(p, "bands", 8.0) as usize).clamp(1, GLOW_BANDS);
    let range = opt_f(p, "db_range", 24.0);
    let (f_min, f_max) = (opt_f(p, "f_min", 20.0), opt_f(p, "f_max", 20_000.0));
    let spectrum = p.opt("spectrum").map_or_else(|| format!("{ns}/spectrum"), ToString::to_string);
    let root = b.add(parent, {
        let mut n = draw::frame(&p.name, 0.0, 0.0, w, h);
        if p.flag("ground") {
            n.fills = vec![draw::solid("#0a0a0a")];
        }
        n.clips_content = true;
        n
    });
    let spec = GlowSpec {
        ns: &ns,
        bands,
        used: !p.flag("no_used"),
        bypass: true,
        spectrum: spectrum.clone(),
        f: (f_min, f_max),
        span: GlowSpan::Fixed(range),
        floor: opt_f(p, "floor", -90.0),
        tint: None,
    };
    let light = glow_light(b, root, (w, h), &spec);

    // The graph's own analyser (`paint_spectrum`): the glow draws it as
    // light; without the stage, this.
    let frame = b.add(light, draw::frame("analyser", 0.0, 0.0, w, h));
    let mut fill = Node::new(NodeKind::Vector, "fill");
    fill.size = Size { width: w, height: h };
    fill.fills = vec![draw::solid(&rgba([100, 180, 255], 20.0 / 255.0))];
    let fill = b.add(frame, fill);
    let mut line = Node::new(NodeKind::Vector, "line");
    line.size = Size { width: w, height: h };
    line.stroke = Stroke { paints: vec![draw::solid(&rgba([100, 180, 255], 90.0 / 255.0))], weight: 1.0, join: StrokeJoin::Round, align: StrokeAlign::Center, ..Stroke::default() };
    let line = b.add(frame, line);
    let c = Control::new(ControlKind::Plot { style: PlotStyle::Area { baseline: -range }, x: Axis::Log { f_min, f_max }, y: PlotRange::linear(-range, range) })
        .bound(Binding::stream(spectrum))
        .slot(SlotRole::Area, fill.0)
        .slot(SlotRole::Trace, line.0);
    b.controls.insert(frame, c);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "viz.comp_scope",
            family: Family::Viz,
            title: "Compressor scope (lit)",
            description: "processor's compressor panel: the input level rising from the floor in white-grey light, the gain reduction hanging from the ceiling in red, the soft-knee transfer curve glowing across it with a ball where the signal sits, and a threshold rule to drag (wheel = ratio). GPU shader `viz.comp`, vector fallback otherwise. Binds `<ns>/{threshold,ratio,knee}` and feeds `<ns>/in`, `<ns>/gr`, `<ns>/in_history`, `<ns>/gr_history` (`ns=`, `color=#hex`, `range=` dB, `on` binds `<ns>/on`, `h=`).",
            variants: &[],
            default_size: 620.0,
            binds: "params by namespace",
            build: comp_scope,
        },
        ComponentDef {
            id: "viz.delay_taps",
            family: Family::Viz,
            title: "Delay taps (lit)",
            description: "processor's delay lane: a deep blue field with each repeat as a lit bar above (left) or below (right) the channel rule, the tail's haze, a playhead sweeping the beat grid and the machine's own marks (tape wow, bucket-brigade hiss, pitch ladders…). Taps follow time, feedback, tap divisions and tempo. GPU shader `viz.delay`, vector fallback otherwise. Binds `<ns>/{time,feedback,mix,tap_div_l,tap_div_r,family,bpm,on}` (`ns=`, `color=#hex`, `no_on`, `h=`).",
            variants: &[],
            default_size: 760.0,
            binds: "params by namespace",
            build: delay_taps,
        },
        ComponentDef {
            id: "viz.reverb_space",
            family: Family::Viz,
            title: "Reverb space (lit)",
            description: "processor's reverb lane: a violet field with the pre-delay gap, early reflections as sparks, the tail as a narrowing haze with a bright (damped) core, and each family's signature (room flutter, plate ringing, spring chirps, velvet grain, a gate's cliff). GPU shader `viz.reverb`, vector fallback otherwise. Binds `<ns>/{decay,predelay,density,damp,mix,family,bpm,on}` (`ns=`, `color=#hex`, `no_on`, `h=`).",
            variants: &[],
            default_size: 760.0,
            binds: "params by namespace",
            build: reverb_space,
        },
        ComponentDef {
            id: "viz.mod_lfo",
            family: Family::Viz,
            title: "Modulation (lit)",
            description: "processor's modulation panel, a picture per engine: a chorus braid, a flanger's sliding comb, a phaser's wandering notches, a tremolo's pumping waveform, a vibrato's stretching wave, a rotary's orbiting horn — moving at the LFO rate. GPU shader `viz.modulation`, vector fallback otherwise. Binds `<ns>/{rate,depth,mix,engine,on}` (`ns=`, `color=#hex` — cyan for modulation, `#f472b6` for motion, `no_on`, `h=`).",
            variants: &[],
            default_size: 760.0,
            binds: "params by namespace",
            build: mod_lfo,
        },
        ComponentDef {
            id: "viz.eq_glow",
            family: Family::Viz,
            title: "EQ glow layer",
            description: "processor's EQ light, as a layer to put under an EQ graph: the analyser as a hue-swept glowing field and a bloom in each band's colour under its node. GPU shader `eq.glow`; without it the graph's plain analyser (a faint blue fill and line). Binds `<ns>/b{n}_{freq,gain,on}` and the `<ns>/spectrum` feed (`ns=`, `bands=`, `db_range=` (±24), `f_min=`, `f_max=`, `floor=`, `spectrum=`, `ground` fills it, `h=`).",
            variants: &[],
            default_size: 800.0,
            binds: "params by namespace",
            build: eq_glow,
        },
    ]
}
