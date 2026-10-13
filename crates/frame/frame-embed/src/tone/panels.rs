//! The Tone rack's panels as frame nodes and controls.
//!
//! One function per unit, each drawing into a body box at a tier, with the
//! geometry of session's `tone.rs` (`body_of`, `display_of`, `lane_of`,
//! `sat_split`, the comp's strips and arrow, the gate's two lines): the
//! same boxes, so the curves across a mixer of both sit on the same lines.
//!
//! What moves with params is a control or a [`crate::Derived`] property;
//! what moves with the audio is a `Plot` (or a `Curve` reading a feed) and
//! so lands in the embed's live pass.

use document_proto::{NodeId, NodeKind, Paint, TextAlign, Transform};
use surface_kit::draw;
use surface_proto::{
    Axis, Binding, ButtonMode, Control, ControlKind, CurveInput, HandleAxis, Orientation,
    PlotRange, PlotStyle, SlotRole,
};

use super::catalog::EQ_RANGES;
use super::style::{self as s, alpha};
use super::{Kit, Phase, Rect4, Row, Which};
use crate::curves::{self, CIRCUITS, DELAY_FAMILIES, ROOM_FAMILIES};
use crate::embed::{BuildCtx, Values};
use crate::form::Tier;

// ── the column ───────────────────────────────────────────────────────────

/// Draw `rows` into `parent` (their rects are in `parent`'s space).
pub fn column(k: &mut Kit, parent: NodeId, ctx: &BuildCtx, rows: &[(Row, Rect4)], bands: usize) {
    let tier = ctx.tier;
    if !tier.on() {
        return;
    }
    let folded = super::folded(&ctx.values);
    for (row, at) in rows {
        // Culled: nothing to draw outside the box.
        if at.1 + at.3 < 0.0 || at.1 > ctx.size.1 {
            continue;
        }
        match (*row, tier) {
            (Row::Head(p) | Row::Blank(p), Tier::Minimal) => {
                let tint = if matches!(row, Row::Blank(_)) {
                    alpha(p.tint(), s::BLANK_ALPHA)
                } else {
                    p.tint().to_string()
                };
                s::fill(
                    &mut k.b(),
                    parent,
                    "phase tick",
                    (at.0, at.1 + 3.0, 2.0, (at.3 - 6.0).max(0.0)),
                    &tint,
                );
            }
            (Row::Head(p), _) => container(k, parent, p, *at, folded.contains(&p)),
            (Row::Blank(p), _) => blank(k, parent, p, *at),
            (Row::Unit(w), Tier::Minimal) => minimal(k, parent, ctx, w, inset(*at, 1.0), bands),
            (Row::Unit(w), _) => unit(k, parent, ctx, w, *at, bands),
        }
    }
}

fn inset((x, y, w, h): Rect4, by: f64) -> Rect4 {
    (
        x + by,
        y + by,
        (w - 2.0 * by).max(0.0),
        (h - 2.0 * by).max(0.0),
    )
}

/// `by` off the top, and what is left.
fn split_top((x, y, w, h): Rect4, by: f64) -> (Rect4, Rect4) {
    ((x, y, w, by.min(h)), (x, y + by, w, (h - by).max(0.0)))
}

/// A unit's body: its box less the 2 px inset and, when the row can spare
/// it, the header (session `body_of`).
#[must_use]
pub fn body_of(at: Rect4) -> Rect4 {
    let inner = inset(at, 2.0);
    if inner.3 > s::HEAD * 2.0 {
        split_top(inner, s::HEAD).1
    } else {
        inner
    }
}

/// The part of a body a unit's main display takes (session `display_of`).
#[must_use]
pub fn display_of(body: Rect4, which: Which, tier: Tier) -> Rect4 {
    if !tier.detailed() {
        return body;
    }
    let keep = match which {
        Which::Gate => s::LANE + 2.0,
        Which::DeEss => s::BAND_STRIP + 2.0,
        Which::Sat | Which::Delay | Which::Reverb => s::SELECTOR + 2.0,
        _ => 0.0,
    };
    let over = which == Which::DeEss;
    (
        body.0,
        if over { body.1 + keep } else { body.1 },
        body.2,
        (body.3 - keep).max(0.0),
    )
}

/// The lane under (or, for the de-esser, over) a display, when it has one.
#[must_use]
pub fn lane_of(body: Rect4, which: Which, tier: Tier) -> Option<Rect4> {
    let d = display_of(body, which, tier);
    if d.3 >= body.3 {
        return None;
    }
    if which == Which::DeEss {
        return Some((body.0, body.1, body.2, s::BAND_STRIP));
    }
    Some((
        body.0,
        d.1 + d.3 + 2.0,
        body.2,
        (body.3 - d.3 - 2.0).max(0.0),
    ))
}

// ── containers ───────────────────────────────────────────────────────────

fn container(k: &mut Kit, parent: NodeId, phase: Phase, at: Rect4, shut: bool) {
    let (x, y, w, h) = at;
    let bar = s::fill(&mut k.b(), parent, "phase", at, s::WELL);
    s::fill(
        &mut k.b(),
        parent,
        "phase tint",
        (x, y, 2.0, h),
        phase.tint(),
    );
    s::text(
        &mut k.b(),
        parent,
        "phase name",
        phase.name(),
        (x + 6.0, y + h - 4.0, (w - 20.0).max(1.0)),
        8.0,
        s::TEXT_DIM,
        TextAlign::Left,
    );
    // The chevron: up when open, down when shut — where a click moves it.
    let (cx, cy, arm) = (x + w - 9.0, y + h / 2.0, 3.2);
    let (tip, base) = if shut {
        (cy + arm * 0.6, cy - arm * 0.6)
    } else {
        (cy - arm * 0.6, cy + arm * 0.6)
    };
    s::line(
        &mut k.b(),
        parent,
        "chevron",
        (w, h),
        s::poly(&[(cx - arm, base), (cx, tip), (cx + arm, base)]),
        s::TEXT_FAINT,
        1.4,
    );
    // The whole bar folds the phase.
    let c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .bound(Binding::param(phase.fold_addr()));
    k.controls.insert(bar, c);
}

fn blank(k: &mut Kit, parent: NodeId, phase: Phase, at: Rect4) {
    let (x, y, w, h) = at;
    s::fill(&mut k.b(), parent, "blank phase", at, &alpha(s::WELL, 0.5));
    s::fill(
        &mut k.b(),
        parent,
        "blank tint",
        (x, y, 2.0, h),
        &alpha(phase.tint(), s::BLANK_ALPHA),
    );
    s::text(
        &mut k.b(),
        parent,
        "phase name",
        phase.name(),
        (x + 6.0, y + h - 4.0, (w - 8.0).max(1.0)),
        8.0,
        &alpha(s::TEXT_FAINT, s::BLANK_ALPHA),
        TextAlign::Left,
    );
}

// ── one unit ─────────────────────────────────────────────────────────────

fn unit(k: &mut Kit, parent: NodeId, ctx: &BuildCtx, which: Which, at: Rect4, bands: usize) {
    let tier = ctx.tier;
    s::fill(
        &mut k.b(),
        parent,
        &format!("{} ground", which.name()),
        at,
        s::WELL,
    );
    let inner = inset(at, 2.0);
    let body = body_of(at);
    let head = (body.1 > inner.1).then(|| split_top(inner, s::HEAD).0);
    if body.2 > 0.0 && body.3 > 0.0 {
        let panel = s::clip(&mut k.b(), parent, which.name(), body);
        let local = (0.0, 0.0, body.2, body.3);
        match which {
            w if w.is_eq() => eq(k, panel, ctx, which, local, tier, bands),
            Which::Gate => gate(k, panel, which, local, tier),
            w if w.is_comp() => comp(k, panel, which, local, tier),
            Which::Sat => sat(k, panel, ctx, which, local, tier),
            Which::DeEss => deess(k, panel, which, local, tier),
            Which::Delay => delay(k, panel, ctx, which, local, tier),
            _ => reverb(k, panel, ctx, which, local, tier),
        }
        if let Some(lane) = lane_of(local, which, tier)
            && matches!(which, Which::Sat | Which::Delay | Which::Reverb)
        {
            selector(k, panel, ctx, which, lane);
        }
    }
    scrim(k, parent, which, at, head.unwrap_or(inner));
    if let Some(head) = head {
        header(k, parent, ctx, which, head);
    }
}

/// The bypass: a scrim over the whole unit, lit by `<unit>/bypass`, and
/// the header row is its switch.
fn scrim(k: &mut Kit, parent: NodeId, which: Which, at: Rect4, switch: Rect4) {
    let scrim = s::fill(
        &mut k.b(),
        parent,
        "bypass scrim",
        at,
        &alpha(s::WELL, 0.78),
    );
    let hit = s::frame(&mut k.b(), parent, "bypass switch", switch);
    let c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .bound(Binding::param(format!("{}/bypass", which.ns())))
    .slot(SlotRole::Glow, scrim.0);
    k.controls.insert(hit, c);
}

