//! A gain-reduction movement read right to left — the modern vari-mu's
//! meter: a bright white card printed REDUCTION dB, its scale running from
//! 0 at the right stop out to 10 and beyond at the left (1, 2, 4, 6, 10,
//! crowded toward the left as a real movement's law puts them), black
//! print, a thin black needle. No reduction rests the needle on the right
//! stop's 0; reduction swings it left.
//!
//! The card only (no frame): a face mounts it in its own case. The control
//! is a `Gauge` over `-20 … 0` dB whose binding curve (for a stream bound
//! linearly) puts every printed mark where the card prints it.

use document_proto::{Node, NodeId, TextAlign, TextData};
use surface_proto::{ControlKind, Curve, CurvePoint, SlotRole, Source};
use surface_widgets::Builder;

use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// Design space: a 100 × 50 card, the pivot below its bottom edge.
const CW: f64 = 100.0;
const CH: f64 = 50.0;
const PIVOT: (f64, f64) = (50.0, 88.0);
const NEEDLE: f64 = 76.0;
/// Half the swing, degrees either side of straight up.
const HALF: f64 = 38.0;
const FLOOR: f64 = -20.0;
const CEILING: f64 = 0.0;

/// The card's law: (dB of reduction, fraction of travel from the left
/// stop). Read off the photographed card.
const LAW: &[(f64, f64)] = &[(-20.0, 0.0), (-10.0, 0.276), (-8.0, 0.345), (-6.0, 0.403), (-5.0, 0.46), (-4.0, 0.518), (-3.0, 0.605), (-2.0, 0.719), (-1.0, 0.858), (0.0, 1.0)];
/// Printed ticks: (dB, label).
const TICKS: &[(f64, &str)] = &[(-10.0, "10"), (-8.0, ""), (-6.0, "6"), (-5.0, ""), (-4.0, "4"), (-3.0, ""), (-2.0, "2"), (-1.0, "1"), (0.0, "0")];

fn fraction(db: f64) -> f64 {
    let db = db.clamp(FLOOR, CEILING);
    for w in LAW.windows(2) {
        let ((a, fa), (b, fb)) = (w[0], w[1]);
        if db <= b {
            return fa + (fb - fa) * (db - a) / (b - a);
        }
    }
    1.0
}

fn angle(db: f64) -> f64 {
    (fraction(db) * 2.0 - 1.0) * HALF
}

/// The binding curve: a linear reading over `FLOOR..CEILING` → the card.
fn law() -> Curve {
    Curve::Points { points: LAW.iter().map(|(db, f)| CurvePoint { x: (db - FLOOR) / (CEILING - FLOOR), y: *f }).collect() }
}

fn print(name: &str, s: &str, (cx, cy): (f64, f64), size: f64, color: &str, weight: f64) -> Node {
    let mut n = draw::label_at(name, s, cx, cy, size, color);
    if let document_proto::NodeKind::Text(TextData { font_weight, align, .. }) = &mut n.kind {
        *font_weight = Some(weight);
        *align = TextAlign::Center;
    }
    n
}

