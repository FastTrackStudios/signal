//! The Tone rack's look: session's palette and inks, and the node helpers
//! the panels are built from.
//!
//! Colours are session's, taken from where session takes them (the FTS
//! dark theme tokens, `comp_ui::comp_graph_svg::colors`, the plugins' inks)
//! so a frame panel and the rack it replaces are the same colours.

use crate::document::proto::{
    Color, Node, NodeId, NodeKind, Paint, Size, Stroke, StrokeAlign, StrokeCap, StrokeJoin,
    TextAlign, TextData, Transform,
};
use crate::surface::kit::draw;
use crate::surface::widgets::Builder;

/// FTS dark theme tokens (`daw-ui` `Theme::dark`).
pub const SURFACE: &str = "#09090b";
pub const SURFACE_RAISED: &str = "#18181b";
/// `tcp_meter_well`, `tcp_field`: a panel's well.
pub const WELL: &str = "#0a0a0c";
pub const TEXT: &str = "#e4e4e7";
pub const TEXT_DIM: &str = "#a1a1aa";
pub const TEXT_FAINT: &str = "#71717a";
/// `grid_measure` (white 36/255) and `grid_beat` (white 20/255).
pub const GRID: &str = "#ffffff24";
pub const GRID_BEAT: &str = "#ffffff14";

/// The EQ's total response (the plugin painter's gold).
pub const EQ_INK: &str = "#d4a932";
/// The compressor display (`comp_graph_svg::colors`).
pub const THRESHOLD: &str = "#ff7878";
pub const REDUCTION_EDGE: &str = "#ff5050";
pub const REDUCTION_FILL: &str = "#dc2828";
pub const COMP_GREY: &str = "#8a8a92";
/// The gate's green (the door lane's "open").
pub const GATE_INK: &str = "#34d399";
/// The saturator: red, hotter, glow.
pub const SAT_INK: &str = "#ff4a3d";
pub const SAT_HOT: &str = "#ffb347";
pub const SAT_GLOW: &str = "#ff2e0c";
/// The de-esser's pink.
pub const DEESS_INK: &str = "#f472b6";
/// The returns: delay blue, reverb violet.
pub const DELAY_INK: &str = "#3b82f6";
pub const REVERB_INK: &str = "#8b5cf6";

/// A panel header's height, px (7 pt type plus air).
pub const HEAD: f64 = 10.0;
/// Between two panels.
pub const GAP: f64 = 3.0;
/// A phase container's bar.
pub const HEAD_H: f64 = 15.0;
/// An EQ band handle's radius.
pub const HANDLE: f64 = 2.6;
/// The machine selector under a delay, reverb or saturator.
pub const SELECTOR: f64 = 16.0;
/// One chip of it.
pub const CHIP: f64 = 14.0;
/// The gate's door lane.
pub const LANE: f64 = 9.0;
/// The de-esser's band strip.
pub const BAND_STRIP: f64 = 18.0;
/// How faint a blank phase is.
pub const BLANK_ALPHA: f64 = 0.35;

/// `#rrggbb` with an alpha.
#[must_use]
pub fn alpha(c: &str, a: f64) -> String {
    let h = c.trim_start_matches('#');
    let base = &h[..6.min(h.len())];
    let a0 = if h.len() == 8 {
        f64::from(u8::from_str_radix(&h[6..8], 16).unwrap_or(255)) / 255.0
    } else {
        1.0
    };
    format!(
        "#{base}{:02x}",
        ((a0 * a).clamp(0.0, 1.0) * 255.0).round() as u8
    )
}

/// An EQ band's hue by frequency (`eq_graph_model::freq_to_color`: red →
/// violet across 20 Hz–20 kHz, HSL s 0.68 l 0.58).
#[must_use]
pub fn band_color(hz: f64) -> Color {
    let lo = 20.0_f64.log10();
    let hi = 20_000.0_f64.log10();
    let t = ((hz.max(1.0).log10() - lo) / (hi - lo)).clamp(0.0, 1.0);
    hsl(t * 290.0, 0.68, 0.58)
}

fn hsl(h: f64, s: f64, l: f64) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    // Quantised like the plugin's `#rrggbb`.
    let q = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) / 255.0;
    Color {
        r: q(r),
        g: q(g),
        b: q(b),
        a: 1.0,
    }
}

/// A colour's `#rrggbbaa`.
#[must_use]
pub fn to_hex(c: Color) -> String {
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b), b(c.a))
}

// ── nodes ────────────────────────────────────────────────────────────────

/// A frame at (`x`, `y`), `w × h`, parent-relative.
pub fn frame(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
) -> NodeId {
    b.add(parent, draw::frame(name, x, y, w.max(0.0), h.max(0.0)))
}

/// A clipping frame.
pub fn clip(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
) -> NodeId {
    let mut n = draw::frame(name, x, y, w.max(0.0), h.max(0.0));
    n.clips_content = true;
    b.add(parent, n)
}

