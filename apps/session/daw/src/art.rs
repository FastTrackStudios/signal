//! `daw_theme_art` drawings, painted into the recorded scene.
//!
//! This is the second consumer of [`daw_theme_art::paint`]: the first is
//! the SVG the DOM and the PNG rasteriser use. Both read the same shape
//! data, so the knob on the canvas and the knob in the browser are the
//! same knob — which is the entire reason the shape layer exists rather
//! than the canvas drawing its own.
//!
//! Nothing here decides what a control looks like. If a knob is wrong,
//! it is wrong in `paint`, and fixing it there fixes both surfaces.

use anyrender::{PaintScene, Scene};
use daw_theme::Color as ThemeColor;
use daw_theme_art::paint::{Align, Brush, Drawing, Op, Shape};
use vello::kurbo::{
    Affine, Arc, BezPath, Ellipse, Line, Point, Rect, RoundedRect, Shape as _, Stroke as KStroke,
    Vec2,
};
use vello::peniko::{Color, ColorStop, ColorStops, Fill, Gradient};

use crate::text::Font;

/// Draw `drawing` with its top-left at `(x, y)`.
pub fn place(scene: &mut Scene, drawing: &Drawing, font: &Font, x: f64, y: f64) {
    scaled(scene, drawing, font, x, y, 1.0);
}

/// The same, resized.
pub fn scaled(scene: &mut Scene, drawing: &Drawing, font: &Font, x: f64, y: f64, scale: f64) {
    squashed(scene, drawing, font, x, y, scale, scale);
}

/// The same, resized differently in each axis.
///
/// A control is authored at the size REAPER's image is, and a row does
/// not always have that much height — a track at fourteen pixels still
/// has all of its controls, it just has them flatter. Squashing the
/// drawing keeps ONE definition of each control rather than a second
/// set drawn small, which is the same argument that put the shape layer
/// in `daw_theme_art` to begin with.
///
/// Width is usually left alone: the panel is as wide as it ever was, so
/// a control that shrinks in both axes would be small AND surrounded by
/// gaps, where one that only flattens still reads as the same control.
pub fn squashed(
    scene: &mut Scene,
    drawing: &Drawing,
    font: &Font,
    x: f64,
    y: f64,
    scale_x: f64,
    scale_y: f64,
) {
    let scale = scale_y;
    let at = Affine::scale_non_uniform(scale_x, scale_y).then_translate(Vec2::new(x, y));
    for op in &drawing.ops {
        match op {
            Op::Fill(shape, brush) => {
                let brush = paint(brush);
                scene.fill(Fill::NonZero, at, &brush, None, &path_of(shape));
            }
            Op::Stroke(shape, brush, stroke) => {
                let style = KStroke::new(stroke.width).with_caps(if stroke.round_cap {
                    vello::kurbo::Cap::Round
                } else {
                    vello::kurbo::Cap::Butt
                });
                let brush = paint(brush);
                scene.stroke(&style, at, &brush, None, &path_of(shape));
            }
            Op::Text {
                body,
                x: tx,
                baseline,
                size,
                color,
                align,
            } => {
                let left = match align {
                    Align::Left => *tx,
                    Align::Centre => *tx - font.width(body, *size) / 2.0,
                };
                // Glyphs are placed by transform rather than drawn into
                // the scaled space, so the scale is applied here too.
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::as_conversions,
                    reason = "a font size in points, which every text API takes as f32"
                )]
                let size = (f64::from(*size) * scale) as f32;
                crate::tcp::glyphs(
                    scene,
                    font,
                    convert(*color),
                    body,
                    left.mul_add(scale_x, x),
                    (*baseline).mul_add(scale, y),
                    size,
                );
            }
        }
    }
}

/// The kurbo path for a shape.
///
/// Everything becomes a `BezPath` rather than staying its own type.
/// `PaintScene::fill` is generic over the shape, so the alternative is a
/// separate call per variant — and this runs while the scene is RECORDED,
/// once per control per track, not per frame. The allocation is paid at
/// project open and never again.
pub(crate) fn path_of(shape: &Shape) -> BezPath {
    /// Fine enough that a 9px knob ring has no visible facets.
    const TOLERANCE: f64 = 0.05;

    match *shape {
        Shape::Rect { x, y, w, h, r } => {
            let rect = Rect::new(x, y, x + w, y + h);
            if r > 0.0 {
                RoundedRect::from_rect(rect, r).to_path(TOLERANCE)
            } else {
                rect.to_path(TOLERANCE)
            }
        }
        Shape::Ellipse { cx, cy, rx, ry } => {
            Ellipse::new(Point::new(cx, cy), (rx, ry), 0.0).to_path(TOLERANCE)
        }
        // Degrees clockwise from twelve o'clock, which is how the knobs
        // were measured; kurbo counts counter-clockwise from three, so
        // this is the whole of the conversion.
        Shape::Arc {
            cx,
            cy,
            r,
            start,
            sweep,
        } => Arc::new(
            (cx, cy),
            (r, r),
            (start - 90.0).to_radians(),
            sweep.to_radians(),
            0.0,
        )
        .to_path(TOLERANCE),
        Shape::Line { from, to } => Line::new((from.0, from.1), (to.0, to.1)).to_path(TOLERANCE),
        Shape::Poly(ref points) => {
            let mut path = BezPath::new();
            if points.is_empty() {
                return path;
            }
            for (i, (px, py)) in points.iter().enumerate() {
                let p = (*px, *py);
                if i == 0 {
                    path.move_to(p);
                } else {
                    path.line_to(p);
                }
            }
            path.close_path();
            path
        }
    }
}

/// Convert a theme colour.
const fn convert(c: ThemeColor) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// The brush for one operation, in the drawing's own coordinates.
fn paint(brush: &Brush) -> anyrender::Paint {
    match brush {
        Brush::Solid(c) => anyrender::Paint::Solid(convert(*c)),
        Brush::Linear { from, to, stops } => anyrender::Paint::Gradient(
            Gradient::new_linear((from.0, from.1), (to.0, to.1)).with_stops(collect(stops)),
        ),
        Brush::Radial {
            centre,
            radius,
            stops,
        } => {
            // The one narrowing conversion in this file, and it is the
            // API's: `peniko::Gradient::new_radial` takes an f32 radius
            // while the shape layer is f64 throughout. The values are
            // control radii of a few pixels, which f32 holds exactly.
            #[expect(
                clippy::cast_possible_truncation,
                clippy::as_conversions,
                reason = "peniko's radial gradient takes an f32 radius; these are pixel radii"
            )]
            let radius = *radius as f32;
            anyrender::Paint::Gradient(
                Gradient::new_radial((centre.0, centre.1), radius).with_stops(collect(stops)),
            )
        }
    }
}

fn collect(stops: &[(f32, ThemeColor)]) -> ColorStops {
    let mut out = ColorStops::new();
    for (offset, color) in stops {
        out.push(ColorStop {
            offset: *offset,
            color: convert(*color).into(),
        });
    }
    out
}
