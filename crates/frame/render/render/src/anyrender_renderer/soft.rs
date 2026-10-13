//! Soft effects that follow an outline: drop shadows, glows, inner shadows
//! and blurs of arbitrary shapes (paths, ellipses, glyphs).
//!
//! vello blurs only rounded rectangles, so these are rasterised on the CPU —
//! an antialiased coverage mask of the shape, blurred with a separable
//! Gaussian — and drawn as an image in the shape's own space. Masks are
//! rasterised at the target's resolution (coarser for wide blurs, which
//! don't need the detail) and cached, so a redraw of an unchanged shape
//! costs one image draw.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, LazyLock, Mutex};

use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape};
use peniko::{
    Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality, ImageSampler,
};

use super::types::Rgba;

/// Largest mask side, px; bigger shapes rasterise coarser.
const MAX_SIDE: f64 = 2048.0;
/// A blur wider than this (σ, mask px) rasterises coarser instead.
const MAX_SIGMA_PX: f64 = 6.0;
/// Cached masks kept before the cache is reset.
const CACHE_CAP: usize = 512;

/// Which soft effect to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Soft {
    /// The shape grown by `spread`, blurred, drawn offset by
    /// `(dx, dy)` — a drop shadow, a glow (no offset) or a layer blur of a
    /// solid shape (no offset, no spread).
    Outer { dx: f64, dy: f64, spread: f64 },
    /// Inside the shape: the colour, minus a blurred copy of the shape
    /// shrunk by `spread` and offset by `(dx, dy)`.
    Inner { dx: f64, dy: f64, spread: f64 },
}

/// A rasterised, blurred, coloured mask and where it goes.
struct Baked {
    brush: ImageBrush,
    /// Shape-space position of the mask's top-left corner.
    origin: Point,
    /// Mask px per shape unit.
    scale: f64,
}

/// Paint `soft` for the union of `shapes` (filled, non-zero), in `colour`,
/// blurred by `radius` (a CSS/Figma blur radius, σ = radius / 2).
/// `transform` maps shape space to the target.
pub(super) fn paint<S: super::target::PaintTarget>(
    scene: &mut S,
    transform: Affine,
    shapes: &[&BezPath],
    colour: &Rgba,
    radius: f64,
    soft: Soft,
) {
    if colour.a <= 0.0 || shapes.iter().all(|s| s.elements().is_empty()) {
        return;
    }
    let c = transform.as_coeffs();
    let device_scale = (c[0] * c[3] - c[1] * c[2]).abs().sqrt();
    if !device_scale.is_finite() || device_scale < 1e-6 {
        return;
    }
    let Some(baked) = bake(shapes, colour, radius.max(0.0), soft, device_scale) else {
        return;
    };
    let offset = match soft {
        Soft::Outer { dx, dy, .. } => (dx, dy),
        Soft::Inner { .. } => (0.0, 0.0),
    };
    let at = transform
        * Affine::translate((baked.origin.x + offset.0, baked.origin.y + offset.1))
        * Affine::scale(1.0 / baked.scale);
    scene.draw_image(&baked.brush, at);
}