fn build(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p.size.unwrap_or(160.0).max(20.0);
    let k = w / CW;
    let h = CH * k;
    let ink = "#141414";
    let at = |(x, y): (f64, f64)| (x * k, y * k);
    let polar = |r: f64, a: f64| at(draw::polar(PIVOT.0, PIVOT.1, r, a));

    let mut card = draw::frame(&p.name, 0.0, 0.0, w, h);
    card.fills = vec![draw::linear(180.0, &[(0.0, "#fbfbf8"), (0.7, "#f3f3ef"), (1.0, "#e4e4df")])];
    card.clips_content = true;
    let card = draw::with_effects(draw::stroked(card, "#00000080", (0.8 * k).max(0.5)), vec![draw::inner_shadow("#00000059", 0.0, 1.5 * k, 3.5 * k)]);
    let mut card = card;
    if p.flag("glass") {
        card.shader = Some(document_proto::ShaderFx { shader: "glass".into(), params: std::iter::once(("strength".to_string(), 0.35)).collect(), inputs: Vec::new() });
    }
    let root = b.add(parent, card);

    // The arc, hooked down at both stops, its ticks standing out of it.
    let r = NEEDLE - 8.0;
    let (a0, a1) = (angle(FLOOR), angle(CEILING));
    let arc = draw::sector(PIVOT.0 * k, PIVOT.1 * k, (r - 0.35) * k, (r + 0.35) * k, a0, a1);
    b.add(root, draw::path("scale arc", 0.0, 0.0, w, h, arc, draw::solid(ink)));
    let seg = |a: (f64, f64), c: (f64, f64), wd: f64| {
        let (ux, uy) = (c.0 - a.0, c.1 - a.1);
        let len = ux.hypot(uy).max(1e-9);
        let (nx, ny) = (-uy / len * wd / 2.0, ux / len * wd / 2.0);
        draw::polygon(&[(a.0 + nx, a.1 + ny), (c.0 + nx, c.1 + ny), (c.0 - nx, c.1 - ny), (a.0 - nx, a.1 - ny)]) + " "
    };
    let mut d = String::new();
    // The hooks: short strokes down and out from each end of the arc.
    let (l, rr) = (polar(r, a0), polar(r, a1));
    d.push_str(&seg(l, (l.0 - 4.0 * k, l.1 - 2.6 * k), 0.7 * k));
    d.push_str(&seg(rr, (rr.0 + 4.0 * k, rr.1 - 2.6 * k), 0.7 * k));
    for (db, label) in TICKS {
        let a = angle(*db);
        let len = if label.is_empty() { 3.0 } else { 4.6 };
        if *db < CEILING {
            d.push_str(&seg(polar(r, a), polar(r + len, a), 0.7 * k));
        }
        if !label.is_empty() {
            let at_label = if *db >= CEILING { polar(r + 4.0, a + 2.5) } else { polar(r + 9.0, a) };
            b.add(root, print("numeral", label, at_label, 6.2 * k, ink, 600.0));
        }
    }
    b.add(root, draw::path("ticks", 0.0, 0.0, w, h, d, draw::solid(ink)));
    let legend = p.legend.clone().unwrap_or_else(|| "Reduction dB".into());
    if !legend.trim().is_empty() {
        let mut t = print("legend", &legend, at((CW / 2.0, CH * 0.80)), 4.6 * k, ink, 600.0);
        if let document_proto::NodeKind::Text(td) = &mut t.kind {
            td.letter_spacing = 0.3 * k;
            td.text_case = p.case();
        }
        b.add(root, t);
    }

    // The needle, in a square frame centred on the pivot, drawn straight up.
    let len = NEEDLE * k;
    let nw = (0.55 * k).max(0.8);
    let pivot = b.add(root, draw::frame("needle pivot", PIVOT.0 * k - len, PIVOT.1 * k - len, 2.0 * len, 2.0 * len));
    let needle = b.add(pivot, draw::rect("needle", len - nw / 2.0, 0.0, nw, len, draw::solid("#111111")));
    // Glass: the sheen off the window, top left.
    b.add(root, draw::rect("glass", 0.0, 0.0, w, h, draw::linear(160.0, &[(0.0, "#ffffff40"), (0.35, "#ffffff00"), (1.0, "#0000000f")])));

    let binding = p.binding.clone().map(|mut bd| {
        if matches!(bd.source, Source::Stream { .. }) && bd.curve == Curve::Linear {
            bd.curve = law();
        }
        bd
    });
    let mut c = p.control(ControlKind::Gauge { sweep_degrees: 2.0 * HALF, floor_db: FLOOR, ceiling_db: CEILING }).slot(SlotRole::Needle, needle.0);
    c.binding = binding;
    c.reference_dbfs = Some(surface_proto::VU_REFERENCE_DBFS);
    b.controls.insert(root, c);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "meter.reduction",
        family: Family::Meter,
        title: "Reduction meter",
        description: "A white gain-reduction card read right to left — 0 at the right stop, 1, 2, 4, 6, 10 dB of reduction out to the left, black print, REDUCTION dB under the arc, a thin black needle: the modern vari-mu's meter. Card only (mount it in the face's own case); bind a gain-reduction stream and no reduction rests on the 0. Size = card width.",
        variants: &[],
        default_size: 160.0,
        binds: "level stream",
        build,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_law_puts_zero_on_the_right_stop_and_is_monotonic() {
        assert!((fraction(0.0) - 1.0).abs() < 1e-12 && fraction(-20.0).abs() < 1e-12);
        assert!(LAW.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
        assert!((law().apply(0.5) - fraction(-10.0)).abs() < 1e-12);
    }
}
