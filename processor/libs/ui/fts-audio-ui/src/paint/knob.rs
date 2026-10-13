//! The flat rotary knob, painted.
//!
//! Geometry is the kit's knob: a 270° track starting at 135° (7 o'clock),
//! a value arc from the track start (or from 12 o'clock for a bipolar
//! parameter), a cap disc inside the track, a pointer across the arc, an
//! optional modulation arc just inside the track, and a detent tick at the
//! centre for bipolar parameters. Everything is in CSS pixels of a
//! `diameter × diameter` box.

use std::f64::consts::PI;

use anyrender::{PaintScene, Scene};
use kurbo::{Affine, Arc, BezPath, Circle, Line, Point, Shape, Stroke, Vec2};
use peniko::{Color, Fill};

/// Track start angle (degrees, clockwise from +x) and sweep.
pub const START_ANGLE: f64 = 135.0;
pub const SWEEP: f64 = 270.0;

/// Angle (degrees) the pointer sits at for a normalized value.
pub fn angle_for_value(v: f64) -> f64 {
    START_ANGLE + v.clamp(0.0, 1.0) * SWEEP
}

/// Point on a circle at `angle_deg`.
pub fn arc_point(cx: f64, cy: f64, r: f64, angle_deg: f64) -> (f64, f64) {
    let rad = angle_deg * PI / 180.0;
    (cx + r * rad.cos(), cy + r * rad.sin())
}

/// Everything the painter needs to draw one knob. Built by the component
/// from its props, handle, and the resolved theme; pure data.
#[derive(Clone, Debug, PartialEq)]
pub struct KnobLook {
    /// Box size in CSS px (the knob is square).
    pub diameter: f64,
    /// Normalized value 0..1.
    pub value: f64,
    /// Draw the value arc from the centre detent instead of the track start.
    pub bipolar: bool,
    /// Modulation range arc, normalized `(min, max)`, if any.
    pub mod_range: Option<(f64, f64)>,
    /// Hovered or dragging: slightly bolder arc with a glow.
    pub active: bool,
    pub accent: Color,
    pub track: Color,
    pub pointer: Color,
    pub detent: Color,
    pub mod_color: Color,
    pub cap_fill: Color,
    pub cap_stroke: Color,
}

fn arc_path(cx: f64, cy: f64, r: f64, start_deg: f64, end_deg: f64) -> BezPath {
    let sweep = end_deg - start_deg;
    let arc = Arc::new(
        Point::new(cx, cy),
        Vec2::new(r, r),
        start_deg.to_radians(),
        sweep.to_radians(),
        0.0,
    );
    arc.to_path(0.1)
}

/// Paint the knob into a fresh scene.
pub fn scene(look: &KnobLook) -> Scene {
    let mut s = Scene::new();
    paint(&mut s, look);
    s
}

