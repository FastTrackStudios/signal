//! frame paint primitives → a [`Canvas`].
//!
//! The same primitives `render` paints into a vello scene
//! (`crate::render::build_scene_primitives`), painted through the host-agnostic
//! [`Canvas`] instead. It covers what surfaces are built from — frames and
//! rectangles with solid and gradient fills, aligned strokes and corner
//! radii, authored vectors (fills, strokes, dashes), text (as outlines),
//! clips, opacity layers and group transforms — and approximates the rest:
//! a glow is a soft wider copy of the shape, drop/inner shadows and blurs
//! are skipped, and masks draw their content unmasked. At strip sizes those
//! are the effects that do not read anyway.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use crate::document::proto::{CornerRadii, NodeId};
use crate::render::anyrender_renderer::{
    AffineTransform2D, EffectKind, FillPaint, GradientStop, NodeEffect, StrokeAlign, StrokeCap,
    StrokeJoin, StrokeStyle, TextAlignment, TextCase,
};
use crate::render::{PaintPrimitive, Rgba};
use kurbo::{Affine, BezPath, Point, Rect, RoundedRect, RoundedRectRadii, Shape, Stroke};
use peniko::{Brush, Color, ColorStop, Fill, Gradient};

use crate::render::canvas::canvas::{Canvas, Recording, TextAlign, TextRun};
use crate::render::canvas::text::Font;

/// Paint `primitives` into `canvas` under `root` (canvas ← primitive space).
/// Primitives whose node is in `skip` (with everything they open) are left
/// out; `only`, when given, keeps just those nodes' primitives.
pub fn paint_primitives(
    canvas: &mut (impl Canvas + ?Sized),
    primitives: &[PaintPrimitive],
    root: Affine,
    font: &Font,
) {
    paint_filtered(canvas, primitives, root, font, &|_| true);
}

