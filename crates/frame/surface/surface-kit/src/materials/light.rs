//! The kit's light: one key light from the **upper left**, the same for
//! every material, knob and enclosure, so parts built separately sit in one
//! room.
//!
//! - It comes from [`AZIMUTH`] (kit angles: 0 = up, clockwise; −35° is up
//!   and a little left), high above the panel.
//! - Faces tilted toward it are lit, away from it shaded; a rim catches it
//!   on its upper-left arc; a gloss hotspot sits up-left of a dome's crown.
//! - Shadows fall **away** from it — down and a little right
//!   ([`shadow_offset`]) — as a tight contact shadow (ambient occlusion)
//!   plus a soft cast shadow that grows with the part's height.
//!
//! Everything here is a fixed paint or node: none of it may sit inside a
//! knob's `Needle` frame (see KIT.md, *What turns and what doesn't*).

use document_proto::{Effect, Node, Paint, Stroke, StrokeAlign};

use crate::controls::parts::alpha;
use crate::draw;

/// Where the key light comes from: degrees, 0 = up, clockwise.
pub const AZIMUTH: f64 = -35.0;

/// A unit vector in screen space (y down) pointing *toward* the light.
#[must_use]
pub fn toward_light() -> (f64, f64) {
    let a = AZIMUTH.to_radians();
    (a.sin(), -a.cos())
}

/// The offset of a shadow cast by a part standing `height` px off the
/// surface: away from the light, foreshortened (the light is high).
#[must_use]
pub fn shadow_offset(height: f64) -> (f64, f64) {
    let (lx, ly) = toward_light();
    (-lx * height * 0.55, -ly * height * 0.8)
}

/// The CSS angle of a linear gradient running from the lit side to the
/// shaded side (for [`draw::linear`]).
#[must_use]
pub fn sheen_angle() -> f64 {
    AZIMUTH + 180.0
}

/// Where a dome's gloss hotspot sits in its box's unit square.
#[must_use]
pub fn hotspot() -> (f64, f64) {
    let (lx, ly) = toward_light();
    (0.5 + lx * 0.3, 0.5 + ly * 0.3)
}

/// A broad sheen over a face: `lit` (white alpha) at the light's side,
/// clear through the middle, `dark` (black alpha) at the far side.
#[must_use]
pub fn sheen(lit: f64, dark: f64) -> Paint {
    draw::linear(
        sheen_angle(),
        &[
            (0.0, &alpha("#ffffff", lit)),
            (0.42, "#ffffff00"),
            (0.62, "#00000000"),
            (1.0, &alpha("#000000", dark)),
        ],
    )
}

/// A raised edge (a bevel of `depth` px): lit on the light's side, shaded on
/// the far side — two inner shadows on the face.
#[must_use]
pub fn bevel(depth: f64, strength: f64) -> Vec<Effect> {
    let (lx, ly) = toward_light();
    let d = depth.max(0.5);
    vec![
        draw::inner_shadow(
            &alpha("#ffffff", 0.45 * strength),
            -lx * d,
            -ly * d,
            d * 1.6,
        ),
        draw::inner_shadow(&alpha("#000000", 0.55 * strength), lx * d, ly * d, d * 2.0),
    ]
}

/// A recess `depth` px deep (a grille set into a baffle): shaded under the
/// light's side, a thin lit lip on the far side.
#[must_use]
pub fn recess(depth: f64, strength: f64) -> Vec<Effect> {
    let (lx, ly) = toward_light();
    let d = depth.max(0.5);
    vec![
        draw::inner_shadow(&alpha("#000000", 0.7 * strength), -lx * d, -ly * d, d * 2.2),
        draw::inner_shadow(
            &alpha("#ffffff", 0.18 * strength),
            lx * d * 0.4,
            ly * d * 0.4,
            d * 0.6,
        ),
    ]
}

/// A lit edge round a (rounded) box, `depth` px wide: `raised` (a bevel —
/// lit on the light's side, shaded on the far side) or sunk (a recess —
/// the reverse). Two inside strokes with gradient paints, a crisp one and a
/// soft one — no effect layers, so it costs the same at any size. Prefer it
/// to [`bevel`] / [`recess`] on large faces.
#[must_use]
pub fn edge(
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    radius: f64,
    depth: f64,
    strength: f64,
    raised: bool,
) -> [Node; 2] {
    let (near, far) = if raised {
        ("#ffffff", "#000000")
    } else {
        ("#000000", "#ffffff")
    };
    let (na, fa) = if raised { (0.5, 0.55) } else { (0.65, 0.2) };
    let paint = |k: f64| {
        draw::linear(
            sheen_angle(),
            &[
                (0.0, &alpha(near, na * strength * k)),
                (0.4, &alpha(near, na * strength * k * 0.25)),
                (0.5, "#00000000"),
                (0.62, &alpha(far, fa * strength * k * 0.3)),
                (1.0, &alpha(far, fa * strength * k)),
            ],
        )
    };
    let ring = |nm: &str, wt: f64, k: f64| {
        let mut n = draw::rounded(draw::rect(nm, x, y, w, h, draw::solid("#00000000")), radius);
        n.fills.clear();
        n.stroke = Stroke {
            paints: vec![paint(k)],
            weight: wt,
            align: StrokeAlign::Inside,
            ..Stroke::default()
        };
        n.layout_child.absolute = true;
        n
    };
    let d = depth.max(0.5);
    [
        ring(&format!("{name} soft"), d * 1.8, 0.45),
        ring(name, (d * 0.55).max(0.6), 1.0),
    ]
}

/// The shadows of a part standing `height` px off the surface: a tight,
/// dark contact shadow and a soft cast shadow away from the light.
/// `strength` 0..1 (1 = the kit's default).
#[must_use]
pub fn stand(height: f64, strength: f64) -> Vec<Effect> {
    let (cx, cy) = shadow_offset(height * 0.25);
    let (sx, sy) = shadow_offset(height);
    vec![
        draw::drop_shadow(
            &alpha("#000000", 0.55 * strength),
            cx,
            cy,
            (height * 0.35).max(1.0),
        ),
        draw::drop_shadow(
            &alpha("#000000", 0.42 * strength),
            sx,
            sy,
            (height * 1.6).max(2.0),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_light_is_up_and_left_and_shadows_fall_down_and_right() {
        let (lx, ly) = toward_light();
        assert!(lx < 0.0 && ly < 0.0);
        let (dx, dy) = shadow_offset(10.0);
        assert!(dx > 0.0 && dy > dx);
        let (hx, hy) = hotspot();
        assert!(hx < 0.5 && hy < 0.5);
    }
}