/// A unit's header: glyph, name, and its settings in one line — or
/// "BYPASS" when it is switched out.
fn header(k: &mut Kit, parent: NodeId, ctx: &BuildCtx, which: Which, at: Rect4) {
    let (x, y, w, _) = at;
    const SIZE: f64 = 7.0;
    let baseline = y + SIZE;
    let mut left = x;
    if let Some((family, index)) = glyph_of(which, ctx) {
        glyph(k, parent, family, index, (x, baseline - 0.5), s::TEXT_FAINT);
        left += GLYPH_W;
    }
    let name_w = s::text_width(which.name(), SIZE) + (left - x);
    if name_w > w {
        return;
    }
    let name = s::text(
        &mut k.b(),
        parent,
        "unit name",
        which.name(),
        (left, baseline, name_w.max(1.0) + 4.0),
        SIZE,
        s::TEXT_FAINT,
        TextAlign::Left,
    );
    let bypass = format!("{}/bypass", which.ns());
    let b2 = bypass.clone();
    k.derive(name, move |v, n| {
        if v.get(&b2).is_some_and(|b| *b >= 0.5) {
            n.fills = vec![Paint::solid(draw::hex(&alpha(s::TEXT_FAINT, 0.55)))];
        }
    });
    let value = s::text(
        &mut k.b(),
        parent,
        "unit value",
        "",
        (x, baseline, w),
        SIZE,
        s::TEXT_DIM,
        TextAlign::Right,
    );
    let tier = ctx.tier;
    k.derive(value, move |v, n| {
        let bypassed = v.get(&bypass).is_some_and(|b| *b >= 0.5);
        let text = if bypassed {
            "BYPASS".to_string()
        } else {
            summary(which, v, tier)
        };
        // No room: the row keeps its height, the value is not drawn.
        let fits = name_w + s::text_width(&text, SIZE) + 6.0 <= w;
        if let NodeKind::Text(t) = &mut n.kind {
            t.characters = if fits { text } else { String::new() };
        }
        if bypassed {
            n.fills = vec![Paint::solid(draw::hex(s::TEXT_FAINT))];
        }
    });
}

fn millis(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.1}s", ms / 1000.0)
    } else if ms < 10.0 {
        format!("{ms:.1}ms")
    } else {
        format!("{ms:.0}ms")
    }
}

fn get(v: &Values, addr: &str, default: f64) -> f64 {
    v.get(addr)
        .copied()
        .filter(|x| x.is_finite())
        .unwrap_or(default)
}

/// Whether an EQ band is doing anything.
fn band_used(v: &Values, ns: &str, n: usize) -> bool {
    let on = get(v, &format!("{ns}/b{n}_on"), 1.0) >= 0.5;
    let gain = get(v, &format!("{ns}/b{n}_gain"), 0.0);
    let shape = get(v, &format!("{ns}/b{n}_shape"), 0.0).round() as i64;
    on && (gain.abs() > 0.05 || matches!(shape, 3..=6))
}

/// What a unit's settings come to, in one short line (session
/// `Which::summary`).
#[must_use]
pub fn summary(which: Which, v: &Values, tier: Tier) -> String {
    let ns = which.ns();
    let g = |k: &str, d: f64| get(v, &format!("{ns}/{k}"), d);
    match which {
        w if w.is_eq() => {
            let used: Vec<usize> = (1..=24)
                .filter(|n| v.contains_key(&format!("{ns}/b{n}_on")) && band_used(v, ns, *n))
                .collect();
            let range = used
                .iter()
                .map(|n| get(v, &format!("{ns}/b{n}_gain"), 0.0).abs())
                .fold(0.0, f64::max);
            if used.is_empty() {
                "flat".into()
            } else {
                format!("{} · {range:.1}dB", used.len())
            }
        }
        w if w.is_comp() => {
            let head = format!("{:.0}dB · {:.1}:1", g("threshold", -18.0), g("ratio", 4.0));
            if tier.editing() {
                format!(
                    "{head} · {}/{}",
                    millis(g("attack", 10.0)),
                    millis(g("release", 150.0))
                )
            } else {
                head
            }
        }
        Which::Gate => format!("{:.0}dB · {:.0}", g("threshold", -40.0), g("range", -18.0)),
        Which::Sat => {
            let rungs = curves::harmonics(
                g("drive", 2.0),
                g("bias", 0.15),
                g("circuit", 0.0).round().max(0.0) as usize,
                1.0,
            );
            format!(
                "x{:.1} · 2nd {:.0}%",
                g("drive", 2.0),
                curves::even_share(&rungs) * 100.0
            )
        }
        Which::DeEss => format!("{:.0}dB · {:.0}", g("depth", 6.0), g("threshold", -24.0)),
        Which::Delay => {
            let head = format!(
                "{} · {:.0}%",
                millis(g("time", 300.0)),
                g("feedback", 0.38) * 100.0
            );
            let fam = DELAY_FAMILIES
                .get(g("family", 0.0).round().max(0.0) as usize)
                .copied()
                .unwrap_or("");
            if tier.editing() {
                format!("{head} · {}", fam.to_lowercase())
            } else {
                head
            }
        }
        _ => {
            let head = format!("{:.1}s · {:.0}%", g("decay", 1.8), g("mix", 0.18) * 100.0);
            let fam = ROOM_FAMILIES
                .get(g("family", 1.0).round().max(0.0) as usize)
                .copied()
                .unwrap_or("");
            if tier.editing() {
                format!("{head} · {}", fam.to_lowercase())
            } else {
                head
            }
        }
    }
}

// ── controls and traces ──────────────────────────────────────────────────

fn curve_control(
    provider: &str,
    inputs: Vec<CurveInput>,
    series: Option<&str>,
    style: PlotStyle,
    x: Axis,
    y: PlotRange,
) -> Control {
    Control::new(ControlKind::Curve {
        provider: provider.into(),
        inputs,
        series: series.map(Into::into),
        style,
        x,
        y,
    })
}

/// A curve (or plot) frame the size of `at` holding its area and/or line.
#[allow(clippy::too_many_arguments)]
fn traced(
    k: &mut Kit,
    parent: NodeId,
    name: &str,
    at: Rect4,
    control: Control,
    line: Option<(&str, f64, bool)>,
    area: Option<&str>,
) -> (NodeId, Option<NodeId>, Option<NodeId>) {
    let f = s::frame(&mut k.b(), parent, name, at);
    let mut c = control;
    let a = area.map(|color| {
        let n = s::area(&mut k.b(), f, "area", (at.2, at.3), color);
        c = c.clone().slot(SlotRole::Area, n.0);
        n
    });
    let l = line.map(|(color, width, dash)| {
        let n = s::trace(&mut k.b(), f, "line", (at.2, at.3), color, width, dash);
        c = c.clone().slot(SlotRole::Trace, n.0);
        n
    });
    k.controls.insert(f, c);
    (f, a, l)
}

fn hline(k: &mut Kit, parent: NodeId, at: Rect4, y: f64, color: &str, width: f64) -> NodeId {
    s::line(
        &mut k.b(),
        parent,
        "rule",
        (at.2, at.3),
        s::seg((at.0, y), (at.0 + at.2, y)),
        color,
        width,
    )
}

fn vline(k: &mut Kit, parent: NodeId, at: Rect4, x: f64, color: &str) -> NodeId {
    s::line(
        &mut k.b(),
        parent,
        "rule",
        (at.2, at.3),
        s::seg((x, at.1), (x, at.1 + at.3)),
        color,
        1.0,
    )
}

// ── EQ ───────────────────────────────────────────────────────────────────

const DECADES: [(f64, &str); 3] = [(100.0, "100"), (1_000.0, "1k"), (10_000.0, "10k")];

fn ladder_step(range: f64) -> f64 {
    if range <= 6.0 {
        3.0
    } else if range <= 12.0 {
        6.0
    } else {
        12.0
    }
}

fn freq_x(hz: f64, w: f64) -> f64 {
    (hz / 20.0).log10() / 3.0 * w
}

