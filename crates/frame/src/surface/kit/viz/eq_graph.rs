//! `viz.eq_graph`: a full parametric EQ editor over an analyser.
//!
//! Everything is ordinary frame nodes and controls, built from the general
//! interaction pieces (`crate::surface::proto::interact`):
//!
//! - one **band template** (`Repeat` over `<ns>/b{n}_*`, shown while
//!   `b{n}_used` is on) holding the band's handle — a disc tinted by its
//!   frequency, grey while bypassed, a hover halo and a selection ring —
//!   and its name label riding above it (a text field renamed in place by
//!   double-clicking the handle); a second template draws each band's own
//!   curve, its stem to 0 dB and its dynamic range;
//! - the plot area is a `Canvas`: a double-click adds a band there (its
//!   shape from where: a low cut at the far left, shelves near the edges, a
//!   notch deep down, a bell elsewhere), a drag draws a marquee, a click
//!   clears the selection, a right-click opens the graph menu;
//! - handles select (Cmd toggles, Shift ranges), drag together, drag out
//!   of the graph to delete; Alt drags frequency only, Cmd drags resonance;
//!   the wheel turns Q (a cut's slope), Cmd the gain, Alt the dynamic range;
//!   Alt-click bypasses, Alt+Cmd-click cycles the shape, Alt+Shift-click the
//!   slope; Delete removes the selection;
//! - one **band panel** follows the active band (a context popover):
//!   bypass, shape, name, stereo placement, focus, delete, and FREQ / GAIN /
//!   Q (or SLOPE) / DYN knobs; a readout chip rides above the band;
//! - a band's right-click menu (bypass, focus, invert gain, shape / slope /
//!   placement submenus, reset gain, rename, delete);
//! - a range switch (±3…±30 dB), pre / post / external analyser layers.
//!
//! Placed inside another unit — a saturator's emphasis EQ — it can be
//! `fixed` (its bands its own: always there, always on, only frequency,
//! gain, Q and shape; nothing to add, delete, bypass or rename), held to
//! the first `shapes=` shapes, at a fixed `db_range=`, in one `tint=`
//! colour, with `no_analyser`.
//!
//! It draws as the EQ in signal's guitar rig does — processor's
//! `EqGraphWidget`: a full-bleed plot on `#0a0a0a`, the GPU glow
//! (`eq.glow`, `viz::lit`) under processor's grid, band curves in each
//! band's colour, the gold sum and the node rings, with the axis labels and
//! range picker where its DOM puts them. The glow draws the analyser as
//! light; without the effects stage the analyser layers draw instead. The
//! `panel` variant keeps the earlier look: a lit panel and well with the
//! scales printed round it, the glow in its `glow` frame under the traces.

use crate::document::proto::{
    Node, NodeId, NodeKind, Stroke, StrokeAlign, StrokeCap, StrokeJoin, TextAlign, TextData,
};
use crate::surface::proto::{
    AddRule, AddZone, Assign, Axis, Binding, ButtonMode, Chord, ClickAction, ClickRule, Command,
    Condition, Control, ControlKind, CurveInput, DragMode, DragRule, HandleAxis, Interact,
    ItemRole, KeyRule, MenuItem, ParamAddr, Placement, PlotRange, PlotStyle, Show, SlotRole,
    Source, ValueFormat, WheelRule, Zoom,
};
use crate::surface::widgets::Builder;

use super::catalog::{PLACEMENTS, RANGES, SHAPES, SLOPES};
use super::{Area, LABEL, area, grid, label, ns, opt_f, panel};
use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

/// A band handle's touch box, px (its disc is 14).
const HANDLE: f64 = 30.0;
const DISC: f64 = 14.0;
const PANEL_W: f64 = 300.0;
const PANEL_H: f64 = 104.0;
const MENU_W: f64 = 184.0;
const SUB_W: f64 = 132.0;
const ROW_H: f64 = 20.0;
const GOLD: &str = "#d4a932";
const PANEL_BG: &str = "#14141a";
const PANEL_RIM: &str = "#2a2a30";
const INK: &str = "#e9ecef";
const DIM: &str = "#8a8a96";

/// Shapes with no gain (a cut, a notch, an all-pass).
const NO_GAIN: [f64; 4] = [3.0, 4.0, 5.0, 9.0];
/// Shapes with a slope (the cuts).
const CUTS: [f64; 2] = [3.0, 4.0];