/// A filled rectangle.
pub fn fill(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    color: &str,
) -> NodeId {
    b.add(
        parent,
        draw::rect(name, x, y, w.max(0.0), h.max(0.0), draw::solid(color)),
    )
}

/// A stroked path (SVG data in the parent's box space).
pub fn line(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (w, h): (f64, f64),
    d: String,
    color: &str,
    width: f64,
) -> NodeId {
    let mut n = Node::new(NodeKind::Vector, name);
    n.size = Size {
        width: w,
        height: h,
    };
    n.fill_geometry = vec![d];
    n.stroke = Stroke {
        paints: vec![draw::solid(color)],
        weight: width,
        cap: StrokeCap::Round,
        join: StrokeJoin::Round,
        align: StrokeAlign::Center,
        ..Stroke::default()
    };
    b.add(parent, n)
}

/// A filled path.
pub fn shape(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (w, h): (f64, f64),
    d: String,
    color: &str,
) -> NodeId {
    b.add(
        parent,
        draw::path(name, 0.0, 0.0, w, h, d, draw::solid(color)),
    )
}

/// An empty vector for a `Trace` (stroked) slot.
pub fn trace(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (w, h): (f64, f64),
    color: &str,
    width: f64,
    dash: bool,
) -> NodeId {
    let mut n = Node::new(NodeKind::Vector, name);
    n.size = Size {
        width: w,
        height: h,
    };
    n.stroke = Stroke {
        paints: vec![draw::solid(color)],
        weight: width,
        cap: StrokeCap::Round,
        join: StrokeJoin::Round,
        align: StrokeAlign::Center,
        dash_pattern: if dash { vec![2.5, 2.0] } else { Vec::new() },
        ..Stroke::default()
    };
    b.add(parent, n)
}

/// An empty vector for an `Area` (filled) slot.
pub fn area(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (w, h): (f64, f64),
    color: &str,
) -> NodeId {
    let mut n = Node::new(NodeKind::Vector, name);
    n.size = Size {
        width: w,
        height: h,
    };
    n.fills = vec![draw::solid(color)];
    b.add(parent, n)
}

/// A filled circle.
pub fn dot(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (cx, cy): (f64, f64),
    r: f64,
    color: &str,
) -> NodeId {
    b.add(parent, draw::circle(name, cx, cy, r, draw::solid(color)))
}

/// A line of text with its baseline at `baseline`, left at `x`, in a box
/// `w` wide (aligned within it).
#[allow(clippy::too_many_arguments)]
pub fn text(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    s: &str,
    (x, baseline, w): (f64, f64, f64),
    size: f64,
    color: &str,
    align: TextAlign,
) -> NodeId {
    // The painter sets the first baseline at box top + 1 + ascent (≈ 0.97
    // of the size for Inter).
    let top = baseline - size * 0.97 - 1.0;
    let mut n = Node::new(
        NodeKind::Text(TextData {
            characters: s.to_string(),
            font_size: size,
            align,
            ..TextData::default()
        }),
        name,
    );
    n.transform = Transform::translate(x, top);
    n.size = Size {
        width: w.max(1.0),
        height: (size * 1.3).ceil(),
    };
    n.fills = vec![Paint::solid(draw::hex(color))];
    b.add(parent, n)
}

/// The rough width of `s` at `size` (Inter): for fitting labels at build
/// time, where the font is not at hand.
#[must_use]
pub fn text_width(s: &str, size: f64) -> f64 {
    s.chars()
        .map(|c| match c {
            'i' | 'l' | '.' | ',' | ':' | '·' | '\'' | '|' | ' ' => 0.3,
            'I' | '1' | 'j' | 't' | 'f' | 'r' => 0.42,
            'M' | 'W' | 'm' | 'w' => 0.85,
            c if c.is_uppercase() => 0.68,
            c if c.is_ascii_digit() => 0.6,
            _ => 0.56,
        })
        .sum::<f64>()
        * size
}

/// `M x0 y0 L x1 y1`.
#[must_use]
pub fn seg((x0, y0): (f64, f64), (x1, y1): (f64, f64)) -> String {
    format!("M {x0:.2} {y0:.2} L {x1:.2} {y1:.2}")
}

/// A polyline.
#[must_use]
pub fn poly(points: &[(f64, f64)]) -> String {
    let mut d = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        d.push_str(&format!(
            "{} {x:.2} {y:.2} ",
            if i == 0 { 'M' } else { 'L' }
        ));
    }
    d.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_hues_match_the_plugins() {
        // eq_graph_model::freq_to_color(20.0) is hsl(0, .68, .58) = #e75353 (approx).
        let low = band_color(20.0);
        assert!(low.r > 0.85 && low.g < 0.4 && low.b < 0.4, "{low:?}");
        let high = band_color(20_000.0);
        assert!(high.b > high.g, "violet at the top: {high:?}");
        assert_eq!(alpha("#ff0000", 0.5), "#ff000080");
        assert_eq!(alpha("#ffffff24", 0.5), "#ffffff12");
    }
}
