//! The painting seam: what frame paints *into*.
//!
//! A host has its own scene type — session records into an anyrender 0.11
//! `Scene` and replays it under a transform; REAPER hands over a LICE
//! bitmap; a test wants pixels. [`Canvas`] is the handful of calls all of
//! them share (anyrender's `PaintScene` shape, in kurbo 0.13 / peniko 0.6,
//! which every one of those hosts already links), so frame paints its
//! primitives straight into the host's scene rather than into a bitmap the
//! host then has to composite.
//!
//! [`Recording`] is frame's own replayable list of those calls: record once
//! when params change, replay every frame under whatever transform the
//! host's scroll and zoom put the strip at — the same model session's mixer
//! uses for everything else it draws.

use kurbo::{Affine, BezPath, Rect, Shape, Stroke};
use peniko::{Brush, Color, Fill};

/// A 2-D painter. Coordinates are the embed's own logical px; `transform`
/// maps them to the host's space.
pub trait Canvas {
    /// Fill `shape` with `brush`. `brush_transform` maps the brush's space
    /// to the shape's (a gradient laid out in a unit square, say).
    fn fill(
        &mut self,
        transform: Affine,
        rule: Fill,
        brush: &Brush,
        brush_transform: Option<Affine>,
        shape: &BezPath,
    );
    fn stroke(&mut self, transform: Affine, style: &Stroke, brush: &Brush, shape: &BezPath);
    /// Clip everything until the matching [`Canvas::pop`] to `shape`.
    fn push_clip(&mut self, transform: Affine, shape: &BezPath);
    /// Composite everything until the matching [`Canvas::pop`] at `alpha`,
    /// clipped to `shape`.
    fn push_opacity(&mut self, transform: Affine, alpha: f32, shape: &BezPath);
    fn pop(&mut self);

    /// Whether this canvas draws text itself ([`Canvas::text`]) — a host
    /// with its own fonts and shaping, so text in a theme looks like the
    /// host's text rather than frame's outlines of it.
    fn handles_text(&self) -> bool {
        false
    }

    /// Draw one line of text the host's way. False: not handled, and the
    /// caller draws its outlines instead.
    fn text(&mut self, transform: Affine, run: &TextRun) -> bool {
        let _ = (transform, run);
        false
    }

    /// Draw `run`, or `fallback` (its outlines, already under `transform`
    /// … `fallback_at` is what to replay them under) when the canvas does
    /// not draw text. A [`Recording`] keeps both, so a host that draws text
    /// gets the run back when the recording is replayed into it.
    fn text_with_fallback(&mut self, transform: Affine, run: &TextRun, fallback: &Recording, fallback_at: Affine) {
        if !self.text(transform, run) {
            fallback.replay(self, fallback_at);
        }
    }
}

/// One line of text, as a host that draws its own text needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    /// The box's left edge and top, in the canvas transform's space: a
    /// host puts the baseline its own ascent below `top`.
    pub left: f64,
    pub top: f64,
    /// The box's width, which `align` places the line within.
    pub width: f64,
    pub align: TextAlign,
    pub size: f64,
    pub weight: Option<f64>,
    pub family: Option<String>,
    pub letter_spacing: f64,
    pub color: Color,
}

/// Where a line sits in its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// Convenience calls on any [`Canvas`].
pub trait CanvasExt: Canvas {
    fn fill_color(&mut self, transform: Affine, color: Color, shape: &impl Shape) {
        self.fill(
            transform,
            Fill::NonZero,
            &Brush::Solid(color),
            None,
            &shape.to_path(0.1),
        );
    }

    fn stroke_color(&mut self, transform: Affine, color: Color, width: f64, shape: &impl Shape) {
        self.stroke(
            transform,
            &Stroke::new(width),
            &Brush::Solid(color),
            &shape.to_path(0.1),
        );
    }

    fn fill_rect(&mut self, transform: Affine, color: Color, rect: Rect) {
        self.fill_color(transform, color, &rect);
    }
}

impl<C: Canvas + ?Sized> CanvasExt for C {}

/// One recorded call.
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    Fill {
        transform: Affine,
        rule: Fill,
        brush: Brush,
        brush_transform: Option<Affine>,
        shape: BezPath,
    },
    Stroke {
        transform: Affine,
        style: Stroke,
        brush: Brush,
        shape: BezPath,
    },
    PushClip {
        transform: Affine,
        shape: BezPath,
    },
    PushOpacity {
        transform: Affine,
        alpha: f32,
        shape: BezPath,
    },
    Pop,
    /// A line of text, and its outlines for a canvas that does not draw
    /// text.
    Text {
        transform: Affine,
        run: TextRun,
        fallback: Recording,
    },
}

/// A replayable list of canvas calls.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recording {
    pub cmds: Vec<Cmd>,
}

impl Recording {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.cmds.clear();
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.cmds.len()
    }

    /// Replay into `canvas` with `transform` applied on top of each call's
    /// own (host space ← recording space).
    pub fn replay(&self, canvas: &mut (impl Canvas + ?Sized), transform: Affine) {
        for cmd in &self.cmds {
            match cmd {
                Cmd::Fill {
                    transform: t,
                    rule,
                    brush,
                    brush_transform,
                    shape,
                } => canvas.fill(transform * *t, *rule, brush, *brush_transform, shape),
                Cmd::Stroke {
                    transform: t,
                    style,
                    brush,
                    shape,
                } => canvas.stroke(transform * *t, style, brush, shape),
                Cmd::PushClip {
                    transform: t,
                    shape,
                } => canvas.push_clip(transform * *t, shape),
                Cmd::PushOpacity {
                    transform: t,
                    alpha,
                    shape,
                } => canvas.push_opacity(transform * *t, *alpha, shape),
                Cmd::Pop => canvas.pop(),
                Cmd::Text { transform: t, run, fallback } => {
                    canvas.text_with_fallback(transform * *t, run, fallback, transform);
                }
            }
        }
    }

    /// The bounding box of everything drawn, in recording space.
    #[must_use]
    pub fn bounds(&self) -> Option<Rect> {
        self.cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::Fill {
                    transform, shape, ..
                } => Some(transform.transform_rect_bbox(shape.bounding_box())),
                Cmd::Stroke {
                    transform,
                    style,
                    shape,
                    ..
                } => Some(
                    transform.transform_rect_bbox(
                        shape
                            .bounding_box()
                            .inflate(style.width / 2.0, style.width / 2.0),
                    ),
                ),
                Cmd::Text { fallback, .. } => fallback.bounds(),
                _ => None,
            })
            .reduce(|a, b| a.union(b))
    }
}