/// `hsl(t·290°, 68 %, 58 %)` as `#rrggbb`: red lows to violet highs.
fn freq_hex(t: f64) -> String {
    let h = t.clamp(0.0, 1.0) * 290.0;
    let (s, l) = (0.68, 0.58);
    let c = (1.0 - (2.0_f64 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let u = |v: f64| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", u(r), u(g), u(b))
}

/// The frequency colour ramp over an axis `x` (sampled so a position on
/// `x` gets the colour of its frequency on 20 Hz–20 kHz).
fn ramp_over(x: &Axis) -> Vec<String> {
    let band = Axis::Log {
        f_min: 20.0,
        f_max: 20_000.0,
    };
    (0..=16)
        .map(|i| freq_hex(band.to_unit(x.from_unit(f64::from(i) / 16.0))))
        .collect()
}

fn with_slots(mut c: Control, slots: &[(SlotRole, NodeId)]) -> Control {
    for (role, id) in slots {
        c = c.slot(*role, id.0);
    }
    c
}

#[allow(clippy::too_many_arguments)]
fn text(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    s: &str,
    (x, y, w): (f64, f64, f64),
    size: f64,
    color: &str,
    align: TextAlign,
    weight: f64,
) -> NodeId {
    let mut n = draw::text(name, s, x, y, w, size, color, align);
    if let NodeKind::Text(TextData { font_weight, .. }) = &mut n.kind {
        *font_weight = Some(weight);
    }
    b.add(parent, n)
}

fn frame(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    fill: Option<&str>,
    radius: f64,
) -> NodeId {
    let mut n = draw::frame(name, x, y, w, h);
    if let Some(f) = fill {
        n.fills = vec![draw::solid(f)];
    }
    b.add(parent, draw::rounded(n, radius))
}

fn stroked_frame(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    r: (f64, f64, f64, f64),
    fill: &str,
    rim: &str,
    radius: f64,
) -> NodeId {
    let mut n = draw::frame(name, r.0, r.1, r.2, r.3);
    n.fills = vec![draw::solid(fill)];
    b.add(parent, draw::rounded(draw::stroked(n, rim, 1.0), radius))
}

/// A trace vector (`fill`, or a stroked `line`) the size of the area.
fn vector(
    b: &mut Builder<'_>,
    parent: NodeId,
    a: &Area,
    name: &str,
    fill: Option<&str>,
    line: Option<(&str, f64, bool)>,
) -> NodeId {
    let mut n = Node::new(NodeKind::Vector, name);
    n.size = crate::document::proto::Size {
        width: a.w,
        height: a.h,
    };
    if let Some(f) = fill {
        n.fills = vec![draw::solid(f)];
    }
    if let Some((c, w, dash)) = line {
        n.stroke = Stroke {
            paints: vec![draw::solid(c)],
            weight: w,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            align: StrokeAlign::Center,
            dash_pattern: if dash { vec![4.0, 3.0] } else { Vec::new() },
            ..Stroke::default()
        };
    }
    b.add(parent, n)
}

/// The EQ graph's addresses.
struct Addrs {
    ns: String,
}

impl Addrs {
    /// A band param, templated (`<ns>/b{n}_<k>`).
    fn band(&self, k: &str) -> String {
        format!("{}/b{{n}}_{k}", self.ns)
    }

    fn view(&self, k: &str) -> String {
        format!("{}/{k}", self.ns)
    }

    fn zoom(&self) -> Zoom {
        Zoom {
            param: ParamAddr::new(self.view("range")),
            spans: RANGES.to_vec(),
        }
    }
}

/// The inputs `eq.response` reads for one band (`n` literal or `{n}`); a
/// fixed band (`fixed`) has only its frequency, gain, Q and shape.
fn band_inputs(ad: &Addrs, n: &str, fixed: bool) -> Vec<CurveInput> {
    let addr = |k: &str| format!("{}/b{n}_{k}", ad.ns);
    if fixed {
        return ["freq", "gain", "q", "shape"]
            .iter()
            .map(|k| CurveInput::param(format!("b{n}_{k}"), addr(k)))
            .collect();
    }
    let mut v: Vec<CurveInput> = ["freq", "gain", "q", "shape", "on", "used", "dyn_range"]
        .iter()
        .map(|k| CurveInput::param(format!("b{n}_{k}"), addr(k)))
        .collect();
    v.push(CurveInput::param(
        format!("b{n}_slope_index"),
        addr("slope"),
    ));
    v
}

fn eq_graph(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let ad = Addrs { ns: ns(p) };
    let w = p.size.unwrap_or(820.0).max(360.0);
    let h = opt_f(p, "h", (w * 0.42).round());
    let bands = opt_f(p, "bands", 24.0).clamp(1.0, 24.0) as u32;
    // The panel look; else signal's rig look (processor's graph widget).
    let lit_panel = p.variant.as_deref() == Some("panel");
    let rig = !lit_panel;
    let f = (opt_f(p, "f_min", 20.0), opt_f(p, "f_max", 20_000.0));
    let (root, (ax, ay, right, bottom)) = if rig {
        let mut n = draw::frame(&p.name, 0.0, 0.0, w, h);
        n.fills = vec![draw::solid(RIG_GROUND)];
        (b.add(parent, n), (0.0, 0.0, 0.0, 0.0))
    } else {
        (panel(b, parent, &p.name, w, h), (12.0, 30.0, 34.0, 22.0))
    };
    // A touchscreen's (`touch`): the plot narrower, the band panel pinned
    // beside it at a finger's size.
    let touch = rig && p.flag("touch");
    let pw = if touch { w - TOUCH_PANEL_W } else { w };
    let a = if rig {
        let area = b.add(root, draw::frame("plot area", 0.0, 0.0, pw, h));
        let mut clip = draw::frame("traces", 0.0, 0.0, pw, h);
        clip.clips_content = true;
        let clip = b.add(area, clip);
        Area {
            area,
            clip,
            w: pw,
            h,
        }
    } else {
        area(b, root, ax, ay, w - ax - right, h - ay - bottom)
    };
    // `fixed`: the bands are the EQ's own, always there and always on —
    // each only a frequency, gain, Q and shape (`shapes=` the first of
    // [`SHAPES`] it may take); nothing to add, delete, bypass or rename.
    let fixed = p.flag("fixed");
    let shapes = (opt_f(p, "shapes", SHAPES.len() as f64) as usize).clamp(1, SHAPES.len());
    // `db_range=`: a fixed ± dB span (no zoom, no range picker).
    let db_range = p.opt("db_range").and_then(|v| v.parse::<f64>().ok());
    let span = db_range.unwrap_or(12.0);
    let x = Axis::Log {
        f_min: f.0,
        f_max: f.1,
    };
    let y = Axis::Linear {
        min: -span,
        max: span,
    };
    // `tint=#hex`: one colour for every band, the sum and the glow, in place
    // of the frequency ramp.
    let tint_hex = p.opt("tint").map(ToString::to_string);
    let tint = tint_hex
        .as_ref()
        .map_or_else(|| ramp_over(&x), |c| vec![c.clone(), c.clone()]);

    // ── grid, printed scales ────────────────────────────────────────────
    const LINES: [f64; 27] = [
        20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 200.0, 300.0, 400.0, 500.0, 600.0,
        700.0, 800.0, 900.0, 1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0,
        9000.0, 10_000.0,
    ];
    let xs: Vec<(f64, bool)> = LINES
        .iter()
        .chain(&[20_000.0])
        .filter(|v| **v > f.0 && **v < f.1)
        .map(|v| (x.to_unit(*v), [100.0, 1000.0, 10_000.0].contains(v)))
        .collect();
    let ys: Vec<(f64, bool)> = (1..8).map(|i| (f64::from(i) / 8.0, i == 4)).collect();
    // The glow, fed by the spectrum and the bands: under everything in the
    // rig look (its analyser layers are its fallback), in the `glow` frame
    // under the traces in the panel look.
    let spectrum = match &p.binding {
        Some(Binding {
            source: Source::Stream { id, .. },
            ..
        }) => id.0.clone(),
        _ => p
            .opt("spectrum")
            .map_or_else(|| ad.view("spectrum"), ToString::to_string),
    };
    let floor = opt_f(p, "floor", -90.0);
    let glow_spec = super::lit::GlowSpec {
        ns: &ad.ns,
        bands: bands as usize,
        used: !fixed,
        bypass: !fixed,
        spectrum: spectrum.clone(),
        f,
        span: db_range.map_or_else(
            || super::lit::GlowSpan::Param {
                addr: ad.view("range"),
                spans: RANGES.to_vec(),
            },
            super::lit::GlowSpan::Fixed,
        ),
        floor,
        tint: tint_hex.clone(),
    };
    let rig_glow = rig.then(|| super::lit::glow_light(b, a.area, (a.w, a.h), &glow_spec));
    if rig {
        rig_grid(b, &a, &x, f, &ad, db_range);
        // Glow, then grid, then the traces.
        if let Some(n) = b.scene.node_mut(a.area) {
            let clip = a.clip;
            n.children.retain(|c| *c != clip);
            n.children.push(clip);
        }
    } else {
        grid(b, &a, &xs, &ys);
    }
    const FREQ_LABELS: [(f64, &str); 10] = [
        (20.0, "20"),
        (50.0, "50"),
        (100.0, "100"),
        (200.0, "200"),
        (500.0, "500"),
        (1000.0, "1k"),
        (2000.0, "2k"),
        (5000.0, "5k"),
        (10_000.0, "10k"),
        (20_000.0, "20k"),
    ];
    for (i, (hz, s)) in FREQ_LABELS.into_iter().enumerate() {
        if hz < f.0 || hz > f.1 {
            continue;
        }
        let u = x.to_unit(hz);
        if rig {
            // processor's DOM: 2 px off the floor, the ends kept inside.
            let align = if i == 0 {
                TextAlign::Left
            } else if i == FREQ_LABELS.len() - 1 {
                TextAlign::Right
            } else {
                TextAlign::Center
            };
            let t = label(b, root, s, (u * a.w, a.h - 2.0 - 6.0), align, RIG_FREQ_INK);
            if let Some(n) = b.scene.node_mut(t) {
                n.effects = vec![draw::drop_shadow("#000000ff", 0.0, 1.0, 2.0)];
            }
            continue;
        }
        let align = if u < 0.02 {
            TextAlign::Left
        } else if u > 0.98 {
            TextAlign::Right
        } else {
            TextAlign::Center
        };
        label(b, root, s, (ax + u * a.w, ay + a.h + 11.0), align, LABEL);
    }
    // dB numerals follow the range: a readout of it, one label per span.
    for frac in [1.0, 0.5, 0.0, -0.5, -1.0_f64] {
        // A fixed span prints its half-spans and zero only: the ends would
        // crowd the top rim and sit on the 20k.
        if db_range.is_some() && frac.abs() > 0.75 {
            continue;
        }
        let labels: Vec<String> = RANGES
            .iter()
            .map(|r| {
                let v = r * frac;
                if v.abs() < 1e-9 {
                    "0".to_string()
                } else {
                    format!("{v:+.0}").replace(".5", "½")
                }
            })
            .collect();
        let cy = ay + (1.0 - (frac + 1.0) / 2.0) * a.h;
        let (holder, t) = if rig {
            // processor's DOM: a pill 3 px in from the right edge, kept
            // clear of the top rim and the frequency labels.
            let cy = cy.clamp(20.0, (a.h - 16.0).max(20.0));
            let holder = frame(
                b,
                root,
                "dB label",
                (a.w - 3.0 - 24.0, cy - 6.0, 24.0, 12.0),
                Some(RIG_DB_PILL),
                2.0,
            );
            (
                holder,
                text(
                    b,
                    holder,
                    "value",
                    "",
                    (0.0, 1.0, 24.0),
                    9.0,
                    RIG_DB_INK,
                    TextAlign::Center,
                    500.0,
                ),
            )
        } else {
            let holder = frame(
                b,
                root,
                "dB label",
                (
                    ax + a.w + 3.0,
                    (cy - 6.0).clamp(ay, ay + a.h - 12.0),
                    28.0,
                    12.0,
                ),
                None,
                0.0,
            );
            (
                holder,
                text(
                    b,
                    holder,
                    "value",
                    "",
                    (0.0, 0.0, 28.0),
                    9.0,
                    LABEL,
                    TextAlign::Left,
                    500.0,
                ),
            )
        };
        if let Some(r) = db_range {
            // A fixed span: its numerals printed.
            let v = r * frac;
            let s = if v.abs() < 1e-9 {
                "0".to_string()
            } else {
                format!("{v:+.0}")
            };
            if let Some(NodeKind::Text(d)) = b.scene.node_mut(t).map(|n| &mut n.kind) {
                d.characters = s;
            }
            continue;
        }
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let c = Control::new(ControlKind::Readout)
            .bound(Binding::param(ad.view("range")))
            .slot(SlotRole::Value, t.0)
            .formatted(ValueFormat::labels(&refs));
        b.controls.insert(holder, c);
    }

    // ── traces: glow, analyser layers, band curves, the sum ─────────────
    let glow_frame = b.add(a.clip, draw::frame("glow", 0.0, 0.0, a.w, a.h));
    if !rig {
        super::lit::glow_light(b, glow_frame, (a.w, a.h), &glow_spec);
    }
    // The analyser: processor's colours, its level axis (0 dB at the top,
    // 90 down) in the rig look. There the glow draws it as light, so the
    // layers are the glow's fallback.
    let layers = if rig {
        [
            (
                "analyser pre",
                ad.view("spectrum_pre"),
                "show_pre",
                Some("#8c8c9610"),
                Some(("#8c8c9646", 1.0)),
            ),
            (
                "analyser post",
                spectrum,
                "show_post",
                Some("#64b4ff1c"),
                Some(("#64b4ff78", 1.0)),
            ),
            (
                "analyser ext",
                ad.view("sidechain"),
                "show_ext",
                None,
                Some(("#ffaa5096", 1.0)),
            ),
        ]
    } else {
        [
            (
                "analyser pre",
                ad.view("spectrum_pre"),
                "show_pre",
                Some("#8c8c9612"),
                Some(("#8c8c9646", 1.0)),
            ),
            (
                "analyser post",
                spectrum,
                "show_post",
                Some("#64b4ff1f"),
                Some(("#64b4ff78", 1.0)),
            ),
            (
                "analyser ext",
                ad.view("sidechain"),
                "show_ext",
                None,
                Some(("#ffaa5096", 1.0)),
            ),
        ]
    };
    let (layer_parent, level) = match rig_glow {
        Some(g) => (g, PlotRange::linear(-90.0, 0.0)),
        None => (a.clip, PlotRange::linear(floor, 6.0)),
    };
    // `no_analyser`: an EQ with nothing to analyse draws no analyser layers
    // (the glow then lights its bands alone).
    let layers = if p.flag("no_analyser") {
        Vec::new()
    } else {
        layers.to_vec()
    };
    for (name, feed, toggle, fill, line) in layers {
        let holder = b.add(layer_parent, draw::frame(name, 0.0, 0.0, a.w, a.h));
        let mut slots = Vec::new();
        if let Some(fc) = fill {
            slots.push((
                SlotRole::Area,
                vector(b, holder, &a, "fill", Some(fc), None),
            ));
        }
        if let Some((lc, lw)) = line {
            slots.push((
                SlotRole::Trace,
                vector(b, holder, &a, "line", None, Some((lc, lw, false))),
            ));
        }
        let c = Control::new(ControlKind::Plot {
            style: PlotStyle::Line,
            x: x.clone(),
            y: level.clone(),
        })
        .bound(Binding::stream(feed))
        .shown_if(Condition::on(ad.view(toggle)));
        b.controls.insert(holder, with_slots(c, &slots));
    }

    // Each band's own curve, stem and dynamic range: a template.
    let curves = b.add(a.clip, draw::frame("band {n} curves", 0.0, 0.0, a.w, a.h));
    let used = (!fixed).then(|| ParamAddr::new(ad.band("used")));
    b.controls.insert(
        curves,
        Control::new(ControlKind::Repeat {
            first: 1,
            count: bands,
            used: used.clone(),
        }),
    );
    let mut own = band_inputs(&ad, "{n}", fixed);
    own.push(CurveInput::param("tint", ad.band("freq")));
    let zoom = ad.zoom();
    let band_curve = |b: &mut Builder<'_>,
                      name: &str,
                      series: &str,
                      style: PlotStyle,
                      extra: Vec<CurveInput>,
                      look: (Option<&str>, Option<(&str, f64, bool)>)| {
        let holder = b.add(curves, draw::frame(name, 0.0, 0.0, a.w, a.h));
        let mut slots = Vec::new();
        if let Some(fc) = look.0 {
            let n = vector(b, holder, &a, "fill", Some(fc), None);
            slots.extend([(SlotRole::Area, n), (SlotRole::Tint, n)]);
        }
        if let Some(l) = look.1 {
            let n = vector(b, holder, &a, "line", None, Some(l));
            slots.extend([(SlotRole::Trace, n), (SlotRole::Tint, n)]);
        }
        let mut inputs = own.clone();
        inputs.extend(extra);
        let mut c = Control::new(ControlKind::Curve {
            provider: "eq.response".into(),
            inputs,
            series: Some(series.into()),
            style,
            x: x.clone(),
            y: PlotRange::linear(-span, span),
        });
        c.zoom = db_range.is_none().then(|| zoom.clone());
        c.tint = tint.clone();
        b.controls.insert(holder, with_slots(c, &slots));
    };
    // processor: the band's colour at 25 % under a 60 % line.
    let band_fill = if rig { "#ffffff40" } else { "#ffffff38" };
    band_curve(
        b,
        "curve",
        "b{n}",
        PlotStyle::Area { baseline: 0.0 },
        Vec::new(),
        (Some(band_fill), Some(("#ffffff99", 1.5, false))),
    );
    band_curve(
        b,
        "stem",
        "b{n}_stem",
        PlotStyle::Line,
        vec![CurveInput::constant("stems", 1.0)],
        (None, Some(("#ffffff80", 1.5, false))),
    );
    if !fixed {
        band_curve(
            b,
            "dynamic range",
            "b{n}_dyn",
            PlotStyle::Line,
            Vec::new(),
            (None, Some(("#ffffffb0", 1.2, true))),
        );
    }

    // The combined response over every band.
    let sum = b.add(a.clip, draw::frame("response", 0.0, 0.0, a.w, a.h));
    let sum_ink = tint_hex.clone().unwrap_or_else(|| GOLD.to_string());
    let fill = vector(b, sum, &a, "fill", Some(&format!("{sum_ink}14")), None);
    let line = vector(b, sum, &a, "line", None, Some((&sum_ink, 2.0, false)));
    // A soft under-stroke instead of emitted light: light composites over
    // everything, and the band panel and menus float above this curve.
    let halo = vector(b, sum, &a, "halo", None, Some(("#ffc94d33", 6.0, false)));
    if let Some(n) = b.scene.node_mut(sum) {
        let (h, l) = (halo, line);
        n.children.retain(|c| *c != h);
        let at = n.children.iter().position(|c| *c == l).unwrap_or(0);
        n.children.insert(at, h);
    }
    // processor strokes the sum once, 2 px, no halo.
    if rig && let Some(n) = b.scene.node_mut(halo) {
        n.visible = false;
    }
    let inputs: Vec<CurveInput> = (1..=bands)
        .flat_map(|n| band_inputs(&ad, &n.to_string(), fixed))
        .collect();
    let mut c = Control::new(ControlKind::Curve {
        provider: "eq.response".into(),
        inputs,
        series: Some("sum".into()),
        style: PlotStyle::Area { baseline: 0.0 },
        x: x.clone(),
        y: PlotRange::linear(-span, span),
    });
    c.zoom = db_range.is_none().then(|| zoom.clone());
    b.controls.insert(
        sum,
        with_slots(
            c,
            &[
                (SlotRole::Area, fill),
                (SlotRole::Trace, halo),
                (SlotRole::Trace, line),
            ],
        ),
    );

    // ── the band template: handle and name label ───────────────────────
    let band = b.add(a.area, draw::frame("band {n}", 0.0, 0.0, a.w, a.h));
    b.controls.insert(
        band,
        Control::new(ControlKind::Repeat {
            first: 1,
            count: bands,
            used,
        }),
    );
    let handle = b.add(
        band,
        draw::frame(
            "band {n} handle",
            (a.w - HANDLE) / 2.0,
            (a.h - HANDLE) / 2.0,
            HANDLE,
            HANDLE,
        ),
    );
    let c0 = HANDLE / 2.0;
    let r = DISC / 2.0;
    let (halo, hover) = if rig {
        // processor's node: a 3 px white ring 4 px out (8 %, 18 % on hover),
        // and on hover a brighter outline.
        let halo = b.add(handle, ring_node("halo", c0, r + 4.0, 3.0, "#ffffff14"));
        let hover = b.add(handle, draw::frame("hover", 0.0, 0.0, HANDLE, HANDLE));
        b.add(hover, ring_node("glow", c0, r + 4.0, 3.0, "#ffffff2e"));
        b.add(hover, ring_node("outline", c0, r, 1.5, "#ffffffb3"));
        (halo, hover)
    } else {
        (
            b.add(
                handle,
                draw::circle("halo", c0, c0, DISC / 2.0 + 4.0, draw::solid("#ffffff14")),
            ),
            b.add(
                handle,
                draw::circle("hover", c0, c0, DISC / 2.0 + 5.0, draw::solid("#ffffff2e")),
            ),
        )
    };
    let mut ring = draw::circle(
        "selection",
        c0,
        c0,
        if rig { r + 5.0 } else { DISC / 2.0 + 5.5 },
        draw::solid("#00000000"),
    );
    ring.fills.clear();
    ring.stroke = Stroke {
        paints: vec![draw::solid("#ffffffe6")],
        weight: 1.5,
        align: StrokeAlign::Center,
        ..Stroke::default()
    };
    let ring = b.add(handle, ring);
    // On / off: a Readout of `on` shows the lit disc or the grey one.
    let state = b.add(handle, draw::frame("state", 0.0, 0.0, HANDLE, HANDLE));
    let (lit, lit_shown) = if rig {
        // processor's node: the disc in the band's colour under a 40 %
        // white outline (the outline keeps its white: it isn't tinted).
        let group = b.add(state, draw::frame("lit", 0.0, 0.0, HANDLE, HANDLE));
        let disc = b.add(
            group,
            draw::circle("disc", c0, c0, r, draw::solid("#ff6b6b")),
        );
        b.add(group, ring_node("outline", c0, r, 1.5, "#ffffff66"));
        (disc, group)
    } else {
        let mut lit = draw::circle("disc", c0, c0, DISC / 2.0, draw::solid("#ff6b6b"));
        lit.stroke = Stroke {
            paints: vec![draw::solid("#ffffff66")],
            weight: 1.5,
            align: StrokeAlign::Inside,
            ..Stroke::default()
        };
        let lit = b.add(
            state,
            draw::with_effects(lit, vec![draw::drop_shadow("#00000099", 0.0, 1.0, 4.0)]),
        );
        (lit, lit)
    };
    let grey = b.add(
        state,
        draw::circle("bypassed", c0, c0, DISC / 2.0, draw::solid("#555555")),
    );
    if fixed {
        // Always on: the lit disc only.
        if let Some(n) = b.scene.node_mut(grey) {
            n.visible = false;
        }
    } else {
        b.controls.insert(
            state,
            with_slots(
                Control::new(ControlKind::Readout).bound(Binding::param(ad.band("on"))),
                &[(SlotRole::On, lit_shown), (SlotRole::Off, grey)],
            ),
        );
    }

    // The name label riding above the handle; double-click renames (a
    // fixed band has no name).
    let field = if fixed {
        None
    } else {
        Some(name_label(b, band, handle, &ad))
    };
    let rename: Vec<Command> = field
        .map(|f| vec![Command::Edit { node: f.0 }])
        .unwrap_or_default();

    // ── the band menu, its submenus, the graph menu ─────────────────────
    let menus = b.add(root, draw::frame("menus", 0.0, 0.0, 0.0, 0.0));
    let shape_sub = submenu(
        b,
        menus,
        "shape menu",
        &ad,
        band,
        &SHAPES[..shapes],
        "shape",
        |i| {
            let mut v = vec![Command::Set {
                addr: ParamAddr::new(ad.band("shape")),
                value: i as f64,
            }];
            if CUTS.contains(&(i as f64)) {
                v.push(Command::Set {
                    addr: ParamAddr::new(ad.band("q")),
                    value: 0.707,
                });
            }
            if NO_GAIN.contains(&(i as f64)) {
                v.push(Command::Set {
                    addr: ParamAddr::new(ad.band("gain")),
                    value: 0.0,
                });
            }
            v
        },
    );
    let rows: Vec<Row> = if fixed {
        vec![
            Row::header("BAND {n} ·", Some(ad.band("freq"))),
            Row::divider(),
            Row::action(
                "Invert Gain",
                vec![Command::Invert {
                    addr: ParamAddr::new(ad.band("gain")),
                }],
            ),
            Row::sub("Shape", shape_sub, ad.band("shape")),
            Row::divider(),
            Row::action(
                "Reset Gain",
                vec![Command::Set {
                    addr: ParamAddr::new(ad.band("gain")),
                    value: 0.0,
                }],
            ),
        ]
    } else {
        let slope_sub = submenu(b, menus, "slope menu", &ad, band, &SLOPES, "slope", |i| {
            vec![Command::Set {
                addr: ParamAddr::new(ad.band("slope")),
                value: i as f64,
            }]
        });
        let place_names = ["Stereo", "Left", "Right", "Mid", "Side"];
        let place_sub = submenu(
            b,
            menus,
            "placement menu",
            &ad,
            band,
            &place_names,
            "placement",
            |i| {
                vec![Command::Set {
                    addr: ParamAddr::new(ad.band("placement")),
                    value: i as f64,
                }]
            },
        );
        vec![
            Row::header("BAND {n} ·", Some(ad.band("freq"))),
            Row::divider(),
            Row::action(
                "Bypass",
                vec![Command::Toggle {
                    addr: ParamAddr::new(ad.band("on")),
                }],
            )
            .checked(Condition::on(ad.band("on")).negated()),
            Row::action(
                "Focus",
                vec![Command::Toggle {
                    addr: ParamAddr::new(ad.band("focus")),
                }],
            )
            .checked(Condition::on(ad.band("focus"))),
            Row::action(
                "Invert Gain",
                vec![Command::Invert {
                    addr: ParamAddr::new(ad.band("gain")),
                }],
            ),
            Row::divider(),
            Row::sub("Shape", shape_sub, ad.band("shape")),
            Row::sub("Slope", slope_sub, ad.band("slope")),
            Row::sub("Stereo Placement", place_sub, ad.band("placement")),
            Row::divider(),
            Row::action(
                "Reset Gain",
                vec![Command::Set {
                    addr: ParamAddr::new(ad.band("gain")),
                    value: 0.0,
                }],
            ),
            Row::action("Rename", rename.clone()),
            Row::action("Delete Band", vec![Command::Remove]).danger(),
        ]
    };
    let band_menu = menu(b, menus, "band menu", MENU_W, &rows, Some(band));
    let graph_rows = vec![
        Row::header("GRAPH", None),
        Row::divider(),
        Row::action("Add Band Here", vec![Command::Add]),
    ];
    let graph_menu = (!fixed).then(|| menu(b, menus, "graph menu", 150.0, &graph_rows, None));

    // ── the handle's control ────────────────────────────────────────────
    let q_per_notch = 1.15_f64.ln() / (40.0_f64 / 0.025).ln();
    let q_per_px = 2.0_f64.ln() / (40.0_f64 / 0.025).ln() / 60.0;
    let run = |cmds: Vec<Command>| ClickAction::Run { commands: cmds };
    let interact = if fixed {
        // The band's own: no bypass, slope, dynamic range, placement or name.
        Interact {
            selectable: true,
            removable: false,
            click: vec![
                ClickRule {
                    chord: Chord::OPTION_COMMAND,
                    action: run(vec![Command::Cycle {
                        addr: ParamAddr::new(ad.band("shape")),
                    }]),
                },
                ClickRule {
                    chord: Chord::COMMAND,
                    action: ClickAction::ToggleSelection,
                },
                ClickRule {
                    chord: Chord::COMMAND_SHIFT,
                    action: ClickAction::ToggleSelection,
                },
                ClickRule {
                    chord: Chord::SHIFT,
                    action: ClickAction::RangeSelect,
                },
            ],
            drag: vec![
                DragRule {
                    chord: Chord::COMMAND,
                    mode: DragMode::Wheel { per_px: q_per_px },
                    when: None,
                },
                DragRule {
                    chord: Chord::OPTION,
                    mode: DragMode::XOnly,
                    when: None,
                },
            ],
            wheel: vec![
                WheelRule {
                    chord: Chord::OPTION_COMMAND,
                    target: ParamAddr::new(ad.band("gain")),
                    step: 1.0 / 60.0,
                    when: None,
                },
                WheelRule {
                    chord: Chord::COMMAND,
                    target: ParamAddr::new(ad.band("gain")),
                    step: 1.0 / 60.0,
                    when: None,
                },
                WheelRule {
                    chord: Chord::NONE,
                    target: ParamAddr::new(ad.band("q")),
                    step: q_per_notch,
                    when: None,
                },
            ],
            double_click: Vec::new(),
            keys: Vec::new(),
            press: Vec::new(),
            menu: Some(band_menu.0),
        }
    } else {
        Interact {
            selectable: true,
            removable: true,
            click: vec![
                ClickRule {
                    chord: Chord::OPTION_COMMAND,
                    action: run(vec![Command::Cycle {
                        addr: ParamAddr::new(ad.band("shape")),
                    }]),
                },
                ClickRule {
                    chord: Chord::OPTION_SHIFT,
                    action: run(vec![Command::Cycle {
                        addr: ParamAddr::new(ad.band("slope")),
                    }]),
                },
                ClickRule {
                    chord: Chord::OPTION,
                    action: run(vec![Command::Toggle {
                        addr: ParamAddr::new(ad.band("on")),
                    }]),
                },
                ClickRule {
                    chord: Chord::COMMAND,
                    action: ClickAction::ToggleSelection,
                },
                ClickRule {
                    chord: Chord::COMMAND_SHIFT,
                    action: ClickAction::ToggleSelection,
                },
                ClickRule {
                    chord: Chord::SHIFT,
                    action: ClickAction::RangeSelect,
                },
            ],
            drag: vec![
                DragRule {
                    chord: Chord::COMMAND,
                    mode: DragMode::Wheel { per_px: q_per_px },
                    when: None,
                },
                DragRule {
                    chord: Chord::OPTION,
                    mode: DragMode::XOnly,
                    when: None,
                },
                // A cut, a notch, an all-pass has no gain: it slides across.
                DragRule {
                    chord: Chord::NONE,
                    mode: DragMode::XOnly,
                    when: Some(Condition::any_of(ad.band("shape"), &NO_GAIN)),
                },
            ],
            wheel: vec![
                WheelRule {
                    chord: Chord::OPTION_COMMAND,
                    target: ParamAddr::new(ad.band("gain")),
                    step: 1.0 / 60.0,
                    when: None,
                },
                WheelRule {
                    chord: Chord::OPTION,
                    target: ParamAddr::new(ad.band("dyn_range")),
                    step: 1.0 / 60.0,
                    when: None,
                },
                WheelRule {
                    chord: Chord::COMMAND,
                    target: ParamAddr::new(ad.band("gain")),
                    step: 1.0 / 60.0,
                    when: None,
                },
                WheelRule {
                    chord: Chord::NONE,
                    target: ParamAddr::new(ad.band("slope")),
                    step: 0.1,
                    when: Some(Condition::any_of(ad.band("shape"), &CUTS)),
                },
                WheelRule {
                    chord: Chord::NONE,
                    target: ParamAddr::new(ad.band("q")),
                    step: q_per_notch,
                    when: None,
                },
            ],
            double_click: rename,
            // Placement keys (Dioxus: m / s / l / r / n) and focus.
            keys: [("n", 0.0), ("l", 1.0), ("r", 2.0), ("m", 3.0), ("s", 4.0)]
                .into_iter()
                .map(|(k, v)| KeyRule {
                    key: k.into(),
                    commands: vec![Command::Set {
                        addr: ParamAddr::new(ad.band("placement")),
                        value: v,
                    }],
                })
                .chain(std::iter::once(KeyRule {
                    key: "f".into(),
                    commands: vec![Command::Toggle {
                        addr: ParamAddr::new(ad.band("focus")),
                    }],
                }))
                .collect(),
            press: Vec::new(),
            menu: Some(band_menu.0),
        }
    };
    let mut hc = Control::new(ControlKind::Handle {
        x: Some(Binding::param(ad.band("freq"))),
        y: Some(Binding::param(ad.band("gain"))),
        wheel: Some(Binding::param(ad.band("q"))),
        x_map: HandleAxis::new(x.clone()),
        y_map: HandleAxis::new(y.clone()),
    })
    .interacting(interact);
    hc.zoom = db_range.is_none().then(|| zoom.clone());
    hc.tint = tint.clone();
    let hc = with_slots(
        hc,
        &[
            (SlotRole::Hovered, hover),
            (SlotRole::Selected, ring),
            (SlotRole::Tint, lit),
            (SlotRole::Tint, ring),
        ],
    );
    b.controls.insert(handle, hc);
    let _ = halo;

    // ── the plot area: add, marquee, clear, graph menu ──────────────────
    let mut mq = draw::frame("marquee", 0.0, 0.0, 10.0, 10.0);
    mq.fills = vec![draw::solid("#6496ff26")];
    mq.stroke = Stroke {
        paints: vec![draw::solid("#6496ff99")],
        weight: 1.0,
        dash_pattern: vec![3.0, 3.0],
        align: StrokeAlign::Inside,
        ..Stroke::default()
    };
    mq.visible = false;
    let mq = b.add(a.area, mq);
    let zone = |x_min: f64, x_max: f64, y_min: f64, y_max: f64, shape: f64, keep_y: bool| {
        let mut set = vec![Assign {
            addr: ParamAddr::new(ad.band("shape")),
            value: shape,
        }];
        if CUTS.contains(&shape) {
            set.push(Assign {
                addr: ParamAddr::new(ad.band("q")),
                value: 0.707,
            });
        }
        AddZone {
            x_min,
            x_max,
            y_min,
            y_max,
            set,
            keep_y,
        }
    };
    // Dioxus's `filter_type_for_position`, in unit height (0.5 = 0 dB): a
    // notch below −82 % of the range; cuts near the edges when near 0 dB;
    // shelves toward the edges; a bell elsewhere.
    let add = AddRule {
        repeat: band.0,
        x: Some(ParamAddr::new(ad.band("freq"))),
        y: Some(ParamAddr::new(ad.band("gain"))),
        set: vec![
            Assign::new(ad.band("q"), 1.0),
            Assign::new(ad.band("on"), 1.0),
        ],
        zones: vec![
            zone(0.0, 1e9, 0.0, 0.09, 5.0, false),
            zone(0.0, 30.0, 0.4, 0.6, 3.0, false),
            zone(15_000.0, 1e9, 0.4, 0.6, 4.0, false),
            zone(0.0, 80.0, 0.0, 1.0, 1.0, true),
            zone(8000.0, 1e9, 0.0, 1.0, 2.0, true),
        ],
    };
    let mut canvas = Control::new(ControlKind::Canvas {
        x: x.clone(),
        y: y.clone(),
        add: (!fixed).then_some(add),
    })
    .interacting(Interact {
        menu: graph_menu.map(|m| m.0),
        ..Interact::default()
    })
    .slot(SlotRole::Marquee, mq.0);
    canvas.zoom = db_range.is_none().then(|| zoom.clone());
    b.controls.insert(a.area, canvas);

    // ── the band panel and readout chip (follow the active band) ────────
    if touch {
        band_panel_touch(b, root, &ad, band, handle, (pw, 0.0, TOUCH_PANEL_W, h));
    } else {
        band_panel(b, root, &ad, (band, handle), a.h, (fixed, &tint));
    }
    chip(b, root, &ad, band, handle);

    // ── toolbar: range, analyser layers ─────────────────────────────────
    let range = stroked_frame(
        b,
        root,
        "range",
        (if rig { 6.0 } else { ax }, 6.0, 64.0, 18.0),
        "#0f0f12eb",
        "#50505599",
        4.0,
    );
    let rv = text(
        b,
        range,
        "value",
        "±12 dB",
        (0.0, 3.0, 64.0),
        10.0,
        "#dddddd",
        TextAlign::Center,
        500.0,
    );
    let rc = Control::new(ControlKind::Button {
        mode: ButtonMode::Cycle,
    })
    .bound(Binding::param(ad.view("range")))
    .slot(SlotRole::Value, rv.0)
    .slot(SlotRole::Label, rv.0)
    .interacting(Interact {
        double_click: vec![Command::Reset {
            addr: ParamAddr::new(ad.view("range")),
        }],
        ..Interact::default()
    });
    // A fixed span has no range to pick: the picker is only drawn, hidden.
    if db_range.is_none() {
        b.controls.insert(range, rc);
    }
    // `no_range`: signal's rig mounts the graph where Blitz draws its range
    // `<select>` as nothing (a 12 × 6 box, no text) — the rig shows no
    // picker, and a surface recreating the rig shows none either.
    if (p.flag("no_range") || db_range.is_some())
        && let Some(n) = b.scene.node_mut(range)
    {
        n.visible = false;
    }
    // (The rig's graph has no layer toggles: its analyser shows what's fed.)
    let toggles = if rig {
        Vec::new()
    } else {
        vec![
            ("pre", "PRE", "#8c8c96"),
            ("post", "POST", "#64b4ff"),
            ("ext", "EXT", "#ffaa50"),
        ]
    };
    for (i, (k, legend, color)) in toggles.into_iter().enumerate() {
        let bx = w - right - 3.0 * 46.0 + i as f64 * 46.0;
        let btn = stroked_frame(
            b,
            root,
            &format!("{legend} analyser"),
            (bx, 6.0, 42.0, 18.0),
            "#0f0f12eb",
            "#50505599",
            4.0,
        );
        let dot_on = b.add(btn, draw::circle("on", 9.0, 9.0, 3.0, draw::solid(color)));
        let dot_off = b.add(
            btn,
            draw::circle("off", 9.0, 9.0, 3.0, draw::solid("#3a3a40")),
        );
        let t = text(
            b,
            btn,
            "label",
            legend,
            (15.0, 3.5, 26.0),
            9.0,
            "#bbbbbb",
            TextAlign::Left,
            600.0,
        );
        let c = Control::new(ControlKind::Button {
            mode: ButtonMode::Toggle,
        })
        .bound(Binding::param(ad.view(&format!("show_{k}"))))
        .slot(SlotRole::On, dot_on.0)
        .slot(SlotRole::Off, dot_off.0)
        .slot(SlotRole::Label, t.0);
        b.controls.insert(btn, c);
    }
    root
}

/// A band's name label riding above its handle: a text field of
/// `b{n}_name`, renamed in place. Returns the field.
fn name_label(b: &mut Builder<'_>, band: NodeId, handle: NodeId, ad: &Addrs) -> NodeId {
    let label_pop = b.add(band, draw::frame("band {n} label", 0.0, 0.0, 120.0, 16.0));
    let field = b.add(label_pop, draw::frame("name", 0.0, 0.0, 120.0, 16.0));
    let sel = frame(
        b,
        field,
        "selection",
        (0.0, 1.0, 1.0, 14.0),
        Some("#4dabf766"),
        2.0,
    );
    let name_text = text(
        b,
        field,
        "value",
        "",
        (0.0, 1.0, 120.0),
        10.0,
        INK,
        TextAlign::Center,
        600.0,
    );
    if let Some(n) = b.scene.node_mut(name_text) {
        n.effects = vec![draw::drop_shadow("#000000ff", 0.0, 1.0, 2.0)];
    }
    let caret = frame(b, field, "caret", (0.0, 1.0, 1.5, 14.0), Some(INK), 0.0);
    let c = Control::new(ControlKind::TextField {
        click_to_edit: false,
        max_len: 32,
    })
    .bound(Binding::param(ad.band("name")));
    b.controls.insert(
        field,
        with_slots(
            c,
            &[
                (SlotRole::Value, name_text),
                (SlotRole::Caret, caret),
                (SlotRole::TextSelection, sel),
            ],
        ),
    );
    b.controls.insert(
        label_pop,
        Control::new(ControlKind::Popover {
            anchor: Some(handle.0),
            placement: Placement::Above,
            gap: -7.0,
            show: Show::Always,
            context: None,
        }),
    );
    field
}

/// The rig look's ground (processor's `(10, 10, 10)`).
const RIG_GROUND: &str = "#0a0a0a";
/// processor's DOM frequency labels, `rgba(160,160,172,0.85)`.
const RIG_FREQ_INK: &str = "#a0a0acd9";
/// processor's DOM dB labels: `rgba(170,170,182,0.95)` on `rgba(10,10,14,0.78)`.
const RIG_DB_INK: &str = "#aaaab6f2";
const RIG_DB_PILL: &str = "#0a0a0ec7";

/// A stroked ring of radius `r` centred at (`c`, `c`).
fn ring_node(name: &str, c: f64, r: f64, weight: f64, color: &str) -> Node {
    let mut n = draw::circle(name, c, c, r, draw::solid("#00000000"));
    n.fills.clear();
    n.stroke = Stroke {
        paints: vec![draw::solid(color)],
        weight,
        align: StrokeAlign::Center,
        ..Stroke::default()
    };
    n
}

/// processor's `paint_grid`: log frequency lines in three tiers (decades
/// thickest, the 1-2-5 anchors medium, the rest thin) and dB lines every
/// 2/3/6 dB by the current span (`eq.grid`), zero thickest.
fn rig_grid(
    b: &mut Builder<'_>,
    a: &Area,
    x: &Axis,
    f: (f64, f64),
    ad: &Addrs,
    db_range: Option<f64>,
) {
    const MINOR: &str = "#37373c5a";
    const MID: &str = "#46464b6e";
    const MAJOR: &str = "#5a5a5f8c";
    #[rustfmt::skip]
    const LINES: [(f64, u8); 29] = [
        (15.0, 0), (20.0, 1), (30.0, 0), (40.0, 0), (50.0, 1), (60.0, 0), (70.0, 0), (80.0, 0), (90.0, 0),
        (100.0, 2), (200.0, 1), (300.0, 0), (400.0, 0), (500.0, 1), (600.0, 0), (700.0, 0), (800.0, 0), (900.0, 0),
        (1000.0, 2), (2000.0, 1), (3000.0, 0), (4000.0, 0), (5000.0, 1), (6000.0, 0), (7000.0, 0), (8000.0, 0), (9000.0, 0),
        (10_000.0, 2), (20_000.0, 1),
    ];
    for (tier, color, weight) in [(0_u8, MINOR, 0.5), (1, MID, 0.75), (2, MAJOR, 1.25)] {
        let d: String = LINES
            .iter()
            .filter(|(hz, t)| *t == tier && *hz >= f.0 && *hz <= f.1)
            .map(|(hz, _)| format!("M {:.2} 0 V {:.1} ", x.to_unit(*hz) * a.w, a.h))
            .collect();
        if d.is_empty() {
            continue;
        }
        let mut n = draw::path(
            "grid",
            0.0,
            0.0,
            a.w,
            a.h,
            d.trim_end().to_string(),
            draw::solid("#00000000"),
        );
        n.fills.clear();
        n.stroke = Stroke {
            paints: vec![draw::solid(color)],
            weight,
            align: StrokeAlign::Center,
            ..Stroke::default()
        };
        b.add(a.area, n);
    }
    let inputs = if let Some(r) = db_range {
        vec![CurveInput::constant("db_range", r)]
    } else {
        let mut v = vec![CurveInput::param("range", ad.view("range"))];
        v.extend(
            RANGES
                .iter()
                .enumerate()
                .map(|(i, r)| CurveInput::constant(format!("span_{i}"), *r)),
        );
        v
    };
    for (series, color, weight) in [("minor", MINOR, 0.5), ("zero", MAJOR, 1.25)] {
        let holder = b.add(
            a.area,
            draw::frame(&format!("dB grid {series}"), 0.0, 0.0, a.w, a.h),
        );
        let line = vector(b, holder, a, "line", None, Some((color, weight, false)));
        if let Some(n) = b.scene.node_mut(line) {
            n.stroke.cap = StrokeCap::None;
        }
        let c = Control::new(ControlKind::Curve {
            provider: "eq.grid".into(),
            inputs: inputs.clone(),
            series: Some(series.into()),
            style: PlotStyle::Line,
            x: Axis::Linear { min: 0.0, max: 1.0 },
            y: PlotRange::linear(0.0, 1.0),
        });
        b.controls
            .insert(holder, with_slots(c, &[(SlotRole::Trace, line)]));
    }
}

/// One menu row as drawn.
struct Row {
    label: String,
    role: ItemRole,
    commands: Vec<Command>,
    checked: Option<Condition>,
    submenu: Option<NodeId>,
    /// A readout printed at the right (a submenu's current value).
    value: Option<String>,
    danger: bool,
}

impl Row {
    fn action(label: &str, commands: Vec<Command>) -> Self {
        Self {
            label: label.into(),
            role: ItemRole::Action,
            commands,
            checked: None,
            submenu: None,
            value: None,
            danger: false,
        }
    }
    fn header(label: &str, value: Option<String>) -> Self {
        Self {
            role: ItemRole::Header,
            value,
            ..Self::action(label, Vec::new())
        }
    }
    fn divider() -> Self {
        Self {
            role: ItemRole::Divider,
            ..Self::action("", Vec::new())
        }
    }
    fn sub(label: &str, menu: NodeId, value: String) -> Self {
        Self {
            submenu: Some(menu),
            value: Some(value),
            ..Self::action(&format!("{label}  ›"), Vec::new())
        }
    }
    fn checked(mut self, c: Condition) -> Self {
        self.checked = Some(c);
        self
    }
    fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}

/// A context menu of `rows` under `parent` (hidden until opened), bound to
/// the band template's items (`context`).
fn menu(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    w: f64,
    rows: &[Row],
    context: Option<NodeId>,
) -> NodeId {
    let height = |r: &Row| {
        if r.role == ItemRole::Divider {
            9.0
        } else {
            ROW_H
        }
    };
    let h = rows.iter().map(height).sum::<f64>() + 8.0;
    let mut shell = draw::frame(name, 0.0, 0.0, w, h);
    shell.fills = vec![draw::solid("#121216fa")];
    let shell = draw::with_effects(
        draw::rounded(draw::stroked(shell, "#5a5a628c", 1.0), 6.0),
        vec![draw::drop_shadow("#00000099", 0.0, 8.0, 24.0)],
    );
    let root = b.add(parent, shell);
    let hl = frame(
        b,
        root,
        "highlight",
        (4.0, 4.0, w - 8.0, ROW_H),
        Some("#7896ff29"),
        4.0,
    );
    let mut items = Vec::new();
    let mut y = 4.0;
    for r in rows {
        let rh = height(r);
        let row = b.add(
            root,
            draw::frame(
                if r.label.is_empty() {
                    "divider"
                } else {
                    &r.label
                },
                0.0,
                y,
                w,
                rh,
            ),
        );
        let mut item = MenuItem::action(row.0, r.commands.clone());
        item.role = r.role;
        item.submenu = r.submenu.map(|s| s.0);
        match r.role {
            ItemRole::Divider => {
                frame(
                    b,
                    row,
                    "rule",
                    (8.0, 4.0, w - 16.0, 1.0),
                    Some("#ffffff1a"),
                    0.0,
                );
            }
            ItemRole::Header => {
                let t = text(
                    b,
                    row,
                    "header",
                    &r.label,
                    (10.0, 4.5, w - 20.0),
                    9.0,
                    "#ffd43b",
                    TextAlign::Left,
                    700.0,
                );
                if let Some(n) = b.scene.node_mut(t)
                    && let NodeKind::Text(d) = &mut n.kind
                {
                    d.letter_spacing = 0.6;
                }
                if let Some(addr) = &r.value {
                    readout(
                        b,
                        row,
                        (w * 0.42, 4.5, w * 0.58 - 10.0),
                        addr,
                        "#ffd43b",
                        TextAlign::Left,
                        700.0,
                    );
                }
            }
            ItemRole::Action => {
                if let Some(c) = &r.checked {
                    let mark = text(
                        b,
                        row,
                        "check",
                        "✓",
                        (8.0, 3.5, 12.0),
                        10.0,
                        "#69db7c",
                        TextAlign::Left,
                        700.0,
                    );
                    item.check = Some(mark.0);
                    item.checked = Some(c.clone());
                }
                text(
                    b,
                    row,
                    "label",
                    &r.label,
                    (24.0, 3.5, w - 30.0),
                    10.0,
                    if r.danger { "#ff6e6ef2" } else { "#d4d4d8" },
                    TextAlign::Left,
                    500.0,
                );
                if let Some(addr) = &r.value {
                    readout(
                        b,
                        row,
                        (w * 0.55, 3.5, w * 0.45 - 12.0),
                        addr,
                        DIM,
                        TextAlign::Right,
                        500.0,
                    );
                }
            }
        }
        items.push(item);
        y += rh;
    }
    let c = Control::new(ControlKind::Menu {
        items,
        context: context.map(|c| c.0),
    })
    .slot(SlotRole::Highlight, hl.0);
    b.controls.insert(root, c);
    root
}

/// A submenu picking one of `options` for band param `k`, each checked
/// while selected.
#[allow(clippy::too_many_arguments)]
fn submenu(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    ad: &Addrs,
    context: NodeId,
    options: &[&str],
    k: &str,
    commands: impl Fn(usize) -> Vec<Command>,
) -> NodeId {
    let rows: Vec<Row> = options
        .iter()
        .enumerate()
        .map(|(i, o)| {
            Row::action(o, commands(i)).checked(Condition::any_of(ad.band(k), &[i as f64]))
        })
        .collect();
    menu(b, parent, name, SUB_W, &rows, Some(context))
}

/// A text readout of `addr`.
fn readout(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w): (f64, f64, f64),
    addr: &str,
    color: &str,
    align: TextAlign,
    weight: f64,
) -> NodeId {
    let holder = b.add(parent, draw::frame("readout", x, y, w, 13.0));
    let t = text(
        b,
        holder,
        "value",
        "",
        (0.0, 0.0, w),
        10.0,
        color,
        align,
        weight,
    );
    b.controls.insert(
        holder,
        Control::new(ControlKind::Readout)
            .bound(Binding::param(addr))
            .slot(SlotRole::Value, t.0),
    );
    holder
}

/// A small button of `mode` bound to `addr`, printing `text` (or the
/// value), lit (`On` slot) while on.
#[allow(clippy::too_many_arguments)]
fn button(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    r: (f64, f64, f64, f64),
    mode: ButtonMode,
    addr: Option<&str>,
    glyph: Option<&str>,
    color: &str,
) -> NodeId {
    let btn = stroked_frame(b, parent, name, r, "#1b1b22", PANEL_RIM, 4.0);
    let lit = frame(
        b,
        btn,
        "lit",
        (1.0, 1.0, r.2 - 2.0, r.3 - 2.0),
        Some("#ffffff14"),
        3.0,
    );
    let t = text(
        b,
        btn,
        "value",
        glyph.unwrap_or(""),
        (0.0, (r.3 - 13.0) / 2.0, r.2),
        10.0,
        color,
        TextAlign::Center,
        600.0,
    );
    let mut c = Control::new(ControlKind::Button { mode }).slot(SlotRole::Label, t.0);
    if glyph.is_none() {
        c = c.slot(SlotRole::Value, t.0);
    }
    if matches!(mode, ButtonMode::Toggle) {
        c = c.slot(SlotRole::On, lit.0);
    } else if let Some(n) = b.scene.node_mut(lit) {
        n.visible = false;
    }
    c.binding = addr.map(Binding::param);
    b.controls.insert(btn, c);
    btn
}

/// The band panel: one popover for all bands, following the active one:
/// docked to the bottom of the plot and following its band
/// only horizontally (Pro-Q style), so it never jumps while the band is
/// dragged and its readouts stay where the eye is. It floats 18 px above
/// the plot's floor, flush on a short plot where the gap would cost curve.
/// A fixed band's panel (`fixed`) has its shape and FREQ / GAIN / Q only;
/// `tint` colours its top edge.
fn band_panel(
    b: &mut Builder<'_>,
    root: NodeId,
    ad: &Addrs,
    (band, handle): (NodeId, NodeId),
    plot_h: f64,
    (fixed, tint): (bool, &[String]),
) {
    let mut shell = draw::frame("band panel", 0.0, 0.0, PANEL_W, PANEL_H);
    shell.fills = vec![draw::solid(PANEL_BG)];
    let shell = draw::with_effects(
        draw::rounded(draw::stroked(shell, PANEL_RIM, 1.0), 10.0),
        vec![draw::drop_shadow("#0000008c", 0.0, 6.0, 18.0)],
    );
    let pop = b.add(root, shell);
    // The band's colour along the top edge.
    let bar_holder = b.add(
        pop,
        draw::frame("band colour", 10.0, 0.0, PANEL_W - 20.0, 2.0),
    );
    let bar = frame(
        b,
        bar_holder,
        "bar",
        (0.0, 0.0, PANEL_W - 20.0, 2.0),
        Some("#ff6b6b"),
        1.0,
    );
    let mut bc = Control::new(ControlKind::Readout)
        .bound(Binding::param(ad.band("freq")))
        .slot(SlotRole::Tint, bar.0);
    let freq_axis = Axis::Log {
        f_min: 10.0,
        f_max: 30_000.0,
    };
    bc.tint = if tint.len() == 2 && tint[0] == tint[1] {
        tint.to_vec()
    } else {
        ramp_over(&freq_axis)
    };
    b.controls.insert(bar_holder, bc);

    // Row 1: bypass, shape, name, placement, focus, delete.
    let y = 9.0;
    if fixed {
        button(
            b,
            pop,
            "shape",
            (8.0, y, PANEL_W - 16.0, 22.0),
            ButtonMode::Cycle,
            Some(&ad.band("shape")),
            None,
            "#ffd43b",
        );
    } else {
        button(
            b,
            pop,
            "bypass",
            (8.0, y, 22.0, 22.0),
            ButtonMode::Toggle,
            Some(&ad.band("on")),
            Some("●"),
            INK,
        );
        let shape = button(
            b,
            pop,
            "shape",
            (34.0, y, 70.0, 22.0),
            ButtonMode::Cycle,
            Some(&ad.band("shape")),
            None,
            "#ffd43b",
        );
        let _ = shape;
        let name = stroked_frame(
            b,
            pop,
            "name field",
            (108.0, y, 104.0, 22.0),
            "#0c0c10f5",
            PANEL_RIM,
            4.0,
        );
        let sel = frame(
            b,
            name,
            "selection",
            (6.0, 4.0, 1.0, 14.0),
            Some("#4dabf766"),
            2.0,
        );
        let hint = text(
            b,
            name,
            "placeholder",
            "Name…",
            (6.0, 5.0, 92.0),
            10.0,
            "#6a6a76",
            TextAlign::Left,
            500.0,
        );
        let value = text(
            b,
            name,
            "value",
            "",
            (6.0, 5.0, 92.0),
            10.0,
            INK,
            TextAlign::Left,
            600.0,
        );
        let caret = frame(b, name, "caret", (6.0, 4.0, 1.5, 14.0), Some(INK), 0.0);
        let nc = Control::new(ControlKind::TextField {
            click_to_edit: true,
            max_len: 32,
        })
        .bound(Binding::param(ad.band("name")))
        .slot(SlotRole::Value, value.0)
        .slot(SlotRole::Placeholder, hint.0)
        .slot(SlotRole::Caret, caret.0)
        .slot(SlotRole::TextSelection, sel.0);
        b.controls.insert(name, nc);
        button(
            b,
            pop,
            "placement",
            (216.0, y, 26.0, 22.0),
            ButtonMode::Cycle,
            Some(&ad.band("placement")),
            None,
            INK,
        );
        button(
            b,
            pop,
            "focus",
            (246.0, y, 22.0, 22.0),
            ButtonMode::Toggle,
            Some(&ad.band("focus")),
            Some("F"),
            INK,
        );
        let del = button(
            b,
            pop,
            "delete",
            (272.0, y, 22.0, 22.0),
            ButtonMode::Trigger,
            None,
            Some("×"),
            "#ff6e6e",
        );
        if let Some(c) = b.controls.get_mut(&del) {
            c.interact = Some(Interact {
                press: vec![Command::Remove],
                ..Interact::default()
            });
        }
    }

    // Row 2: FREQ, GAIN, Q (a cut: SLOPE), DYN.
    let knobs: [(&str, &str, Option<Condition>); 4] = [
        ("FREQ", "freq", None),
        (
            "GAIN",
            "gain",
            Some(Condition::any_of(ad.band("shape"), &NO_GAIN).negated()),
        ),
        (
            "Q",
            "q",
            Some(Condition::any_of(ad.band("shape"), &CUTS).negated()),
        ),
        (
            "DYN",
            "dyn_range",
            Some(Condition::any_of(ad.band("shape"), &NO_GAIN).negated()),
        ),
    ];
    let knobs = &knobs[..if fixed { 3 } else { 4 }];
    let cw = (PANEL_W - 16.0) / knobs.len() as f64;
    for (i, (legend, k, when)) in knobs.iter().cloned().enumerate() {
        let x0 = 8.0 + cw * i as f64;
        let cell = b.add(
            pop,
            draw::frame(&format!("{legend} cell"), x0, 36.0, cw, 64.0),
        );
        let p = Place::new(legend)
            .size(30.0)
            .bind(Binding::param(ad.band(k)))
            .legend(legend);
        let Ok(knob) = crate::surface::kit::place_in_cell(b, cell, "knob.soft_pointer", &p) else {
            continue;
        };
        let holder = b.scene.node(knob).and_then(|n| n.parent).unwrap_or(knob);
        let hw = b.scene.node(holder).map_or(0.0, |n| n.size.width);
        if let Some(n) = b.scene.node_mut(holder) {
            n.transform = crate::document::proto::Transform::translate((cw - hw) / 2.0, 0.0);
        }
        if let (Some(c), Some(when)) = (b.controls.get_mut(&knob), when.clone()) {
            c.show_if = Some(when);
        }
        if k == "q" && !fixed {
            // A cut shows its slope where the Q is.
            let slope = button(
                b,
                cell,
                "slope",
                ((cw - 64.0) / 2.0, 14.0, 64.0, 22.0),
                ButtonMode::Cycle,
                Some(&ad.band("slope")),
                None,
                INK,
            );
            if let Some(c) = b.controls.get_mut(&slope) {
                c.show_if = Some(Condition::any_of(ad.band("shape"), &CUTS));
            }
            text(
                b,
                slope,
                "slope legend",
                "SLOPE",
                (0.0, 28.0, 64.0),
                8.0,
                DIM,
                TextAlign::Center,
                600.0,
            );
        }
    }
    let gap = if plot_h < 420.0 { 0.0 } else { 18.0 };
    let c = Control::new(ControlKind::Popover {
        anchor: Some(handle.0),
        placement: Placement::DockBottom,
        gap,
        show: Show::Active,
        context: Some(band.0),
    });
    b.controls.insert(pop, c);
}

/// The touch band panel's width.
const TOUCH_PANEL_W: f64 = 300.0;

/// The band panel at a finger's size, pinned beside the plot (`touch`):
/// bypass, shape (or a cut's slope), placement, focus and delete on a row
/// of buttons 40 tall; FREQ, GAIN, Q and DYN dials under them. Its ground
/// is always there; its controls are up while a band is active.
fn band_panel_touch(
    b: &mut Builder<'_>,
    root: NodeId,
    ad: &Addrs,
    band: NodeId,
    handle: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
) {
    // The ground, always drawn.
    let mut ground = draw::frame("band panel ground", x, y, w, h);
    ground.fills = vec![draw::solid(PANEL_BG)];
    let ground = draw::stroked(ground, PANEL_RIM, 1.0);
    b.add(root, ground);
    let mut shell = draw::frame("band panel", x, y, w, h);
    shell.fills = vec![draw::solid(PANEL_BG)];
    let pop = b.add(root, shell);
    // The band's colour along the top edge.
    let bar_holder = b.add(pop, draw::frame("band colour", 0.0, 0.0, w, 3.0));
    let bar = frame(
        b,
        bar_holder,
        "bar",
        (0.0, 0.0, w, 3.0),
        Some("#ff6b6b"),
        0.0,
    );
    let mut bc = Control::new(ControlKind::Readout)
        .bound(Binding::param(ad.band("freq")))
        .slot(SlotRole::Tint, bar.0);
    bc.tint = ramp_over(&Axis::Log {
        f_min: 10.0,
        f_max: 30_000.0,
    });
    b.controls.insert(bar_holder, bc);

    // Row 1: bypass, shape (a cut: its slope beside) — the corner beyond
    // is the host's (its preset stepper).
    let (y1, bh) = (12.0, 40.0);
    button(
        b,
        pop,
        "bypass",
        (10.0, y1, 40.0, bh),
        ButtonMode::Toggle,
        Some(&ad.band("on")),
        Some("●"),
        INK,
    );
    button(
        b,
        pop,
        "shape",
        (56.0, y1, 92.0, bh),
        ButtonMode::Cycle,
        Some(&ad.band("shape")),
        None,
        "#ffd43b",
    );
    let slope = button(
        b,
        pop,
        "slope",
        (154.0, y1, 54.0, bh),
        ButtonMode::Cycle,
        Some(&ad.band("slope")),
        None,
        INK,
    );
    if let Some(c) = b.controls.get_mut(&slope) {
        c.show_if = Some(Condition::any_of(ad.band("shape"), &CUTS));
    }
    // Row 2: FREQ, GAIN, Q, DYN — dials a finger turns.
    let knobs: [(&str, &str, Option<Condition>); 4] = [
        ("FREQ", "freq", None),
        (
            "GAIN",
            "gain",
            Some(Condition::any_of(ad.band("shape"), &NO_GAIN).negated()),
        ),
        (
            "Q",
            "q",
            Some(Condition::any_of(ad.band("shape"), &CUTS).negated()),
        ),
        (
            "DYN",
            "dyn_range",
            Some(Condition::any_of(ad.band("shape"), &NO_GAIN).negated()),
        ),
    ];
    let cw = (w - 12.0) / 4.0;
    for (i, (legend, k, when)) in knobs.into_iter().enumerate() {
        let cell = b.add(
            pop,
            draw::frame(
                &format!("{legend} cell"),
                6.0 + cw * i as f64,
                y1 + bh + 14.0,
                cw,
                110.0,
            ),
        );
        let place = Place::new(legend)
            .size(52.0)
            .bind(Binding::param(ad.band(k)))
            .legend(legend);
        let Ok(knob) = crate::surface::kit::place_in_cell(b, cell, "knob.soft_pointer", &place)
        else {
            continue;
        };
        let holder = b.scene.node(knob).and_then(|n| n.parent).unwrap_or(knob);
        let hw = b.scene.node(holder).map_or(0.0, |n| n.size.width);
        if let Some(n) = b.scene.node_mut(holder) {
            n.transform = crate::document::proto::Transform::translate((cw - hw) / 2.0, 0.0);
        }
        if let (Some(c), Some(when)) = (b.controls.get_mut(&knob), when) {
            c.show_if = Some(when);
        }
    }

    // Row 3: where it plays (L/R/M/S), focus, delete.
    let y3 = h - bh - 12.0;
    button(
        b,
        pop,
        "placement",
        (10.0, y3, 70.0, bh),
        ButtonMode::Cycle,
        Some(&ad.band("placement")),
        None,
        INK,
    );
    button(
        b,
        pop,
        "focus",
        (86.0, y3, 70.0, bh),
        ButtonMode::Toggle,
        Some(&ad.band("focus")),
        Some("Focus"),
        INK,
    );
    // Delete, at the far corner from the dials.
    let del = button(
        b,
        pop,
        "delete",
        (w - 50.0, y3, 40.0, bh),
        ButtonMode::Trigger,
        None,
        Some("×"),
        "#ff6e6e",
    );
    if let Some(c) = b.controls.get_mut(&del) {
        c.interact = Some(Interact {
            press: vec![Command::Remove],
            ..Interact::default()
        });
    }
    let c = Control::new(ControlKind::Popover {
        anchor: Some(handle.0),
        placement: Placement::Pinned,
        gap: 0.0,
        show: Show::Active,
        context: Some(band.0),
    });
    b.controls.insert(pop, c);
}

/// The readout chip above the active band: frequency and gain, Q.
fn chip(b: &mut Builder<'_>, root: NodeId, ad: &Addrs, band: NodeId, handle: NodeId) {
    let mut shell = draw::frame("band chip", 0.0, 0.0, 124.0, 32.0);
    shell.fills = vec![draw::solid("#121216f0")];
    let shell = draw::with_effects(
        draw::rounded(draw::stroked(shell, PANEL_RIM, 1.0), 6.0),
        vec![draw::drop_shadow("#00000080", 0.0, 3.0, 10.0)],
    );
    let pop = b.add(root, shell);
    readout(
        b,
        pop,
        (6.0, 3.0, 62.0),
        &ad.band("freq"),
        "#ffd43b",
        TextAlign::Left,
        600.0,
    );
    let g = readout(
        b,
        pop,
        (62.0, 3.0, 56.0),
        &ad.band("gain"),
        "#ffd43b",
        TextAlign::Right,
        600.0,
    );
    if let Some(c) = b.controls.get_mut(&g) {
        c.format = Some(ValueFormat::decimals(1));
        c.show_if = Some(Condition::any_of(ad.band("shape"), &NO_GAIN).negated());
    }
    text(
        b,
        pop,
        "q legend",
        "Q",
        (6.0, 17.0, 12.0),
        9.0,
        DIM,
        TextAlign::Left,
        500.0,
    );
    let q = readout(
        b,
        pop,
        (18.0, 17.0, 50.0),
        &ad.band("q"),
        DIM,
        TextAlign::Left,
        500.0,
    );
    if let Some(c) = b.controls.get_mut(&q) {
        c.format = Some(ValueFormat::decimals(2).unit(""));
    }
    let shape = readout(
        b,
        pop,
        (62.0, 17.0, 56.0),
        &ad.band("shape"),
        DIM,
        TextAlign::Right,
        500.0,
    );
    let _ = shape;
    let c = Control::new(ControlKind::Popover {
        anchor: Some(handle.0),
        placement: Placement::Above,
        gap: 20.0,
        show: Show::Active,
        context: Some(band.0),
    });
    b.controls.insert(pop, c);
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    let _ = PLACEMENTS;
    vec![ComponentDef {
        id: "viz.eq_graph",
        family: Family::Viz,
        title: "EQ graph",
        description: "A full parametric EQ editor over an analyser: up to 24 bands (`<ns>/b{n}_{used,on,freq,gain,q,shape,slope,name,placement,focus,dyn_range}`, `bands=`), each a handle tinted by its frequency with its name riding above it. Double-click empty space to add a band (its shape from where you click), drag handles (Alt: frequency only, Cmd: resonance), marquee- and Cmd/Shift-click to select several and drag them together, drag one out of the graph (or press Delete) to remove it, double-click to rename, right-click for its menu. The active band gets a panel (bypass, shape, name, placement, focus, delete, FREQ/GAIN/Q/DYN knobs) and a readout chip. `<ns>/range` zooms ±3…±30 dB; `<ns>/show_{pre,post,ext}` show the `<ns>/spectrum_pre`, `<ns>/spectrum` (or the binding) and `<ns>/sidechain` analysers. Drawn as signal's rig EQ (processor's graph: full-bleed, the GPU glow — analyser as light, a bloom per band — under its grid, curves and node rings); variant `panel` is a lit panel with printed scales. An EQ inside another unit (a saturator's emphasis): `fixed` — its bands always there and on, each only `b{n}_{freq,gain,q,shape}`, nothing adds, deletes, bypasses or renames one; `shapes=N` — the first N shapes only; `db_range=` — a fixed ± dB span, no picker; `tint=#hex` — bands, sum and glow in one colour; `no_analyser` — no analyser layers.",
        variants: &["panel"],
        default_size: 820.0,
        binds: "params by namespace",
        build: eq_graph,
    }]
}