/// The EQ graph: the plugin painter's look — each band filled in its own
/// hue, the total in gold, handles on the curve — at a strip's size.
fn eq(k: &mut Kit, p: NodeId, ctx: &BuildCtx, which: Which, at: Rect4, tier: Tier, bands: usize) {
    let ns = which.ns();
    let (w, h) = (at.2, at.3);
    let range = EQ_RANGES
        .get(ctx.value(&format!("{ns}/range"), 2.0).round().max(0.0) as usize)
        .copied()
        .unwrap_or(12.0);
    let db_y = |db: f64| (0.5 - db / (2.0 * range)) * h;
    // Minimal draws the graph at Full's detail (session: a rail is thin,
    // not blind), without the analyser.
    let detail = if tier == Tier::Minimal {
        Tier::Full
    } else {
        tier
    };

    if detail == Tier::Full {
        for (hz, name) in DECADES {
            let x = freq_x(hz, w);
            vline(k, p, at, x, s::GRID_BEAT);
            let lw = s::text_width(name, 6.0);
            if x - lw / 2.0 >= 0.0 && x + lw / 2.0 <= w && tier != Tier::Minimal {
                s::text(
                    &mut k.b(),
                    p,
                    "decade",
                    name,
                    (x - lw, h - 2.0, lw * 2.0),
                    6.0,
                    s::TEXT_FAINT,
                    TextAlign::Center,
                );
            }
        }
    }
    if detail == Tier::Focus {
        // The plugin's own grid: every 1-2-5, decades bolder, labelled.
        for dec in [10.0, 100.0, 1000.0, 10_000.0] {
            for m in [2.0, 5.0, 10.0] {
                let hz: f64 = dec * m;
                if hz > 20.0 && hz < 20_000.0 {
                    vline(
                        k,
                        p,
                        at,
                        freq_x(hz, w),
                        if m == 10.0 { s::GRID } else { s::GRID_BEAT },
                    );
                }
            }
        }
        for (hz, name) in [
            (50.0, "50"),
            (100.0, "100"),
            (200.0, "200"),
            (500.0, "500"),
            (1000.0, "1k"),
            (2000.0, "2k"),
            (5000.0, "5k"),
            (10_000.0, "10k"),
        ] {
            let x = freq_x(hz, w);
            s::text(
                &mut k.b(),
                p,
                "freq",
                name,
                (x - 14.0, h - 2.0, 28.0),
                7.0,
                s::TEXT_FAINT,
                TextAlign::Center,
            );
        }
    }
    if detail.detailed() {
        let step = ladder_step(range);
        let mut gain = step;
        while gain <= range + 1e-9 {
            for db in [gain, -gain] {
                hline(k, p, at, db_y(db), s::GRID_BEAT, 1.0);
                if detail == Tier::Focus && gain < range {
                    s::text(
                        &mut k.b(),
                        p,
                        "db",
                        &format!("{db:+.0}"),
                        (2.0, db_y(db) + 3.0, 24.0),
                        6.5,
                        s::TEXT_FAINT,
                        TextAlign::Left,
                    );
                }
            }
            gain += step;
        }
    }
    // Unity, always: without it a boost and a cut look the same.
    hline(k, p, at, db_y(0.0), s::GRID, 1.0);

    let x_axis = Axis::Log {
        f_min: 20.0,
        f_max: 20_000.0,
    };
    let y = PlotRange::linear(-range, range);

    // The analyser: a wash behind the graph at a glance width, the
    // painter's own at focus.
    if tier != Tier::Minimal {
        let (fill, line) = if tier == Tier::Focus {
            ("#8c8c9624", "#8c8c9659")
        } else {
            ("#8c8c9612", "#8c8c9638")
        };
        let c = Control::new(ControlKind::Plot {
            style: PlotStyle::Area { baseline: -40.0 },
            x: x_axis.clone(),
            y: PlotRange::linear(-40.0, 16.0),
        })
        .bound(Binding::stream(format!("{ns}/spectrum")));
        traced(
            k,
            p,
            "spectrum",
            at,
            c,
            Some((line, 1.0, false)),
            Some(fill),
        );
    }

    let mut inputs = Vec::new();
    for n in 1..=bands {
        for key in ["freq", "gain", "q", "shape", "on"] {
            inputs.push(CurveInput::param(
                format!("b{n}_{key}"),
                format!("{ns}/b{n}_{key}"),
            ));
        }
        inputs.push(CurveInput::constant(format!("b{n}_slope"), 12.0));
    }
    // Each band in its own hue, filled to unity (the painter's look).
    for n in 1..=bands {
        let c = curve_control(
            "eq.response",
            inputs.clone(),
            Some(&format!("b{n}")),
            PlotStyle::Area { baseline: 0.0 },
            x_axis.clone(),
            y.clone(),
        );
        let (_, area, line) = traced(
            k,
            p,
            &format!("band {n}"),
            at,
            c,
            Some(("#ffffffbf", 1.0, false)),
            Some("#ffffff59"),
        );
        let (fa, fl) = (format!("{ns}/b{n}_freq"), format!("{ns}/b{n}_on"));
        for (node, a) in [(area, 0.35), (line, 0.75)] {
            let Some(node) = node else { continue };
            let (fa, fl) = (fa.clone(), fl.clone());
            k.derive(node, move |v, nd| {
                let hue = s::band_color(get(v, &fa, 1000.0));
                let paint = Paint::solid(document_proto::Color { a, ..hue });
                if nd.stroke.weight > 0.0 && !nd.stroke.paints.is_empty() {
                    nd.stroke.paints = vec![paint];
                } else {
                    nd.fills = vec![paint];
                }
                if get(v, &fl, 1.0) < 0.5 {
                    nd.visible = false;
                }
            });
        }
    }
    // The total, in the painter's gold.
    let width = if tier == Tier::Focus { 2.0 } else { 1.5 };
    let c = curve_control(
        "eq.response",
        inputs,
        Some("sum"),
        PlotStyle::Line,
        x_axis.clone(),
        y,
    );
    traced(
        k,
        p,
        "response",
        at,
        c,
        Some((s::EQ_INK, width, false)),
        None,
    );

    // The bands as handles: dots at Full (and on a rail), the plugin's
    // bigger nodes at focus, none at Curves (a kink, not a target).
    if matches!(detail, Tier::Full | Tier::Focus) {
        let focus = detail == Tier::Focus;
        let r = if focus { 4.5 } else { s::HANDLE };
        let bx = if focus { 16.0 } else { 12.0 };
        for n in 1..=bands {
            let node = s::frame(
                &mut k.b(),
                p,
                &format!("band {n} handle"),
                ((w - bx) / 2.0, (h - bx) / 2.0, bx, bx),
            );
            let ring = s::dot(
                &mut k.b(),
                node,
                "ring",
                (bx / 2.0, bx / 2.0),
                r + 1.0,
                if focus { "#ffffffd9" } else { s::WELL },
            );
            let dot = s::dot(&mut k.b(), node, "dot", (bx / 2.0, bx / 2.0), r, "#ffffff");
            let _ = ring;
            let kind = ControlKind::Handle {
                x: Some(Binding::param(format!("{ns}/b{n}_freq"))),
                y: Some(Binding::param(format!("{ns}/b{n}_gain"))),
                wheel: Some(Binding::param(format!("{ns}/b{n}_q"))),
                x_map: HandleAxis::new(x_axis.clone()),
                y_map: HandleAxis::new(Axis::Linear {
                    min: -range,
                    max: range,
                }),
            };
            k.controls.insert(node, Control::new(kind));
            let (fa, n2, ns2) = (format!("{ns}/b{n}_freq"), n, ns.to_string());
            k.derive(dot, move |v, nd| {
                nd.fills = vec![Paint::solid(s::band_color(get(v, &fa, 1000.0)))]
            });
            k.derive(node, move |v, nd| nd.visible = band_used(v, &ns2, n2));
        }
    }
    // The zoom chip: what the height means, and the control that changes it.
    if detail == Tier::Full {
        let (cw, ch) = (30.0_f64.min(w), 12.0_f64.min(h));
        if cw >= 20.0 && ch >= 8.0 {
            let chip = s::fill(
                &mut k.b(),
                p,
                "zoom",
                ((w - cw) / 2.0, 1.0, cw, ch),
                s::WELL,
            );
            let label = s::text(
                &mut k.b(),
                p,
                "zoom value",
                &format!("±{range:.0}"),
                ((w - cw) / 2.0, 1.0 + ch - 3.0, cw),
                8.0,
                s::TEXT_FAINT,
                TextAlign::Center,
            );
            let _ = label;
            let c =
                Control::new(ControlKind::Selector).bound(Binding::param(format!("{ns}/range")));
            k.controls.insert(chip, c);
        }
    }
}

// ── compressor ───────────────────────────────────────────────────────────

/// dB (0 at the top … −60 at the floor) → y (the comp editor's axis).
fn comp_y(db: f64, h: f64) -> f64 {
    ((-db) / 60.0).clamp(0.0, 1.0) * h
}

const ARROW_X: f64 = 14.0;
const STRIP_H: f64 = 5.0;
const STRIP_GAP: f64 = 4.0;
const STRIP_LABEL_W: f64 = 8.0;
const STRIPS_H: f64 = STRIP_H * 2.0 + STRIP_GAP * 3.0 + 7.0;

