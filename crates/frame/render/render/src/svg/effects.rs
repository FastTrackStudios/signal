use super::render::{
    apply_corner_radii_rect, fmt_f64, push_node, svg_color_paint_values, svg_linecap, svg_linejoin,
    svg_mix_blend_mode, to_u8,
};

/// Format a shadow color component (0.0–1.0) with Figma-compatible precision.
fn fmt_shadow_component(v: f64) -> String {
    let precision = if v.abs() >= 0.1 { 6 } else { 7 };
    let s = format!("{:.*}", precision, v.clamp(0.0, 1.0));
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Build an SVG filter element for drop shadows, inner shadows, and/or layer blur.
///
/// Returns `(filter_element, filter_id)`, or `None` if no effects require a filter.
fn build_effect_filter(
    root_id_suffix: &str,
    effect_count: &mut u64,
    effects: &[crate::anyrender_renderer::NodeEffect],
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Option<(svg::node::element::Filter, String)> {
    use crate::anyrender_renderer::EffectKind;

    let drop_shadows: Vec<&crate::anyrender_renderer::NodeEffect> = effects
        .iter()
        .filter(|effect| matches!(effect.kind, EffectKind::DropShadow | EffectKind::Glow))
        .collect();
    let inner_shadows: Vec<&crate::anyrender_renderer::NodeEffect> = effects
        .iter()
        .filter(|effect| matches!(effect.kind, EffectKind::InnerShadow))
        .collect();
    let layer_blur = effects
        .iter()
        .find(|effect| matches!(effect.kind, EffectKind::LayerBlur));

    if drop_shadows.is_empty() && inner_shadows.is_empty() && layer_blur.is_none() {
        return None;
    }

    // Build effect suffix for the filter ID.
    let mut effect_suffix = String::new();
    for _ in 0..drop_shadows.len() {
        effect_suffix.push('d');
    }
    for _ in 0..inner_shadows.len() {
        effect_suffix.push('i');
    }
    if layer_blur.is_some() {
        effect_suffix.push('f');
    }
    if effect_suffix.is_empty() {
        effect_suffix.push('f');
    }
    let filter_id = format!(
        "filter{}_{}_{}",
        *effect_count, effect_suffix, root_id_suffix
    );
    *effect_count += 1;

    // Compute filter bounds.
    let (mut min_x, mut min_y, mut max_x, mut max_y) = if drop_shadows.is_empty() {
        (x, y, x + width, y + height)
    } else {
        (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        )
    };
    for shadow in &drop_shadows {
        let radius = shadow.radius.max(0.0);
        let spread = shadow.spread.abs();
        let sx = x + shadow.offset_x - radius - spread;
        let sy = y + shadow.offset_y - radius - spread;
        let sw = width + (radius + spread) * 2.0;
        let sh = height + (radius + spread) * 2.0;
        min_x = min_x.min(sx);
        min_y = min_y.min(sy);
        max_x = max_x.max(sx + sw);
        max_y = max_y.max(sy + sh);
    }
    for shadow in &inner_shadows {
        let sx = x.min(x + shadow.offset_x);
        let sy = y.min(y + shadow.offset_y);
        let sw = width + shadow.offset_x.abs();
        let sh = height + shadow.offset_y.abs();
        min_x = min_x.min(sx);
        min_y = min_y.min(sy);
        max_x = max_x.max(sx + sw);
        max_y = max_y.max(sy + sh);
    }
    if let Some(blur) = layer_blur {
        let radius = blur.radius.max(0.0);
        min_x = min_x.min(x - radius);
        min_y = min_y.min(y - radius);
        max_x = max_x.max(x + width + radius);
        max_y = max_y.max(y + height + radius);
    }
    let filter_x = min_x.floor();
    let filter_y = min_y.floor();
    let filter_w = (max_x.ceil() - filter_x).max(0.0);
    let filter_h = (max_y.ceil() - filter_y).max(0.0);

    // Build filter primitives.
    let mut fe_flood = svg::node::element::Element::new("feFlood");
    svg::Node::assign(&mut fe_flood, "flood-opacity", "0");
    svg::Node::assign(&mut fe_flood, "result", "BackgroundImageFix");

    let mut filter = svg::node::element::Filter::new()
        .set("id", filter_id.clone())
        .set("x", fmt_f64(filter_x))
        .set("y", fmt_f64(filter_y))
        .set("width", fmt_f64(filter_w))
        .set("height", fmt_f64(filter_h))
        .set("filterUnits", "userSpaceOnUse")
        .set("color-interpolation-filters", "sRGB")
        .add(fe_flood);

    // Drop shadows.
    if !drop_shadows.is_empty() {
        for (idx, shadow) in drop_shadows.iter().enumerate() {
            let step = idx + 1;
            let radius = shadow.radius.max(0.0);
            let spread = shadow.spread;
            let std_deviation = (radius * 0.5).max(0.0);
            let prev_result = if step == 1 {
                "BackgroundImageFix".to_string()
            } else {
                format!("effect{}_dropShadow_{}", step - 1, root_id_suffix)
            };
            let this_result = format!("effect{}_dropShadow_{}", step, root_id_suffix);

            let mut fe_color_alpha = svg::node::element::Element::new("feColorMatrix");
            svg::Node::assign(&mut fe_color_alpha, "in", "SourceAlpha");
            svg::Node::assign(&mut fe_color_alpha, "type", "matrix");
            svg::Node::assign(
                &mut fe_color_alpha,
                "values",
                "0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0",
            );
            svg::Node::assign(&mut fe_color_alpha, "result", "hardAlpha");
            filter = filter.add(fe_color_alpha);

            if spread.abs() > 1e-6 {
                let mut fe_morphology = svg::node::element::Element::new("feMorphology");
                svg::Node::assign(&mut fe_morphology, "radius", fmt_f64(spread.abs()));
                svg::Node::assign(
                    &mut fe_morphology,
                    "operator",
                    if spread > 0.0 { "dilate" } else { "erode" },
                );
                svg::Node::assign(&mut fe_morphology, "in", "SourceAlpha");
                svg::Node::assign(&mut fe_morphology, "result", this_result.clone());
                filter = filter.add(fe_morphology);
            }

            let mut fe_offset = svg::node::element::Element::new("feOffset");
            if shadow.offset_x.abs() > f64::EPSILON {
                svg::Node::assign(&mut fe_offset, "dx", fmt_f64(shadow.offset_x));
            }
            if shadow.offset_y.abs() > f64::EPSILON {
                svg::Node::assign(&mut fe_offset, "dy", fmt_f64(shadow.offset_y));
            }
            filter = filter.add(fe_offset);

            if std_deviation > 1e-10 {
                let mut fe_blur = svg::node::element::Element::new("feGaussianBlur");
                svg::Node::assign(&mut fe_blur, "stdDeviation", fmt_f64(std_deviation));
                filter = filter.add(fe_blur);
            }

            // Figma clips each drop shadow against the original shape alpha
            // (feComposite "out") in two cases:
            // 1. When spread >= blur — the spread-dilated shadow overlaps the shape.
            // 2. When blur is very small (< 1px) — the shadow barely extends past
            //    the shape edge and would be visible under the shape without clipping.
            // For large blurs (>= 2px typically) with no spread, the shadow is
            // diffuse enough that the portion under the shape is negligible.
            if std_deviation <= spread.abs() || std_deviation < 1.0 {
                let mut fe_composite = svg::node::element::Element::new("feComposite");
                svg::Node::assign(&mut fe_composite, "in2", "hardAlpha");
                svg::Node::assign(&mut fe_composite, "operator", "out");
                filter = filter.add(fe_composite);
            }

            let mut fe_color = svg::node::element::Element::new("feColorMatrix");
            svg::Node::assign(&mut fe_color, "type", "matrix");
            svg::Node::assign(
                &mut fe_color,
                "values",
                format!(
                    "0 0 0 0 {} 0 0 0 0 {} 0 0 0 0 {} 0 0 0 {} 0",
                    fmt_shadow_component(shadow.color.r),
                    fmt_shadow_component(shadow.color.g),
                    fmt_shadow_component(shadow.color.b),
                    fmt_shadow_component(shadow.color.a)
                ),
            );
            filter = filter.add(fe_color);

            let mut fe_blend_shadow = svg::node::element::Element::new("feBlend");
            svg::Node::assign(&mut fe_blend_shadow, "mode", "normal");
            svg::Node::assign(&mut fe_blend_shadow, "in2", prev_result);
            svg::Node::assign(&mut fe_blend_shadow, "result", this_result);
            filter = filter.add(fe_blend_shadow);
        }

        let mut fe_blend_shape = svg::node::element::Element::new("feBlend");
        svg::Node::assign(&mut fe_blend_shape, "mode", "normal");
        svg::Node::assign(&mut fe_blend_shape, "in", "SourceGraphic");
        svg::Node::assign(
            &mut fe_blend_shape,
            "in2",
            format!("effect{}_dropShadow_{}", drop_shadows.len(), root_id_suffix),
        );
        svg::Node::assign(&mut fe_blend_shape, "result", "shape");
        filter = filter.add(fe_blend_shape);
    } else {
        let mut fe_blend = svg::node::element::Element::new("feBlend");
        svg::Node::assign(&mut fe_blend, "mode", "normal");
        svg::Node::assign(&mut fe_blend, "in", "SourceGraphic");
        svg::Node::assign(&mut fe_blend, "in2", "BackgroundImageFix");
        svg::Node::assign(&mut fe_blend, "result", "shape");
        filter = filter.add(fe_blend);
    }

    // Inner shadows.
    if !inner_shadows.is_empty() {
        let mut inner_base = "shape".to_string();
        for (idx, shadow) in inner_shadows.iter().enumerate() {
            let step = idx + 1;
            let std_deviation = (shadow.radius * 0.5).max(0.0);
            let this_result = format!("effect{}_innerShadow_{}", step, root_id_suffix);

            let mut fe_color_alpha = svg::node::element::Element::new("feColorMatrix");
            svg::Node::assign(&mut fe_color_alpha, "in", "SourceAlpha");
            svg::Node::assign(&mut fe_color_alpha, "type", "matrix");
            svg::Node::assign(
                &mut fe_color_alpha,
                "values",
                "0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0",
            );
            svg::Node::assign(&mut fe_color_alpha, "result", "hardAlpha");
            filter = filter.add(fe_color_alpha);

            if shadow.spread.abs() > 1e-6 {
                let mut fe_morphology = svg::node::element::Element::new("feMorphology");
                svg::Node::assign(&mut fe_morphology, "radius", fmt_f64(shadow.spread.abs()));
                // Inner shadow: positive spread erodes (contracts shadow inward),
                // negative spread dilates (expands shadow) — opposite of drop shadows.
                svg::Node::assign(
                    &mut fe_morphology,
                    "operator",
                    if shadow.spread > 0.0 {
                        "erode"
                    } else {
                        "dilate"
                    },
                );
                svg::Node::assign(&mut fe_morphology, "in", "SourceAlpha");
                svg::Node::assign(&mut fe_morphology, "result", this_result.clone());
                filter = filter.add(fe_morphology);
            }

            let mut fe_offset = svg::node::element::Element::new("feOffset");
            if shadow.offset_x.abs() > f64::EPSILON {
                svg::Node::assign(&mut fe_offset, "dx", fmt_f64(shadow.offset_x));
            }
            if shadow.offset_y.abs() > f64::EPSILON {
                svg::Node::assign(&mut fe_offset, "dy", fmt_f64(shadow.offset_y));
            }
            filter = filter.add(fe_offset);

            if std_deviation > 1e-10 {
                let mut fe_blur = svg::node::element::Element::new("feGaussianBlur");
                svg::Node::assign(&mut fe_blur, "stdDeviation", fmt_f64(std_deviation));
                filter = filter.add(fe_blur);
            }

            let mut fe_comp = svg::node::element::Element::new("feComposite");
            svg::Node::assign(&mut fe_comp, "in2", "hardAlpha");
            svg::Node::assign(&mut fe_comp, "operator", "arithmetic");
            svg::Node::assign(&mut fe_comp, "k2", "-1");
            svg::Node::assign(&mut fe_comp, "k3", "1");
            filter = filter.add(fe_comp);

            let mut fe_color = svg::node::element::Element::new("feColorMatrix");
            svg::Node::assign(&mut fe_color, "type", "matrix");
            svg::Node::assign(
                &mut fe_color,
                "values",
                format!(
                    "0 0 0 0 {} 0 0 0 0 {} 0 0 0 0 {} 0 0 0 {} 0",
                    fmt_shadow_component(shadow.color.r),
                    fmt_shadow_component(shadow.color.g),
                    fmt_shadow_component(shadow.color.b),
                    fmt_shadow_component(shadow.color.a)
                ),
            );
            filter = filter.add(fe_color);

            let mut fe_blend = svg::node::element::Element::new("feBlend");
            svg::Node::assign(&mut fe_blend, "mode", "normal");
            svg::Node::assign(&mut fe_blend, "in2", inner_base.clone());
            svg::Node::assign(&mut fe_blend, "result", this_result.clone());
            filter = filter.add(fe_blend);
            inner_base = this_result;
        }
    }

    // Layer blur.
    if let Some(effect) = layer_blur {
        let radius = effect.radius.max(0.0);
        let std_deviation = (radius * 0.5).max(0.0);
        let mut fe_gaussian_blur = svg::node::element::Element::new("feGaussianBlur");
        svg::Node::assign(
            &mut fe_gaussian_blur,
            "stdDeviation",
            fmt_f64(std_deviation),
        );
        svg::Node::assign(
            &mut fe_gaussian_blur,
            "result",
            format!(
                "effect{}_foregroundBlur_{root_id_suffix}",
                drop_shadows.len() + inner_shadows.len() + 1
            ),
        );
        filter = filter.add(fe_gaussian_blur);
    }

    Some((filter, filter_id))
}

pub(super) fn push_rect_with_effects(
    svg_doc: &mut svg::Document,
    group_stack: &mut Vec<svg::node::element::Group>,
    defs: &mut svg::node::element::Definitions,
    has_defs: &mut bool,
    root_id_suffix: &str,
    effect_count: &mut u64,
    bg_blur_count: &mut u64,
    rect: svg::node::element::Rectangle,
    stroke: Option<&crate::anyrender_renderer::StrokeStyle>,
    effects: &[crate::anyrender_renderer::NodeEffect],
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    corner_radii: &document_proto::CornerRadii,
    extra_rects: &[svg::node::element::Rectangle],
) {
    use crate::anyrender_renderer::EffectKind;

    if let Some((filter, filter_id)) =
        build_effect_filter(root_id_suffix, effect_count, effects, x, y, width, height)
    {
        *defs = defs.clone().add(filter);
        *has_defs = true;

        let drop_shadows: Vec<&crate::anyrender_renderer::NodeEffect> = effects
            .iter()
            .filter(|effect| matches!(effect.kind, EffectKind::DropShadow | EffectKind::Glow))
            .collect();

        let mut filtered =
            svg::node::element::Group::new().set("filter", format!("url(#{filter_id})"));
        if !drop_shadows.is_empty() {
            if let Some(stroke) = stroke.as_ref() {
                let rect_attrs = rect.to_string();
                let parse_attr = |name: &str| -> Option<String> {
                    let needle = format!("{name}=\"");
                    let start = rect_attrs.find(&needle)? + needle.len();
                    let tail = &rect_attrs[start..];
                    let end = tail.find('"')?;
                    Some(tail[..end].to_string())
                };
                let mut fill_rect = svg::node::element::Rectangle::new()
                    .set("x", fmt_f64(x))
                    .set("y", fmt_f64(y))
                    .set("width", fmt_f64(width))
                    .set("height", fmt_f64(height));
                fill_rect = apply_corner_radii_rect(fill_rect, corner_radii);
                if let Some(fill) = parse_attr("fill") {
                    fill_rect = fill_rect.set("fill", fill);
                }
                if let Some(fill_opacity) = parse_attr("fill-opacity") {
                    fill_rect = fill_rect.set("fill-opacity", fill_opacity);
                }
                if let Some(opacity) = parse_attr("opacity") {
                    fill_rect = fill_rect.set("opacity", opacity);
                }
                let inset = (stroke.width * 0.5).max(0.0);
                let mut stroke_rect = svg::node::element::Rectangle::new()
                    .set("x", fmt_f64(x + inset))
                    .set("y", fmt_f64(y + inset))
                    .set("width", fmt_f64((width - stroke.width).max(0.0)))
                    .set("height", fmt_f64((height - stroke.width).max(0.0)))
                    .set("stroke", {
                        let (hex, _) = svg_color_paint_values(
                            to_u8(stroke.color.r),
                            to_u8(stroke.color.g),
                            to_u8(stroke.color.b),
                            stroke.color.a,
                        );
                        hex
                    })
                    .set("stroke-width", fmt_f64(stroke.width.max(0.0)))
                    .set("stroke-linecap", svg_linecap(stroke.cap))
                    .set("stroke-linejoin", svg_linejoin(stroke.join));
                let (_, stroke_opacity) = svg_color_paint_values(
                    to_u8(stroke.color.r),
                    to_u8(stroke.color.g),
                    to_u8(stroke.color.b),
                    stroke.color.a,
                );
                if let Some(opacity) = stroke_opacity {
                    stroke_rect = stroke_rect.set("stroke-opacity", opacity);
                }
                let inset_radii = document_proto::CornerRadii {
                    top_left: (corner_radii.top_left - inset).max(0.0),
                    top_right: (corner_radii.top_right - inset).max(0.0),
                    bottom_right: (corner_radii.bottom_right - inset).max(0.0),
                    bottom_left: (corner_radii.bottom_left - inset).max(0.0),
                };
                stroke_rect = apply_corner_radii_rect(stroke_rect, &inset_radii);
                filtered = filtered.add(fill_rect).add(stroke_rect);
                for extra in extra_rects {
                    filtered = filtered.add(extra.clone());
                }
            } else {
                filtered = filtered.add(rect);
                for extra in extra_rects {
                    filtered = filtered.add(extra.clone());
                }
            }
        } else {
            filtered = filtered.add(rect);
            for extra in extra_rects {
                filtered = filtered.add(extra.clone());
            }
        }
        push_node(svg_doc, group_stack, filtered);
        return;
    }

    if let Some(effect) = effects
        .iter()
        .find(|effect| matches!(effect.kind, EffectKind::BackgroundBlur))
    {
        let radius = effect.radius.max(0.0);
        let blur = (radius * 0.5).max(0.0);
        let bx = x - radius;
        let by = y - radius;
        let bw = width + radius * 2.0;
        let bh = height + radius * 2.0;
        let clip_id = format!("bgblur_{}_{}_clip_path", *bg_blur_count, root_id_suffix);
        *bg_blur_count += 1;

        let mut clip_rect = svg::node::element::Rectangle::new()
            .set("x", fmt_f64(x))
            .set("y", fmt_f64(y))
            .set("width", fmt_f64(width))
            .set("height", fmt_f64(height));
        clip_rect = apply_corner_radii_rect(clip_rect, corner_radii);

        *defs = defs.clone().add(
            svg::node::element::ClipPath::new()
                .set("id", clip_id.clone())
                .set(
                    "transform",
                    format!("translate({} {})", fmt_f64(-bx), fmt_f64(-by)),
                )
                .add(clip_rect),
        );
        *has_defs = true;

        let mut foreign = svg::node::element::ForeignObject::new()
            .set("x", fmt_f64(bx))
            .set("y", fmt_f64(by))
            .set("width", fmt_f64(bw))
            .set("height", fmt_f64(bh));
        let mut div = svg::node::element::Element::new("div");
        svg::Node::assign(&mut div, "xmlns", "http://www.w3.org/1999/xhtml");
        svg::Node::assign(
            &mut div,
            "style",
            format!(
                "backdrop-filter:blur({}px);clip-path:url(#{clip_id});height:100%;width:100%",
                fmt_f64(blur)
            ),
        );
        foreign = foreign.add(div);
        push_node(svg_doc, group_stack, foreign);
        push_node(
            svg_doc,
            group_stack,
            rect.set("data-figma-bg-blur-radius", fmt_f64(radius)),
        );
        for extra in extra_rects {
            push_node(svg_doc, group_stack, extra.clone());
        }
        return;
    }

    push_node(svg_doc, group_stack, rect);
    for extra in extra_rects {
        push_node(svg_doc, group_stack, extra.clone());
    }
}

pub(super) fn push_group_with_blend_and_effects(
    svg_doc: &mut svg::Document,
    group_stack: &mut Vec<svg::node::element::Group>,
    defs: &mut svg::node::element::Definitions,
    has_defs: &mut bool,
    root_id_suffix: &str,
    effect_count: &mut u64,
    mut group: svg::node::element::Group,
    blend: &crate::anyrender_renderer::BlendMix,
    effects: &[crate::anyrender_renderer::NodeEffect],
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) {
    if let Some(mode) = svg_mix_blend_mode(blend) {
        group = group.set("style", format!("mix-blend-mode:{mode}"));
    }

    if let Some((filter, filter_id)) =
        build_effect_filter(root_id_suffix, effect_count, effects, x, y, width, height)
    {
        *defs = defs.clone().add(filter);
        *has_defs = true;
        push_node(
            svg_doc,
            group_stack,
            group.set("filter", format!("url(#{filter_id})")),
        );
        return;
    }

    push_node(svg_doc, group_stack, group);
}

#[cfg(test)]
mod tests {
    use crate::anyrender_renderer::{EffectKind, NodeEffect, Rgba};

    #[test]
    fn emission_glow_exports_as_a_gaussian_blur_filter() {
        let glow = NodeEffect {
            kind: EffectKind::Glow,
            color: Rgba {
                r: 1.0,
                g: 0.4,
                b: 0.1,
                a: 0.8,
            },
            offset_x: 0.0,
            offset_y: 0.0,
            radius: 12.0,
            spread: 0.0,
        };
        let mut count = 0;
        let (filter, _) =
            super::build_effect_filter("root", &mut count, &[glow], 10.0, 10.0, 20.0, 20.0)
                .expect("a filter");
        let svg = filter.to_string();
        assert!(
            svg.contains("feGaussianBlur") && svg.contains("stdDeviation=\"6\""),
            "{svg}"
        );
        assert!(!svg.contains("feOffset dx"), "no offset: {svg}");
    }
}