/// Paint the knob into an existing scene at the origin.
///
/// Drawn back to front: cap, then its lit rim, then the track, then the marks
/// on the track, then the value arc over it, then the pointer. The order is
/// the point — the value arc has to sit ON the track, and the track has to be
/// visible for the arc to mean anything. It was not: `track` resolved to the
/// theme's `border`, which against a near-black panel is the panel, so every
/// knob read as a bare white stroke floating in space with no scale behind it.
pub fn paint(s: &mut Scene, look: &KnobLook) {
    let d = look.diameter;
    let (cx, cy) = (d / 2.0, d / 2.0);
    let r = d / 2.0 - 4.0;
    let val = look.value.clamp(0.0, 1.0);
    let cap_r = (r - 7.0).max(2.0);
    let at = Affine::IDENTITY;
    let track_w = 4.0;

    // ── The cap ─────────────────────────────────────────────────────────
    let cap = Circle::new((cx, cy), cap_r);
    s.fill(Fill::NonZero, at, look.cap_fill, None, &cap);

    // Lit from above, like every physical knob anyone has photographed: a
    // bright arc across the top of the rim and a dark one under it. Two
    // strokes are enough to read as a rounded cap and cost nothing next to a
    // gradient.
    let rim = |from: f64, to: f64, c: Color, w: f64| {
        (
            arc_path(cx, cy, cap_r - w / 2.0, from, to),
            Stroke::new(w).with_caps(kurbo::Cap::Butt),
            c,
        )
    };
    let (top, top_stroke, top_c) = rim(190.0, 350.0, look.pointer.with_alpha(0.22), 1.6);
    s.stroke(&top_stroke, at, top_c, None, &top);
    let (bot, bot_stroke, bot_c) = rim(10.0, 170.0, Color::BLACK.with_alpha(0.45), 1.6);
    s.stroke(&bot_stroke, at, bot_c, None, &bot);
    s.stroke(
        &Stroke::new(0.75),
        at,
        look.cap_stroke.with_alpha(0.95),
        None,
        &cap,
    );

    // ── The track ───────────────────────────────────────────────────────
    let track = arc_path(cx, cy, r, START_ANGLE, START_ANGLE + SWEEP);
    s.stroke(
        &Stroke::new(track_w).with_caps(kurbo::Cap::Round),
        at,
        look.track,
        None,
        &track,
    );

    // End stops. A knob with no marks says nothing about where its travel
    // ends; two ticks say it without the mush a full tick ring becomes at
    // 44 px.
    for angle in [START_ANGLE, START_ANGLE + SWEEP] {
        let (x1, y1) = arc_point(cx, cy, r + track_w / 2.0 + 1.0, angle);
        let (x2, y2) = arc_point(cx, cy, r + track_w / 2.0 + 3.0, angle);
        s.stroke(
            &Stroke::new(1.0),
            at,
            look.detent.with_alpha(0.55),
            None,
            &Line::new((x1, y1), (x2, y2)),
        );
    }

    // Centre detent (bipolar only, so the 0 mark is visible).
    if look.bipolar {
        let centre = START_ANGLE + SWEEP / 2.0;
        let (x1, y1) = arc_point(cx, cy, r - track_w / 2.0 - 1.0, centre);
        let (x2, y2) = arc_point(cx, cy, r + track_w / 2.0 + 3.0, centre);
        s.stroke(
            &Stroke::new(1.5),
            at,
            look.detent.with_alpha(0.9),
            None,
            &Line::new((x1, y1), (x2, y2)),
        );
    }

    // Modulation range arc, just inside the track.
    if let Some((lo, hi)) = look.mod_range {
        let lo_a = angle_for_value(lo.clamp(0.0, 1.0));
        let hi_a = angle_for_value(hi.clamp(0.0, 1.0));
        let (a, b) = if lo_a <= hi_a {
            (lo_a, hi_a)
        } else {
            (hi_a, lo_a)
        };
        let path = arc_path(cx, cy, r - 2.0, a, b);
        s.stroke(
            &Stroke::new(2.5).with_caps(kurbo::Cap::Round),
            at,
            look.mod_color.with_alpha(0.85),
            None,
            &path,
        );
    }

    // ── The value ───────────────────────────────────────────────────────
    let end_angle = angle_for_value(val);
    let value_path = if look.bipolar {
        let centre = START_ANGLE + SWEEP / 2.0;
        if (val - 0.5).abs() < 0.001 {
            None
        } else if val > 0.5 {
            Some(arc_path(cx, cy, r, centre, end_angle))
        } else {
            Some(arc_path(cx, cy, r, end_angle, centre))
        }
    } else if val > 0.001 {
        Some(arc_path(cx, cy, r, START_ANGLE, end_angle))
    } else {
        None
    };
    if let Some(path) = value_path {
        if look.active {
            s.stroke(
                &Stroke::new(10.0).with_caps(kurbo::Cap::Round),
                at,
                look.accent.with_alpha(0.30),
                None,
                &path,
            );
        }
        s.stroke(
            &Stroke::new(track_w + 0.5).with_caps(kurbo::Cap::Round),
            at,
            look.accent,
            None,
            &path,
        );
    }

    // ── The pointer ─────────────────────────────────────────────────────
    // Sits on the cap rather than bridging cap and track: bridging made it
    // read as a spoke joining two rings instead of as the thing pointing at
    // the value.
    let (tx, ty) = arc_point(cx, cy, cap_r * 0.32, end_angle);
    let (tx2, ty2) = arc_point(cx, cy, cap_r - 2.0, end_angle);
    // A dark under-stroke so the pointer holds up over a pale cap too.
    s.stroke(
        &Stroke::new(3.4).with_caps(kurbo::Cap::Round),
        at,
        Color::BLACK.with_alpha(0.35),
        None,
        &Line::new((tx, ty), (tx2, ty2)),
    );
    s.stroke(
        &Stroke::new(2.0).with_caps(kurbo::Cap::Round),
        at,
        look.pointer,
        None,
        &Line::new((tx, ty), (tx2, ty2)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(value: f64, bipolar: bool) -> KnobLook {
        KnobLook {
            diameter: 56.0,
            value,
            bipolar,
            mod_range: None,
            active: false,
            accent: Color::WHITE,
            track: Color::BLACK,
            pointer: Color::WHITE,
            detent: Color::WHITE,
            mod_color: Color::WHITE,
            cap_fill: Color::BLACK,
            cap_stroke: Color::BLACK,
        }
    }

    // r[verify fx.control.painted]
    #[test]
    fn a_knob_paints_without_a_renderer() {
        // A scene is a recording: building one needs no window, no GPU, no DOM.
        let _ = scene(&look(0.0, false));
        let _ = scene(&look(1.0, false));
        let _ = scene(&look(0.5, true));
        let _ = scene(&KnobLook {
            mod_range: Some((0.2, 0.8)),
            active: true,
            ..look(0.3, false)
        });
    }

    #[test]
    fn the_pointer_sweeps_270_degrees_from_7_oclock() {
        assert_eq!(angle_for_value(0.0), 135.0);
        assert_eq!(angle_for_value(0.5), 270.0);
        assert_eq!(angle_for_value(1.0), 405.0);
    }
}