fn bake(
    shapes: &[&BezPath],
    colour: &Rgba,
    radius: f64,
    soft: Soft,
    device_scale: f64,
) -> Option<Arc<Baked>> {
    static CACHE: LazyLock<Mutex<HashMap<u64, Arc<Baked>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let sigma = radius * 0.5;
    let key = {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for s in shapes {
            hash_path(s, &mut h);
            0xfeed_u32.hash(&mut h);
        }
        for v in [colour.r, colour.g, colour.b, colour.a, radius, device_scale] {
            v.to_bits().hash(&mut h);
        }
        match soft {
            Soft::Outer { spread, .. } => (0_u8, spread.to_bits(), 0_u64, 0_u64).hash(&mut h),
            Soft::Inner { dx, dy, spread } => {
                (1_u8, spread.to_bits(), dx.to_bits(), dy.to_bits()).hash(&mut h)
            }
        }
        h.finish()
    };
    if let Some(hit) = CACHE.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Some(hit);
    }

    let bounds = shapes
        .iter()
        .map(|s| s.bounding_box())
        .reduce(|a, b| a.union(b))?;
    if !(bounds.x0.is_finite()
        && bounds.y0.is_finite()
        && bounds.x1.is_finite()
        && bounds.y1.is_finite())
    {
        return None;
    }
    let (spread, margin) = match soft {
        Soft::Outer { spread, .. } => (spread, 3.0 * sigma + spread.max(0.0)),
        // The inner canvas covers the shape; the hole's blur reaches in
        // from up to 3σ (plus the offset) outside it.
        Soft::Inner { dx, dy, spread } => {
            (spread, 3.0 * sigma + dx.abs().max(dy.abs()) + spread.abs())
        }
    };
    let area = bounds.inflate(margin + 1.0, margin + 1.0);
    let mut scale = device_scale;
    if sigma * scale > MAX_SIGMA_PX {
        scale = MAX_SIGMA_PX / sigma;
    }
    let side = area.width().max(area.height()) * scale;
    if side > MAX_SIDE {
        scale *= MAX_SIDE / side;
    }
    let (w, h) = (
        (area.width() * scale).ceil().max(1.0) as usize,
        (area.height() * scale).ceil().max(1.0) as usize,
    );
    let origin = Point::new(area.x0, area.y0);
    let to_mask =
        |dx: f64, dy: f64| Affine::scale(scale) * Affine::translate((dx - origin.x, dy - origin.y));

    let alpha: Vec<f32> = match soft {
        Soft::Outer { .. } => {
            let mut m = grown(shapes, spread, to_mask(0.0, 0.0), w, h);
            blur(&mut m, w, h, sigma * scale);
            m
        }
        Soft::Inner { dx, dy, .. } => {
            let inside = grown(shapes, 0.0, to_mask(0.0, 0.0), w, h);
            let mut hole = grown(shapes, -spread, to_mask(dx, dy), w, h);
            blur(&mut hole, w, h, sigma * scale);
            inside
                .iter()
                .zip(&hole)
                .map(|(a, b)| a * (1.0 - b))
                .collect()
        }
    };

    let (r, g, b) = (to_u8(colour.r), to_u8(colour.g), to_u8(colour.b));
    let mut rgba = Vec::with_capacity(w * h * 4);
    for a in alpha {
        rgba.extend_from_slice(&[r, g, b, to_u8(f64::from(a) * colour.a)]);
    }
    let brush = ImageBrush {
        image: ImageData {
            data: Blob::new(Arc::new(rgba)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: w as u32,
            height: h as u32,
        },
        sampler: ImageSampler {
            x_extend: peniko::Extend::Pad,
            y_extend: peniko::Extend::Pad,
            quality: ImageQuality::Medium,
            alpha: 1.0,
        },
    };
    let baked = Arc::new(Baked {
        brush,
        origin,
        scale,
    });
    if let Ok(mut cache) = CACHE.lock() {
        if cache.len() >= CACHE_CAP {
            cache.clear();
        }
        cache.insert(key, Arc::clone(&baked));
    }
    Some(baked)
}

