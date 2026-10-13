//! Lit meters: segmented LED bars (horizontal or vertical), LED ladders
//! (a row of round lamps, each lit at its printed threshold) and a lamp
//! whose brightness follows a scalar stream.
//!
//! A bar or ladder is a `Gauge` with no sweep whose `Led` slots are its
//! lamps, in order: the runtime lights lamp `i` of `n` once the reading
//! reaches position `(i + 1) / n`, and the binding's curve puts each
//! printed threshold exactly there. Lit lamps give off light; an unlit one
//! shows its dark lens. Configured through `Place::options` entries:
//!
//! | option | meaning |
//! |---|---|
//! | `floor=-40` / `ceiling=3` | the dB span (bars) |
//! | `segments=16` | how many segments (bars) |
//! | `gr` | reads gain reduction: lamps light as reduction *deepens* |
//! | `vertical` | a column (ladders; bars choose by id) |
//!
//! `Place::scale` prints labels along a bar, and *is* a ladder's thresholds
//! (dB, `"+3"` / `"-10"`; for `gr`, dB of reduction: `"1" … "20"`).
//! `Place::reverse` lays lamp 0 at the far end (a ladder lit right to left).

use crate::document::proto::{NodeId, TextAlign};
use crate::surface::proto::{ControlKind, Curve, CurvePoint, SlotRole};
use crate::surface::widgets::Builder;

use crate::surface::kit::controls::parts::{
    PANEL_INK, alpha, cased, ink, lin, mixs, rad, shade, tint,
};
use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

/// A lamp's colour at `db` on a level meter: green, amber from −6, red
/// from 0 — or one colour for every lamp.
fn lamp_color(scheme: Option<&str>, db: f64, gr: bool) -> &'static str {
    match scheme {
        Some("green") => "#46e05a",
        Some("amber") => "#ffb21e",
        Some("red") => "#ff3b2f",
        Some("blue") => "#4aa8ff",
        _ if gr => "#ffb21e",
        _ if db >= 0.0 => "#ff3b2f",
        _ if db >= -6.0 => "#ffb21e",
        _ => "#46e05a",
    }
}

fn num(p: &Place, key: &str, default: f64) -> f64 {
    p.opt(key)
        .and_then(|v| v.trim_start_matches('+').parse().ok())
        .unwrap_or(default)
}

/// A housing: a black recessed strip.
fn housing(name: &str, (x, y, w, h): (f64, f64, f64, f64), r: f64) -> crate::document::proto::Node {
    let mut n = draw::frame(name, x, y, w, h);
    n.fills = vec![lin(
        180.0,
        &[(0.0, "#050506".into()), (1.0, "#121315".into())],
    )];
    draw::with_effects(
        draw::stroked(draw::rounded(n, r), "#000000", 1.0),
        vec![
            draw::inner_shadow("#000000", 0.0, 1.0, 3.0),
            draw::drop_shadow("#ffffff14", 0.0, 1.0, 0.0),
        ],
    )
}

/// A bar's segment: its unlit glass, then the lit segment (an `Led`).
fn segment(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    color: &str,
    horizontal: bool,
) -> NodeId {
    let across = if horizontal { 180.0 } else { 90.0 };
    let dark = draw::rounded(
        draw::rect(
            "unlit",
            x,
            y,
            w,
            h,
            lin(
                across,
                &[
                    (0.0, mixs(color, "#000000", 0.72)),
                    (1.0, mixs(color, "#000000", 0.86)),
                ],
            ),
        ),
        1.0,
    );
    b.add(parent, dark);
    let lit = draw::rect(
        "segment",
        x,
        y,
        w,
        h,
        lin(
            across,
            &[
                (0.0, tint(color, 0.28)),
                (0.5, color.to_string()),
                (1.0, shade(color, 0.25)),
            ],
        ),
    );
    let lit = draw::with_effects(
        draw::rounded(lit, 1.0),
        vec![draw::drop_shadow(
            &alpha(color, 0.7),
            0.0,
            0.0,
            w.min(h) * 0.9,
        )],
    );
    b.add(
        parent,
        draw::emissive(lit, draw::hex(color), 0.8, w.min(h) * 1.6),
    )
}