/// The input coming up from the floor, the reduction hanging from the top
/// (live), the threshold as a line across it, the ratio as an arrow down
/// from it, and the two times as strips along the floor.
fn comp(k: &mut Kit, p: NodeId, which: Which, at: Rect4, tier: Tier) {
    let ns = which.ns();
    let h = at.3;
    if tier.detailed() {
        for db in [-12.0, -24.0, -36.0, -48.0] {
            hline(k, p, at, comp_y(db, h), s::GRID_BEAT, 1.0);
        }
    }
    levels(k, p, ns, at, "in_history", s::COMP_GREY);
    // The reduction, hanging from the top.
    let c = Control::new(ControlKind::Plot {
        style: PlotStyle::Area { baseline: 0.0 },
        x: Axis::Time {
            len: super::catalog::HISTORY,
        },
        y: PlotRange::linear(-60.0, 0.0),
    })
    .bound(Binding::stream(format!("{ns}/gr_history")));
    traced(
        k,
        p,
        "reduction",
        at,
        c,
        Some((s::REDUCTION_EDGE, 1.2, false)),
        Some(&alpha(s::REDUCTION_FILL, 0.6)),
    );

    threshold(
        k,
        p,
        &format!("{ns}/threshold"),
        at,
        s::THRESHOLD,
        tier.detailed(),
    );
    if tier.detailed() {
        arrow(k, p, ns, at);
        strips(k, p, ns, at);
    }
}

/// A level history coming up from the floor (live).
fn levels(k: &mut Kit, p: NodeId, ns: &str, at: Rect4, feed: &str, ink: &str) {
    let c = Control::new(ControlKind::Plot {
        style: PlotStyle::Area { baseline: -60.0 },
        x: Axis::Time {
            len: super::catalog::HISTORY,
        },
        y: PlotRange::linear(-60.0, 0.0),
    })
    .bound(Binding::stream(format!("{ns}/{feed}")));
    traced(
        k,
        p,
        "input",
        at,
        c,
        Some((&alpha(ink, 0.85), 1.0, false)),
        Some(&alpha(ink, 0.32)),
    );
}

/// A threshold: a line across the display, grabbed anywhere near it.
fn threshold(k: &mut Kit, p: NodeId, addr: &str, at: Rect4, ink: &str, tab: bool) -> NodeId {
    let (w, h) = (at.2, at.3);
    const GRAB: f64 = 6.0;
    let node = s::frame(
        &mut k.b(),
        p,
        "threshold",
        (0.0, h / 2.0 - GRAB, w, GRAB * 2.0),
    );
    s::line(
        &mut k.b(),
        node,
        "threshold line",
        (w, GRAB * 2.0),
        s::seg((0.0, GRAB), (w, GRAB)),
        ink,
        1.5,
    );
    if tab {
        s::dot(
            &mut k.b(),
            node,
            "threshold tab",
            (w - s::HANDLE - 1.0, GRAB),
            s::HANDLE,
            ink,
        );
    }
    let kind = ControlKind::Handle {
        x: None,
        y: Some(Binding::param(addr)),
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear {
            min: -60.0,
            max: 0.0,
        }),
    };
    k.controls.insert(node, Control::new(kind));
    node
}

/// The ratio: an arrow down from the threshold as far as a full-scale
/// signal would be taken (session `ratio_drop`), the ratio at its tip.
fn arrow(k: &mut Kit, p: NodeId, ns: &str, at: Rect4) {
    let (w, h) = (at.2, at.3);
    let x = ARROW_X;
    let shape = s::line(
        &mut k.b(),
        p,
        "ratio arrow",
        (w, h),
        String::new(),
        &alpha(s::THRESHOLD, 0.85),
        1.2,
    );
    let head = s::shape(
        &mut k.b(),
        p,
        "ratio head",
        (w, h),
        String::new(),
        &alpha(s::THRESHOLD, 0.85),
    );
    let label = s::text(
        &mut k.b(),
        p,
        "ratio label",
        "",
        (x + 5.0, 0.0, 40.0),
        6.0,
        &alpha(s::THRESHOLD, 0.85),
        TextAlign::Left,
    );
    let geo = {
        let (thr, rat) = (format!("{ns}/threshold"), format!("{ns}/ratio"));
        move |v: &Values| {
            let t = get(v, &thr, -18.0);
            let r = get(v, &rat, 4.0).max(1.0);
            let top = comp_y(t, h);
            let floor = h - STRIPS_H;
            let drop = -t * (1.0 - 1.0 / r) / 60.0 * h;
            (top, (top + drop).min(floor.max(top)), r)
        }
    };
    let g1 = geo.clone();
    k.derive(shape, move |v, n| {
        let (top, tip, _) = g1(v);
        n.fill_geometry = vec![s::seg((x, top), (x, tip))];
    });
    let g2 = geo.clone();
    k.derive(head, move |v, n| {
        let (_, tip, _) = g2(v);
        n.fill_geometry = vec![format!(
            "{} Z",
            s::poly(&[(x - 3.0, tip - 4.8), (x + 3.0, tip - 4.8), (x, tip)])
        )];
    });
    k.derive(label, move |v, n| {
        let (_, tip, r) = geo(v);
        n.transform = Transform::translate(x + 5.0, tip + 1.0 - 6.0 * 0.97 - 1.0);
        if let NodeKind::Text(t) = &mut n.kind {
            t.characters = if (r - r.round()).abs() < 0.05 {
                format!("{r:.0}:1")
            } else {
                format!("{r:.1}:1")
            };
        }
    });
    // Grab the arrow's column: drag down for more.
    let hit = s::frame(
        &mut k.b(),
        p,
        "ratio grip",
        (x - 5.5, 0.0, 11.0, (h - STRIPS_H).max(1.0)),
    );
    let b = Binding {
        min: 1.0,
        max: 0.0,
        ..Binding::param(format!("{ns}/ratio"))
    };
    k.controls.insert(
        hit,
        Control::new(ControlKind::Knob {
            sweep_degrees: 270.0,
        })
        .bound(b),
    );
}

/// The two times along the floor: attack over release, FAST at the left,
/// the default marked, a bar from it to the setting.
fn strips(k: &mut Kit, p: NodeId, ns: &str, at: Rect4) {
    let (w, h) = (at.2, at.3);
    if w < 70.0 || h < 40.0 {
        return;
    }
    let ink = s::THRESHOLD;
    let (left, right) = (3.0, w - 3.0 - STRIP_LABEL_W);
    let floor = h - STRIP_GAP;
    let release = (left, floor - STRIP_H, right - left, STRIP_H);
    let attack = (left, release.1 - STRIP_GAP - STRIP_H, right - left, STRIP_H);
    let over = attack.1 - STRIP_GAP + 1.0;
    s::text(
        &mut k.b(),
        p,
        "fast",
        "FAST",
        (left, over, 30.0),
        6.0,
        &alpha(ink, 0.6),
        TextAlign::Left,
    );
    s::text(
        &mut k.b(),
        p,
        "slow",
        "SLOW",
        (right - 30.0, over, 30.0),
        6.0,
        &alpha(ink, 0.6),
        TextAlign::Right,
    );
    for (strip, key, label, (lo, hi, def)) in [
        (attack, "attack", "A", (0.1, 200.0, 10.0)),
        (release, "release", "R", (5.0, 3000.0, 150.0)),
    ] {
        let place = move |ms: f64| ((ms.clamp(lo, hi) / lo).ln() / (hi / lo).ln()) * strip.2;
        let mut groove = draw::rect(
            "groove",
            strip.0,
            strip.1,
            strip.2,
            strip.3,
            draw::solid(&alpha(ink, 0.28)),
        );
        groove = draw::rounded(groove, 2.0);
        let groove = k.b().add(p, groove);
        let _ = groove;
        let cx = strip.0 + place(def);
        s::line(
            &mut k.b(),
            p,
            "default",
            (w, h),
            s::seg((cx, strip.1 - 1.5), (cx, strip.1 + strip.3 + 1.5)),
            &alpha(ink, 0.6),
            1.0,
        );
        let bar = s::fill(
            &mut k.b(),
            p,
            "departure",
            (cx, strip.1, 0.0, strip.3),
            &alpha(ink, 0.55),
        );
        let addr = format!("{ns}/{key}");
        let a2 = addr.clone();
        k.derive(bar, move |v, n| {
            let x = strip.0 + place(get(v, &a2, def));
            let (x0, x1) = (x.min(cx), x.max(cx));
            n.transform = Transform::translate(x0, strip.1);
            n.size.width = x1 - x0;
        });
        s::text(
            &mut k.b(),
            p,
            "strip label",
            label,
            (strip.0 + strip.2 + 3.0, strip.1 + strip.3 + 1.0, 8.0),
            7.0,
            &alpha(ink, 0.7),
            TextAlign::Left,
        );
        // The strip is a slider; its dot is the thumb.
        // Wider than the strip by the thumb, so the dot's centre spans it.
        let lane = s::frame(
            &mut k.b(),
            p,
            &format!("{key} strip"),
            (
                strip.0 - s::HANDLE,
                strip.1 - 3.0,
                strip.2 + 2.0 * s::HANDLE,
                strip.3 + 6.0,
            ),
        );
        let thumb = s::frame(
            &mut k.b(),
            lane,
            "thumb",
            (
                0.0,
                3.0 + strip.3 / 2.0 - s::HANDLE,
                2.0 * s::HANDLE,
                2.0 * s::HANDLE,
            ),
        );
        s::dot(
            &mut k.b(),
            thumb,
            "dot",
            (s::HANDLE, s::HANDLE),
            s::HANDLE,
            ink,
        );
        let c = Control::new(ControlKind::Fader {
            orientation: Orientation::Horizontal,
        })
        .bound(Binding::param(addr))
        .slot(SlotRole::Thumb, thumb.0);
        k.controls.insert(lane, c);
    }
}

