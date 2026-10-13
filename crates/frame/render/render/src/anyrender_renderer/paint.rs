use super::cache::{cached_bez_path, cached_svg_decode, cached_svg_fit_transform};
use super::geometry::{compute_path_fit_transform, extract_scale, to_peniko};
use super::scene_collect::first_color;
use super::soft::{self, Soft};
use super::subpaths::split_open_closed;
use super::target::{GlyphRun, PaintTarget};
use super::types::*;
use render_fonts::FontBook;

const TEXT_ANCHOR_SHIFT_X: f64 = 1.0;
const TEXT_ANCHOR_SHIFT_Y: f64 = 1.0;

/// Paint a frame document (`root`'s subtree) into a vello scene. Text is
/// set from `fonts` (the host's book) plus the fonts the document carries;
/// without a book, text is drawn as baseline bars.
pub fn paint_scene_into<S: PaintTarget>(
    scene: &mut S,
    doc: &document_proto::Scene,
    root: document_proto::NodeId,
    scene_transform: kurbo::Affine,
    fonts: Option<&FontBook>,
) {
    let primitives = super::scene_collect::build_scene_primitives(doc, root);
    let book = fonts.map(|f| crate::fonts::book_for_scene(f, doc));
    paint_primitives_into_scene_with(scene, &primitives, scene_transform, book.as_deref());
}

/// Draw one invisible late-bound resource (a transparent gradient on a pixel
/// far outside any target) so vello never resolves this scene on its
/// solid-paths-only fast path.
///
/// That path (vello 0.10's `Resolver::resolve` for a scene without images,
/// gradients or glyphs) reports an empty image atlas, so the renderer drops
/// its atlas texture for a 1×1 one — while the resolver still counts every
/// image uploaded before as resident. The next scene with those images
/// gets a fresh, blank atlas and uploads nothing: its textures draw as
/// nothing. The effects stage renders several scenes a frame (segments,
/// bloom passes of plain shapes), so from the second frame on every texture
/// vanished; a reused `Offscreen` did the same after any plain scene.
pub(super) fn keep_image_atlas(scene: &mut vello::Scene) {
    let clear = peniko::Color::TRANSPARENT;
    let gradient =
        peniko::Gradient::new_linear((0.0, 0.0), (1.0, 0.0)).with_stops([clear, clear].as_slice());
    let far = kurbo::Rect::new(-1.0e6, -1.0e6, -1.0e6 + 1.0, -1.0e6 + 1.0);
    scene.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        &gradient,
        None,
        &far,
    );
}

/// A face's bytes as a vello font, cached by face so every frame (and
/// vello's glyph cache) sees the same blob.
fn font_data(face: &render_fonts::Face) -> peniko::FontData {
    use std::collections::HashMap;
    use std::sync::{Arc, LazyLock, Mutex};
    static CACHE: LazyLock<Mutex<HashMap<u64, peniko::FontData>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    if let Some(hit) = CACHE.lock().ok().and_then(|c| c.get(&face.id()).cloned()) {
        return hit;
    }
    let data = peniko::FontData::new(
        peniko::Blob::new(Arc::new(face.font_bytes().clone())),
        face.index,
    );
    if let Ok(mut c) = CACHE.lock() {
        c.insert(face.id(), data.clone());
    }
    data
}