/// [`paint_primitives`], keeping only the primitives `keep` says yes to.
/// Structure (groups, clips, layers) is kept whenever anything inside it is.
pub fn paint_filtered(
    canvas: &mut (impl Canvas + ?Sized),
    primitives: &[PaintPrimitive],
    root: Affine,
    font: &Font,
    keep: &dyn Fn(NodeId) -> bool,
) {
    let mut groups: Vec<Affine> = Vec::new();
    // Open structural primitives: whether each was pushed on the canvas.
    let mut open: Vec<bool> = Vec::new();
    for p in primitives {
        let t = groups.last().copied().unwrap_or(root);
        match p {
            PaintPrimitive::GroupStart { transform, .. } => groups.push(t * affine(transform)),
            PaintPrimitive::GroupEnd { .. } => {
                groups.pop();
            }
            PaintPrimitive::ClipStart {
                x,
                y,
                width,
                height,
                corner_radii,
                ..
            } => {
                canvas.push_clip(
                    t,
                    &rounded(Rect::new(*x, *y, x + width, y + height), corner_radii).to_path(0.05),
                );
                open.push(true);
            }
            PaintPrimitive::LayerStart {
                x,
                y,
                width,
                height,
                opacity,
                ..
            } => {
                if (*opacity - 1.0).abs() > 1e-6 {
                    canvas.push_opacity(
                        t,
                        *opacity as f32,
                        &Rect::new(*x, *y, x + width, y + height).to_path(0.05),
                    );
                    open.push(true);
                } else {
                    open.push(false);
                }
            }
            PaintPrimitive::ClipEnd { .. } | PaintPrimitive::LayerEnd { .. } => {
                if open.pop() == Some(true) {
                    canvas.pop();
                }
            }
            // Masks: the content is drawn, unmasked; the definition is not.
            PaintPrimitive::MaskDefStart { .. }
            | PaintPrimitive::MaskApplyStart { .. }
            | PaintPrimitive::MaskApplyEnd { .. } => {}
            PaintPrimitive::MaskDefEnd { .. } => {}
            PaintPrimitive::Rect {
                node_id,
                x,
                y,
                width,
                height,
                fills,
                stroke,
                corner_radii,
                effects,
                rotation,
                ..
            } => {
                if !keep(*node_id) {
                    continue;
                }
                let t = if rotation.abs() > 1e-9 {
                    t * Affine::translate((*x, *y))
                        * Affine::rotate(*rotation)
                        * Affine::translate((-*x, -*y))
                } else {
                    t
                };
                let rect = Rect::new(*x, *y, x + width, y + height);
                let shape = rounded(rect, corner_radii).to_path(0.05);
                glow(canvas, t, &shape, effects);
                for (fill, _) in fills {
                    fill_paint(canvas, t, &shape, rect, fill);
                }
                if let Some(s) = stroke.as_ref().filter(|s| s.width > 0.0) {
                    let r = match s.align {
                        StrokeAlign::Inside => rect.inset(-s.width / 2.0),
                        StrokeAlign::Outside => rect.inset(s.width / 2.0),
                        StrokeAlign::Center => rect,
                    };
                    let radii = shift_radii(
                        corner_radii,
                        match s.align {
                            StrokeAlign::Inside => -s.width / 2.0,
                            StrokeAlign::Outside => s.width / 2.0,
                            StrokeAlign::Center => 0.0,
                        },
                    );
                    stroke_shape(canvas, t, &rounded(r, &radii).to_path(0.05), rect, s);
                }
            }
            PaintPrimitive::Path {
                node_id,
                x,
                y,
                width,
                height,
                fill_paths,
                stroke_paths,
                fills,
                stroke,
                effects,
                rotation,
                ..
            } => {
                if !keep(*node_id) {
                    continue;
                }
                let t = t
                    * Affine::translate((*x, *y))
                    * if rotation.abs() > 1e-9 {
                        Affine::rotate(*rotation)
                    } else {
                        Affine::IDENTITY
                    };
                let box_rect = Rect::new(0.0, 0.0, *width, *height);
                let fill_geo: Vec<Arc<BezPath>> =
                    fill_paths.iter().filter_map(|d| parsed(d)).collect();
                let stroke_geo: Vec<Arc<BezPath>> =
                    stroke_paths.iter().filter_map(|d| parsed(d)).collect();
                let stroke = stroke.as_ref().filter(|s| s.width > 0.0);
                for g in &fill_geo {
                    glow(canvas, t, g, effects);
                }
                if !fills.is_empty() {
                    for g in &fill_geo {
                        // A stroked outline's open subpaths are lines, not areas.
                        let area = if stroke.is_some() && stroke_geo.is_empty() {
                            closed_only(g)
                        } else {
                            (**g).clone()
                        };
                        if area.elements().is_empty() {
                            continue;
                        }
                        for (fill, _) in fills {
                            fill_paint(canvas, t, &area, box_rect, fill);
                        }
                    }
                }
                if let Some(s) = stroke {
                    if stroke_geo.is_empty() {
                        for g in &fill_geo {
                            stroke_shape(canvas, t, g, box_rect, s);
                        }
                    } else {
                        // Stroke geometry is the stroke's own outline.
                        let paint = s
                            .gradient
                            .clone()
                            .unwrap_or_else(|| FillPaint::Solid(s.color.clone()));
                        for g in &stroke_geo {
                            fill_paint(canvas, t, g, box_rect, &paint);
                        }
                    }
                }
                if fill_geo.is_empty() && stroke_geo.is_empty() {
                    let shape = box_rect.to_path(0.05);
                    for (fill, _) in fills {
                        fill_paint(canvas, t, &shape, box_rect, fill);
                    }
                }
            }
            PaintPrimitive::Text {
                node_id,
                x,
                y,
                text,
                color: c,
                font_size,
                line_height,
                letter_spacing,
                text_case,
                font: spec,
                style_runs,
                width,
                align,
                ..
            } => {
                if !keep(*node_id) || text.is_empty() {
                    continue;
                }
                // One plain line: the host may draw it itself.
                let run = (style_runs.is_empty() && !text.contains('\n')).then(|| TextRun {
                    text: cased(text, text_case),
                    left: x + 1.0,
                    top: y + 1.0,
                    width: *width,
                    align: match align {
                        TextAlignment::Left => TextAlign::Left,
                        TextAlignment::Center => TextAlign::Center,
                        TextAlignment::Right => TextAlign::Right,
                    },
                    size: *font_size,
                    weight: spec.weight,
                    family: spec.family.clone(),
                    letter_spacing: *letter_spacing,
                    color: color(c),
                });
                if let Some(run) = &run
                    && canvas.handles_text()
                    && canvas.text(t, run)
                {
                    continue;
                }
                let mut outlines = Recording::new();
                let target: &mut dyn Canvas = &mut outlines;
                let lh = line_height.unwrap_or(font_size * 1.2).max(1.0);
                // Styled spans (the whole text as one without runs), each in
                // its own family, weight, size and colour, split into lines.
                let runs: Vec<(String, crate::render::fonts::FontSpec, f64, f64, &Rgba)> =
                    if style_runs.is_empty() {
                        vec![(
                            cased(text, text_case),
                            spec.clone(),
                            *font_size,
                            *letter_spacing,
                            c,
                        )]
                    } else {
                        style_runs
                            .iter()
                            .map(|r| {
                                (
                                    cased(&r.text, text_case),
                                    r.font().over(spec),
                                    r.font_size.unwrap_or(*font_size),
                                    r.letter_spacing.unwrap_or(*letter_spacing),
                                    r.color.as_ref().unwrap_or(c),
                                )
                            })
                            .collect()
                    };
                // Per line: (pieces placed from the line's pen start, width).
                let mut lines: Vec<Line<'_>> = vec![(Vec::new(), 0.0)];
                for (s, run_spec, size, spacing, run_color) in &runs {
                    let ascent = font.ascent_of(run_spec, *size);
                    for (i, part) in s.split('\n').enumerate() {
                        if i > 0 {
                            lines.push((Vec::new(), 0.0));
                        }
                        let laid = font.line_of(run_spec, part, *size, *spacing);
                        if let Some((pieces, w)) = lines.last_mut() {
                            let at = Affine::translate((*w, ascent)) * laid.path;
                            pieces.push((at, *run_color));
                            *w += laid.width;
                        }
                    }
                }
                for (i, (pieces, w)) in lines.iter().enumerate() {
                    let dx = match align {
                        TextAlignment::Left => 0.0,
                        TextAlignment::Center => ((width - w) / 2.0).max(0.0),
                        TextAlignment::Right => (width - w).max(0.0),
                    };
                    let at = Affine::translate((x + 1.0 + dx, y + 1.0 + i as f64 * lh));
                    for (path, run_color) in pieces {
                        target.fill(
                            t * at,
                            Fill::NonZero,
                            &Brush::Solid(color(run_color)),
                            None,
                            path,
                        );
                    }
                }
                match &run {
                    Some(run) => canvas.text_with_fallback(t, run, &outlines, Affine::IDENTITY),
                    None => outlines.replay(canvas, Affine::IDENTITY),
                }
            }
        }
    }
    // Close anything a truncated list left open.
    for pushed in open.into_iter().rev() {
        if pushed {
            canvas.pop();
        }
    }
}