// ── gate ─────────────────────────────────────────────────────────────────

/// The level against a line, what the gate takes off shaded below it down
/// to the range, and the door lane under it (open, in green).
fn gate(k: &mut Kit, p: NodeId, which: Which, body: Rect4, tier: Tier) {
    let ns = which.ns();
    let at = display_of(body, which, tier);
    let (w, h) = (at.2, at.3);
    let disp = s::frame(&mut k.b(), p, "display", at);
    let local = (0.0, 0.0, w, h);
    if tier.detailed() {
        for db in [-12.0, -24.0, -36.0, -48.0] {
            hline(k, disp, local, comp_y(db, h), s::GRID_BEAT, 1.0);
        }
    }
    let band = s::fill(
        &mut k.b(),
        disp,
        "shut band",
        (0.0, 0.0, w, 0.0),
        &alpha(s::GATE_INK, 0.18),
    );
    let (thr, rng) = (format!("{ns}/threshold"), format!("{ns}/range"));
    {
        let (thr, rng) = (thr.clone(), rng.clone());
        k.derive(band, move |v, n| {
            let line = comp_y(get(v, &thr, -40.0), h);
            let floor = comp_y(get(v, &thr, -40.0) + get(v, &rng, -18.0), h);
            n.transform = Transform::translate(0.0, line);
            n.size.height = (floor - line).max(0.0);
        });
    }
    levels(k, disp, ns, local, "in_history", s::COMP_GREY);
    threshold(k, disp, &thr, local, s::GATE_INK, tier.detailed());
    if tier.detailed() {
        // The range: a second, fainter line — how far down "shut" goes.
        const GRAB: f64 = 5.0;
        let node = s::frame(
            &mut k.b(),
            disp,
            "range",
            (0.0, h / 2.0 - GRAB, w, GRAB * 2.0),
        );
        s::line(
            &mut k.b(),
            node,
            "range line",
            (w, GRAB * 2.0),
            s::seg((0.0, GRAB), (w, GRAB)),
            &alpha(s::GATE_INK, 0.55),
            1.0,
        );
        let kind = ControlKind::Handle {
            x: None,
            y: Some(Binding::param(rng.clone())),
            wheel: None,
            x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
            y_map: HandleAxis::new(Axis::Linear {
                min: -60.0,
                max: 0.0,
            })
            .after(&[thr.as_str()], 0.0),
        };
        k.controls.insert(node, Control::new(kind));
        // The times, standing up from the floor to the line: rise, hold,
        // fall — a gate opens, and up is open.
        let glyph = s::line(
            &mut k.b(),
            disp,
            "times",
            (w, h),
            String::new(),
            s::GATE_INK,
            1.4,
        );
        let ns2 = ns.to_string();
        k.derive(glyph, move |v, n| {
            let g = |key: &str, d: f64| get(v, &format!("{ns2}/{key}"), d);
            let line = comp_y(g("threshold", -40.0), h);
            let floor = comp_y(g("threshold", -40.0) + g("range", -18.0), h);
            let norm = |ms: f64, lo: f64, hi: f64| {
                ((ms.clamp(lo, hi) / lo).ln() / (hi / lo).ln()).clamp(0.0, 1.0)
            };
            let x0 = w * 0.3;
            let rise = x0 + w * (0.04 + 0.16 * norm(g("attack", 1.0), 0.05, 50.0));
            let hold = rise + w * (0.04 + 0.16 * norm(g("hold", 30.0), 1.0, 500.0));
            let fall = hold + w * (0.04 + 0.2 * norm(g("release", 120.0), 5.0, 2000.0));
            n.fill_geometry = vec![s::poly(&[
                (x0, floor),
                (rise, line),
                (hold, line),
                (fall.min(w - 8.0), floor),
            ])];
        });
    }
    // The door lane (live): when the gate was open.
    if let Some(lane) = lane_of(body, which, tier) {
        s::fill(&mut k.b(), p, "lane", lane, &alpha(s::GATE_INK, 0.06));
        let c = Control::new(ControlKind::Plot {
            style: PlotStyle::Steps,
            x: Axis::Time {
                len: super::catalog::HISTORY,
            },
            y: PlotRange::linear(0.0, 1.0),
        })
        .bound(Binding::stream(format!("{ns}/open_history")));
        traced(k, p, "door", lane, c, None, Some(&alpha(s::GATE_INK, 0.7)));
    }
}

// ── saturator ────────────────────────────────────────────────────────────

/// The saturator's curve square and the ladder beside it (session
/// `sat_split`).
fn sat_split(body: Rect4, tier: Tier) -> (Rect4, Option<Rect4>) {
    if !tier.detailed() || body.2 < 80.0 {
        return (body, None);
    }
    let lw = body.2 * 0.34;
    (
        (body.0, body.1, body.2 - lw - 4.0, body.3),
        Some((body.0 + body.2 - lw, body.1, lw, body.3 - 6.0)),
    )
}

/// How hot the stage runs at full scale, 0..1 (session: the sum of the
/// ladder's rungs, on a 0.7 power).
fn heat(v: &Values, ns: &str) -> f64 {
    let g = |key: &str, d: f64| get(v, &format!("{ns}/{key}"), d);
    let rungs = curves::harmonics(
        g("drive", 2.0),
        g("bias", 0.15),
        g("circuit", 0.0).round().max(0.0) as usize,
        1.0,
    );
    (rungs
        .iter()
        .map(|r| (r * 4.0).clamp(0.0, 1.0).sqrt())
        .sum::<f64>()
        / 7.0
        / 0.9)
        .clamp(0.0, 1.0)
        .powf(0.7)
}

fn glow(k: &mut Kit, p: NodeId, ns: &str, at: Rect4) {
    let mut n = draw::rect(
        "heat",
        at.0,
        at.1,
        at.2,
        at.3,
        draw::solid(&alpha(s::SAT_GLOW, 0.22)),
    );
    let (hot, mid, cold) = (
        alpha(s::SAT_HOT, 0.7),
        alpha(s::SAT_GLOW, 0.55),
        alpha(s::SAT_GLOW, 0.0),
    );
    n.fills.push(draw::radial(&[
        (0.0, hot.as_str()),
        (0.35, mid.as_str()),
        (1.0, cold.as_str()),
    ]));
    let node = k.b().add(p, n);
    let ns = ns.to_string();
    k.derive(node, move |v, n| {
        let h = heat(v, &ns);
        n.opacity = h;
        n.visible = h > 0.02;
    });
}