fn bar(b: &mut Builder<'_>, parent: NodeId, p: &Place, horizontal: bool) -> NodeId {
    let len = p.size.unwrap_or(160.0).max(24.0);
    let thick = num(p, "thick", (len * 0.075).clamp(6.0, 18.0));
    let gr = p.flag("gr");
    let floor = num(p, "floor", if gr { 0.0 } else { -40.0 });
    let ceiling = num(p, "ceiling", if gr { -20.0 } else { 3.0 });
    let n = num(p, "segments", 16.0).clamp(2.0, 64.0) as usize;
    let inkc = p.ink_color(PANEL_INK);
    let labels = p.scale.clone().unwrap_or_default();
    let label_size = (thick * 0.8).clamp(7.0, 9.0);
    let gutter = if labels.is_empty() {
        0.0
    } else {
        label_size * 1.6
    };
    let (w, h) = if horizontal {
        (len, thick + gutter)
    } else {
        (thick + gutter * 2.2, len)
    };
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let (hw, hh) = if horizontal {
        (len, thick)
    } else {
        (thick, len)
    };
    let hx = if horizontal { 0.0 } else { w - thick };
    let strip = b.add(root, housing("housing", (hx, 0.0, hw, hh), 2.0));
    let pad = 2.0;
    let (sw, sh) = (hw - 2.0 * pad, hh - 2.0 * pad);
    let pitch = if horizontal {
        sw / n as f64
    } else {
        sh / n as f64
    };
    let mut c = p.control(ControlKind::Gauge {
        sweep_degrees: 0.0,
        floor_db: floor,
        ceiling_db: ceiling,
    });
    for i in 0..n {
        let slot = if p.reverse { n - 1 - i } else { i };
        // The dB at this segment's top edge colours it.
        let db = floor + (ceiling - floor) * (i as f64 + 1.0) / n as f64;
        let color = p
            .lamp_color(i)
            .unwrap_or_else(|| lamp_color(p.variant.as_deref(), db - 0.01, gr).to_string());
        let color = color.as_str();
        let rect = if horizontal {
            (
                pad + slot as f64 * pitch + pitch * 0.12,
                pad,
                pitch * 0.76,
                sh,
            )
        } else {
            // Vertical: lamp 0 at the bottom.
            (
                pad,
                pad + sh - (slot as f64 + 1.0) * pitch + pitch * 0.12,
                sw,
                pitch * 0.76,
            )
        };
        let lit = segment(b, strip, rect, color, horizontal);
        c = c.slot(SlotRole::Led, lit.0);
    }
    let m = labels.len();
    for (i, s) in labels.iter().enumerate() {
        let t = if m < 2 {
            0.5
        } else {
            i as f64 / (m - 1) as f64
        };
        let t = if p.reverse { 1.0 - t } else { t };
        let node = if horizontal {
            let x = pad + t * sw;
            ink(
                "scale",
                s,
                x - 20.0,
                thick + gutter * 0.55,
                40.0,
                label_size,
                &inkc,
                TextAlign::Center,
                600.0,
            )
        } else {
            let y = pad + (1.0 - t) * sh;
            ink(
                "scale",
                s,
                0.0,
                y,
                w - thick - 3.0,
                label_size,
                &inkc,
                TextAlign::Right,
                600.0,
            )
        };
        b.add(root, cased(node, p.case()));
    }
    b.controls.insert(root, c);
    root
}

fn led_bar(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    bar(b, parent, p, true)
}

fn led_column(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    bar(b, parent, p, false)
}

