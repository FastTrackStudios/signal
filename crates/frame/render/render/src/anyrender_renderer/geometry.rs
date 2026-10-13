use super::cache::cached_bez_path;
use super::types::Rgba;
use kurbo::Shape;

pub(super) fn to_peniko(color: Rgba) -> peniko::Color {
    peniko::Color::from_rgba8(
        (color.r * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.g * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.b * 255.0).round().clamp(0.0, 255.0) as u8,
        (color.a * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

pub(super) fn compute_path_fit_transform(
    fill_paths: &[String],
    stroke_paths: &[String],
    target_x: f64,
    target_y: f64,
    target_width: f64,
    target_height: f64,
    normalize_to_origin: bool,
) -> kurbo::Affine {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut found = false;

    for path_data in fill_paths.iter().chain(stroke_paths.iter()) {
        let Some(path) = cached_bez_path(path_data) else {
            continue;
        };
        let bb = path.bounding_box();
        if !bb.x0.is_finite() || !bb.y0.is_finite() || !bb.x1.is_finite() || !bb.y1.is_finite() {
            continue;
        }
        min_x = min_x.min(bb.x0);
        min_y = min_y.min(bb.y0);
        max_x = max_x.max(bb.x1);
        max_y = max_y.max(bb.y1);
        found = true;
    }

    if !found {
        return kurbo::Affine::IDENTITY;
    }

    if normalize_to_origin {
        return kurbo::Affine::translate((-min_x, -min_y));
    }

    // Some bridge payloads already encode absolute scene-space path coordinates.
    // In that case we should cancel the node origin translation only, not refit.
    let src_w_raw = max_x - min_x;
    let src_h_raw = max_y - min_y;
    let approx_abs = (min_x - target_x).abs() <= 2.0
        && (min_y - target_y).abs() <= 2.0
        && (src_w_raw - target_width).abs() <= 2.0
        && (src_h_raw - target_height).abs() <= 2.0;
    if approx_abs {
        return kurbo::Affine::translate((-target_x, -target_y));
    }

    // If geometry is already local to the node bounds, preserve its authored
    // placement/size instead of force-scaling it to fill the full node box.
    let approx_local = min_x >= -2.0
        && min_y >= -2.0
        && max_x <= target_width + 2.0
        && max_y <= target_height + 2.0;
    if approx_local {
        return kurbo::Affine::IDENTITY;
    }

    // Some exports encode node-local geometry translated by node origin.
    let approx_local_offset = (min_x - target_x) >= -2.0
        && (min_y - target_y) >= -2.0
        && max_x <= target_x + target_width + 2.0
        && max_y <= target_y + target_height + 2.0;
    if approx_local_offset {
        return kurbo::Affine::translate((-target_x, -target_y));
    }

    let src_w = (max_x - min_x).max(0.0001);
    let src_h = (max_y - min_y).max(0.0001);
    let effective_w = if target_width > 0.0 {
        target_width
    } else {
        src_w
    };
    let effective_h = if target_height > 0.0 {
        target_height
    } else {
        src_h
    };
    let sx = effective_w / src_w;
    let sy = effective_h / src_h;
    kurbo::Affine::scale_non_uniform(sx, sy) * kurbo::Affine::translate((-min_x, -min_y))
}

pub(super) fn compute_svg_fit_transform(
    svg_text: &str,
    target_width: f64,
    target_height: f64,
) -> kurbo::Affine {
    let opt = anyrender_svg::usvg::Options::default();
    let Ok(tree) = anyrender_svg::usvg::Tree::from_str(svg_text, &opt) else {
        return kurbo::Affine::IDENTITY;
    };
    let size = tree.size();
    let mut src_x = 0.0;
    let mut src_y = 0.0;
    let mut src_w = f64::from(size.width());
    let mut src_h = f64::from(size.height());

    // Fallback to painted bounds when SVG viewport metadata is degenerate.
    if !(src_w.is_finite() && src_h.is_finite() && src_w > 0.0 && src_h > 0.0) {
        let root_bounds = tree.root().abs_layer_bounding_box();
        src_x = f64::from(root_bounds.x());
        src_y = f64::from(root_bounds.y());
        src_w = f64::from(root_bounds.width()).max(0.0001);
        src_h = f64::from(root_bounds.height()).max(0.0001);
    } else {
        src_w = src_w.max(0.0001);
        src_h = src_h.max(0.0001);
    }
    let effective_w = if target_width > 0.0 {
        target_width
    } else {
        src_w
    };
    let effective_h = if target_height > 0.0 {
        target_height
    } else {
        src_h
    };
    let sx = effective_w / src_w;
    let sy = effective_h / src_h;
    kurbo::Affine::scale_non_uniform(sx, sy) * kurbo::Affine::translate((-src_x, -src_y))
}

pub(super) fn extract_scale(transform: kurbo::Affine) -> (f64, f64) {
    let c = transform.as_coeffs();
    // Approximate axis scales from affine matrix columns.
    let sx = (c[0] * c[0] + c[1] * c[1]).sqrt();
    let sy = (c[2] * c[2] + c[3] * c[3]).sqrt();
    (sx, sy)
}