fn sat(k: &mut Kit, p: NodeId, ctx: &BuildCtx, which: Which, body: Rect4, tier: Tier) {
    let ns = which.ns();
    let whole = display_of(body, which, tier);
    let (at, ladder) = sat_split(whole, tier);
    glow(k, p, ns, whole);
    let (w, h) = (at.2, at.3);
    if tier.detailed() {
        hline(k, p, at, at.1 + h / 2.0, s::GRID, 1.0);
        vline(k, p, at, at.0 + w / 2.0, s::GRID);
        s::line(
            &mut k.b(),
            p,
            "unity",
            (w, h),
            s::seg((at.0, at.1 + h), (at.0 + w, at.1)),
            s::GRID_BEAT,
            1.0,
        );
    }
    let inputs = |live: bool| {
        let mut v = vec![
            CurveInput::param("drive", format!("{ns}/drive")),
            CurveInput::param("bias", format!("{ns}/bias")),
            CurveInput::param("circuit", format!("{ns}/circuit")),
            CurveInput::param("bits", format!("{ns}/bits")),
        ];
        if live {
            v.push(CurveInput::stream("reach", format!("{ns}/peak")));
        }
        v
    };
    let unit = PlotRange::linear(-1.0, 1.0);
    let x = Axis::Linear {
        min: -1.0,
        max: 1.0,
    };
    let c = curve_control(
        "sat.transfer",
        inputs(false),
        Some("transfer"),
        PlotStyle::Line,
        x.clone(),
        unit.clone(),
    );
    traced(
        k,
        p,
        "transfer",
        at,
        c,
        Some((s::SAT_INK, 1.5, false)),
        None,
    );
    // The lit reach (live): the part of the curve the signal is on.
    let c = curve_control(
        "sat.transfer",
        inputs(true),
        Some("transfer"),
        PlotStyle::Line,
        x,
        unit,
    );
    traced(k, p, "reach", at, c, Some((s::SAT_HOT, 2.4, false)), None);

    if let Some(lb) = ladder {
        hline(k, p, lb, lb.1 + lb.3 - 1.0, s::GRID, 1.0);
        // Seven rungs, H2…H8, evens in the unit's red and odds in grey, on a
        // square root so the fourth and sixth show beside the second.
        let floor = lb.1 + lb.3 - 1.0;
        let (gap, n) = (1.2, 7.0);
        let bar = ((lb.2 - 2.0 - gap * (n - 1.0)) / n).max(1.0);
        let tall = lb.3 - 4.0;
        for (even, ink) in [
            (true, s::SAT_INK.to_string()),
            (false, alpha(s::TEXT_FAINT, 0.75)),
        ] {
            let node = s::shape(
                &mut k.b(),
                p,
                if even { "evens" } else { "odds" },
                (lb.2, lb.3),
                String::new(),
                &ink,
            );
            let ns = ns.to_string();
            k.derive(node, move |v, nd| {
                let g = |key: &str, d: f64| get(v, &format!("{ns}/{key}"), d);
                let rungs = curves::harmonics(
                    g("drive", 2.0),
                    g("bias", 0.15),
                    g("circuit", 0.0).round().max(0.0) as usize,
                    1.0,
                );
                let mut d = String::new();
                for (k, r) in rungs
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| (k % 2 == 0) == even)
                {
                    let h = (r * 4.0).clamp(0.0, 1.0).sqrt() * tall;
                    let x = lb.0 + 1.0 + k as f64 * (bar + gap);
                    if h > 0.2 {
                        d.push_str(&format!(
                            "M {x:.2} {:.2} H {:.2} V {floor:.2} H {x:.2} Z ",
                            floor - h,
                            x + bar
                        ));
                    }
                }
                nd.fill_geometry = vec![d.trim_end().to_string()];
            });
        }
    }
    let _ = ctx;
}

// ── de-esser ─────────────────────────────────────────────────────────────

const SUPPRESS_FLOOR_DB: f64 = -40.0;
const SUPPRESS_CEIL_DB: f64 = 16.0;

fn suppress_y(db: f64, h: f64) -> f64 {
    let t = ((db - SUPPRESS_FLOOR_DB) / (SUPPRESS_CEIL_DB - SUPPRESS_FLOOR_DB)).clamp(0.0, 1.0);
    h - 1.0 - t * (h - 2.0)
}

/// The band it watches (a strip of the top of the spectrum, over), and
/// what happened in it (time, under).
fn deess(k: &mut Kit, p: NodeId, which: Which, body: Rect4, tier: Tier) {
    let ns = which.ns();
    let at = display_of(body, which, tier);
    let (w, h) = (at.2, at.3);
    let disp = s::frame(&mut k.b(), p, "display", at);
    let local = (0.0, 0.0, w, h);
    if tier.detailed() {
        for db in [-12.0, -24.0] {
            hline(k, disp, local, suppress_y(db, h), s::GRID_BEAT, 1.0);
        }
    }
    let y = PlotRange::linear(SUPPRESS_FLOOR_DB, SUPPRESS_CEIL_DB);
    let c = Control::new(ControlKind::Plot {
        style: PlotStyle::Line,
        x: Axis::Time {
            len: super::catalog::HISTORY,
        },
        y: y.clone(),
    })
    .bound(Binding::stream(format!("{ns}/level_history")));
    traced(
        k,
        disp,
        "band level",
        local,
        c,
        Some((s::DEESS_INK, 1.2, false)),
        None,
    );
    let c = Control::new(ControlKind::Plot {
        style: PlotStyle::Area { baseline: 0.0 },
        x: Axis::Time {
            len: super::catalog::HISTORY,
        },
        y: PlotRange::linear(-24.0, 0.0),
    })
    .bound(Binding::stream(format!("{ns}/gr_history")));
    traced(
        k,
        disp,
        "cut",
        local,
        c,
        None,
        Some(&alpha(s::DEESS_INK, 0.35)),
    );
    // The reference it is judged against.
    let refl = s::line(
        &mut k.b(),
        disp,
        "threshold",
        (w, h),
        String::new(),
        &alpha(s::DEESS_INK, 0.6),
        1.0,
    );
    if let Some(n) = k.scene.node_mut(refl) {
        n.stroke.dash_pattern = vec![2.5, 2.0];
    }
    let thr = format!("{ns}/threshold");
    k.derive(refl, move |v, n| {
        let yy = suppress_y(get(v, &thr, -24.0), h);
        n.fill_geometry = vec![s::seg((0.0, yy), (w, yy))];
    });

    let Some(strip) = lane_of(body, which, tier) else {
        return;
    };
    let (sw, sh) = (strip.2, strip.3);
    let band_frame = s::frame(&mut k.b(), p, "band strip", strip);
    let local = (0.0, 0.0, sw, sh);
    let fx = Axis::Log {
        f_min: 800.0,
        f_max: 20_000.0,
    };
    let c = Control::new(ControlKind::Plot {
        style: PlotStyle::Area {
            baseline: SUPPRESS_FLOOR_DB,
        },
        x: fx.clone(),
        y,
    })
    .bound(Binding::stream(format!("{ns}/spectrum")));
    traced(
        k,
        band_frame,
        "top end",
        local,
        c,
        None,
        Some(&alpha(s::TEXT_FAINT, 0.25)),
    );
    let place = move |hz: f64| (hz / 800.0).ln() / (20_000.0_f64 / 800.0).ln() * sw;
    let wash = s::fill(
        &mut k.b(),
        band_frame,
        "band",
        (0.0, 0.0, 0.0, sh),
        &alpha(s::DEESS_INK, 0.2),
    );
    let (lo, hi) = (format!("{ns}/low"), format!("{ns}/high"));
    {
        let (lo, hi) = (lo.clone(), hi.clone());
        k.derive(wash, move |v, n| {
            let (a, b) = (place(get(v, &lo, 5000.0)), place(get(v, &hi, 10_000.0)));
            n.transform = Transform::translate(a.min(b), 0.0);
            n.size.width = (a - b).abs();
        });
    }
    for hz in [1000.0, 2000.0, 5000.0, 10_000.0] {
        let x = place(hz);
        if x > 3.0 && x < sw - 3.0 {
            s::fill(
                &mut k.b(),
                band_frame,
                "mark",
                (x - 0.5, sh - 3.0, 1.0, 3.0),
                s::GRID,
            );
        }
    }
    for addr in [lo, hi] {
        let node = s::frame(&mut k.b(), band_frame, "edge", (0.0, 0.0, 8.0, sh));
        s::line(
            &mut k.b(),
            node,
            "edge line",
            (8.0, sh),
            s::seg((4.0, 0.0), (4.0, sh)),
            s::DEESS_INK,
            1.2,
        );
        let kind = ControlKind::Handle {
            x: Some(Binding::param(addr)),
            y: None,
            wheel: None,
            x_map: HandleAxis::new(fx.clone()),
            y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        };
        k.controls.insert(node, Control::new(kind));
    }
}

// ── delay ────────────────────────────────────────────────────────────────

/// The dry hit, then the repeats at the level the feedback leaves them;
/// the mix inks them, it does not shrink them.
fn delay(k: &mut Kit, p: NodeId, ctx: &BuildCtx, which: Which, body: Rect4, tier: Tier) {
    let ns = which.ns();
    let at = display_of(body, which, tier);
    let (w, h) = (at.2, at.3);
    let floor = at.1 + h - 1.0;
    let family = ctx.value(&format!("{ns}/family"), 0.0).round().max(0.0) as usize;
    if tier.detailed() {
        hline(k, p, at, floor, s::GRID, 1.0);
        if family == 3 {
            let mut when = 0.0;
            for tap in 0..16 {
                if when > 4.5 {
                    break;
                }
                let x = at.0 + 3.0 + when / 4.5 * (w - 6.0);
                vline(
                    k,
                    p,
                    (at.0, at.1, w, h - 1.0),
                    x,
                    if tap % 4 == 0 { s::GRID_BEAT } else { s::GRID },
                );
                when += [1.0, 0.5, 0.75, 0.5, 1.25][tap % 5];
            }
        }
    }
    let thin = !tier.detailed() && tier != Tier::Curves;
    let taps_at = (at.0 + 3.0, at.1, (w - 6.0).max(1.0), (h - 1.0).max(1.0));
    s::line(
        &mut k.b(),
        p,
        "dry",
        (w, h),
        s::seg((taps_at.0, floor), (taps_at.0, floor - 0.9 * (h - 1.0))),
        s::TEXT,
        if thin { 1.0 } else { 2.0 },
    );
    let inputs = vec![
        CurveInput::param("time", format!("{ns}/time")),
        CurveInput::param("feedback", format!("{ns}/feedback")),
        CurveInput::param("family", format!("{ns}/family")),
    ];
    let c = curve_control(
        "echo.taps",
        inputs,
        Some("taps"),
        PlotStyle::Line,
        Axis::Linear { min: 0.0, max: 4.5 },
        PlotRange::linear(0.0, 1.0 / 0.9),
    );
    let (f, _, _) = traced(
        k,
        p,
        "repeats",
        taps_at,
        c,
        Some((s::DELAY_INK, if thin { 1.0 } else { 1.4 }, false)),
        None,
    );
    let mix = format!("{ns}/mix");
    k.derive(f, move |v, n| {
        n.opacity = (0.35 + get(v, &mix, 0.3).clamp(0.0, 1.0) * 0.65) * 0.85
    });
}