fn to_u8(v: f64) -> u8 {
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

fn hash_path(path: &BezPath, h: &mut impl Hasher) {
    for el in path.elements() {
        let pts: &[Point] = match el {
            PathEl::MoveTo(p) => {
                0_u8.hash(h);
                std::slice::from_ref(p)
            }
            PathEl::LineTo(p) => {
                1_u8.hash(h);
                std::slice::from_ref(p)
            }
            PathEl::QuadTo(a, b) => {
                2_u8.hash(h);
                a.x.to_bits().hash(h);
                a.y.to_bits().hash(h);
                std::slice::from_ref(b)
            }
            PathEl::CurveTo(a, b, c) => {
                3_u8.hash(h);
                for p in [a, b] {
                    p.x.to_bits().hash(h);
                    p.y.to_bits().hash(h);
                }
                std::slice::from_ref(c)
            }
            PathEl::ClosePath => {
                4_u8.hash(h);
                &[]
            }
        };
        for p in pts {
            p.x.to_bits().hash(h);
            p.y.to_bits().hash(h);
        }
    }
}

/// Coverage of the union of `shapes` grown by `spread` (shrunk when
/// negative), through `to_mask`, on a `w × h` mask.
fn grown(shapes: &[&BezPath], spread: f64, to_mask: Affine, w: usize, h: usize) -> Vec<f32> {
    let scale = to_mask.as_coeffs()[0].abs().max(1e-9);
    let tolerance = 0.1 / scale;
    let mut out = vec![0.0_f32; w * h];
    for shape in shapes {
        let mut m = coverage(shape, to_mask, w, h);
        if spread.abs() * scale > 1e-3 {
            let style = kurbo::Stroke::new(2.0 * spread.abs())
                .with_join(kurbo::Join::Round)
                .with_caps(kurbo::Cap::Round);
            let edge = kurbo::stroke(
                shape.elements().iter().copied(),
                &style,
                &kurbo::StrokeOpts::default(),
                tolerance,
            );
            let e = coverage(&edge, to_mask, w, h);
            if spread > 0.0 {
                m.iter_mut().zip(&e).for_each(|(a, b)| *a = a.max(*b));
            } else {
                m.iter_mut().zip(&e).for_each(|(a, b)| *a *= 1.0 - b);
            }
        }
        out.iter_mut().zip(&m).for_each(|(a, b)| *a = a.max(*b));
    }
    out
}

/// Antialiased non-zero coverage of `path` (through `to_mask`) on a
/// `w × h` mask: signed-area accumulation, one row at a time.
pub(super) fn coverage(path: &BezPath, to_mask: Affine, w: usize, h: usize) -> Vec<f32> {
    let stride = w + 2;
    let mut acc = vec![0.0_f32; stride * h];
    let mut start = Point::ZERO;
    let mut last = Point::ZERO;
    let mut line = |a: Point, b: Point| accumulate_line(&mut acc, stride, w, h, a, b);
    kurbo::flatten(
        path.elements().iter().map(|el| to_mask * *el),
        0.05,
        |el| match el {
            PathEl::MoveTo(p) => {
                if last != start {
                    line(last, start);
                }
                start = p;
                last = p;
            }
            PathEl::LineTo(p) => {
                line(last, p);
                last = p;
            }
            PathEl::ClosePath => {
                line(last, start);
                last = start;
            }
            _ => {}
        },
    );
    if last != start {
        line(last, start);
    }
    let mut out = vec![0.0_f32; w * h];
    for y in 0..h {
        let mut sum = 0.0_f32;
        for x in 0..w {
            sum += acc[y * stride + x];
            out[y * w + x] = sum.abs().min(1.0);
        }
    }
    out
}

/// Add the signed area a line contributes to each cell right of it
/// (font-rs's accumulation rasteriser, clipped to the mask).
fn accumulate_line(acc: &mut [f32], stride: usize, w: usize, h: usize, p0: Point, p1: Point) {
    let clamp_x = |x: f64| x.clamp(0.0, w as f64) as f32;
    let (x0, y0, x1, y1) = (clamp_x(p0.x), p0.y as f32, clamp_x(p1.x), p1.y as f32);
    if (y0 - y1).abs() <= f32::EPSILON {
        return;
    }
    let (dir, x0, y0, x1, y1) = if y0 < y1 {
        (1.0, x0, y0, x1, y1)
    } else {
        (-1.0, x1, y1, x0, y0)
    };
    let dxdy = (x1 - x0) / (y1 - y0);
    let top = y0.max(0.0);
    let mut x = x0 + (top - y0) * dxdy;
    let first = top as usize;
    let last = (y1.min(h as f32)).ceil().max(0.0) as usize;
    for y in first..last.min(h) {
        let row = y * stride;
        let dy = ((y + 1) as f32).min(y1) - (y as f32).max(y0);
        if dy <= 0.0 {
            continue;
        }
        let xnext = x + dxdy * dy;
        let d = dy * dir;
        let (xa, xb) = if x < xnext { (x, xnext) } else { (xnext, x) };
        let xa_floor = xa.floor();
        let xa_i = xa_floor as usize;
        let xb_ceil = xb.ceil();
        let xb_i = xb_ceil as usize;
        if xb_i <= xa_i + 1 {
            let xmf = 0.5 * (x + xnext) - xa_floor;
            acc[row + xa_i] += d - d * xmf;
            acc[row + xa_i + 1] += d * xmf;
        } else {
            let s = (xb - xa).recip();
            let xa_f = xa - xa_floor;
            let a0 = 0.5 * s * (1.0 - xa_f) * (1.0 - xa_f);
            let xb_f = xb - xb_ceil + 1.0;
            let am = 0.5 * s * xb_f * xb_f;
            acc[row + xa_i] += d * a0;
            if xb_i == xa_i + 2 {
                acc[row + xa_i + 1] += d * (1.0 - a0 - am);
            } else {
                let a1 = s * (1.5 - xa_f);
                acc[row + xa_i + 1] += d * (a1 - a0);
                for xi in xa_i + 2..xb_i - 1 {
                    acc[row + xi] += d * s;
                }
                let a2 = a1 + (xb_i - xa_i - 3) as f32 * s;
                acc[row + xb_i - 1] += d * (1.0 - a2 - am);
            }
            acc[row + xb_i] += d * am;
        }
        x = xnext;
    }
}

/// Separable Gaussian blur (σ in mask px), zero outside the mask.
fn blur(m: &mut [f32], w: usize, h: usize, sigma: f64) {
    if sigma < 0.2 {
        return;
    }
    let r = (3.0 * sigma).ceil() as usize;
    let kernel: Vec<f32> = {
        let k: Vec<f64> = (0..=2 * r)
            .map(|i| (-0.5 * ((i as f64 - r as f64) / sigma).powi(2)).exp())
            .collect();
        let sum: f64 = k.iter().sum();
        k.iter().map(|v| (v / sum) as f32).collect()
    };
    let mut tmp = vec![0.0_f32; w * h];
    // Rows.
    for y in 0..h {
        let src = &m[y * w..(y + 1) * w];
        for x in 0..w {
            let lo = x.saturating_sub(r);
            let hi = (x + r).min(w - 1);
            let mut s = 0.0;
            for (sx, v) in src.iter().enumerate().take(hi + 1).skip(lo) {
                s += v * kernel[sx + r - x];
            }
            tmp[y * w + x] = s;
        }
    }
    // Columns.
    for x in 0..w {
        for y in 0..h {
            let lo = y.saturating_sub(r);
            let hi = (y + r).min(h - 1);
            let mut s = 0.0;
            for sy in lo..=hi {
                s += tmp[sy * w + x] * kernel[sy + r - y];
            }
            m[y * w + x] = s;
        }
    }
}

/// The rounded rectangle `rect` with per-corner `radii` as a path.
pub(super) fn rounded_rect_path(rect: Rect, radii: &document_proto::CornerRadii) -> BezPath {
    kurbo::RoundedRect::from_rect(
        rect,
        kurbo::RoundedRectRadii::new(
            radii.top_left,
            radii.top_right,
            radii.bottom_right,
            radii.bottom_left,
        ),
    )
    .to_path(0.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64) -> BezPath {
        Rect::new(x, y, x + s, y + s).to_path(0.1)
    }

    #[test]
    fn coverage_fills_the_inside_and_antialiases_the_edge() {
        let m = coverage(&square(2.0, 2.5, 4.0), Affine::IDENTITY, 10, 10);
        assert!((m[4 * 10 + 4] - 1.0).abs() < 1e-5, "inside");
        assert!(m[10 + 4] < 1e-5, "outside");
        assert!(
            (m[2 * 10 + 4] - 0.5).abs() < 1e-4,
            "half-covered row: {}",
            m[2 * 10 + 4]
        );
        let total: f32 = m.iter().sum();
        assert!((total - 16.0).abs() < 1e-3, "area {total}");
    }

    #[test]
    fn opposite_windings_cut_holes() {
        let mut ring = kurbo::Circle::new((10.0, 10.0), 8.0).to_path(0.01);
        let hole = kurbo::Circle::new((10.0, 10.0), 4.0)
            .to_path(0.01)
            .reverse_subpaths();
        ring.extend(hole);
        let m = coverage(&ring, Affine::IDENTITY, 20, 20);
        assert!(m[10 * 20 + 10] < 1e-4, "the hole is empty");
        assert!(m[10 * 20 + 3] > 0.99, "the ring is filled");
    }

    #[test]
    fn blur_keeps_mass_and_spreads() {
        let mut m = coverage(&square(20.0, 20.0, 10.0), Affine::IDENTITY, 50, 50);
        let before: f32 = m.iter().sum();
        blur(&mut m, 50, 50, 3.0);
        let after: f32 = m.iter().sum();
        assert!((before - after).abs() < 0.5, "{before} → {after}");
        assert!(m[25 * 50 + 17] > 0.05, "spreads outside the edge");
        assert!(m[25 * 50 + 25] < 1.0, "softens the middle");
    }

    #[test]
    fn spread_grows_and_shrinks() {
        let s = square(10.0, 10.0, 10.0);
        let grown_m = grown(&[&s], 2.0, Affine::IDENTITY, 30, 30);
        let shrunk = grown(&[&s], -2.0, Affine::IDENTITY, 30, 30);
        assert!(grown_m[15 * 30 + 9] > 0.9, "grown past the edge");
        assert!(shrunk[15 * 30 + 11] < 0.1, "shrunk inside the edge");
        assert!(shrunk[15 * 30 + 15] > 0.9);
    }
}