/// Thresholds from printed labels (`"+3"`, `"-10"`, `"20"`); unparseable
/// labels fall back to their position.
fn thresholds(labels: &[String]) -> Vec<f64> {
    labels
        .iter()
        .enumerate()
        .map(|(i, l)| {
            l.trim()
                .trim_start_matches('+')
                .trim_end_matches("dB")
                .trim()
                .parse()
                .unwrap_or(i as f64)
        })
        .collect()
}

/// The curve that puts threshold `i` of `n` at position `(i + 1) / n` over
/// the span `floor..ceiling` (either way round).
fn ladder_curve(th: &[f64], floor: f64, ceiling: f64) -> Curve {
    let n = th.len() as f64;
    let span = ceiling - floor;
    let mut pts = vec![CurvePoint { x: 0.0, y: 0.0 }];
    for (i, t) in th.iter().enumerate() {
        pts.push(CurvePoint {
            x: ((t - floor) / span).clamp(0.0, 1.0),
            y: (i as f64 + 1.0) / n,
        });
    }
    pts.sort_by(|a, b| a.x.total_cmp(&b.x));
    pts.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9);
    Curve::Points { points: pts }
}

fn led_ladder(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let gr = p.flag("gr");
    let vertical = p.flag("vertical");
    let labels = p.scale.clone().unwrap_or_else(|| {
        let d: &[&str] = if gr {
            &["1", "2", "3", "4", "6", "8", "10", "15", "20"]
        } else {
            &["-20", "-10", "-6", "-3", "0", "+3"]
        };
        d.iter().map(ToString::to_string).collect()
    });
    let mut th = thresholds(&labels);
    if gr {
        th.iter_mut().for_each(|t| *t = -t.abs());
    }
    let n = th.len().max(1);
    let len = p.size.unwrap_or(24.0 * n as f64).max(12.0 * n as f64);
    let pitch = len / n as f64;
    let d = (pitch * 0.5).clamp(5.0, 14.0);
    let size = (pitch * 0.34).clamp(6.5, 9.0);
    let inkc = p.ink_color(PANEL_INK);
    let label_h = size * 1.5;
    let (w, h) = if vertical {
        (d + 8.0 + size * 3.2, len)
    } else {
        (len, d + 6.0 + label_h)
    };
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    // The span the lamps cover: from one step below the first threshold to
    // the last (gain reduction: 0 dB down to the deepest).
    let (floor, ceiling) = if gr {
        (0.0, th.iter().copied().fold(0.0, f64::min).min(-1.0))
    } else {
        let first = th.first().copied().unwrap_or(-20.0);
        let step = th.get(1).map_or(10.0, |s| (s - first).abs().max(1.0));
        (first - step, th.last().copied().unwrap_or(0.0))
    };
    let mut c = p.control(ControlKind::Gauge {
        sweep_degrees: 0.0,
        floor_db: floor,
        ceiling_db: ceiling,
    });
    if let Some(bd) = c.binding.as_mut() {
        bd.curve = ladder_curve(&th, floor, ceiling);
    }
    for (i, (label, t)) in labels.iter().zip(&th).enumerate() {
        let slot = if p.reverse { n - 1 - i } else { i };
        let along = pitch * (slot as f64 + 0.5);
        let (cx, cy) = if vertical {
            (d / 2.0 + 4.0, len - along)
        } else {
            (along, d / 2.0 + 3.0)
        };
        let color = p
            .lamp_color(i)
            .unwrap_or_else(|| lamp_color(p.variant.as_deref(), *t, gr).to_string());
        let color = color.as_str();
        let r = d / 2.0;
        b.add(
            root,
            draw::circle(
                "bezel",
                cx,
                cy,
                r + (d * 0.16).max(1.2),
                lin(180.0, &[(0.0, "#2a2b2e".into()), (1.0, "#060607".into())]),
            ),
        );
        let dark = rad(
            0.38,
            0.3,
            0.95,
            &[
                (0.0, "#ffffff22".into()),
                (0.5, mixs(color, "#000000", 0.62)),
                (1.0, "#000000d0".into()),
            ],
        );
        b.add(
            root,
            draw::with_effects(
                draw::circle("lens", cx, cy, r, dark),
                vec![draw::inner_shadow("#000000b0", 0.0, 0.0, r * 0.7)],
            ),
        );
        let hot = rad(
            0.38,
            0.32,
            0.9,
            &[
                (0.0, "#ffffffee".into()),
                (0.2, tint(color, 0.55)),
                (0.55, color.to_string()),
                (1.0, shade(color, 0.5)),
            ],
        );
        let lit = draw::with_effects(
            draw::circle("lamp", cx, cy, r, hot),
            vec![draw::drop_shadow(&alpha(color, 0.8), 0.0, 0.0, d * 1.1)],
        );
        let lit = b.add(root, draw::emissive(lit, draw::hex(color), 0.9, d * 1.5));
        c = c.slot(SlotRole::Led, lit.0);
        let node = if vertical {
            ink(
                "threshold",
                label,
                d + 8.0,
                cy,
                size * 3.2,
                size,
                &inkc,
                TextAlign::Left,
                700.0,
            )
        } else {
            ink(
                "threshold",
                label,
                cx - pitch / 2.0,
                d + 6.0 + label_h / 2.0,
                pitch,
                size,
                &inkc,
                TextAlign::Center,
                700.0,
            )
        };
        b.add(root, cased(node, p.case()));
    }
    b.controls.insert(root, c);
    root
}