// ── reverb ───────────────────────────────────────────────────────────────

/// An impulse, the gap before its tail, and the tail — its height the
/// mix, its length the decay window.
fn reverb(k: &mut Kit, p: NodeId, ctx: &BuildCtx, which: Which, body: Rect4, tier: Tier) {
    let ns = which.ns();
    let at = display_of(body, which, tier);
    let (w, h) = (at.2, at.3);
    let floor = at.1 + h - 1.0;
    if tier.detailed() {
        hline(k, p, at, floor, s::GRID, 1.0);
    }
    let thin = tier == Tier::Minimal;
    s::line(
        &mut k.b(),
        p,
        "dry",
        (w, h),
        s::seg((at.0 + 1.0, floor), (at.0 + 1.0, floor - 0.92 * h)),
        s::TEXT,
        if thin { 1.0 } else { 2.0 },
    );
    let tail_at = (at.0, at.1, w, (h - 1.0).max(1.0));
    let inputs = |live: bool| {
        let mut v = vec![
            CurveInput::param("decay", format!("{ns}/decay")),
            CurveInput::param("predelay", format!("{ns}/predelay")),
            CurveInput::param("mix", format!("{ns}/mix")),
            CurveInput::param("family", format!("{ns}/family")),
        ];
        if live {
            v.push(CurveInput::stream("wet", format!("{ns}/wet")));
        }
        v
    };
    // Lit by the wet return (live): a reverb washing a chorus is filled.
    if !thin {
        let c = curve_control(
            "room.tail",
            inputs(true),
            Some("tail"),
            PlotStyle::Area { baseline: 0.0 },
            Axis::Linear { min: 0.0, max: 1.0 },
            PlotRange::linear(0.0, 1.0),
        );
        let (_, area, _) = traced(
            k,
            p,
            "wash",
            tail_at,
            c,
            None,
            Some(&alpha(s::REVERB_INK, 0.3)),
        );
        if let Some(a) = area {
            let glow = Control::new(ControlKind::Readout)
                .bound(Binding::stream(format!("{ns}/wet")))
                .slot(SlotRole::Glow, a.0);
            let holder = s::frame(&mut k.b(), p, "wet", (at.0, at.1, 1.0, 1.0));
            k.controls.insert(holder, glow);
        }
    }
    let c = curve_control(
        "room.tail",
        inputs(false),
        Some("tail"),
        PlotStyle::Line,
        Axis::Linear { min: 0.0, max: 1.0 },
        PlotRange::linear(0.0, 1.0),
    );
    traced(
        k,
        p,
        "tail",
        tail_at,
        c,
        Some((s::REVERB_INK, if thin { 1.0 } else { 1.4 }, false)),
        None,
    );
    if tier.detailed() {
        let pre = s::line(
            &mut k.b(),
            p,
            "predelay",
            (w, h),
            String::new(),
            s::GRID_BEAT,
            1.0,
        );
        let (d, pd) = (format!("{ns}/decay"), format!("{ns}/predelay"));
        let x0 = at.0;
        let y0 = at.1;
        k.derive(pre, move |v, n| {
            let window = (get(v, &d, 1.8) * 1000.0).max(1.0);
            let x = x0 + (get(v, &pd, 20.0) / window).clamp(0.0, 0.5) * w;
            n.fill_geometry = vec![s::seg((x, y0), (x, floor))];
        });
    }
    let _ = ctx;
}

// ── the machine selector ─────────────────────────────────────────────────

/// How wide a header glyph is, with its gap.
pub const GLYPH_W: f64 = 11.0;

/// Which family of machines a glyph draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Circuit,
    Delay,
    Room,
}

fn glyph_of(which: Which, ctx: &BuildCtx) -> Option<(Family, usize)> {
    let at = |k: &str, d: f64| {
        ctx.value(&format!("{}/{k}", which.ns()), d)
            .round()
            .max(0.0) as usize
    };
    match which {
        Which::Sat => Some((Family::Circuit, at("circuit", 0.0))),
        Which::Delay => Some((Family::Delay, at("family", 0.0))),
        Which::Reverb => Some((Family::Room, at("family", 1.0))),
        _ => None,
    }
}

/// A machine in 9 px: its left edge at `x`, sitting on `baseline`.
/// Paths, never text (session: a glyph run is the dearest thing drawn).
fn glyph(
    k: &mut Kit,
    parent: NodeId,
    family: Family,
    index: usize,
    (x, baseline): (f64, f64),
    ink: &str,
) -> NodeId {
    let (t, b) = (baseline - 7.0, baseline);
    let d = match (family, index) {
        // A valve: an envelope with a grid.
        (Family::Circuit, 0) => format!(
            "M {:.1} {b:.1} L {:.1} {:.1} A 3 3 0 0 1 {:.1} {:.1} L {:.1} {b:.1} Z M {:.1} {:.1} L {:.1} {:.1}",
            x + 1.5,
            x + 1.5,
            t + 3.0,
            x + 7.5,
            t + 3.0,
            x + 7.5,
            x + 3.0,
            t + 4.0,
            x + 6.0,
            t + 4.0
        ),
        // Tape and reels: two circles.
        (Family::Circuit, 1) | (Family::Delay, 1) => format!(
            "M {:.1} {:.1} A 2 2 0 1 0 {:.1} {:.1} A 2 2 0 1 0 {:.1} {:.1} M {:.1} {:.1} A 2 2 0 1 0 {:.1} {:.1} A 2 2 0 1 0 {:.1} {:.1}",
            x + 0.5,
            t + 3.5,
            x + 4.5,
            t + 3.5,
            x + 0.5,
            t + 3.5,
            x + 4.5,
            t + 3.5,
            x + 8.5,
            t + 3.5,
            x + 4.5,
            t + 3.5
        ),
        // A core: a box with its windings.
        (Family::Circuit, 2) => format!(
            "M {:.1} {:.1} H {:.1} V {b:.1} H {:.1} Z M {:.1} {:.1} H {:.1} M {:.1} {:.1} H {:.1}",
            x + 1.0,
            t + 1.0,
            x + 8.0,
            x + 1.0,
            x + 1.0,
            t + 3.5,
            x + 8.0,
            x + 1.0,
            t + 5.5,
            x + 8.0
        ),
        // Solid state: an op-amp.
        (Family::Circuit, 3) => format!(
            "M {:.1} {:.1} L {:.1} {:.1} L {:.1} {b:.1} Z",
            x + 1.0,
            t + 0.5,
            x + 8.5,
            t + 4.0,
            x + 1.0
        ),
        // Steps: a staircase (also the rhythmic delay's grid).
        (Family::Circuit, _) | (Family::Delay, 3) => format!(
            "M {:.1} {b:.1} V {:.1} H {:.1} V {:.1} H {:.1} V {:.1} H {:.1}",
            x + 0.5,
            t + 5.0,
            x + 3.0,
            t + 3.0,
            x + 5.5,
            t + 1.0,
            x + 8.5
        ),
        // Digital: a square wave.
        (Family::Delay, 0) => format!(
            "M {:.1} {b:.1} V {:.1} H {:.1} V {b:.1} H {:.1} V {:.1} H {:.1}",
            x + 0.5,
            t + 1.0,
            x + 3.0,
            x + 5.5,
            t + 1.0,
            x + 8.5
        ),
        // Analog: a chip on its legs.
        (Family::Delay, 2) => format!(
            "M {:.1} {:.1} H {:.1} V {:.1} H {:.1} Z M {:.1} {:.1} V {b:.1} M {:.1} {:.1} V {b:.1} M {:.1} {:.1} V {b:.1}",
            x + 1.0,
            t + 1.0,
            x + 8.0,
            t + 5.0,
            x + 1.0,
            x + 2.5,
            t + 5.0,
            x + 4.5,
            t + 5.0,
            x + 6.5,
            t + 5.0
        ),
        // Pitch: rising repeats.
        (Family::Delay, 4) => format!(
            "M {:.1} {b:.1} V {:.1} M {:.1} {:.1} V {:.1} M {:.1} {:.1} V {:.1}",
            x + 1.0,
            t + 4.0,
            x + 4.5,
            b - 1.5,
            t + 2.0,
            x + 8.0,
            b - 3.0,
            t
        ),
        // Special: a reversed swell.
        (Family::Delay, _) => format!(
            "M {:.1} {b:.1} L {:.1} {b:.1} L {:.1} {t:.1} Z",
            x + 0.5,
            x + 8.5,
            x + 8.5
        ),
        // A room: a box.
        (Family::Room, 0) => format!(
            "M {:.1} {:.1} H {:.1} V {b:.1} H {:.1} Z",
            x + 1.0,
            t + 1.5,
            x + 8.0,
            x + 1.0
        ),
        // A hall: an arch.
        (Family::Room, 1) => format!(
            "M {:.1} {b:.1} V {:.1} A 3.5 3.5 0 0 1 {:.1} {:.1} V {b:.1}",
            x + 1.0,
            t + 3.5,
            x + 8.0,
            t + 3.5
        ),
        // A plate: a slanted sheet.
        (Family::Room, 2) => format!(
            "M {:.1} {b:.1} L {:.1} {:.1} H {:.1} L {:.1} {b:.1} Z",
            x + 0.5,
            x + 2.5,
            t + 1.0,
            x + 8.5,
            x + 6.5
        ),
        // A spring: a coil.
        (Family::Room, 3) => format!(
            "M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1}",
            x + 0.5,
            t + 3.5,
            x + 2.0,
            t + 0.5,
            x + 3.5,
            b,
            x + 5.0,
            t + 0.5,
            x + 6.5,
            b,
            x + 8.5,
            t + 3.5
        ),
        // Bloom: a swell that opens.
        (Family::Room, _) => format!(
            "M {:.1} {b:.1} Q {:.1} {:.1} {:.1} {:.1} Q {:.1} {:.1} {:.1} {b:.1}",
            x + 0.5,
            x + 2.5,
            t,
            x + 4.5,
            t + 1.0,
            x + 6.5,
            t + 2.0,
            x + 8.5
        ),
    };
    s::line(&mut k.b(), parent, "glyph", (1.0, 1.0), d, ink, 1.1)
}

