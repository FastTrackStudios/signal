//! Parts shared by the switching families (and the panels): colour
//! arithmetic on hex strings, gradients built from computed stops, weighted
//! silkscreen text, and the jewel lamp every lit part uses.

use crate::document::proto::{Color, Node, NodeId, NodeKind, Paint, TextAlign, TextCase};
use crate::surface::widgets::Builder;

use crate::surface::kit::draw;

/// `#rrggbbaa` for a colour.
#[must_use]
pub(crate) fn hexc(c: Color) -> String {
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b), b(c.a))
}

/// `c` moved `t` of the way to `to`.
#[must_use]
pub(crate) fn mixs(c: &str, to: &str, t: f64) -> String {
    hexc(draw::mix(draw::hex(c), draw::hex(to), t))
}

/// `c` lightened toward white by `t`.
#[must_use]
pub(crate) fn tint(c: &str, t: f64) -> String {
    mixs(c, "#ffffff", t)
}

/// `c` darkened toward black by `t`.
#[must_use]
pub(crate) fn shade(c: &str, t: f64) -> String {
    mixs(c, "#000000", t)
}

/// `c` at alpha `a`.
#[must_use]
pub(crate) fn alpha(c: &str, a: f64) -> String {
    hexc(draw::with_alpha(draw::hex(c), a))
}

fn refs(stops: &[(f64, String)]) -> Vec<(f64, &str)> {
    stops.iter().map(|(p, s)| (*p, s.as_str())).collect()
}

/// [`draw::linear`] over computed stops.
#[must_use]
pub(crate) fn lin(angle: f64, stops: &[(f64, String)]) -> Paint {
    draw::linear(angle, &refs(stops))
}

/// [`draw::radial_at`] over computed stops.
#[must_use]
pub(crate) fn rad(cx: f64, cy: f64, r: f64, stops: &[(f64, String)]) -> Paint {
    draw::radial_at(cx, cy, r, &refs(stops))
}

/// Silkscreen: text in a `w`-wide box whose vertical centre is `cy`, at a
/// stated weight, uppercased.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub(crate) fn ink(
    name: &str,
    s: &str,
    x: f64,
    cy: f64,
    w: f64,
    size: f64,
    color: &str,
    align: TextAlign,
    weight: f64,
) -> Node {
    cased(
        ink_raw(name, s, x, cy, w, size, color, align, weight),
        TextCase::Upper,
    )
}

#[allow(clippy::too_many_arguments)]
fn ink_raw(
    name: &str,
    s: &str,
    x: f64,
    cy: f64,
    w: f64,
    size: f64,
    color: &str,
    align: TextAlign,
    weight: f64,
) -> Node {
    let mut n = draw::text(name, s, x, cy - size * 0.68, w, size, color, align);
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(weight);
        t.letter_spacing = size * 0.04;
    }
    n
}

/// A text node set in `case` (a placed component's [`Place::case`](crate::surface::kit::Place::case)).
#[must_use]
pub(crate) fn cased(mut n: Node, case: TextCase) -> Node {
    if let NodeKind::Text(t) = &mut n.kind {
        t.text_case = case;
    }
    n
}

/// Rough rendered width of `s` at `size` (for laying out legends).
#[must_use]
pub(crate) fn text_w(s: &str, size: f64) -> f64 {
    s.chars().count() as f64 * size * 0.66
}

/// Take `n` out of any auto layout its parent gets, so decoration stays put.
#[must_use]
pub(crate) fn fixed(mut n: Node) -> Node {
    n.layout_child.absolute = true;
    n
}

/// A panel indicator lamp — a domed jewel in a bezel — centred at
/// (`cx`, `cy`), diameter `d`. Returns the *lit* lens, the node to put in a
/// `Led` slot: the unlit lens underneath still reads as a lens when it
/// dims, because an empty hole on a panel reads as a missing part.
///
/// Explicit stops, as the source does: the core has to be brighter than the
/// colour itself for the lens to read as lit from within.
pub(crate) fn lamp(
    b: &mut Builder<'_>,
    parent: NodeId,
    cx: f64,
    cy: f64,
    d: f64,
    color: &str,
) -> NodeId {
    let r = d / 2.0;
    let rim = (d * 0.12).max(1.0);
    b.add(
        parent,
        draw::circle("bezel", cx, cy, r + rim, draw::solid("#0c0c0e")),
    );
    let dark = vec![
        (0.0, alpha("#ffffff", 0.10)),
        (0.52, shade(color, 0.35)),
        (1.0, alpha("#000000", 0.78)),
    ];
    let unlit = draw::with_effects(
        draw::circle("lens", cx, cy, r, rad(0.36, 0.30, 0.95, &dark)),
        vec![draw::inner_shadow("#000000c0", 0.0, 0.0, d * 0.34)],
    );
    b.add(parent, unlit);
    let hot = vec![
        (0.0, alpha("#ffffff", 0.95)),
        (0.16, alpha(&tint(color, 0.55), 1.0)),
        (0.46, color.to_string()),
        (0.70, color.to_string()),
        (1.0, shade(color, 0.55)),
    ];
    let lit = draw::with_effects(
        draw::circle("lamp", cx, cy, r, rad(0.36, 0.30, 0.95, &hot)),
        vec![draw::drop_shadow(&alpha(color, 0.85), 0.0, 0.0, d * 0.8)],
    );
    // The light it throws, on the lit lens itself so an `Led` slot dimming
    // it (off = 20%) dims the light with it.
    b.add(parent, draw::emissive(lit, draw::hex(color), 0.9, d))
}

/// A solid paint from a computed colour string.
#[must_use]
pub(crate) fn fill(c: &str) -> Paint {
    draw::solid(c)
}

/// Silkscreen ink for legends printed on the panel around a control. Light:
/// most of the units the kit comes from are dark-faced (1176, SSL, Neve,
/// Fairchild, Pultec blue); on a light panel, restyle the text nodes.
pub(crate) const PANEL_INK: &str = "#e6e2d8";