/// Primitive `node_id`s in paint order, for tests and filters.
#[must_use]
pub fn node_of(p: &PaintPrimitive) -> Option<NodeId> {
    match p {
        PaintPrimitive::Rect { node_id, .. }
        | PaintPrimitive::Path { node_id, .. }
        | PaintPrimitive::Text { node_id, .. } => Some(*node_id),
        _ => None,
    }
}

// ── helpers ──────────────────────────────────────────────────────────────

fn affine(t: &AffineTransform2D) -> Affine {
    Affine::new([t.a, t.c, t.b, t.d, t.tx, t.ty])
}

#[must_use]
pub(crate) fn color(c: &Rgba) -> Color {
    Color::new([
        c.r as f32,
        c.g as f32,
        c.b as f32,
        c.a.clamp(0.0, 1.0) as f32,
    ])
}

fn rounded(rect: Rect, r: &CornerRadii) -> RoundedRect {
    let max = rect.width().min(rect.height()) / 2.0;
    let c = |v: f64| v.clamp(0.0, max.max(0.0));
    RoundedRect::from_rect(
        rect,
        RoundedRectRadii::new(
            c(r.top_left),
            c(r.top_right),
            c(r.bottom_right),
            c(r.bottom_left),
        ),
    )
}

fn shift_radii(r: &CornerRadii, by: f64) -> CornerRadii {
    let s = |v: f64| if v > 0.0 { (v + by).max(0.0) } else { 0.0 };
    CornerRadii {
        top_left: s(r.top_left),
        top_right: s(r.top_right),
        bottom_right: s(r.bottom_right),
        bottom_left: s(r.bottom_left),
    }
}