/// One chip per family, the current one lit and underlined in the unit's
/// colour, the machine's name beside them.
fn selector(k: &mut Kit, p: NodeId, ctx: &BuildCtx, which: Which, lane: Rect4) {
    let ns = which.ns();
    let (family, names, key, tint): (Family, &[&str], &str, &str) = match which {
        Which::Sat => (Family::Circuit, &CIRCUITS, "circuit", "#facc15"),
        Which::Delay => (Family::Delay, &DELAY_FAMILIES, "family", s::DELAY_INK),
        _ => (Family::Room, &ROOM_FAMILIES, "family", s::REVERB_INK),
    };
    let baseline = lane.1 + lane.3 - 4.0;
    let count = names.len() as u32;
    for (i, _) in names.iter().enumerate() {
        let x = lane.0 + 2.0 + i as f64 * s::CHIP;
        let chip = s::frame(&mut k.b(), p, "chip", (x - 1.0, lane.1, s::CHIP, lane.3));
        let ink = glyph(k, p, family, i, (x, baseline), s::TEXT);
        let under = s::fill(
            &mut k.b(),
            p,
            "current",
            (x - 1.0, lane.1 + lane.3 - 1.75, 10.0, 1.5),
            tint,
        );
        let c = Control::new(ControlKind::Button {
            mode: ButtonMode::Select {
                index: i as u32,
                count,
            },
        })
        .bound(Binding::param(format!("{ns}/{key}")))
        .slot(SlotRole::Led, ink.0)
        .slot(SlotRole::Led, under.0);
        k.controls.insert(chip, c);
    }
    let chips_w = names.len() as f64 * s::CHIP + 2.0;
    let current = ctx.value(&format!("{ns}/{key}"), 0.0).round().max(0.0) as usize;
    let name = names.get(current).copied().unwrap_or("");
    let nw = s::text_width(name, 6.5);
    if chips_w + nw + 4.0 <= lane.2 {
        s::text(
            &mut k.b(),
            p,
            "machine",
            name,
            (lane.0 + chips_w, baseline, lane.2 - chips_w),
            6.5,
            s::TEXT_DIM,
            TextAlign::Right,
        );
    }
}

// ── the rail (Minimal) ───────────────────────────────────────────────────

/// A rail's row: the same row at the same height, one live indicator in
/// it instead of a panel.
fn minimal(k: &mut Kit, parent: NodeId, ctx: &BuildCtx, which: Which, row: Rect4, bands: usize) {
    let ns = which.ns();
    s::fill(&mut k.b(), parent, "rail ground", row, &alpha(s::WELL, 0.5));
    let inner = inset(row, 2.0);
    let at = body_of(row);
    if at.1 > inner.1 {
        let head = split_top(inner, s::HEAD).0;
        let name_w = s::text_width(which.name(), 7.0);
        if name_w <= head.2 {
            s::text(
                &mut k.b(),
                parent,
                "unit name",
                which.name(),
                (head.0, head.1 + 7.0, head.2),
                7.0,
                s::TEXT_FAINT,
                TextAlign::Left,
            );
        }
    }
    let (w, h) = (at.2, at.3);
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let panel = s::clip(&mut k.b(), parent, which.name(), at);
    let local = (0.0, 0.0, w, h);
    match which {
        wh if wh.is_eq() => eq(k, panel, ctx, which, local, Tier::Minimal, bands),
        wh if wh.is_comp() || wh == Which::Gate => {
            let (ink, feed) = if wh == Which::Gate {
                (s::GATE_INK, "in")
            } else {
                (s::THRESHOLD, "gr")
            };
            let line = s::line(
                &mut k.b(),
                panel,
                "threshold",
                (w, h),
                String::new(),
                &alpha(ink, 0.7),
                1.0,
            );
            let thr = format!("{ns}/threshold");
            k.derive(line, move |v, n| {
                let y = comp_y(get(v, &thr, -18.0), h);
                n.fill_geometry = vec![s::seg((0.0, y), (w, y))];
            });
            if wh == Which::Gate {
                // The level, up from the floor.
                bar(
                    k,
                    panel,
                    &format!("{ns}/{feed}"),
                    (2.0, 0.0, w - 4.0, h),
                    &alpha(s::COMP_GREY, 0.6),
                    (-60.0, 0.0),
                    false,
                );
            } else {
                // What it takes off now, hanging from the top.
                bar(
                    k,
                    panel,
                    &format!("{ns}/{feed}"),
                    (2.0, 0.0, w - 4.0, h),
                    s::REDUCTION_EDGE,
                    (0.0, -60.0),
                    true,
                );
            }
        }
        Which::Sat => glow(k, panel, ns, local),
        Which::DeEss => bar(
            k,
            panel,
            &format!("{ns}/gr"),
            (2.0, 0.0, w - 4.0, h),
            s::DEESS_INK,
            (0.0, -12.0),
            true,
        ),
        Which::Delay => delay(k, panel, ctx, which, local, Tier::Minimal),
        _ => reverb(k, panel, ctx, which, local, Tier::Minimal),
    }
    // The bypass, over the row.
    scrim(
        k,
        parent,
        which,
        row,
        (row.0, row.1, row.2, s::HEAD.min(row.3)),
    );
}

/// A live bar: a gauge on a level or reduction feed, `(floor, ceiling)` dB,
/// growing up from the floor — or down from the top when `hang`.
fn bar(
    k: &mut Kit,
    p: NodeId,
    feed: &str,
    at: Rect4,
    ink: &str,
    (floor, ceiling): (f64, f64),
    hang: bool,
) {
    let (x, y, w, h) = at;
    let mut holder = draw::frame("bar", x, y, w, h);
    if hang {
        // Turned half round about its centre: its bottom is the top.
        holder.transform = Transform {
            a: -1.0,
            b: 0.0,
            tx: x + w,
            c: 0.0,
            d: -1.0,
            ty: y + h,
        };
    }
    let holder = k.b().add(p, holder);
    let fill = s::fill(&mut k.b(), holder, "fill", (0.0, 0.0, w, h), ink);
    let c = Control::new(ControlKind::Gauge {
        sweep_degrees: 0.0,
        floor_db: floor,
        ceiling_db: ceiling,
    })
    .bound(Binding::stream(feed))
    .slot(SlotRole::Fill, fill.0);
    k.controls.insert(holder, c);
}