/// A panel lamp whose brightness is a stream: a jewel in a chrome bezel,
/// its lit lens a `Glow` slot (opacity — and so its light — follows the
/// value), the dark lens under it. An LFO rate lamp, a signal-present lamp.
fn lamp(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let s = p.size.unwrap_or(22.0).max(6.0);
    let color = match p.variant.as_deref() {
        Some("green") => "#43d17a",
        Some("amber") => "#f4a62c",
        Some("white") => "#fff2d0",
        Some("blue") => "#4aa0f4",
        _ => "#e33a2a",
    };
    let custom = p.lamp_color(0);
    let color = custom.as_deref().unwrap_or(color);
    let legend = p.legend.clone().unwrap_or_default();
    let size = (s * 0.4).clamp(7.0, 9.0);
    let cap_h = if legend.is_empty() { 0.0 } else { size * 1.6 };
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, s, s + cap_h));
    let c0 = s / 2.0;
    let chrome = draw::circle(
        "bezel",
        c0,
        c0,
        c0,
        draw::conic(
            20.0,
            &[
                (0.0, "#f2f2ef"),
                (0.25, "#8d8e8b"),
                (0.5, "#e6e6e2"),
                (0.75, "#7c7d7a"),
                (1.0, "#f2f2ef"),
            ],
        ),
    );
    b.add(
        root,
        draw::with_effects(
            chrome,
            vec![draw::drop_shadow("#00000080", 0.0, s * 0.05, s * 0.1)],
        ),
    );
    let r = s * 0.31;
    let dark = rad(
        0.38,
        0.3,
        0.95,
        &[
            (0.0, "#ffffff22".into()),
            (0.5, mixs(color, "#000000", 0.62)),
            (1.0, "#000000d0".into()),
        ],
    );
    b.add(
        root,
        draw::with_effects(
            draw::circle("lens", c0, c0, r, dark),
            vec![draw::inner_shadow("#000000b0", 0.0, 0.0, r * 0.7)],
        ),
    );
    let hot = rad(
        0.38,
        0.32,
        0.9,
        &[
            (0.0, "#ffffffee".into()),
            (0.2, tint(color, 0.55)),
            (0.55, color.to_string()),
            (1.0, shade(color, 0.5)),
        ],
    );
    let lit = draw::with_effects(
        draw::circle("lamp", c0, c0, r, hot),
        vec![draw::drop_shadow(&alpha(color, 0.85), 0.0, 0.0, s * 0.6)],
    );
    let lit = b.add(root, draw::emissive(lit, draw::hex(color), 1.0, s));
    // The bulb takes a moment to warm and cool: its glow follows the
    // stream through a spring (a lag on the Glow slot's reading), and with
    // `flicker` its light wavers like a filament's.
    if let Some(crate::surface::proto::Binding {
        source: crate::surface::proto::Source::Stream { id, .. },
        ..
    }) = &p.binding
    {
        crate::surface::kit::motion::add(
            b,
            lit,
            crate::surface::kit::motion::spring_lag(
                &format!("scalar({})", id.0),
                crate::surface::kit::motion::Target::Opacity,
                1.0,
            ),
        );
    }
    if p.flag("flicker") {
        crate::surface::kit::motion::add(b, lit, crate::surface::kit::motion::flicker(0.12));
    }
    let mut c = p.control(ControlKind::Readout).slot(SlotRole::Glow, lit.0);
    if !legend.is_empty() {
        let inkc = p.ink_color(PANEL_INK);
        let t = b.add(
            root,
            cased(
                ink(
                    "legend",
                    &legend,
                    -s,
                    s + cap_h * 0.55,
                    3.0 * s,
                    size,
                    &inkc,
                    TextAlign::Center,
                    700.0,
                ),
                p.case(),
            ),
        );
        c = c.slot(SlotRole::Label, t.0);
    }
    b.controls.insert(root, c);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    let def = |id, title, description, variants, default_size, binds, build| ComponentDef {
        id,
        family: Family::Meter,
        title,
        description,
        variants,
        default_size,
        binds,
        build,
    };
    vec![
        def(
            "meter.led_bar",
            "LED bar (horizontal)",
            "A horizontal segmented LED bar lit from the left — a digital unit's input/output ladder. Green, amber from −6, red from 0 (or one colour by variant; any colours with `color=#hex` or per lamp `colors=#a,#b,…`, the last carrying on). `options`: `floor=-40`, `ceiling=3`, `segments=16`, `gr` (lights as gain reduction deepens; span 0…−20). `scale` prints labels along it; `reverse` lights it from the right. Honours the binding's curve. Size = length.",
            &["traffic", "green", "amber", "red", "blue"],
            160.0,
            "level stream",
            led_bar,
        ),
        def(
            "meter.led_column",
            "LED column (vertical)",
            "A vertical segmented LED bar lit from the bottom — a console's channel meter. As `meter.led_bar` (options `floor=`, `ceiling=`, `segments=`, `gr`; `scale` labels beside it). Size = height.",
            &["traffic", "green", "amber", "red", "blue"],
            160.0,
            "level stream",
            led_column,
        ),
        def(
            "meter.led_ladder",
            "LED ladder",
            "A row of round lamps, each lit once the reading reaches the threshold printed under it — the Distressor's GR ladder, a signal/over pair. `scale` = the thresholds in dB (default −20 −10 −6 −3 0 +3; with the `gr` option, dB of reduction, default 1…20). `vertical` stacks it; `reverse` lights it right to left. Lit lamps give off light. `color=` / `colors=` as `meter.led_bar`. Size = length.",
            &["traffic", "green", "amber", "red", "blue"],
            144.0,
            "level stream",
            led_ladder,
        ),
        def(
            "meter.lamp",
            "Stream lamp",
            "A jewel lamp in a chrome bezel whose brightness follows a scalar stream (0…1 of its range) — an LFO rate lamp, a signal-present lamp. The lit lens is a `Glow` slot, so its light scales with the value; it warms and cools through a spring, and the `flicker` option makes its light waver like a filament. `legend` prints under it; `color=#hex` lights it in any colour. Size = bezel diameter.",
            &["red", "green", "amber", "white", "blue"],
            22.0,
            "scalar stream",
            lamp,
        ),
    ]
}