fn kurbo_stroke(s: &StrokeStyle) -> Stroke {
    let cap = match s.cap {
        StrokeCap::Round => kurbo::Cap::Round,
        StrokeCap::Square => kurbo::Cap::Square,
        _ => kurbo::Cap::Butt,
    };
    let join = match s.join {
        StrokeJoin::Round => kurbo::Join::Round,
        StrokeJoin::Bevel => kurbo::Join::Bevel,
        StrokeJoin::Miter => kurbo::Join::Miter,
    };
    let mut k = Stroke::new(s.width)
        .with_caps(cap)
        .with_join(join)
        .with_miter_limit(s.miter_limit.max(1.0));
    if !s.dash_pattern.is_empty() {
        k = k.with_dashes(s.dash_offset, s.dash_pattern.iter().copied());
    }
    k
}

fn stops(s: &[GradientStop]) -> Vec<ColorStop> {
    s.iter()
        .map(|g| ColorStop::from((g.offset as f32, color(&g.color))))
        .collect()
}

fn uv(t: Option<&AffineTransform2D>, u: f64, v: f64) -> (f64, f64) {
    t.map_or((u, v), |t| t.transform_point(u, v))
}

/// Stroke `shape` with `s`: a solid stroke directly, a gradient one as its
/// outline filled with the gradient laid across the node's box `b`.
fn stroke_shape(
    canvas: &mut (impl Canvas + ?Sized),
    t: Affine,
    shape: &BezPath,
    b: Rect,
    s: &StrokeStyle,
) {
    let style = kurbo_stroke(s);
    match &s.gradient {
        Some(g) => {
            let outline = kurbo::stroke(
                shape.elements().iter().copied(),
                &style,
                &kurbo::StrokeOpts::default(),
                0.05,
            );
            fill_paint(canvas, t, &outline, b, g);
        }
        None => canvas.stroke(t, &style, &Brush::Solid(color(&s.color)), shape),
    }
}