impl Canvas for Recording {
    fn fill(
        &mut self,
        transform: Affine,
        rule: Fill,
        brush: &Brush,
        brush_transform: Option<Affine>,
        shape: &BezPath,
    ) {
        self.cmds.push(Cmd::Fill {
            transform,
            rule,
            brush: brush.clone(),
            brush_transform,
            shape: shape.clone(),
        });
    }

    fn stroke(&mut self, transform: Affine, style: &Stroke, brush: &Brush, shape: &BezPath) {
        self.cmds.push(Cmd::Stroke {
            transform,
            style: style.clone(),
            brush: brush.clone(),
            shape: shape.clone(),
        });
    }

    fn push_clip(&mut self, transform: Affine, shape: &BezPath) {
        self.cmds.push(Cmd::PushClip {
            transform,
            shape: shape.clone(),
        });
    }

    fn push_opacity(&mut self, transform: Affine, alpha: f32, shape: &BezPath) {
        self.cmds.push(Cmd::PushOpacity {
            transform,
            alpha,
            shape: shape.clone(),
        });
    }

    fn pop(&mut self) {
        self.cmds.push(Cmd::Pop);
    }

    fn text_with_fallback(&mut self, transform: Affine, run: &TextRun, fallback: &Recording, fallback_at: Affine) {
        // Kept in recording space: the outlines under `fallback_at`, the
        // run under `transform`, both relative to this recording.
        let mut outlines = Recording::new();
        fallback.replay(&mut outlines, fallback_at);
        self.cmds.push(Cmd::Text { transform, run: run.clone(), fallback: outlines });
    }
}

/// Counts calls and throws them away — for timing the paint path without a
/// rasteriser.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub fills: usize,
    pub strokes: usize,
    pub layers: usize,
}

impl Canvas for Tally {
    fn fill(&mut self, _: Affine, _: Fill, _: &Brush, _: Option<Affine>, _: &BezPath) {
        self.fills += 1;
    }
    fn stroke(&mut self, _: Affine, _: &Stroke, _: &Brush, _: &BezPath) {
        self.strokes += 1;
    }
    fn push_clip(&mut self, _: Affine, _: &BezPath) {
        self.layers += 1;
    }
    fn push_opacity(&mut self, _: Affine, _: f32, _: &BezPath) {
        self.layers += 1;
    }
    fn pop(&mut self) {}
}

#[cfg(feature = "anyrender-0-11")]
mod anyrender_0_11_impl {
    //! Paint into any anyrender 0.11 `PaintScene` — session's recorded
    //! `anyrender::Scene` included.
    use anyrender_0_11 as anyrender;
    use kurbo::{Affine, BezPath, Stroke};
    use peniko::{BlendMode, Brush, Fill};

    use super::Canvas;

    /// Wraps a `PaintScene` as a [`Canvas`].
    pub struct AnyRender<'a, S: anyrender::PaintScene>(pub &'a mut S);

    fn paint(brush: &Brush) -> anyrender::PaintRef<'_> {
        match brush {
            Brush::Solid(c) => anyrender::PaintRef::from(*c),
            Brush::Gradient(g) => anyrender::PaintRef::from(g),
            Brush::Image(_) => anyrender::PaintRef::from(peniko::Color::TRANSPARENT),
        }
    }

    impl<S: anyrender::PaintScene> Canvas for AnyRender<'_, S> {
        fn fill(
            &mut self,
            transform: Affine,
            rule: Fill,
            brush: &Brush,
            brush_transform: Option<Affine>,
            shape: &BezPath,
        ) {
            self.0
                .fill(rule, transform, paint(brush), brush_transform, shape);
        }
        fn stroke(&mut self, transform: Affine, style: &Stroke, brush: &Brush, shape: &BezPath) {
            self.0.stroke(style, transform, paint(brush), None, shape);
        }
        fn push_clip(&mut self, transform: Affine, shape: &BezPath) {
            self.0.push_clip_layer(transform, shape);
        }
        fn push_opacity(&mut self, transform: Affine, alpha: f32, shape: &BezPath) {
            self.0
                .push_layer(BlendMode::default(), alpha, transform, shape, None, None);
        }
        fn pop(&mut self) {
            self.0.pop_layer();
        }
    }
}

#[cfg(feature = "anyrender-0-11")]
pub use anyrender_0_11_impl::AnyRender;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_replays_under_a_transform() {
        let mut rec = Recording::new();
        rec.fill_rect(
            Affine::IDENTITY,
            Color::WHITE,
            Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        let mut out = Recording::new();
        rec.replay(&mut out, Affine::translate((100.0, 5.0)));
        assert_eq!(out.bounds(), Some(Rect::new(100.0, 5.0, 110.0, 15.0)));
        let mut t = Tally::default();
        rec.replay(&mut t, Affine::IDENTITY);
        assert_eq!(t.fills, 1);
    }
}