pub fn paint_primitives_into_scene_with<S: PaintTarget>(
    scene: &mut S,
    primitives: &[PaintPrimitive],
    root_transform: kurbo::Affine,
    fonts: Option<&FontBook>,
) {
    use base64::Engine;
    use kurbo::{BezPath, Rect, RoundedRect, Stroke};
    use peniko::{Blob, Fill, ImageBrush, ImageData, ImageSampler};
    use std::sync::Arc;
    use vello::Glyph;

    scene.begin();

    // Mask buffering state: collect primitives between MaskDefStart..MaskDefEnd
    // so they can be replayed as a luminance mask when MaskApplyEnd is reached.
    let mut capturing_mask = false;
    let mut mask_def_primitives: Vec<&PaintPrimitive> = Vec::new();
    let mut mask_bounds: Option<Rect> = None;
    // Inherited group transforms (`GroupStart`), outermost first; each entry
    // is already composed with the ones before it.
    let mut group_stack: Vec<kurbo::Affine> = Vec::new();

    for primitive in primitives {
        // If we're capturing mask definition primitives, buffer them instead of rendering
        if capturing_mask {
            match primitive {
                PaintPrimitive::MaskDefEnd { .. } => {
                    capturing_mask = false;
                }
                _ => {
                    mask_def_primitives.push(primitive);
                }
            }
            continue;
        }

        let scene_transform = group_stack.last().copied().unwrap_or(root_transform);
        match primitive {
            PaintPrimitive::GroupStart { transform, .. } => {
                group_stack.push(scene_transform * to_kurbo_affine(transform));
            }
            PaintPrimitive::GroupEnd { .. } => {
                group_stack.pop();
            }
            PaintPrimitive::LayerStart {
                x,
                y,
                width,
                height,
                blend,
                opacity,
                ..
            } => {
                let (x, y, width, height, opacity) = (*x, *y, *width, *height, *opacity);
                let clip_rect = Rect::new(x, y, x + width, y + height);
                scene.push_layer(
                    Fill::NonZero,
                    peniko::BlendMode {
                        mix: to_peniko_mix(&blend),
                        compose: peniko::Compose::SrcOver,
                    },
                    opacity as f32,
                    scene_transform,
                    &clip_rect,
                );
            }
            PaintPrimitive::LayerEnd { .. } => {
                scene.pop_layer();
            }
            PaintPrimitive::ClipStart {
                x,
                y,
                width,
                height,
                corner_radii,
                ..
            } => {
                let (x, y, width, height) = (*x, *y, *width, *height);
                let rect = Rect::new(x, y, x + width, y + height);
                if !corner_radii.is_zero() {
                    let rr = RoundedRect::from_rect(
                        rect,
                        kurbo::RoundedRectRadii::new(
                            corner_radii.top_left,
                            corner_radii.top_right,
                            corner_radii.bottom_right,
                            corner_radii.bottom_left,
                        ),
                    );
                    scene.push_clip_layer(Fill::NonZero, scene_transform, &rr);
                } else {
                    scene.push_clip_layer(Fill::NonZero, scene_transform, &rect);
                }
            }
            PaintPrimitive::ClipEnd { .. } => {
                scene.pop_layer();
            }
            PaintPrimitive::MaskDefStart {
                x,
                y,
                width,
                height,
                ..
            } => {
                capturing_mask = true;
                mask_def_primitives.clear();
                mask_bounds = Some(Rect::new(*x, *y, *x + *width, *y + *height));
            }
            PaintPrimitive::MaskDefEnd { .. } => {
                // Handled above in the capturing_mask branch
            }
            PaintPrimitive::MaskApplyStart {
                x,
                y,
                width,
                height,
                ..
            } => {
                let rect = Rect::new(*x, *y, *x + *width, *y + *height);
                // Isolation layer — everything inside gets composited then masked
                scene.push_layer(
                    Fill::NonZero,
                    peniko::BlendMode::default(),
                    1.0,
                    scene_transform,
                    &rect,
                );
            }
            PaintPrimitive::MaskApplyEnd { .. } => {
                // Apply the buffered mask primitives as a luminance mask
                let clip = mask_bounds.unwrap_or(Rect::new(0.0, 0.0, 1e6, 1e6));
                scene.push_luminance_mask_layer(Fill::NonZero, 1.0, scene_transform, &clip);
                // Replay mask definition primitives
                if !mask_def_primitives.is_empty() {
                    render_mask_primitives(scene, &mask_def_primitives, scene_transform);
                }
                scene.pop_layer(); // luminance mask
                scene.pop_layer(); // isolation
                mask_def_primitives.clear();
                mask_bounds = None;
            }
            PaintPrimitive::Rect {
                x,
                y,
                width,
                height,
                fills,
                stroke,
                corner_radii,
                effects,
                blend: _,
                rotation,
                authored,
                ..
            } => {
                let (x, y, width, height, rotation) = (*x, *y, *width, *height, *rotation);
                let corner_radii = *corner_radii;
                // Authored rects with per-corner radii shadow their real
                // outline (vello blurs uniform rounded rects only).
                let soft_outline = (*authored && !radii_uniform(&corner_radii)).then(|| {
                    soft::rounded_rect_path(Rect::new(0.0, 0.0, width, height), &corner_radii)
                });
                let (node_transform, lx, ly) = if rotation.abs() > 1e-6 {
                    let t = scene_transform
                        * kurbo::Affine::translate((x, y))
                        * kurbo::Affine::rotate(rotation);
                    (t, 0.0, 0.0)
                } else {
                    (scene_transform, x, y)
                };
                let rect = Rect::new(lx, ly, lx + width, ly + height);
                let uniform_corner = corner_radii.top_left;
                let blur_color_hint = fills
                    .first()
                    .and_then(|(f, _)| first_color(f))
                    .or_else(|| stroke.as_ref().map(|s| s.color.clone()));
                let has_layer_blur = effects
                    .iter()
                    .any(|e| matches!(e.kind, EffectKind::LayerBlur));
                for effect in effects {
                    if matches!(effect.kind, EffectKind::LayerBlur) {
                        paint_blurred_rect_content(
                            scene,
                            node_transform,
                            lx,
                            ly,
                            width,
                            height,
                            uniform_corner,
                            fills.first().map(|(f, _)| f),
                            stroke.as_ref(),
                            effect,
                        );
                    }
                }
                for effect in effects {
                    if matches!(effect.kind, EffectKind::DropShadow | EffectKind::Glow) {
                        if let Some(outline) = &soft_outline {
                            let soft = Soft::Outer {
                                dx: effect.offset_x,
                                dy: effect.offset_y,
                                spread: effect.spread,
                            };
                            let at = node_transform * kurbo::Affine::translate((lx, ly));
                            soft::paint(scene, at, &[outline], &effect.color, effect.radius, soft);
                            continue;
                        }
                        paint_effect(
                            scene,
                            node_transform,
                            &rect,
                            uniform_corner,
                            effect,
                            blur_color_hint.as_ref(),
                        );
                    }
                }
                // When LayerBlur is active, the blurred rect IS the content —
                // don't draw sharp fills/strokes on top.
                if !has_layer_blur {
                    if !corner_radii.is_zero() {
                        let radii = kurbo::RoundedRectRadii::new(
                            corner_radii.top_left,
                            corner_radii.top_right,
                            corner_radii.bottom_right,
                            corner_radii.bottom_left,
                        );
                        let stroke_rect = stroke_aligned_rect(rect, stroke.as_ref());
                        let stroke_radius =
                            stroke_aligned_corner_radius(uniform_corner, stroke.as_ref());
                        let rr = RoundedRect::from_rect(rect, radii);
                        for (fill, fill_blend) in fills {
                            let need_blend_layer = *fill_blend != BlendMix::Normal;
                            if need_blend_layer {
                                scene.push_layer(
                                    Fill::NonZero,
                                    peniko::BlendMode {
                                        mix: to_peniko_mix(fill_blend),
                                        compose: peniko::Compose::SrcOver,
                                    },
                                    1.0,
                                    node_transform,
                                    &rect,
                                );
                            }
                            fill_shape(scene, node_transform, &rr, lx, ly, width, height, fill);
                            if need_blend_layer {
                                scene.pop_layer();
                            }
                        }
                        if let Some(stroke) = stroke.as_ref() {
                            let (brush, brush_transform) =
                                stroke_brush(stroke, lx, ly, width, height);
                            if stroke.width > 0.0
                                && *authored
                                && let Some(ring) = aligned_stroke_ring(rect, &corner_radii, stroke)
                            {
                                scene.fill(
                                    Fill::EvenOdd,
                                    node_transform,
                                    &brush,
                                    brush_transform,
                                    &ring,
                                );
                            } else if stroke.width > 0.0 {
                                let stroke_style = to_kurbo_stroke(stroke, 1.0);
                                let stroke_rr = RoundedRect::from_rect(stroke_rect, stroke_radius);
                                scene.stroke(
                                    &stroke_style,
                                    node_transform,
                                    &brush,
                                    brush_transform,
                                    &stroke_rr,
                                );
                            }
                        }
                    } else {
                        for (fill, fill_blend) in fills {
                            let need_blend_layer = *fill_blend != BlendMix::Normal;
                            if need_blend_layer {
                                scene.push_layer(
                                    Fill::NonZero,
                                    peniko::BlendMode {
                                        mix: to_peniko_mix(fill_blend),
                                        compose: peniko::Compose::SrcOver,
                                    },
                                    1.0,
                                    node_transform,
                                    &rect,
                                );
                            }
                            fill_shape(scene, node_transform, &rect, lx, ly, width, height, fill);
                            if need_blend_layer {
                                scene.pop_layer();
                            }
                        }
                        if let Some(stroke) = stroke.as_ref() {
                            let (brush, brush_transform) =
                                stroke_brush(stroke, lx, ly, width, height);
                            if stroke.width > 0.0
                                && *authored
                                && let Some(ring) = aligned_stroke_ring(rect, &corner_radii, stroke)
                            {
                                scene.fill(
                                    Fill::EvenOdd,
                                    node_transform,
                                    &brush,
                                    brush_transform,
                                    &ring,
                                );
                            } else if stroke.width > 0.0 {
                                let stroke_style = to_kurbo_stroke(stroke, 1.0);
                                let stroke_rect = stroke_aligned_rect(rect, Some(stroke));
                                scene.stroke(
                                    &stroke_style,
                                    node_transform,
                                    &brush,
                                    brush_transform,
                                    &stroke_rect,
                                );
                            }
                        }
                    }
                } // !has_layer_blur
                for effect in effects {
                    if matches!(
                        effect.kind,
                        EffectKind::InnerShadow | EffectKind::BackgroundBlur
                    ) {
                        if let (Some(outline), EffectKind::InnerShadow) =
                            (&soft_outline, &effect.kind)
                        {
                            let soft = Soft::Inner {
                                dx: effect.offset_x,
                                dy: effect.offset_y,
                                spread: effect.spread,
                            };
                            let at = node_transform * kurbo::Affine::translate((lx, ly));
                            soft::paint(scene, at, &[outline], &effect.color, effect.radius, soft);
                            continue;
                        }
                        paint_effect(
                            scene,
                            node_transform,
                            &rect,
                            uniform_corner,
                            effect,
                            blur_color_hint.as_ref(),
                        );
                    }
                }
            }
            PaintPrimitive::Path {
                x: path_x,
                y: path_y,
                width,
                height,
                normalize_to_origin,
                text_outline,
                fill_paths,
                stroke_paths,
                svg_base64,
                fills,
                stroke,
                effects,
                blend: _,
                rotation,
                authored,
                ..
            } => {
                let mut path_x = *path_x;
                let mut path_y = *path_y;
                let (width, height, rotation) = (*width, *height, *rotation);
                if *authored
                    && !*text_outline
                    && (!fill_paths.is_empty() || !stroke_paths.is_empty())
                {
                    let base_transform = if rotation.abs() > 1e-6 {
                        scene_transform
                            * kurbo::Affine::translate((path_x, path_y))
                            * kurbo::Affine::rotate(rotation)
                    } else {
                        scene_transform * kurbo::Affine::translate((path_x, path_y))
                    };
                    paint_authored_path(
                        scene,
                        base_transform,
                        width,
                        height,
                        fill_paths,
                        stroke_paths,
                        fills,
                        stroke.as_ref(),
                        effects,
                    );
                    continue;
                }
                if *text_outline {
                    path_x += TEXT_ANCHOR_SHIFT_X;
                    path_y += TEXT_ANCHOR_SHIFT_Y;
                }
                let (base_transform, lx, ly) = if rotation.abs() > 1e-6 {
                    let t = scene_transform
                        * kurbo::Affine::translate((path_x, path_y))
                        * kurbo::Affine::rotate(rotation);
                    (t, 0.0, 0.0)
                } else {
                    (scene_transform, path_x, path_y)
                };
                let mut had_any_path = false;
                let has_native_geometry = !fill_paths.is_empty() || !stroke_paths.is_empty();
                let bounds_rect = Rect::new(lx, ly, lx + width, ly + height);
                let blur_color_hint = fills
                    .first()
                    .and_then(|(f, _)| first_color(f))
                    .or_else(|| stroke.as_ref().map(|s| s.color.clone()));
                let path_origin_transform = base_transform * kurbo::Affine::translate((lx, ly));
                let path_fit = compute_path_fit_transform(
                    fill_paths,
                    stroke_paths,
                    lx,
                    ly,
                    width,
                    height,
                    *normalize_to_origin,
                );
                let path_transform = path_origin_transform * path_fit;
                let (sx, sy) = extract_scale(path_fit);
                let stroke_scale = ((sx.abs() + sy.abs()) * 0.5).max(0.01);
                let decoded_svg_text: Option<std::sync::Arc<String>> = if has_native_geometry {
                    None
                } else {
                    svg_base64.as_ref().and_then(|b64| cached_svg_decode(b64))
                };
                let has_layer_blur = effects
                    .iter()
                    .any(|e| matches!(e.kind, EffectKind::LayerBlur));

                if let (Some(svg_text), Some(layer_blur)) = (
                    decoded_svg_text.as_deref(),
                    effects
                        .iter()
                        .find(|e| matches!(e.kind, EffectKind::LayerBlur)),
                ) {
                    paint_blurred_svg_content(
                        scene,
                        base_transform,
                        svg_text,
                        lx,
                        ly,
                        width,
                        height,
                        layer_blur,
                    );
                }
                for effect in effects {
                    if matches!(effect.kind, EffectKind::LayerBlur) {
                        paint_blurred_path_content(
                            scene,
                            path_transform,
                            fill_paths,
                            stroke_paths,
                            fills.first().map(|(f, _)| f),
                            stroke.as_ref(),
                            stroke_scale,
                            lx,
                            ly,
                            width,
                            height,
                            effect,
                        );
                    }
                }
                for effect in effects {
                    // Figma imports (and snapshot-only vectors) keep box
                    // shadows, as parity expects; a glow is a rounded box.
                    // Authored vectors follow their outline (above).
                    let corner = match effect.kind {
                        EffectKind::DropShadow => 0.0,
                        EffectKind::Glow => width.min(height) * 0.5,
                        _ => continue,
                    };
                    paint_effect(
                        scene,
                        base_transform,
                        &bounds_rect,
                        corner,
                        effect,
                        blur_color_hint.as_ref(),
                    );
                }
                if !has_layer_blur {
                    if let Some(svg_text) = decoded_svg_text.as_deref() {
                        let svg_fit = cached_svg_fit_transform(svg_text, width, height);
                        let svg_transform = path_origin_transform * svg_fit;
                        if scene.draw_svg(svg_text, svg_transform) {
                            had_any_path = true;
                        }
                    }
                } // !has_layer_blur for SVG

                if !had_any_path && !has_layer_blur {
                    for (fill_paint, fill_blend) in fills {
                        let need_blend_layer = *fill_blend != BlendMix::Normal;
                        if need_blend_layer {
                            scene.push_layer(
                                Fill::NonZero,
                                peniko::BlendMode {
                                    mix: to_peniko_mix(fill_blend),
                                    compose: peniko::Compose::SrcOver,
                                },
                                1.0,
                                base_transform,
                                &bounds_rect,
                            );
                        }
                        for path_data in fill_paths {
                            if let Some(path) = cached_bez_path(path_data) {
                                fill_path_shape(
                                    scene,
                                    path_transform,
                                    &*path,
                                    lx,
                                    ly,
                                    width,
                                    height,
                                    fill_paint,
                                );
                                had_any_path = true;
                            }
                        }
                        if need_blend_layer {
                            scene.pop_layer();
                        }
                    }

                    if let Some(stroke) = stroke.as_ref() {
                        if stroke.width > 0.0 {
                            // Path space is node-local, as for the fills.
                            let (brush, brush_transform) =
                                stroke_brush(stroke, 0.0, 0.0, width, height);
                            for path_data in stroke_paths {
                                if let Some(path) = cached_bez_path(path_data) {
                                    // Figma strokeGeometry is already the outlined stroke shape.
                                    // Fill it directly to avoid double-stroking artifacts.
                                    scene.fill(
                                        peniko::Fill::NonZero,
                                        path_transform,
                                        &brush,
                                        brush_transform,
                                        &*path,
                                    );
                                    had_any_path = true;
                                }
                            }
                        }
                    }
                }

                if !had_any_path {
                    let rect = Rect::new(lx, ly, lx + width, ly + height);
                    for (fill, fill_blend) in fills {
                        let need_blend_layer = *fill_blend != BlendMix::Normal;
                        if need_blend_layer {
                            scene.push_layer(
                                Fill::NonZero,
                                peniko::BlendMode {
                                    mix: to_peniko_mix(fill_blend),
                                    compose: peniko::Compose::SrcOver,
                                },
                                1.0,
                                base_transform,
                                &rect,
                            );
                        }
                        fill_shape(scene, base_transform, &rect, lx, ly, width, height, fill);
                        if need_blend_layer {
                            scene.pop_layer();
                        }
                    }
                    if let Some(stroke) = stroke.as_ref() {
                        if stroke.width > 0.0 {
                            let stroke_style = to_kurbo_stroke(stroke, 1.0);
                            let (brush, brush_transform) =
                                stroke_brush(stroke, lx, ly, width, height);
                            scene.stroke(
                                &stroke_style,
                                base_transform,
                                &brush,
                                brush_transform,
                                &rect,
                            );
                        }
                    }
                }
                for effect in effects {
                    if matches!(
                        effect.kind,
                        EffectKind::InnerShadow | EffectKind::BackgroundBlur
                    ) {
                        paint_effect(
                            scene,
                            base_transform,
                            &bounds_rect,
                            0.0,
                            effect,
                            blur_color_hint.as_ref(),
                        );
                    }
                }
            }
            PaintPrimitive::Text {
                x,
                y,
                text,
                color,
                font_size,
                line_height,
                letter_spacing,
                text_case,
                font,
                style_runs,
                blend: _,
                width: box_width,
                align,
                effects,
                ..
            } => {
                let (x, y, font_size, letter_spacing) = (
                    *x + TEXT_ANCHOR_SHIFT_X,
                    *y + TEXT_ANCHOR_SHIFT_Y,
                    *font_size,
                    *letter_spacing,
                );
                let text = apply_text_case(text, text_case);
                let line_height = line_height.unwrap_or(font_size * 1.2).max(1.0);
                let max_line_chars = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
                let approx_width =
                    (max_line_chars as f64 * (font_size * 0.6 + letter_spacing)).max(font_size);
                let line_count = text.lines().count().max(1) as f64;
                let _bounds_rect = Rect::new(x, y, x + approx_width, y + line_count * line_height);
                if let Some(book) = fonts {
                    let runs = if style_runs.is_empty() {
                        vec![TextStyleRun {
                            text: text.clone(),
                            color: Some(color.clone()),
                            font_size: Some(font_size),
                            letter_spacing: Some(letter_spacing),
                            font_style: None,
                            font_weight: None,
                            font_family: None,
                            font_width: None,
                        }]
                    } else {
                        style_runs
                            .iter()
                            .cloned()
                            .map(|mut run| {
                                run.text = apply_text_case(&run.text, text_case);
                                run
                            })
                            .collect::<Vec<_>>()
                    };
                    // Each run's faces: its own settings over the node's.
                    let chains: Vec<render_fonts::Chain> = runs
                        .iter()
                        .map(|run| book.chain(&run.font().over(font).query()))
                        .collect();

                    // Measure each line first so it can be aligned in its box.
                    let mut line_widths: Vec<f64> = vec![0.0];
                    for (run, chain) in runs.iter().zip(&chains) {
                        let size = run.font_size.unwrap_or(font_size).max(1.0);
                        let spacing = run.letter_spacing.unwrap_or(letter_spacing);
                        for ch in run.text.chars() {
                            if ch == '\n' {
                                line_widths.push(0.0);
                                continue;
                            }
                            let advance = chain.pick(ch).map_or(0.0, |(f, g)| f.advance(g, size));
                            if let Some(w) = line_widths.last_mut() {
                                *w += advance + spacing;
                            }
                        }
                    }
                    let line_start = |line: usize| -> f64 {
                        let w = line_widths.get(line).copied().unwrap_or(0.0);
                        match align {
                            TextAlignment::Left => 0.0,
                            TextAlignment::Center => ((*box_width - w) * 0.5).max(0.0),
                            TextAlignment::Right => (*box_width - w).max(0.0),
                        }
                    };
                    let mut line_idx: usize = 0;
                    let mut pen_x: f64 = line_start(0);
                    let mut placed: Vec<Placed<'_>> = Vec::new();
                    for (run, chain) in runs.iter().zip(&chains) {
                        let run_size = run.font_size.unwrap_or(font_size).max(1.0);
                        let run_letter_spacing =
                            run.letter_spacing.unwrap_or(letter_spacing) as f32;
                        let run_color = run.color.clone().unwrap_or_else(|| color.clone());
                        let ascent = chain
                            .primary()
                            .map_or(run_size * 0.8, |f| f.ascent(run_size));
                        for ch in run.text.chars() {
                            if ch == '\n' {
                                line_idx += 1;
                                pen_x = line_start(line_idx);
                                continue;
                            }
                            let Some((face, gid)) = chain.pick(ch) else {
                                continue;
                            };
                            let advance = face.advance(gid, run_size);
                            let local = kurbo::Affine::translate((
                                x + pen_x,
                                y + ascent + (line_idx as f64 * line_height),
                            ));
                            placed.push(Placed {
                                face,
                                gid,
                                size: run_size,
                                color: run_color.clone(),
                                local,
                            });
                            pen_x += advance + f64::from(run_letter_spacing);
                        }
                    }
                    // Shadows and glows follow the glyph outlines.
                    let outline = (!effects.is_empty()).then(|| glyph_outlines(&placed));
                    for effect in effects {
                        if let (Some(o), EffectKind::DropShadow | EffectKind::Glow) =
                            (&outline, &effect.kind)
                        {
                            let soft = Soft::Outer {
                                dx: effect.offset_x,
                                dy: effect.offset_y,
                                spread: effect.spread,
                            };
                            soft::paint(
                                scene,
                                scene_transform,
                                &[o],
                                &effect.color,
                                effect.radius,
                                soft,
                            );
                        }
                    }
                    for g in &placed {
                        let face = g.face;
                        let grow = face.embolden(g.size);
                        let dx = grow as f32;
                        let font = font_data(&face.face);
                        let brush = to_peniko(g.color.clone());
                        scene.draw_glyph_run(&GlyphRun {
                            font: &font,
                            font_size: g.size as f32,
                            normalized_coords: &face.coords,
                            brush: (&brush).into(),
                            brush_alpha: 1.0,
                            transform: scene_transform * g.local,
                            // Glyph space is y-up: a synthetic italic leans the
                            // tops right.
                            glyph_transform: face.synthetic_italic.then(|| {
                                kurbo::Affine::new([
                                    1.0,
                                    0.0,
                                    render_fonts::SYNTHETIC_SKEW,
                                    1.0,
                                    0.0,
                                    0.0,
                                ])
                            }),
                            embolden: if grow > 0.0 {
                                kurbo::Vec2::new(grow, grow * 0.5)
                            } else {
                                kurbo::Vec2::ZERO
                            },
                            glyphs: &[Glyph {
                                id: g.gid.to_u32(),
                                x: dx,
                                y: 0.0,
                            }],
                        });
                    }
                    for effect in effects {
                        if let (Some(o), EffectKind::InnerShadow) = (&outline, &effect.kind) {
                            let soft = Soft::Inner {
                                dx: effect.offset_x,
                                dy: effect.offset_y,
                                spread: effect.spread,
                            };
                            soft::paint(
                                scene,
                                scene_transform,
                                &[o],
                                &effect.color,
                                effect.radius,
                                soft,
                            );
                        }
                    }
                } else {
                    // fallback to a baseline block when no text font is configured
                    for (line_idx, line_text) in text.lines().enumerate() {
                        let width = (line_text.chars().count() as f64
                            * (font_size * 0.5 + letter_spacing))
                            .max(font_size);
                        let y_line = y + (line_idx as f64 * line_height);
                        let line = Rect::new(
                            x,
                            y_line + font_size * 0.85,
                            x + width,
                            y_line + font_size * 0.95,
                        );
                        scene.fill(
                            Fill::NonZero,
                            scene_transform,
                            to_peniko(color.clone()),
                            None,
                            &line,
                        );
                    }
                }
            }
        }
    }

    /// A glyph placed in text space: which face draws it, where its pen
    /// starts on the baseline.
    struct Placed<'c> {
        face: &'c render_fonts::Resolved,
        gid: skrifa::GlyphId,
        size: f64,
        color: Rgba,
        local: kurbo::Affine,
    }

    /// The placed glyphs' outlines in text space, as drawn (instanced,
    /// slanted, emboldened).
    fn glyph_outlines(placed: &[Placed<'_>]) -> BezPath {
        let mut out = BezPath::new();
        for g in placed {
            if let Some(p) = g.face.outline(g.gid, g.size) {
                out.extend((g.local * p).elements().iter().copied());
            }
        }
        out
    }

    fn radii_uniform(r: &document_proto::CornerRadii) -> bool {
        r.top_left == r.top_right && r.top_left == r.bottom_right && r.top_left == r.bottom_left
    }

    /// An inside or outside stroke of a (rounded) rect as the exact ring
    /// between its two edges, filled even-odd. Stroking the inset/outset
    /// outline instead lets vello's GPU stroker put an edge a hair above a
    /// tile row, which fills that whole 16 px row with the stroke colour.
    /// `None` for centred and dashed strokes (stroked as usual).
    fn aligned_stroke_ring(
        rect: Rect,
        radii: &document_proto::CornerRadii,
        stroke: &StrokeStyle,
    ) -> Option<BezPath> {
        use kurbo::Shape;
        if !stroke.dash_pattern.is_empty() {
            return None;
        }
        let w = stroke.width.max(0.0);
        let rounded = |r: Rect, grow: f64| {
            // Growing a square corner keeps it square unless the join
            // rounds it; shrinking a corner stops at square.
            let corner = |c: f64| {
                if c <= 0.0 && grow > 0.0 && !matches!(stroke.join, StrokeJoin::Round) {
                    0.0
                } else {
                    (c + grow).max(0.0)
                }
            };
            RoundedRect::from_rect(
                r,
                kurbo::RoundedRectRadii::new(
                    corner(radii.top_left),
                    corner(radii.top_right),
                    corner(radii.bottom_right),
                    corner(radii.bottom_left),
                ),
            )
            .to_path(0.01)
        };
        let (mut ring, inner, grow) = match stroke.align {
            StrokeAlign::Center => return None,
            StrokeAlign::Inside => (rounded(rect, 0.0), rect.inset(-w), -w),
            StrokeAlign::Outside => (rounded(rect.inset(w), w), rect, 0.0),
        };
        if inner.width() > 0.0 && inner.height() > 0.0 {
            ring.extend(rounded(inner, grow));
        }
        Some(ring)
    }

    /// A vector authored in frame: geometry drawn as written, in node-local
    /// coordinates (`transform` maps them to the target); a stroke without
    /// stroke geometry strokes the fill geometry (open subpaths as lines,
    /// never filled); effects follow the outline.
    #[allow(clippy::too_many_arguments)]
    fn paint_authored_path<S: PaintTarget>(
        scene: &mut S,
        transform: kurbo::Affine,
        width: f64,
        height: f64,
        fill_paths: &[String],
        stroke_paths: &[String],
        fills: &[(FillPaint, BlendMix)],
        stroke: Option<&StrokeStyle>,
        effects: &[NodeEffect],
    ) {
        use kurbo::Shape;
        let stroke = stroke.filter(|s| s.width > 0.0);
        let fill_geo: Vec<Arc<BezPath>> = fill_paths
            .iter()
            .filter_map(|d| cached_bez_path(d))
            .collect();
        let stroke_geo: Vec<Arc<BezPath>> = stroke_paths
            .iter()
            .filter_map(|d| cached_bez_path(d))
            .collect();
        // Stroke geometry is the stroke's own filled outline (as Figma
        // exports it); without it, a stroke strokes the fill geometry.
        let strokes_fill = stroke.is_some() && stroke_geo.is_empty();
        let filled: Vec<BezPath> = fill_geo
            .iter()
            .map(|p| {
                if strokes_fill {
                    split_open_closed(p).0
                } else {
                    (**p).clone()
                }
            })
            .filter(|p| !p.elements().is_empty())
            .collect();
        let stroke_area: Vec<BezPath> = match stroke {
            _ if !stroke_geo.is_empty() => stroke_geo.iter().map(|p| (**p).clone()).collect(),
            Some(s) => {
                let style = to_kurbo_stroke(s, 1.0);
                fill_geo
                    .iter()
                    .map(|p| {
                        kurbo::stroke(
                            p.elements().iter().copied(),
                            &style,
                            &kurbo::StrokeOpts::default(),
                            0.05,
                        )
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        let fill_refs: Vec<&BezPath> = if fills.is_empty() {
            Vec::new()
        } else {
            filled.iter().collect()
        };
        let outline: Vec<&BezPath> = fill_refs
            .iter()
            .copied()
            .chain(stroke_area.iter())
            .collect();
        let bounds = outline
            .iter()
            .map(|p| p.bounding_box())
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::new(0.0, 0.0, width, height))
            .inflate(1.0, 1.0);

        let layer_blur = effects
            .iter()
            .find(|e| matches!(e.kind, EffectKind::LayerBlur));
        if let Some(blur) = layer_blur {
            let still = Soft::Outer {
                dx: 0.0,
                dy: 0.0,
                spread: 0.0,
            };
            for (fill, _) in fills {
                match fill {
                    FillPaint::Solid(c) => {
                        soft::paint(scene, transform, &fill_refs, c, blur.radius, still)
                    }
                    _ => {
                        let owned: Vec<String> = filled.iter().map(BezPath::to_svg).collect();
                        paint_blurred_path_content(
                            scene,
                            transform,
                            &owned,
                            &[],
                            Some(fill),
                            None,
                            1.0,
                            0.0,
                            0.0,
                            width,
                            height,
                            blur,
                        );
                    }
                }
            }
            if let Some(s) = stroke {
                let refs: Vec<&BezPath> = stroke_area.iter().collect();
                soft::paint(scene, transform, &refs, &s.color, blur.radius, still);
            }
        }
        for effect in effects {
            if matches!(effect.kind, EffectKind::DropShadow | EffectKind::Glow) {
                let soft = Soft::Outer {
                    dx: effect.offset_x,
                    dy: effect.offset_y,
                    spread: effect.spread,
                };
                soft::paint(
                    scene,
                    transform,
                    &outline,
                    &effect.color,
                    effect.radius,
                    soft,
                );
            }
        }
        if layer_blur.is_none() {
            for (fill, fill_blend) in fills {
                let blended = *fill_blend != BlendMix::Normal;
                if blended {
                    scene.push_layer(
                        Fill::NonZero,
                        peniko::BlendMode {
                            mix: to_peniko_mix(fill_blend),
                            compose: peniko::Compose::SrcOver,
                        },
                        1.0,
                        transform,
                        &bounds,
                    );
                }
                for path in &filled {
                    fill_path_shape(scene, transform, path, 0.0, 0.0, width, height, fill);
                }
                if blended {
                    scene.pop_layer();
                }
            }
            if let Some(s) = stroke {
                let brush = stroke_brush(s, 0.0, 0.0, width, height);
                if strokes_fill {
                    for path in &fill_geo {
                        stroke_aligned_path(scene, transform, path, s, &brush);
                    }
                } else {
                    for path in &stroke_geo {
                        scene.fill(Fill::NonZero, transform, &brush.0, brush.1, &**path);
                    }
                }
            }
        }
        for effect in effects {
            match effect.kind {
                EffectKind::InnerShadow => {
                    let inside: Vec<&BezPath> = if fill_refs.is_empty() {
                        stroke_area.iter().collect()
                    } else {
                        fill_refs.clone()
                    };
                    let soft = Soft::Inner {
                        dx: effect.offset_x,
                        dy: effect.offset_y,
                        spread: effect.spread,
                    };
                    soft::paint(
                        scene,
                        transform,
                        &inside,
                        &effect.color,
                        effect.radius,
                        soft,
                    );
                }
                EffectKind::BackgroundBlur => {
                    let hint = fills
                        .first()
                        .and_then(|(f, _)| first_color(f))
                        .or_else(|| stroke.map(|s| s.color.clone()));
                    paint_effect(
                        scene,
                        transform,
                        &Rect::new(0.0, 0.0, width, height),
                        0.0,
                        effect,
                        hint.as_ref(),
                    );
                }
                _ => {}
            }
        }
    }

    /// Stroke `path` with `s` in `brush` (see [`stroke_brush`]), honouring
    /// its alignment on closed subpaths (open ones are always centred).
    fn stroke_aligned_path<S: PaintTarget>(
        scene: &mut S,
        transform: kurbo::Affine,
        path: &BezPath,
        s: &StrokeStyle,
        brush: &(peniko::Brush, Option<kurbo::Affine>),
    ) {
        use kurbo::Shape;
        let (colour, bt) = (&brush.0, brush.1);
        if matches!(s.align, StrokeAlign::Center) {
            scene.stroke(&to_kurbo_stroke(s, 1.0), transform, colour, bt, path);
            return;
        }
        let (closed, open) = split_open_closed(path);
        if !closed.elements().is_empty() {
            // Twice the width, centred on the edge, then cut to one side.
            let double = to_kurbo_stroke(
                &StrokeStyle {
                    width: s.width * 2.0,
                    ..s.clone()
                },
                1.0,
            );
            if matches!(s.align, StrokeAlign::Inside) {
                scene.push_clip_layer(Fill::NonZero, transform, &closed);
                scene.stroke(&double, transform, colour, bt, &closed);
                scene.pop_layer();
            } else {
                let reach = closed
                    .bounding_box()
                    .inflate(s.width * 2.0 + 1.0, s.width * 2.0 + 1.0);
                scene.push_layer(Fill::NonZero, peniko::Mix::Normal, 1.0, transform, &reach);
                scene.stroke(&double, transform, colour, bt, &closed);
                scene.push_layer(
                    Fill::NonZero,
                    peniko::Compose::DestOut,
                    1.0,
                    transform,
                    &closed,
                );
                scene.fill(
                    Fill::NonZero,
                    transform,
                    peniko::Color::BLACK,
                    None,
                    &closed,
                );
                scene.pop_layer();
                scene.pop_layer();
            }
        }
        if !open.elements().is_empty() {
            scene.stroke(&to_kurbo_stroke(s, 1.0), transform, colour, bt, &open);
        }
    }

    /// A stroke's brush: its gradient laid across the node's box
    /// `(x, y, w, h)` exactly as a fill's (with the brush transform a radial
    /// gradient needs), or its colour.
    fn stroke_brush(
        s: &StrokeStyle,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> (peniko::Brush, Option<kurbo::Affine>) {
        let (x, y, w, h) = super::types::stroke_gradient_box(s, x, y, w, h);
        let gradient = match &s.gradient {
            Some(FillPaint::GradientLinear { transform, stops }) => {
                build_linear_gradient(x, y, w, h, transform.as_ref(), stops).map(|g| (g, None))
            }
            Some(FillPaint::GradientRadial { transform, stops }) => {
                build_radial_gradient(x, y, w, h, transform.as_ref(), stops)
            }
            Some(FillPaint::GradientAngular { transform, stops }) => {
                build_angular_gradient(x, y, w, h, transform.as_ref(), stops).map(|g| (g, None))
            }
            _ => None,
        };
        match gradient {
            Some((g, t)) => (peniko::Brush::Gradient(g), t),
            None => (peniko::Brush::Solid(to_peniko(s.color.clone())), None),
        }
    }

    fn fill_shape<S: PaintTarget>(
        scene: &mut S,
        scene_transform: kurbo::Affine,
        shape: &impl kurbo::Shape,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        fill: &FillPaint,
    ) {
        use peniko::Fill;

        match fill {
            FillPaint::Solid(color) => {
                scene.fill(
                    Fill::NonZero,
                    scene_transform,
                    to_peniko(color.clone()),
                    None,
                    shape,
                );
            }
            FillPaint::GradientLinear { transform, stops } => {
                if let Some(gradient) =
                    build_linear_gradient(x, y, width, height, transform.as_ref(), stops)
                {
                    scene.fill(Fill::NonZero, scene_transform, &gradient, None, shape);
                }
            }
            FillPaint::GradientRadial { transform, stops } => {
                if let Some((gradient, brush_xform)) =
                    build_radial_gradient(x, y, width, height, transform.as_ref(), stops)
                {
                    scene.fill(
                        Fill::NonZero,
                        scene_transform,
                        &gradient,
                        brush_xform,
                        shape,
                    );
                }
            }
            FillPaint::GradientAngular { transform, stops } => {
                if let Some(gradient) =
                    build_angular_gradient(x, y, width, height, transform.as_ref(), stops)
                {
                    scene.fill(Fill::NonZero, scene_transform, &gradient, None, shape);
                }
            }
            FillPaint::Image {
                image_hash: _,
                data_base64,
                alpha,
                scale_mode,
                image_transform,
            } => {
                if let Some(image_brush) =
                    decode_image_brush(data_base64, *alpha, *scale_mode, width, height)
                {
                    let iw = image_brush.image.width as f64;
                    let ih = image_brush.image.height as f64;
                    if iw > 0.0 && ih > 0.0 {
                        let brush_transform = image_brush_transform(
                            *scale_mode,
                            image_transform,
                            x,
                            y,
                            width,
                            height,
                            iw,
                            ih,
                        );
                        scene.fill(
                            Fill::NonZero,
                            scene_transform,
                            image_brush.as_ref(),
                            Some(brush_transform),
                            shape,
                        );
                    }
                }
            }
        }
    }

    fn paint_effect<S: PaintTarget>(
        scene: &mut S,
        scene_transform: kurbo::Affine,
        base_rect: &Rect,
        corner_radius: f64,
        effect: &NodeEffect,
        blur_color_hint: Option<&Rgba>,
    ) {
        match effect.kind {
            EffectKind::DropShadow | EffectKind::Glow => {
                // Spread expands the shadow rect outward in all directions
                let shadow_rect = Rect::new(
                    base_rect.x0 + effect.offset_x - effect.spread,
                    base_rect.y0 + effect.offset_y - effect.spread,
                    base_rect.x1 + effect.offset_x + effect.spread,
                    base_rect.y1 + effect.offset_y + effect.spread,
                );
                // Corner radius grows additively with spread
                let shadow_corner = (corner_radius + effect.spread).max(0.0);
                // Figma radius is CSS blur radius (≈ 2×sigma); draw_box_shadow expects sigma
                scene.draw_blurred_rounded_rect(
                    scene_transform,
                    shadow_rect,
                    to_peniko(effect.color.clone()),
                    shadow_corner,
                    (effect.radius * 0.5).max(0.0),
                );
            }
            EffectKind::InnerShadow => {
                paint_inner_shadow(scene, scene_transform, base_rect, corner_radius, effect)
            }
            EffectKind::LayerBlur => {
                // Single GPU-native Gaussian blur of the bounding rect.
                let sigma = (effect.radius * 0.5).max(0.5);
                let hint = blur_color_hint.cloned().unwrap_or(Rgba {
                    r: 0.75,
                    g: 0.78,
                    b: 0.82,
                    a: 1.0,
                });
                scene.draw_blurred_rounded_rect(
                    scene_transform,
                    *base_rect,
                    to_peniko(hint),
                    corner_radius.max(effect.spread),
                    sigma,
                );
            }
            EffectKind::BackgroundBlur => {
                // Single blurred rect approximation for backdrop blur.
                // True background blur requires backdrop sampling which vello doesn't support.
                let sigma = (effect.radius * 0.5).max(0.5);
                let hint_color = blur_color_hint.cloned().unwrap_or(Rgba {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.3,
                });
                scene.draw_blurred_rounded_rect(
                    scene_transform,
                    *base_rect,
                    to_peniko(hint_color),
                    corner_radius.max(effect.spread),
                    sigma,
                );
            }
        }
    }

    /// An inner shadow of `base` (a rect rounded by `corner`): the outline
    /// filled with the shadow colour, minus a blurred hole at the offset
    /// (a DestOut layer), in an isolating layer. (Pattern from the Blitz
    /// reference implementation.)
    ///
    /// Over a large node the layers are bounded to the band along the edge
    /// the shadow can reach ([`inner_shadow_cut`]): deeper inside, the hole
    /// covers everything and whole-outline layers would only cost blend-stack
    /// depth, per-tile commands and tile allocations over the node's full
    /// area — enough, with a few large faces on a page, to overflow vello's
    /// fixed buffers and blank the page. Under an axis-aligned transform the
    /// band is drawn as up to four strips, each its own clip, so every draw's
    /// tiles stay within its strip; otherwise as one even-odd band clip.
    fn paint_inner_shadow<S: PaintTarget>(
        scene: &mut S,
        transform: kurbo::Affine,
        base: &Rect,
        corner: f64,
        effect: &NodeEffect,
    ) {
        let rr = RoundedRect::from_rect(*base, corner);
        let whole = || (Fill::NonZero, kurbo::Shape::to_path(&rr, 0.1));
        let device_area = base.area() * transform.determinant().abs();
        let cut = if device_area >= BAND_MIN_DEVICE_AREA {
            inner_shadow_cut(base, corner, effect)
        } else {
            None
        };
        let Some(cut) = cut else {
            let (fill, clip) = whole();
            inner_shadow_layers(scene, transform, &rr, (fill, &clip), effect);
            return;
        };
        if let Some(strips) = band_strips(transform, base, &cut) {
            let (fill, clip) = whole();
            for strip in strips {
                scene.push_clip_layer(Fill::NonZero, transform, &strip);
                inner_shadow_layers(scene, transform, &rr, (fill, &clip), effect);
                scene.pop_layer();
            }
            return;
        }
        let mut band = kurbo::Shape::to_path(&rr, 0.1);
        band.extend(kurbo::Shape::to_path(&cut, 0.1));
        inner_shadow_layers(scene, transform, &rr, (Fill::EvenOdd, &band), effect);
    }

    /// The inner shadow's layers, the outer one clipped to `clip`.
    fn inner_shadow_layers<S: PaintTarget>(
        scene: &mut S,
        transform: kurbo::Affine,
        rr: &RoundedRect,
        clip: (Fill, &BezPath),
        effect: &NodeEffect,
    ) {
        // Layer 1: isolate inner shadow compositing group
        scene.push_layer(clip.0, peniko::Mix::Normal, 1.0, transform, clip.1);
        // Fill entire clipped area with shadow color
        scene.fill(
            Fill::NonZero,
            transform,
            to_peniko(effect.color.clone()),
            None,
            rr,
        );
        // Layer 2: DestOut — punch a blurred hole at offset position.
        // (Clipped to the whole outline: where the outer clip is partial the
        // hole is complete, and clipping both would leave a faint seam.)
        scene.push_layer(Fill::NonZero, peniko::Compose::DestOut, 1.0, transform, rr);
        // Spread shrinks the hole (opposite of drop shadow)
        let hole_corner = (rr.radii().top_left - effect.spread).max(0.0);
        // Figma radius is CSS blur radius (≈ 2×sigma)
        scene.draw_blurred_rounded_rect(
            transform,
            inner_shadow_hole(&rr.rect(), effect),
            to_peniko(Rgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            }),
            hole_corner,
            (effect.radius * 0.5).max(0.0),
        );
        scene.pop_layer(); // DestOut
        scene.pop_layer(); // isolation
    }

    /// The inner shadow's hole: `base` offset, shrunk by the spread.
    fn inner_shadow_hole(base: &Rect, effect: &NodeEffect) -> Rect {
        Rect::new(
            base.x0 + effect.offset_x + effect.spread,
            base.y0 + effect.offset_y + effect.spread,
            base.x1 + effect.offset_x - effect.spread,
            base.y1 + effect.offset_y - effect.spread,
        )
    }

    /// Nodes smaller than this (device px², 1024 of vello's 16 px tiles)
    /// keep whole-outline layers: they cost little, and bounding them would
    /// only move where vello rounds the backdrop to 8 bits.
    const BAND_MIN_DEVICE_AREA: f64 = 512.0 * 512.0;

    /// The region inside `base` that an inner shadow's blurred hole covers
    /// completely — nothing of the shadow shows there. `None` when that
    /// region is small (a small node, a wide blur or a large offset).
    ///
    /// It keeps a margin of 4σ past the hole's edge (its rounded corners
    /// included), where the blur's tail is under a thousandth — below one
    /// 8-bit step at any shadow opacity — so the shadow comes out as over the
    /// whole node. (What changes is only where vello rounds the backdrop to
    /// 8 bits: inside the cut it no longer passes through the layer's blend
    /// stack, so content drawn over it later can land one step apart.)
    fn inner_shadow_cut(base: &Rect, corner: f64, effect: &NodeEffect) -> Option<Rect> {
        let sigma = (effect.radius * 0.5).max(0.0);
        let hole_corner = (corner - effect.spread).max(0.0);
        let margin = 4.0 * sigma + hole_corner + 1.0;
        let cut = inner_shadow_hole(base, effect)
            .inset(-margin)
            .intersect(*base);
        // Not worth it unless it saves most of the node.
        (cut.width() > 0.0 && cut.height() > 0.0 && cut.area() >= base.area() * 0.25).then_some(cut)
    }

    /// `base` minus `cut` as up to four rectangles (top and bottom full
    /// width, left and right between them), when `transform` is
    /// axis-aligned: their edges are moved to whole device tiles (16 px),
    /// the cut only shrinking, so neighbouring strips meet on pixel
    /// boundaries and no pixel is split between two of them. The outer
    /// edges lie a tile outside `base`, so the node's own outline is
    /// antialiased as without them. `None` under rotation or skew.
    fn band_strips(transform: kurbo::Affine, base: &Rect, cut: &Rect) -> Option<Vec<Rect>> {
        const TILE: f64 = 16.0;
        let [a, b, c, d, _, _] = transform.as_coeffs();
        if b.abs() > 1e-9 || c.abs() > 1e-9 || a.abs() < 1e-9 || d.abs() < 1e-9 {
            return None;
        }
        let device = |r: &Rect| transform.transform_rect_bbox(*r);
        let (outer, inner) = (device(base), device(cut));
        let outer = Rect::new(
            (outer.x0 / TILE).floor() * TILE - TILE,
            (outer.y0 / TILE).floor() * TILE - TILE,
            (outer.x1 / TILE).ceil() * TILE + TILE,
            (outer.y1 / TILE).ceil() * TILE + TILE,
        );
        let inner = Rect::new(
            (inner.x0 / TILE).ceil() * TILE,
            (inner.y0 / TILE).ceil() * TILE,
            (inner.x1 / TILE).floor() * TILE,
            (inner.y1 / TILE).floor() * TILE,
        );
        if inner.width() <= 0.0 || inner.height() <= 0.0 {
            return None;
        }
        let back = transform.inverse();
        let strips = [
            Rect::new(outer.x0, outer.y0, outer.x1, inner.y0),
            Rect::new(outer.x0, inner.y1, outer.x1, outer.y1),
            Rect::new(outer.x0, inner.y0, inner.x0, inner.y1),
            Rect::new(inner.x1, inner.y0, outer.x1, inner.y1),
        ];
        Some(
            strips
                .iter()
                .map(|s| back.transform_rect_bbox(*s))
                .collect(),
        )
    }

    fn paint_blurred_rect_content<S: PaintTarget>(
        scene: &mut S,
        scene_transform: kurbo::Affine,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        corner_radius: f64,
        fill: Option<&FillPaint>,
        stroke: Option<&StrokeStyle>,
        effect: &NodeEffect,
    ) {
        let sigma = (effect.radius * 0.5).max(0.5);
        let rect = Rect::new(x, y, x + width, y + height);

        // For solid fills, use vello's GPU-native Gaussian blur on the rectangle.
        // This produces a proper blur rather than shifted semi-transparent copies.
        if let Some(fill) = fill {
            match fill {
                FillPaint::Solid(color) => {
                    scene.draw_blurred_rounded_rect(
                        scene_transform,
                        rect,
                        to_peniko(color.clone()),
                        corner_radius,
                        sigma,
                    );
                }
                // For gradient/image fills, fall back to shifted copies
                _ => {
                    for (dx, dy, weight) in layer_blur_kernel(effect.radius.max(0.5)) {
                        let shifted_transform =
                            scene_transform * kurbo::Affine::translate((dx, dy));
                        if corner_radius > 0.0 {
                            let rr = RoundedRect::from_rect(rect, corner_radius);
                            let f = fill.clone().with_opacity(weight);
                            fill_shape(scene, shifted_transform, &rr, x, y, width, height, &f);
                        } else {
                            let f = fill.clone().with_opacity(weight);
                            fill_shape(scene, shifted_transform, &rect, x, y, width, height, &f);
                        }
                    }
                }
            }
        }
        // Stroke blur: single blurred rect with stroke color
        if let Some(stroke) = stroke {
            if stroke.width > 0.0 {
                let stroke_rect = stroke_aligned_rect(rect, Some(stroke));
                let stroke_radius = stroke_aligned_corner_radius(corner_radius, Some(stroke));
                scene.draw_blurred_rounded_rect(
                    scene_transform,
                    stroke_rect,
                    to_peniko(stroke.color.clone()),
                    stroke_radius,
                    sigma,
                );
            }
        }
    }

    fn paint_blurred_path_content<S: PaintTarget>(
        scene: &mut S,
        path_transform: kurbo::Affine,
        fill_paths: &[String],
        stroke_paths: &[String],
        fill: Option<&FillPaint>,
        stroke: Option<&StrokeStyle>,
        stroke_scale: f64,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        effect: &NodeEffect,
    ) {
        let radius = effect.radius.max(0.5);
        for (dx, dy, weight) in layer_blur_kernel(radius) {
            let shifted = kurbo::Affine::translate((dx, dy)) * path_transform;
            if let Some(fill_paint) = fill.cloned().map(|f| f.with_opacity(weight)) {
                for path_data in fill_paths {
                    if let Some(path) = cached_bez_path(path_data) {
                        fill_path_shape(scene, shifted, &*path, x, y, width, height, &fill_paint);
                    }
                }
            }
            if let Some(stroke) = stroke {
                if stroke.width > 0.0 {
                    let mut stroke_blur = stroke.clone();
                    stroke_blur.color.a = (stroke_blur.color.a * weight).clamp(0.0, 1.0);
                    let stroke_style = to_kurbo_stroke(&stroke_blur, stroke_scale);
                    for path_data in stroke_paths {
                        if let Some(path) = cached_bez_path(path_data) {
                            scene.stroke(
                                &stroke_style,
                                shifted,
                                to_peniko(stroke_blur.color.clone()),
                                None,
                                &*path,
                            );
                        }
                    }
                }
            }
        }
    }

    fn paint_blurred_svg_content<S: PaintTarget>(
        scene: &mut S,
        scene_transform: kurbo::Affine,
        svg_text: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        effect: &NodeEffect,
    ) {
        let radius = effect.radius.max(0.5);
        let clip_rect = Rect::new(x, y, x + width, y + height);
        let svg_fit = cached_svg_fit_transform(svg_text, width, height);
        // Cache the parsed SVG tree once — blur kernel loops would otherwise
        // re-parse the XML 6-20 times per blurred element per frame.
        // Parsed once (each target caches its tree): the blur kernel loops
        // would otherwise re-parse the XML 6-20 times per blurred element.
        for (dx, dy, weight) in layer_blur_kernel(radius) {
            let svg_transform =
                scene_transform * kurbo::Affine::translate((x + dx, y + dy)) * svg_fit;
            scene.push_layer(
                Fill::NonZero,
                peniko::BlendMode {
                    mix: peniko::Mix::Normal,
                    compose: peniko::Compose::SrcOver,
                },
                weight as f32,
                scene_transform,
                &clip_rect,
            );
            scene.draw_svg(svg_text, svg_transform);
            scene.pop_layer();
        }
    }

    fn fill_path_shape<S: PaintTarget>(
        scene: &mut S,
        path_transform: kurbo::Affine,
        path: &BezPath,
        _x: f64,
        _y: f64,
        width: f64,
        height: f64,
        fill: &FillPaint,
    ) {
        use peniko::Fill;

        // Gradient coordinates must be in the same space as the path shape.
        // fillGeometry paths are in local coords (0,0 origin); path_transform
        // maps them to screen space.  Build gradients at (0,0) so they align.
        match fill {
            FillPaint::Solid(color) => {
                scene.fill(
                    Fill::NonZero,
                    path_transform,
                    to_peniko(color.clone()),
                    None,
                    path,
                );
            }
            FillPaint::GradientLinear { transform, stops } => {
                if let Some(gradient) =
                    build_linear_gradient(0.0, 0.0, width, height, transform.as_ref(), stops)
                {
                    scene.fill(Fill::NonZero, path_transform, &gradient, None, path);
                }
            }
            FillPaint::GradientRadial { transform, stops } => {
                if let Some((gradient, brush_xform)) =
                    build_radial_gradient(0.0, 0.0, width, height, transform.as_ref(), stops)
                {
                    scene.fill(Fill::NonZero, path_transform, &gradient, brush_xform, path);
                }
            }
            FillPaint::GradientAngular { transform, stops } => {
                if let Some(gradient) =
                    build_angular_gradient(0.0, 0.0, width, height, transform.as_ref(), stops)
                {
                    scene.fill(Fill::NonZero, path_transform, &gradient, None, path);
                }
            }
            FillPaint::Image {
                image_hash: _,
                data_base64,
                alpha,
                scale_mode,
                image_transform,
            } => {
                if let Some(image_brush) =
                    decode_image_brush(data_base64, *alpha, *scale_mode, width, height)
                {
                    let iw = image_brush.image.width as f64;
                    let ih = image_brush.image.height as f64;
                    if iw > 0.0 && ih > 0.0 {
                        // Use local-space origin (0, 0) — same as gradient fix above
                        let brush_transform = image_brush_transform(
                            *scale_mode,
                            image_transform,
                            0.0,
                            0.0,
                            width,
                            height,
                            iw,
                            ih,
                        );
                        scene.fill(
                            Fill::NonZero,
                            path_transform,
                            image_brush.as_ref(),
                            Some(brush_transform),
                            path,
                        );
                    }
                }
            }
        }
    }

    fn image_brush_transform(
        scale_mode: ImageScaleMode,
        image_transform: &Option<AffineTransform2D>,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        iw: f64,
        ih: f64,
    ) -> kurbo::Affine {
        match scale_mode {
            ImageScaleMode::Fill => {
                // Scale uniformly to cover the entire node, center the image
                let scale = if width > 0.0 && height > 0.0 {
                    (width / iw).max(height / ih)
                } else {
                    1.0
                };
                let offset_x = x + (width - iw * scale) * 0.5;
                let offset_y = y + (height - ih * scale) * 0.5;
                kurbo::Affine::translate((offset_x, offset_y)) * kurbo::Affine::scale(scale)
            }
            ImageScaleMode::Fit => {
                // Scale uniformly to fit inside the node, center the image
                let scale = if width > 0.0 && height > 0.0 {
                    (width / iw).min(height / ih)
                } else {
                    1.0
                };
                let offset_x = x + (width - iw * scale) * 0.5;
                let offset_y = y + (height - ih * scale) * 0.5;
                kurbo::Affine::translate((offset_x, offset_y)) * kurbo::Affine::scale(scale)
            }
            ImageScaleMode::Crop => {
                // Figma imageTransform maps node-normalized [0,1]×[0,1] to
                // image-normalized [0,1]×[0,1]. The brush transform maps image
                // pixels → scene pixels, so we need the INVERSE of imageTransform:
                //   translate(node_origin) * scale(node_size) * inv(T) * scale(1/img_size)
                if let Some(t) = image_transform {
                    let det = t.determinant();
                    if det.abs() < 1e-12 {
                        let scale = (width / iw).max(height / ih);
                        let ox = x + (width - iw * scale) * 0.5;
                        let oy = y + (height - ih * scale) * 0.5;
                        return kurbo::Affine::translate((ox, oy)) * kurbo::Affine::scale(scale);
                    }
                    // Invert the 2x3 affine: node-norm → image-norm becomes
                    // image-norm → node-norm
                    let inv = kurbo::Affine::new([
                        t.d / det,
                        -t.c / det,
                        -t.b / det,
                        t.a / det,
                        (t.b * t.ty - t.d * t.tx) / det,
                        (t.c * t.tx - t.a * t.ty) / det,
                    ]);
                    kurbo::Affine::translate((x, y))
                        * kurbo::Affine::scale_non_uniform(width, height)
                        * inv
                        * kurbo::Affine::scale_non_uniform(1.0 / iw, 1.0 / ih)
                } else {
                    // No transform — fall back to Fill
                    let scale = (width / iw).max(height / ih);
                    let ox = x + (width - iw * scale) * 0.5;
                    let oy = y + (height - ih * scale) * 0.5;
                    kurbo::Affine::translate((ox, oy)) * kurbo::Affine::scale(scale)
                }
            }
            ImageScaleMode::Tile => {
                // One tile placed by the image transform (image px → the
                // node's box, from its top-left; identity = the image at
                // its own size), repeated by the brush's `Extend::Repeat`.
                kurbo::Affine::translate((x, y))
                    * image_transform
                        .as_ref()
                        .map_or(kurbo::Affine::IDENTITY, to_kurbo_affine)
            }
            ImageScaleMode::Stretch => {
                // Non-uniform stretch to fill node exactly
                let sx = if width > 0.0 { width / iw } else { 1.0 };
                let sy = if height > 0.0 { height / ih } else { 1.0 };
                kurbo::Affine::translate((x, y)) * kurbo::Affine::scale_non_uniform(sx, sy)
            }
        }
    }

    fn layer_blur_kernel(radius: f64) -> Vec<(f64, f64, f64)> {
        let sigma = (radius * 0.5).max(0.5);
        // Cap the integer grid to at most 7x7 = 49 samples to avoid
        // overwhelming the GPU with draw calls per blurred element.
        let half_grid: i32 = 3; // grid goes -3..+3 → 7 taps per axis
        // Step size spreads the grid to cover 2.5σ spatially
        let step = (sigma * 2.5) / half_grid as f64;
        let mut kernel = Vec::new();
        let mut total_weight = 0.0;

        for iy in -half_grid..=half_grid {
            for ix in -half_grid..=half_grid {
                let dx = ix as f64 * step;
                let dy = iy as f64 * step;
                let w = (-0.5 * (dx * dx + dy * dy) / (sigma * sigma)).exp();
                kernel.push((dx, dy, w));
                total_weight += w;
            }
        }

        // Normalize weights to sum to 1.0
        for entry in &mut kernel {
            entry.2 /= total_weight;
        }
        kernel
    }

    fn build_linear_gradient(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        transform: Option<&AffineTransform2D>,
        stops: &[GradientStop],
    ) -> Option<peniko::Gradient> {
        use kurbo::Point;

        if stops.is_empty() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        // Figma linear gradient runs from UV (0, 0.5) to (1, 0.5)
        let (snx, sny) = gradient_transform_point(transform, 0.0, 0.5);
        let (enx, eny) = gradient_transform_point(transform, 1.0, 0.5);
        let start = Point::new(x + snx * width, y + sny * height);
        let end = Point::new(x + enx * width, y + eny * height);
        Some(peniko::Gradient::new_linear(start, end).with_stops(to_peniko_stops(stops).as_slice()))
    }

    fn build_radial_gradient(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        transform: Option<&AffineTransform2D>,
        stops: &[GradientStop],
    ) -> Option<(peniko::Gradient, Option<kurbo::Affine>)> {
        use kurbo::Point;

        if stops.is_empty() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        // Figma radial center is at UV (0.5, 0.5)
        let (cnx, cny) = gradient_transform_point(transform, 0.5, 0.5);
        let center = Point::new(x + cnx * width, y + cny * height);
        let peniko_stops = to_peniko_stops(stops);

        if let Some(t) = transform {
            // Ellipse axes from transform column vectors scaled to pixel space
            let ax = (t.a * width, t.c * height);
            let ay = (t.b * width, t.d * height);
            let rx = (ax.0 * ax.0 + ax.1 * ax.1).sqrt() * 0.5;
            let ry = (ay.0 * ay.0 + ay.1 * ay.1).sqrt() * 0.5;
            let angle = ax.1.atan2(ax.0);

            // Unit circle gradient at origin; brush_transform maps to ellipse
            let gradient =
                peniko::Gradient::new_two_point_radial(Point::ZERO, 0.0, Point::ZERO, 1.0)
                    .with_stops(peniko_stops.as_slice());

            let brush_xform = kurbo::Affine::translate(center.to_vec2())
                * kurbo::Affine::rotate(angle)
                * kurbo::Affine::scale_non_uniform(rx.max(0.5), ry.max(0.5));

            Some((gradient, Some(brush_xform)))
        } else {
            let radius = (width.min(height) * 0.5).max(0.5) as f32;
            let gradient = peniko::Gradient::new_two_point_radial(center, 0.0, center, radius)
                .with_stops(peniko_stops.as_slice());
            Some((gradient, None))
        }
    }

    fn build_angular_gradient(
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        transform: Option<&AffineTransform2D>,
        stops: &[GradientStop],
    ) -> Option<peniko::Gradient> {
        use kurbo::Point;

        if stops.is_empty() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        // Figma angular/sweep center is at UV (0.5, 0.5)
        let (cnx, cny) = gradient_transform_point(transform, 0.5, 0.5);
        let center = Point::new(x + cnx * width, y + cny * height);
        // Extract rotation angle from transform's first column
        let start_angle = if let Some(t) = transform {
            (t.c * height).atan2(t.a * width) as f32
        } else {
            0.0
        };
        let end_angle = start_angle + std::f32::consts::PI * 2.0;
        Some(
            peniko::Gradient::new_sweep(center, start_angle, end_angle)
                // A sweep is periodic: wrap past the seam instead of padding,
                // so a start angle other than +x doesn't leave a flat wedge.
                .with_extend(peniko::Extend::Repeat)
                .with_stops(to_peniko_stops(stops).as_slice()),
        )
    }

    fn gradient_transform_point(
        transform: Option<&AffineTransform2D>,
        u: f64,
        v: f64,
    ) -> (f64, f64) {
        if let Some(t) = transform {
            t.transform_point(u, v)
        } else {
            (u, v)
        }
    }

    fn to_peniko_stops(stops: &[GradientStop]) -> Vec<peniko::ColorStop> {
        use peniko::color::DynamicColor;

        stops
            .iter()
            .map(|stop| peniko::ColorStop {
                offset: stop.offset as f32,
                color: DynamicColor::from_alpha_color(to_peniko(stop.color.clone())),
            })
            .collect()
    }

    fn to_kurbo_stroke(style: &StrokeStyle, width_scale: f64) -> Stroke {
        let cap = match style.cap {
            StrokeCap::None => kurbo::Cap::Butt,
            StrokeCap::Round => kurbo::Cap::Round,
            StrokeCap::Square => kurbo::Cap::Square,
            StrokeCap::LineArrow
            | StrokeCap::TriangleArrow
            | StrokeCap::DiamondFilled
            | StrokeCap::CircleFilled
            | StrokeCap::TriangleFilled
            | StrokeCap::WashiTape1
            | StrokeCap::WashiTape2
            | StrokeCap::WashiTape3
            | StrokeCap::WashiTape4
            | StrokeCap::WashiTape5
            | StrokeCap::WashiTape6 => kurbo::Cap::Butt,
        };
        let join = match style.join {
            StrokeJoin::Miter => kurbo::Join::Miter,
            StrokeJoin::Round => kurbo::Join::Round,
            StrokeJoin::Bevel => kurbo::Join::Bevel,
        };
        let mut stroke = Stroke::new((style.width * width_scale).max(0.0))
            .with_caps(cap)
            .with_join(join)
            .with_miter_limit(style.miter_limit.max(1.0));
        if !style.dash_pattern.is_empty() {
            stroke = stroke.with_dashes(
                style.dash_offset * width_scale,
                style
                    .dash_pattern
                    .iter()
                    .copied()
                    .map(|v| (v * width_scale).max(0.0)),
            );
        }
        stroke
    }

    fn to_peniko_mix(blend: &BlendMix) -> peniko::Mix {
        match blend {
            BlendMix::Normal => peniko::Mix::Normal,
            BlendMix::Multiply => peniko::Mix::Multiply,
            BlendMix::Screen => peniko::Mix::Screen,
            BlendMix::Overlay => peniko::Mix::Overlay,
            BlendMix::Darken => peniko::Mix::Darken,
            BlendMix::Lighten => peniko::Mix::Lighten,
            BlendMix::ColorDodge => peniko::Mix::ColorDodge,
            BlendMix::ColorBurn => peniko::Mix::ColorBurn,
            BlendMix::HardLight => peniko::Mix::HardLight,
            BlendMix::SoftLight => peniko::Mix::SoftLight,
            BlendMix::Difference => peniko::Mix::Difference,
            BlendMix::Exclusion => peniko::Mix::Exclusion,
            BlendMix::Hue => peniko::Mix::Hue,
            BlendMix::Saturation => peniko::Mix::Saturation,
            BlendMix::Color => peniko::Mix::Color,
            BlendMix::Luminosity => peniko::Mix::Luminosity,
        }
    }

    fn stroke_aligned_rect(base: Rect, style: Option<&StrokeStyle>) -> Rect {
        let Some(style) = style else {
            return base;
        };
        let half = (style.width * 0.5).max(0.0);
        let snap_half = |v: f64| ((v * 2.0).round()) * 0.5;
        match style.align {
            StrokeAlign::Inside => Rect::new(
                base.x0 + half,
                base.y0 + half,
                base.x1 - half,
                base.y1 - half,
            ),
            StrokeAlign::Outside => Rect::new(
                base.x0 - half,
                base.y0 - half,
                base.x1 + half,
                base.y1 + half,
            ),
            StrokeAlign::Center => Rect::new(
                snap_half(base.x0),
                snap_half(base.y0),
                snap_half(base.x1),
                snap_half(base.y1),
            ),
        }
    }

    fn stroke_aligned_corner_radius(base: f64, style: Option<&StrokeStyle>) -> f64 {
        let Some(style) = style else {
            return base.max(0.0);
        };
        let half = (style.width * 0.5).max(0.0);
        match style.align {
            StrokeAlign::Inside => (base - half).max(0.0),
            StrokeAlign::Outside => (base + half).max(0.0),
            StrokeAlign::Center => base.max(0.0),
        }
    }

    /// The image in `data_base64` as a brush at `alpha`, decoded once and
    /// cached (see [`decoded_image`]). A tiled image repeats past its edges;
    /// every other mode pads with its edge pixels.
    fn decode_image_brush(
        data_base64: &Arc<str>,
        alpha: f64,
        scale_mode: ImageScaleMode,
        display_w: f64,
        display_h: f64,
    ) -> Option<ImageBrush> {
        let tile = scale_mode == ImageScaleMode::Tile;
        // A tile is drawn at its own size, not stretched over the node:
        // keep every pixel.
        let display = (!tile).then_some((display_w, display_h));
        let image = decoded_image(data_base64, display)?;
        let extend = if tile {
            peniko::Extend::Repeat
        } else {
            peniko::Extend::Pad
        };
        Some(ImageBrush {
            image,
            sampler: ImageSampler {
                x_extend: extend,
                y_extend: extend,
                quality: peniko::ImageQuality::Medium,
                alpha: alpha as f32,
            },
        })
    }

    /// Decoded images kept before the cache is reset, in RGBA bytes.
    const DECODED_CACHE_BYTES: usize = 512 << 20;

    /// The image in `data_base64`, decoded to RGBA and — for `display`
    /// sizes much smaller than the image — downscaled to twice the display
    /// size (in 64 px buckets), so a large photo shown small doesn't upload
    /// megabytes of texture. Cached across frames: by the shared data's
    /// address (collects share one encoding per asset, so a frame's lookup
    /// is a pointer, not a hash of megabytes of text; the entry keeps the
    /// data alive, so the address can't be reused), and on a miss by
    /// content, so fresh copies of the same data don't decode again.
    fn decoded_image(data_base64: &Arc<str>, display: Option<(f64, f64)>) -> Option<ImageData> {
        use std::collections::HashMap;
        use std::hash::{Hash, Hasher};
        use std::sync::{LazyLock, Mutex};

        /// Display size bucket, px ((0, 0): full resolution).
        type Bucket = (u32, u32);
        struct Cache {
            /// (data address, bucket) → (the data, its image).
            by_ptr: HashMap<(usize, Bucket), (Arc<str>, ImageData)>,
            /// (data hash, bucket) → image.
            by_hash: HashMap<(u64, Bucket), ImageData>,
            bytes: usize,
        }
        static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| {
            Mutex::new(Cache {
                by_ptr: HashMap::new(),
                by_hash: HashMap::new(),
                bytes: 0,
            })
        });

        // Display dimensions bucketed to 64 px, so sub-pixel size changes
        // share an entry; (0, 0) for full resolution.
        let bucket = display.map_or((0, 0), |(w, h)| {
            (
                ((w / 64.0).ceil() as u32).max(1) * 64,
                ((h / 64.0).ceil() as u32).max(1) * 64,
            )
        });
        let ptr_key = (Arc::as_ptr(data_base64).cast::<u8>() as usize, bucket);
        if let Ok(cache) = CACHE.lock()
            && let Some((data, image)) = cache.by_ptr.get(&ptr_key)
            && Arc::ptr_eq(data, data_base64)
        {
            return Some(image.clone());
        }
        let hash_key = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            data_base64.hash(&mut hasher);
            (hasher.finish(), bucket)
        };
        let known = CACHE
            .lock()
            .ok()
            .and_then(|c| c.by_hash.get(&hash_key).cloned());
        let image = match known {
            Some(image) => image,
            None => decode_rgba(data_base64, display)?,
        };
        if let Ok(mut cache) = CACHE.lock() {
            let size = image.data.len();
            if cache.bytes + size > DECODED_CACHE_BYTES {
                cache.by_ptr.clear();
                cache.by_hash.clear();
                cache.bytes = 0;
            }
            cache.bytes += size;
            cache.by_hash.insert(hash_key, image.clone());
            cache
                .by_ptr
                .insert(ptr_key, (Arc::clone(data_base64), image.clone()));
        }
        Some(image)
    }

    /// Decode base64 image data to straight RGBA8, downscaled for
    /// `display` (see [`decoded_image`]).
    fn decode_rgba(data_base64: &str, display: Option<(f64, f64)>) -> Option<ImageData> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data_base64)
            .ok()?;
        let decoded = image::load_from_memory(&bytes).ok()?.into_rgba8();
        let (orig_w, orig_h) = (decoded.width(), decoded.height());
        let (final_img, width, height) = match display {
            Some((display_w, display_h))
                if orig_w > ((display_w * 2.0).ceil() as u32).max(64)
                    || orig_h > ((display_h * 2.0).ceil() as u32).max(64) =>
            {
                // Downscale to 2x display size (for retina quality) when the
                // source is significantly larger than the display area.
                let max_w = (display_w * 2.0).ceil() as u32;
                let max_h = (display_h * 2.0).ceil() as u32;
                let scale = (max_w as f64 / orig_w as f64).min(max_h as f64 / orig_h as f64);
                let new_w = ((orig_w as f64 * scale).round() as u32).max(1);
                let new_h = ((orig_h as f64 * scale).round() as u32).max(1);
                let resized = image::imageops::resize(
                    &decoded,
                    new_w,
                    new_h,
                    image::imageops::FilterType::Triangle,
                );
                let (w, h) = (resized.width(), resized.height());
                (resized.into_vec(), w, h)
            }
            _ => (decoded.into_vec(), orig_w, orig_h),
        };
        Some(ImageData {
            data: Blob::new(Arc::new(final_img)),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width,
            height,
        })
    }

    /// Render buffered mask-definition primitives into the scene.
    /// These are the shape primitives captured between MaskDefStart..MaskDefEnd.
    fn render_mask_primitives<S: PaintTarget>(
        scene: &mut S,
        prims: &[&PaintPrimitive],
        scene_transform: kurbo::Affine,
    ) {
        let root_transform = scene_transform;
        let mut group_stack: Vec<kurbo::Affine> = Vec::new();
        for prim in prims {
            let scene_transform = group_stack.last().copied().unwrap_or(root_transform);
            match prim {
                PaintPrimitive::GroupStart { transform, .. } => {
                    group_stack.push(scene_transform * to_kurbo_affine(transform));
                }
                PaintPrimitive::GroupEnd { .. } => {
                    group_stack.pop();
                }
                PaintPrimitive::Rect {
                    x,
                    y,
                    width,
                    height,
                    fills,
                    corner_radii,
                    rotation,
                    ..
                } => {
                    let (x, y, width, height, rotation) = (*x, *y, *width, *height, *rotation);
                    let (node_transform, lx, ly) = if rotation.abs() > 1e-6 {
                        let t = scene_transform
                            * kurbo::Affine::translate((x, y))
                            * kurbo::Affine::rotate(rotation);
                        (t, 0.0, 0.0)
                    } else {
                        (scene_transform, x, y)
                    };
                    let rect = Rect::new(lx, ly, lx + width, ly + height);
                    if !corner_radii.is_zero() {
                        let radii = kurbo::RoundedRectRadii::new(
                            corner_radii.top_left,
                            corner_radii.top_right,
                            corner_radii.bottom_right,
                            corner_radii.bottom_left,
                        );
                        let rr = RoundedRect::from_rect(rect, radii);
                        for (fill, _) in fills {
                            fill_shape(scene, node_transform, &rr, lx, ly, width, height, fill);
                        }
                    } else {
                        for (fill, _) in fills {
                            fill_shape(scene, node_transform, &rect, lx, ly, width, height, fill);
                        }
                    }
                }
                PaintPrimitive::Path {
                    x: path_x,
                    y: path_y,
                    width,
                    height,
                    fill_paths,
                    fills,
                    rotation,
                    normalize_to_origin,
                    stroke_paths,
                    authored,
                    text_outline,
                    ..
                } => {
                    let (width, height, rotation) = (*width, *height, *rotation);
                    let (base_transform, lx, ly) = if rotation.abs() > 1e-6 {
                        let t = scene_transform
                            * kurbo::Affine::translate((*path_x, *path_y))
                            * kurbo::Affine::rotate(rotation);
                        (t, 0.0, 0.0)
                    } else {
                        (scene_transform, *path_x, *path_y)
                    };
                    let path_origin_transform = base_transform * kurbo::Affine::translate((lx, ly));
                    // Authored geometry is drawn as written (see the main painter).
                    let path_fit = if *authored && !*text_outline {
                        kurbo::Affine::IDENTITY
                    } else {
                        compute_path_fit_transform(
                            fill_paths,
                            stroke_paths,
                            lx,
                            ly,
                            width,
                            height,
                            *normalize_to_origin,
                        )
                    };
                    let path_transform = path_origin_transform * path_fit;
                    for (fill_paint, _) in fills {
                        for path_data in fill_paths {
                            if let Some(path) = cached_bez_path(path_data) {
                                fill_path_shape(
                                    scene,
                                    path_transform,
                                    &*path,
                                    lx,
                                    ly,
                                    width,
                                    height,
                                    fill_paint,
                                );
                            }
                        }
                    }
                }
                _ => {
                    // Other primitive types in mask defs are uncommon; skip them.
                }
            }
        }
    }

    /// Row-major `[[a, b, tx], [c, d, ty]]` → kurbo's column-major coefficients.
    fn to_kurbo_affine(t: &AffineTransform2D) -> kurbo::Affine {
        kurbo::Affine::new([t.a, t.c, t.b, t.d, t.tx, t.ty])
    }

    fn apply_text_case(text: &str, text_case: &TextCase) -> String {
        match text_case {
            TextCase::Original => text.to_string(),
            TextCase::Upper | TextCase::SmallCaps | TextCase::SmallCapsForced => {
                text.to_uppercase()
            }
            TextCase::Lower => text.to_lowercase(),
            TextCase::Title => {
                let mut out = String::with_capacity(text.len());
                let mut new_word = true;
                for ch in text.chars() {
                    if ch.is_whitespace() {
                        new_word = true;
                        out.push(ch);
                    } else if new_word {
                        out.extend(ch.to_uppercase());
                        new_word = false;
                    } else {
                        out.push(ch);
                    }
                }
                out
            }
        }
    }
}