fn fill_paint(
    canvas: &mut (impl Canvas + ?Sized),
    t: Affine,
    shape: &BezPath,
    b: Rect,
    fill: &FillPaint,
) {
    let (x, y, w, h) = (b.x0, b.y0, b.width().max(1e-6), b.height().max(1e-6));
    match fill {
        FillPaint::Solid(c) => {
            if c.a > 0.0 {
                canvas.fill(t, Fill::NonZero, &Brush::Solid(color(c)), None, shape);
            }
        }
        FillPaint::GradientLinear {
            transform,
            stops: s,
        } if !s.is_empty() => {
            let (sx, sy) = uv(transform.as_ref(), 0.0, 0.5);
            let (ex, ey) = uv(transform.as_ref(), 1.0, 0.5);
            let g = Gradient::new_linear(
                Point::new(x + sx * w, y + sy * h),
                Point::new(x + ex * w, y + ey * h),
            )
            .with_stops(stops(s).as_slice());
            canvas.fill(t, Fill::NonZero, &Brush::Gradient(g), None, shape);
        }
        FillPaint::GradientRadial {
            transform,
            stops: s,
        } if !s.is_empty() => {
            let (cx, cy) = uv(transform.as_ref(), 0.5, 0.5);
            let centre = Point::new(x + cx * w, y + cy * h);
            let g = Gradient::new_two_point_radial(Point::ZERO, 0.0, Point::ZERO, 1.0)
                .with_stops(stops(s).as_slice());
            let bt = match transform {
                Some(m) => {
                    let ax = (m.a * w, m.c * h);
                    let ay = (m.b * w, m.d * h);
                    let rx = (ax.0.hypot(ax.1) * 0.5).max(0.5);
                    let ry = (ay.0.hypot(ay.1) * 0.5).max(0.5);
                    Affine::translate(centre.to_vec2())
                        * Affine::rotate(ax.1.atan2(ax.0))
                        * Affine::scale_non_uniform(rx, ry)
                }
                None => {
                    Affine::translate(centre.to_vec2()) * Affine::scale((w.min(h) * 0.5).max(0.5))
                }
            };
            canvas.fill(t, Fill::NonZero, &Brush::Gradient(g), Some(bt), shape);
        }
        FillPaint::GradientAngular {
            transform,
            stops: s,
        } if !s.is_empty() => {
            let (cx, cy) = uv(transform.as_ref(), 0.5, 0.5);
            let g = Gradient::new_sweep(
                Point::new(x + cx * w, y + cy * h),
                0.0,
                std::f32::consts::TAU,
            )
            .with_stops(stops(s).as_slice());
            canvas.fill(t, Fill::NonZero, &Brush::Gradient(g), None, shape);
        }
        // Images, and gradients without stops: nothing an embed draws.
        _ => {}
    }
}

/// Light a shape gives off: a soft, wider copy of it under it.
fn glow(canvas: &mut (impl Canvas + ?Sized), t: Affine, shape: &BezPath, effects: &[NodeEffect]) {
    for e in effects
        .iter()
        .filter(|e| matches!(e.kind, EffectKind::Glow) && e.color.a > 0.0)
    {
        let r = e.radius.clamp(0.5, 12.0);
        for (k, a) in [(1.0, 0.18), (0.5, 0.28)] {
            let c = Rgba {
                a: e.color.a * a,
                ..e.color.clone()
            };
            let style = Stroke::new(r * k * 2.0)
                .with_caps(kurbo::Cap::Round)
                .with_join(kurbo::Join::Round);
            canvas.stroke(t, &style, &Brush::Solid(color(&c)), shape);
        }
    }
}

fn closed_only(p: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut cur: Vec<kurbo::PathEl> = Vec::new();
    for el in p.elements() {
        if matches!(el, kurbo::PathEl::MoveTo(_)) {
            cur.clear();
        }
        cur.push(*el);
        if matches!(el, kurbo::PathEl::ClosePath) {
            out.extend(cur.drain(..));
        }
    }
    out
}

/// A laid-out line of text: its coloured pieces and its width.
type Line<'a> = (Vec<(BezPath, &'a Rgba)>, f64);

fn cased(text: &str, case: &TextCase) -> String {
    match case {
        TextCase::Upper | TextCase::SmallCaps | TextCase::SmallCapsForced => text.to_uppercase(),
        TextCase::Lower => text.to_lowercase(),
        _ => text.to_string(),
    }
}

static PATHS: LazyLock<Mutex<HashMap<String, Arc<BezPath>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// SVG path data, parsed once.
fn parsed(d: &str) -> Option<Arc<BezPath>> {
    if let Some(hit) = PATHS.lock().ok()?.get(d) {
        return Some(hit.clone());
    }
    let p = Arc::new(BezPath::from_svg(d).ok()?);
    if let Ok(mut m) = PATHS.lock() {
        // Plots are new path data every frame; don't hoard them.
        if m.len() > 2048 {
            m.clear();
        }
        m.insert(d.to_string(), p.clone());
    }
    Some(p)
}
