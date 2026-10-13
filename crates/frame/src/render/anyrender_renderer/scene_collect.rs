//! `crate::document::proto::Scene` → paint primitives.
//!
//! The render semantics live here (opacity inheritance, compositing layers,
//! clip decisions, sibling masks, instance fallback); the scene only carries
//! authored values.

use std::collections::HashMap;
use std::sync::Arc;

use crate::document::proto::{self as doc, NodeId, NodeKind, Scene};
use base64::Engine as _;
use taffy::TaffyTree;
use taffy::prelude::*;

use super::effects_plan::{EffectsPlan, EmissiveDraw, ShaderRegion};
use super::transform::AffineTransform2D;
use super::types::*;

/// The nearest control, `id` or an ancestor, with its default position.
fn nearest_control(scene: &Scene, id: NodeId) -> Option<(NodeId, Option<f64>)> {
    let mut at = Some(id);
    while let Some(n) = at.and_then(|i| scene.node(i)) {
        if let Some(c) = &n.control {
            return Some((n.id, c.default));
        }
        at = n.parent;
    }
    None
}

// ── value conversion ─────────────────────────────────────────────────────

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// A paint color with the paint's opacity folded into alpha.
fn rgba(c: &doc::Color, paint_opacity: f64) -> Rgba {
    Rgba {
        r: clamp01(c.r),
        g: clamp01(c.g),
        b: clamp01(c.b),
        a: clamp01(clamp01(c.a) * clamp01(paint_opacity)),
    }
}

fn affine(t: &doc::Transform) -> AffineTransform2D {
    AffineTransform2D::from([[t.a, t.b, t.tx], [t.c, t.d, t.ty]])
}

pub(super) fn blend_mix(b: doc::BlendMode) -> BlendMix {
    use doc::BlendMode as B;
    match b {
        B::Multiply => BlendMix::Multiply,
        B::Screen => BlendMix::Screen,
        B::Overlay => BlendMix::Overlay,
        B::Darken => BlendMix::Darken,
        B::Lighten => BlendMix::Lighten,
        B::ColorDodge => BlendMix::ColorDodge,
        B::ColorBurn => BlendMix::ColorBurn,
        B::HardLight => BlendMix::HardLight,
        B::SoftLight => BlendMix::SoftLight,
        B::Difference => BlendMix::Difference,
        B::Exclusion => BlendMix::Exclusion,
        B::Hue => BlendMix::Hue,
        B::Saturation => BlendMix::Saturation,
        B::Color => BlendMix::Color,
        B::Luminosity => BlendMix::Luminosity,
        B::Normal | B::PassThrough | B::LinearBurn | B::LinearDodge => BlendMix::Normal,
    }
}

fn stops(stops: &[doc::GradientStop], paint_opacity: f64) -> Vec<GradientStop> {
    stops
        .iter()
        .map(|s| GradientStop {
            offset: clamp01(s.position),
            color: rgba(&s.color, paint_opacity),
        })
        .collect()
}

pub(crate) fn fill_paint(p: &doc::Paint, scene: &Scene) -> Option<FillPaint> {
    Some(match &p.kind {
        doc::PaintKind::Solid { color } => FillPaint::Solid(rgba(color, p.opacity)),
        doc::PaintKind::LinearGradient {
            stops: s,
            transform,
        } => FillPaint::GradientLinear {
            transform: transform.as_ref().map(affine),
            stops: stops(s, p.opacity),
        },
        doc::PaintKind::RadialGradient {
            stops: s,
            transform,
        } => FillPaint::GradientRadial {
            transform: transform.as_ref().map(affine),
            stops: stops(s, p.opacity),
        },
        doc::PaintKind::AngularGradient {
            stops: s,
            transform,
        } => FillPaint::GradientAngular {
            transform: transform.as_ref().map(affine),
            stops: stops(s, p.opacity),
        },
        doc::PaintKind::Image {
            asset,
            source_ref,
            scale_mode,
            transform,
        } => FillPaint::Image {
            image_hash: source_ref.clone(),
            data_base64: asset_base64(scene, asset),
            alpha: clamp01(p.opacity),
            scale_mode: match scale_mode {
                doc::ImageScaleMode::Fill => ImageScaleMode::Fill,
                doc::ImageScaleMode::Fit => ImageScaleMode::Fit,
                doc::ImageScaleMode::Crop => ImageScaleMode::Crop,
                doc::ImageScaleMode::Tile => ImageScaleMode::Tile,
                doc::ImageScaleMode::Stretch => ImageScaleMode::Stretch,
            },
            image_transform: transform.as_ref().map(affine),
        },
    })
}

/// Visible paints, in stack order.
pub(super) fn fill_paints(paints: &[doc::Paint], scene: &Scene) -> Vec<(FillPaint, BlendMix)> {
    paints
        .iter()
        .filter(|p| p.visible)
        .filter_map(|p| fill_paint(p, scene).map(|f| (f, blend_mix(p.blend))))
        .collect()
}

pub(super) fn first_color(fill: &FillPaint) -> Option<Rgba> {
    match fill {
        FillPaint::Solid(c) => Some(c.clone()),
        FillPaint::GradientLinear { stops, .. }
        | FillPaint::GradientRadial { stops, .. }
        | FillPaint::GradientAngular { stops, .. } => stops.first().map(|s| s.color.clone()),
        FillPaint::Image { .. } => None,
    }
}

/// The drawn stroke: the first visible solid or gradient paint, when the
/// weight is positive. (Image strokes aren't drawn.)
pub(super) fn stroke_style(s: &doc::Stroke, opacity: f64) -> Option<StrokeStyle> {
    let (color, gradient) = s
        .paints
        .iter()
        .filter(|p| p.visible)
        .find_map(|p| match &p.kind {
            doc::PaintKind::Solid { color } => Some((rgba(color, p.opacity), None)),
            doc::PaintKind::LinearGradient {
                stops: s,
                transform,
            } => gradient_stroke(FillPaint::GradientLinear {
                transform: transform.as_ref().map(affine),
                stops: stops(s, p.opacity),
            }),
            doc::PaintKind::RadialGradient {
                stops: s,
                transform,
            } => gradient_stroke(FillPaint::GradientRadial {
                transform: transform.as_ref().map(affine),
                stops: stops(s, p.opacity),
            }),
            doc::PaintKind::AngularGradient {
                stops: s,
                transform,
            } => gradient_stroke(FillPaint::GradientAngular {
                transform: transform.as_ref().map(affine),
                stops: stops(s, p.opacity),
            }),
            doc::PaintKind::Image { .. } => None,
        })?;
    let width = s.weight.max(0.0);
    if width <= 0.0 {
        return None;
    }
    Some(StrokeStyle {
        color: color.with_opacity(opacity),
        gradient: gradient.map(|g| g.with_opacity(opacity)),
        width,
        cap: s.cap,
        join: s.join,
        align: s.align,
        miter_limit: s.miter_limit.max(1.0),
        dash_pattern: s
            .dash_pattern
            .iter()
            .copied()
            .filter(|v| *v > 0.0)
            .collect(),
        dash_offset: s.dash_offset,
    })
}

/// A gradient stroke paint with its first stop as the stroke's colour;
/// `None` without stops.
fn gradient_stroke(g: FillPaint) -> Option<(Rgba, Option<FillPaint>)> {
    Some((first_color(&g)?, Some(g)))
}

pub(super) fn node_effects(effects: &[doc::Effect], opacity: f64) -> Vec<NodeEffect> {
    effects
        .iter()
        .filter(|e| e.visible)
        .map(|e| NodeEffect {
            kind: match e.kind {
                doc::EffectKind::DropShadow => EffectKind::DropShadow,
                doc::EffectKind::InnerShadow => EffectKind::InnerShadow,
                doc::EffectKind::LayerBlur => EffectKind::LayerBlur,
                doc::EffectKind::BackgroundBlur => EffectKind::BackgroundBlur,
            },
            color: Rgba {
                r: e.color.r,
                g: e.color.g,
                b: e.color.b,
                a: e.color.a,
            }
            .with_opacity(opacity),
            offset_x: e.offset_x,
            offset_y: e.offset_y,
            radius: e.radius.max(0.0),
            spread: e.spread,
        })
        .collect()
}

fn text_case(c: doc::TextCase) -> TextCase {
    match c {
        doc::TextCase::Original => TextCase::Original,
        doc::TextCase::Upper => TextCase::Upper,
        doc::TextCase::Lower => TextCase::Lower,
        doc::TextCase::Title => TextCase::Title,
        doc::TextCase::SmallCaps => TextCase::SmallCaps,
        doc::TextCase::SmallCapsForced => TextCase::SmallCapsForced,
    }
}

/// A text node's font settings.
#[must_use]
pub fn text_font(text: &doc::TextData) -> FontSpec {
    FontSpec {
        family: text.font_family.clone(),
        weight: text.font_weight,
        style: text.font_style.clone(),
        width: text.font_width,
    }
}

fn text_style_runs(text: &doc::TextData, opacity: f64) -> Vec<TextStyleRun> {
    text.runs
        .iter()
        .filter(|r| !r.text.is_empty())
        .map(|r| TextStyleRun {
            text: r.text.clone(),
            // The first solid paint of the run, visible or not (matches the
            // exporter's own run color).
            color: r.fills.iter().find_map(|p| match &p.kind {
                doc::PaintKind::Solid { color } => {
                    Some(rgba(color, p.opacity).with_opacity(opacity))
                }
                _ => None,
            }),
            font_size: r.font_size,
            letter_spacing: r.letter_spacing,
            font_style: r.font_style.clone(),
            font_weight: r.font_weight,
            font_family: r.font_family.clone(),
            font_width: r.font_width,
        })
        .collect()
}

/// One fill per visibly-distinct run, for painting text outline glyph groups.
fn text_outline_fills(
    text: &doc::TextData,
    scene: &Scene,
    opacity: f64,
) -> Vec<(FillPaint, BlendMix)> {
    let mut runs: Vec<(FillPaint, BlendMix)> = Vec::new();
    for run in &text.runs {
        if run.text.trim().is_empty() {
            continue;
        }
        let Some((fill, blend)) = fill_paints(&run.fills, scene)
            .into_iter()
            .map(|(f, b)| (f.with_opacity(opacity), b))
            .next()
        else {
            continue;
        };
        if runs
            .last()
            .is_none_or(|(lf, lb)| *lb != blend || *lf != fill)
        {
            runs.push((fill, blend));
        }
    }
    runs
}

fn snapshot_base64(node: &doc::Node, scene: &Scene) -> Option<Arc<str>> {
    let id = node.svg_snapshot.as_ref()?;
    scene.asset(id)?;
    Some(asset_base64(scene, id))
}

/// Cached encodings kept before the cache is reset, in base64 bytes.
const BASE64_CACHE_BYTES: usize = 256 << 20;

/// An asset's bytes, base64-encoded, encoded once and shared across
/// collects (and frames): images and snapshots are carried to the painters
/// and the SVG exporter in base64, and encoding a large texture on every
/// collect was most of a textured page's collect time. Keyed by the asset
/// id with the bytes' length and a sample of them, so an id reused for
/// other bytes re-encodes. A missing asset encodes as empty.
pub(crate) fn asset_base64(scene: &Scene, id: &doc::AssetId) -> Arc<str> {
    use std::hash::{Hash, Hasher};
    use std::sync::{LazyLock, Mutex};
    type Cache = (HashMap<(String, u64), Arc<str>>, usize);
    static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new((HashMap::new(), 0)));

    let bytes = scene
        .asset(id)
        .map(|a| a.bytes.as_slice())
        .unwrap_or_default();
    let fingerprint = {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.len().hash(&mut h);
        bytes[..bytes.len().min(256)].hash(&mut h);
        bytes[bytes.len().saturating_sub(256)..].hash(&mut h);
        h.finish()
    };
    let key = (id.0.clone(), fingerprint);
    if let Some(hit) = CACHE.lock().ok().and_then(|c| c.0.get(&key).cloned()) {
        return hit;
    }
    let encoded: Arc<str> = base64::engine::general_purpose::STANDARD
        .encode(bytes)
        .into();
    let Ok(mut c) = CACHE.lock() else {
        return encoded;
    };
    if let Some(raced) = c.0.get(&key) {
        // Another thread encoded it meanwhile: share theirs.
        return Arc::clone(raced);
    }
    if c.1 + encoded.len() > BASE64_CACHE_BYTES {
        *c = (HashMap::new(), 0);
    }
    c.1 += encoded.len();
    c.0.insert(key, Arc::clone(&encoded));
    encoded
}

enum Class {
    Text,
    Shape,
    Other,
}

fn class(kind: &NodeKind) -> Class {
    match kind {
        NodeKind::Text(_) => Class::Text,
        NodeKind::Rectangle
        | NodeKind::Ellipse
        | NodeKind::Line
        | NodeKind::Polygon
        | NodeKind::Star
        | NodeKind::Vector
        | NodeKind::BooleanOperation { .. } => Class::Shape,
        _ => Class::Other,
    }
}

fn local_rect(node: &doc::Node) -> (f64, f64, f64, f64) {
    (
        node.transform.tx,
        node.transform.ty,
        node.size.width,
        node.size.height,
    )
}

/// Whether `t`'s 2×2 linear part is (within `EPS`) the identity: a pure
/// translation, which positions children but doesn't transform them.
fn is_translation(t: &doc::Transform) -> bool {
    const EPS: f64 = 1e-9;
    (t.a - 1.0).abs() <= EPS && t.b.abs() <= EPS && t.c.abs() <= EPS && (t.d - 1.0).abs() <= EPS
}

/// Whether `node` is drawn as authored rather than as a Figma import (see
/// `authored` on [`PaintPrimitive::Path`]).
fn is_authored(node: &doc::Node) -> bool {
    node.source.as_ref().is_none_or(|s| s.tool != "figma")
}

/// Node-local bounds `(x0, y0, x1, y1)` of an authored node's geometry,
/// stroke included; `None` for Figma imports and nodes without geometry.
fn authored_geometry_bounds(node: &doc::Node) -> Option<(f64, f64, f64, f64)> {
    if !is_authored(node) || (node.fill_geometry.is_empty() && node.stroke_geometry.is_empty()) {
        return None;
    }
    let b = node
        .fill_geometry
        .iter()
        .chain(&node.stroke_geometry)
        .filter_map(cached_path_bounds)
        .filter(|r| r.x0.is_finite() && r.y0.is_finite() && r.x1.is_finite() && r.y1.is_finite())
        .reduce(|a, b| a.union(b))?;
    let stroked = node.stroke.paints.iter().any(|p| p.visible) && node.stroke_geometry.is_empty();
    let pad = if stroked {
        node.stroke.weight.max(0.0) * node.stroke.miter_limit.max(1.0) * 0.5
    } else {
        0.0
    };
    Some((b.x0 - pad, b.y0 - pad, b.x1 + pad, b.y1 + pad))
}

/// Whether a node's transform applies to its children.
///
/// Groups and boolean operations are transparent: like Figma's, their
/// children are authored in the enclosing container's space and already carry
/// the group's transform, so propagating it would apply it twice.
fn propagates_transform(node: &doc::Node) -> bool {
    !matches!(
        node.kind,
        NodeKind::Group | NodeKind::BooleanOperation { .. }
    ) && !is_translation(&node.transform)
}

/// The group matrix for a node whose box origin sits at `(x, y)` in
/// flattened scene space: its linear part, pivoted about its origin
/// (`T(x, y) · L · T(-x, -y)`).
fn group_transform(t: &doc::Transform, x: f64, y: f64) -> AffineTransform2D {
    AffineTransform2D {
        a: t.a,
        b: t.b,
        tx: x - (t.a * x + t.b * y),
        c: t.c,
        d: t.d,
        ty: y - (t.c * x + t.d * y),
    }
}

// ── layout ───────────────────────────────────────────────────────────────

/// Absolute boxes for `root` and its subtree, running auto layout.
///
/// Boxes are axis-aligned in *flattened* scene space: each child sits at its
/// parent's box origin plus its own offset, in the parent's **unrotated**
/// frame. Ancestor rotation/scale/skew is not applied (the painters apply it
/// through [`PaintPrimitive::GroupStart`]); a hit-tester over a transformed
/// container must map the pointer through the inverse of the containers'
/// transforms first.
#[must_use]
pub fn build_scene_layout_boxes(scene: &Scene, root: NodeId) -> HashMap<NodeId, LayoutBox> {
    let mut out = HashMap::new();
    let (x, y, w, h) = scene.node(root).map_or((0.0, 0.0, 0.0, 0.0), local_rect);
    layout_recursive(scene, root, x, y, w, h, &mut out);
    out
}

fn layout_recursive(
    scene: &Scene,
    id: NodeId,
    ax: f64,
    ay: f64,
    w: f64,
    h: f64,
    out: &mut HashMap<NodeId, LayoutBox>,
) {
    out.insert(
        id,
        LayoutBox {
            x: ax,
            y: ay,
            width: w,
            height: h,
        },
    );
    let Some(node) = scene.node(id) else { return };
    if node.children.is_empty() {
        return;
    }
    let free = |out: &mut HashMap<NodeId, LayoutBox>| {
        for c in &node.children {
            let (cx, cy, cw, ch) = scene.node(*c).map_or((0.0, 0.0, 0.0, 0.0), local_rect);
            layout_recursive(scene, *c, ax + cx, ay + cy, cw, ch, out);
        }
    };
    let Some(al) = &node.layout else {
        free(out);
        return;
    };

    let mut taffy = TaffyTree::<()>::new();
    let mut kids = Vec::new();
    for c in &node.children {
        if let Ok(t) = taffy.new_leaf(child_style(scene, *c)) {
            kids.push((*c, t));
        }
    }
    let ids: Vec<_> = kids.iter().map(|(_, t)| *t).collect();
    let Ok(container) = taffy.new_with_children(container_style(al, w, h), &ids) else {
        free(out);
        return;
    };
    let _ = taffy.compute_layout(
        container,
        taffy::Size {
            width: AvailableSpace::Definite(w as f32),
            height: AvailableSpace::Definite(h as f32),
        },
    );
    for (c, t) in kids {
        if let Ok(l) = taffy.layout(t) {
            layout_recursive(
                scene,
                c,
                ax + f64::from(l.location.x),
                ay + f64::from(l.location.y),
                f64::from(l.size.width),
                f64::from(l.size.height),
                out,
            );
        } else {
            let (cx, cy, cw, ch) = scene.node(c).map_or((0.0, 0.0, 0.0, 0.0), local_rect);
            layout_recursive(scene, c, ax + cx, ay + cy, cw, ch, out);
        }
    }
}

fn container_style(al: &doc::AutoLayout, w: f64, h: f64) -> Style {
    let row = matches!(al.direction, doc::LayoutDirection::Horizontal);
    Style {
        display: Display::Flex,
        size: taffy::Size {
            width: Dimension::Length(w as f32),
            height: Dimension::Length(h as f32),
        },
        flex_direction: if row {
            FlexDirection::Row
        } else {
            FlexDirection::Column
        },
        gap: if row {
            taffy::Size {
                width: LengthPercentage::Length(al.gap as f32),
                height: LengthPercentage::Length(0.0),
            }
        } else {
            taffy::Size {
                width: LengthPercentage::Length(0.0),
                height: LengthPercentage::Length(al.gap as f32),
            }
        },
        padding: taffy::Rect {
            left: LengthPercentage::Length(al.padding_left as f32),
            right: LengthPercentage::Length(al.padding_right as f32),
            top: LengthPercentage::Length(al.padding_top as f32),
            bottom: LengthPercentage::Length(al.padding_bottom as f32),
        },
        flex_wrap: if al.wrap {
            FlexWrap::Wrap
        } else {
            FlexWrap::NoWrap
        },
        justify_content: al.main_align.map(|a| match a {
            doc::MainAxisAlign::Start => JustifyContent::Start,
            doc::MainAxisAlign::Center => JustifyContent::Center,
            doc::MainAxisAlign::End => JustifyContent::End,
            doc::MainAxisAlign::SpaceBetween => JustifyContent::SpaceBetween,
        }),
        align_items: al.cross_align.map(|a| match a {
            doc::CrossAxisAlign::Start => AlignItems::Start,
            doc::CrossAxisAlign::Center => AlignItems::Center,
            doc::CrossAxisAlign::End => AlignItems::End,
            doc::CrossAxisAlign::Baseline => AlignItems::Baseline,
        }),
        ..Default::default()
    }
}

fn child_style(scene: &Scene, id: NodeId) -> Style {
    let Some(node) = scene.node(id) else {
        return Style::default();
    };
    let (x, y, w, h) = local_rect(node);
    let lc = &node.layout_child;
    let mut style = Style {
        size: taffy::Size {
            width: Dimension::Length(w as f32),
            height: Dimension::Length(h as f32),
        },
        // Children keep their authored size; auto layout never shrinks them.
        flex_shrink: 0.0,
        min_size: taffy::Size {
            width: Dimension::Length(lc.min_width.unwrap_or(0.0) as f32),
            height: Dimension::Length(lc.min_height.unwrap_or(0.0) as f32),
        },
        max_size: taffy::Size {
            width: lc
                .max_width
                .map_or(Dimension::Auto, |v| Dimension::Length(v as f32)),
            height: lc
                .max_height
                .map_or(Dimension::Auto, |v| Dimension::Length(v as f32)),
        },
        align_self: lc.align_self.map(|a| match a {
            doc::AlignSelf::Start => AlignSelf::Start,
            doc::AlignSelf::Center => AlignSelf::Center,
            doc::AlignSelf::End => AlignSelf::End,
            doc::AlignSelf::Stretch => AlignSelf::Stretch,
        }),
        ..Default::default()
    };
    if let Some(g) = lc.grow {
        style.flex_grow = g as f32;
    }
    // A hidden child leaves the flow, as in Figma: the row closes up
    // behind it. A theme hides a strip's pan section at narrow widths
    // and the controls under it move up into the room.
    if !node.visible {
        style.display = Display::None;
    }
    if lc.absolute {
        style.position = Position::Absolute;
        style.inset = taffy::Rect {
            left: LengthPercentageAuto::Length(x as f32),
            right: LengthPercentageAuto::Auto,
            top: LengthPercentageAuto::Length(y as f32),
            bottom: LengthPercentageAuto::Auto,
        };
    }
    style
}

// ── primitives ───────────────────────────────────────────────────────────

/// How [`build_scene_primitives_with`] treats the optional light/shader
/// properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectOptions {
    /// Draw each `emission` as a [`EffectKind::Glow`] on the node's own
    /// primitive (the fallback for paths without the GPU effects stage).
    pub emission_glow: bool,
}

impl Default for CollectOptions {
    fn default() -> Self {
        Self {
            emission_glow: true,
        }
    }
}

/// The paint primitives for `root`'s subtree, in paint order.
///
/// Emitting nodes carry a [`EffectKind::Glow`] (the no-GPU fallback for
/// light); shaders are skipped. Scenes without `emission` are unaffected.
#[must_use]
pub fn build_scene_primitives(scene: &Scene, root: NodeId) -> Vec<PaintPrimitive> {
    build_scene_primitives_with(scene, root, CollectOptions::default())
}

/// [`build_scene_primitives`] with explicit [`CollectOptions`].
#[must_use]
pub fn build_scene_primitives_with(
    scene: &Scene,
    root: NodeId,
    options: CollectOptions,
) -> Vec<PaintPrimitive> {
    collect(scene, root, options, false).0
}

/// The primitives (without emission glows) and the [`EffectsPlan`] the GPU
/// effects stage runs over them. The primitives are exactly
/// `build_scene_primitives_with(.., CollectOptions { emission_glow: false })`.
#[must_use]
pub fn build_scene_primitives_and_effects(
    scene: &Scene,
    root: NodeId,
) -> (Vec<PaintPrimitive>, EffectsPlan) {
    let (out, plan) = collect(
        scene,
        root,
        CollectOptions {
            emission_glow: false,
        },
        true,
    );
    (out, plan.unwrap_or_default())
}

fn collect(
    scene: &Scene,
    root: NodeId,
    options: CollectOptions,
    plan: bool,
) -> (Vec<PaintPrimitive>, Option<EffectsPlan>) {
    let boxes = build_scene_layout_boxes(scene, root);
    let mut out = Vec::new();
    let mut cx = Cx {
        scene,
        boxes: &boxes,
        out: &mut out,
        options,
        plan: plan.then(EffectsPlan::default),
        groups: Vec::new(),
        clips: Vec::new(),
        in_shader: false,
        in_mask_def: 0,
        flicker: None,
    };
    cx.node(root, 0.0, 0.0, false, 1.0, &[]);
    let plan = cx.plan.take();
    (out, plan)
}

struct Cx<'a> {
    scene: &'a Scene,
    boxes: &'a HashMap<NodeId, LayoutBox>,
    out: &'a mut Vec<PaintPrimitive>,
    options: CollectOptions,
    /// Collected effects, when the caller runs the GPU stage.
    plan: Option<EffectsPlan>,
    /// Open `GroupStart` transforms, each composed with the ones before.
    groups: Vec<AffineTransform2D>,
    /// Open `ClipStart`s with the group transform each was opened under.
    clips: Vec<(AffineTransform2D, PaintPrimitive)>,
    /// Inside a shader region (nested shaders draw plainly).
    in_shader: bool,
    /// Inside a mask definition (no effects there).
    in_mask_def: usize,
    /// The params of the nearest enclosing `flicker` shader.
    flicker: Option<std::collections::BTreeMap<String, f64>>,
}

/// A copy of a node's own primitive painted in `color`: fills become one
/// solid fill, strokes take the colour, effects and blends are dropped.
fn recolor(p: &PaintPrimitive, color: &Rgba) -> Option<PaintPrimitive> {
    let solid = || vec![(FillPaint::Solid(color.clone()), BlendMix::Normal)];
    let stroke_of = |s: &Option<StrokeStyle>| {
        s.clone().map(|s| StrokeStyle {
            color: color.clone(),
            gradient: None,
            ..s
        })
    };
    match p {
        PaintPrimitive::Rect {
            node_id,
            x,
            y,
            width,
            height,
            fills,
            stroke,
            corner_radii,
            rotation,
            authored,
            ..
        } => (!fills.is_empty() || stroke.is_some()).then(|| PaintPrimitive::Rect {
            node_id: *node_id,
            x: *x,
            y: *y,
            width: *width,
            height: *height,
            fills: if fills.is_empty() {
                Vec::new()
            } else {
                solid()
            },
            stroke: stroke_of(stroke),
            corner_radii: *corner_radii,
            effects: Vec::new(),
            blend: BlendMix::Normal,
            rotation: *rotation,
            authored: *authored,
        }),
        PaintPrimitive::Path {
            node_id,
            x,
            y,
            width,
            height,
            normalize_to_origin,
            text_outline,
            fill_paths,
            stroke_paths,
            fills,
            stroke,
            rotation,
            authored,
            ..
        } => {
            // A snapshot-only path can't be recoloured; the caller falls back
            // to the node's box.
            (!fill_paths.is_empty() || !stroke_paths.is_empty()).then(|| PaintPrimitive::Path {
                node_id: *node_id,
                x: *x,
                y: *y,
                width: *width,
                height: *height,
                normalize_to_origin: *normalize_to_origin,
                text_outline: *text_outline,
                // A stroke with no stroke geometry strokes the fill geometry
                // (an open line): keep it so the light follows the line.
                fill_paths: if fills.is_empty() && (stroke.is_none() || !stroke_paths.is_empty()) {
                    Vec::new()
                } else {
                    fill_paths.clone()
                },
                stroke_paths: if stroke.is_some() {
                    stroke_paths.clone()
                } else {
                    Vec::new()
                },
                svg_base64: None,
                fills: if fills.is_empty() {
                    Vec::new()
                } else {
                    solid()
                },
                stroke: stroke_of(stroke),
                effects: Vec::new(),
                blend: BlendMix::Normal,
                rotation: *rotation,
                authored: *authored,
            })
        }
        PaintPrimitive::Text { .. } => {
            let mut t = p.clone();
            if let PaintPrimitive::Text {
                color: c,
                style_runs,
                blend,
                effects,
                ..
            } = &mut t
            {
                *c = color.clone();
                *blend = BlendMix::Normal;
                effects.clear();
                for r in style_runs {
                    r.color = Some(color.clone());
                }
            }
            Some(t)
        }
        _ => None,
    }
}

impl Cx<'_> {
    fn child_box(&self, child: NodeId, px: f64, py: f64) -> (f64, f64, f64, f64) {
        self.boxes.get(&child).map_or_else(
            || {
                let (lx, ly, w, h) = self
                    .scene
                    .node(child)
                    .map_or((0.0, 0.0, 0.0, 0.0), local_rect);
                (px + lx, py + ly, w, h)
            },
            |b| (b.x, b.y, b.width, b.height),
        )
    }

    #[allow(clippy::too_many_lines)]
    fn node(
        &mut self,
        node_id: NodeId,
        parent_x: f64,
        parent_y: f64,
        force_local: bool,
        inherited_opacity: f64,
        inherited_blurs: &[NodeEffect],
    ) {
        let scene = self.scene;
        let Some(node) = scene.node(node_id) else {
            return;
        };
        if !node.visible {
            return;
        }
        let effective_opacity = clamp01(inherited_opacity * clamp01(node.opacity));
        let (lx, ly, lw, lh) = local_rect(node);
        let (chain_x, chain_y) = (parent_x + lx, parent_y + ly);
        let (x, y, width, height) = if force_local {
            (chain_x, chain_y, lw, lh)
        } else {
            self.boxes
                .get(&node_id)
                .map_or((chain_x, chain_y, lw, lh), |b| {
                    (b.x, b.y, b.width, b.height)
                })
        };

        let mut radii = node.corner_radii;
        if matches!(node.kind, NodeKind::Ellipse) && radii.is_zero() {
            radii = doc::CornerRadii::uniform(width.min(height) * 0.5);
        }
        let blend = blend_mix(node.blend);
        let has_filter_effects = node.effects.iter().any(|e| {
            e.visible
                && matches!(
                    e.kind,
                    doc::EffectKind::DropShadow
                        | doc::EffectKind::InnerShadow
                        | doc::EffectKind::LayerBlur
                )
        });
        // Non-normal blends, and filtered nodes with partial opacity, composite
        // through an explicit layer that carries the opacity.
        let needs_layer = (blend != BlendMix::Normal
            || (has_filter_effects && effective_opacity < 1.0))
            && width > 0.0
            && height > 0.0;
        let content_opacity = if needs_layer { 1.0 } else { effective_opacity };
        let primitive_blend = if needs_layer {
            BlendMix::Normal
        } else {
            blend.clone()
        };

        let fills: Vec<(FillPaint, BlendMix)> = fill_paints(&node.fills, scene)
            .into_iter()
            .map(|(f, b)| (f.with_opacity(content_opacity), b))
            .collect();
        let stroke = stroke_style(&node.stroke, content_opacity);
        let mut effects = node_effects(&node.effects, content_opacity);
        if self.options.emission_glow
            && let Some(em) = &node.emission
            && em.radius > 0.0
            && em.intensity > 0.0
        {
            effects.push(NodeEffect {
                kind: EffectKind::Glow,
                color: rgba(&em.color, 1.0)
                    .with_opacity(clamp01(content_opacity * em.intensity * 0.85)),
                offset_x: 0.0,
                offset_y: 0.0,
                radius: em.radius,
                spread: 0.0,
            });
        }
        let mut merged_effects = inherited_blurs.to_vec();
        merged_effects.extend(effects.iter().cloned());
        let mut child_blurs = inherited_blurs.to_vec();
        child_blurs.extend(
            effects
                .iter()
                .filter(|e| matches!(e.kind, EffectKind::LayerBlur))
                .cloned(),
        );

        let fill_paths = node.fill_geometry.clone();
        let stroke_paths = node.stroke_geometry.clone();
        let svg_base64 = snapshot_base64(node, scene);
        let has_native_geometry = !fill_paths.is_empty() || !stroke_paths.is_empty();
        let has_vector_geometry = has_native_geometry || svg_base64.is_some();
        let instance_component = match &node.kind {
            NodeKind::Instance { component } => Some(*component),
            _ => None,
        };
        let is_boolean = matches!(node.kind, NodeKind::BooleanOperation { .. });
        // Figma imports keep the exporter-compatible drawing parity is
        // measured against; everything else is drawn as authored.
        let authored = is_authored(node);

        // A container with a non-translation transform paints itself and its
        // subtree inside a group carrying that transform, so children inherit
        // it. Leaves keep their own `rotation` (no group, unchanged output).
        let has_descendants = !node.children.is_empty()
            || matches!(instance_component, Some(Some(main)) if scene.node(main).is_some_and(|m| !m.children.is_empty()));
        let group = (has_descendants && propagates_transform(node))
            .then(|| group_transform(&node.transform, x, y));
        let rotation = if group.is_some() {
            0.0
        } else {
            node.transform.rotation()
        };

        // Effects bookkeeping (only when a plan is collected).
        let region_start = self.out.len();
        let outer_world = self
            .groups
            .last()
            .copied()
            .unwrap_or_else(AffineTransform2D::identity);
        let shader_here = self.plan.is_some()
            && !self.in_shader
            && self.in_mask_def == 0
            && node.shader.as_ref().is_some_and(|s| !s.shader.is_empty());
        let saved_flicker = self.flicker.clone();
        if let Some(fx) = node.shader.as_ref().filter(|s| s.shader == "flicker") {
            self.flicker = Some(fx.params.clone());
        }
        if shader_here {
            self.in_shader = true;
        }

        if let Some(transform) = group {
            self.out
                .push(PaintPrimitive::GroupStart { node_id, transform });
            self.groups.push(outer_world * transform);
        }
        if needs_layer {
            self.out.push(PaintPrimitive::LayerStart {
                node_id,
                x,
                y,
                width,
                height,
                blend: blend.clone(),
                opacity: effective_opacity,
            });
        }

        let child_overflows = node.clips_content
            && width > 0.0
            && height > 0.0
            && node.children.iter().any(|c| {
                const EPS: f64 = 1e-3;
                let (cx, cy, cw, ch) = self.child_box(*c, x, y);
                let outside = |(x0, y0, x1, y1): (f64, f64, f64, f64)| {
                    x0 < x - EPS || y0 < y - EPS || x1 > x + width + EPS || y1 > y + height + EPS
                };
                outside((cx, cy, cx + cw, cy + ch))
                    // Authored geometry is drawn as written, so it can reach
                    // past its own box.
                    || scene.node(*c).is_some_and(|n| {
                        authored_geometry_bounds(n).is_some_and(|(x0, y0, x1, y1)| {
                            !is_translation(&n.transform) || outside((cx + x0, cy + y0, cx + x1, cy + y1))
                        })
                    })
            });
        let clip_subtree = node.clips_content
            && !node.children.is_empty()
            && (fills.is_empty() || child_overflows);
        let ancestor_clips = if self.plan.is_some() && node.emission.is_some() {
            self.clips.clone()
        } else {
            Vec::new()
        };
        // Clipping content clips the children. Figma imports open the clip
        // before the node's own paint (as parity expects); authored nodes
        // after it, so their own shadows and outside strokes aren't cut.
        let open_clip = clip_subtree && width > 0.0 && height > 0.0;
        if open_clip && !authored {
            self.open_clip(node_id, x, y, width, height, radii);
        }

        let mut atomic_svg = false;
        let path = |x: f64,
                    y: f64,
                    normalize: bool,
                    outline: bool,
                    fp: Vec<String>,
                    sp: Vec<String>,
                    svg: Option<Arc<str>>,
                    fills: Vec<(FillPaint, BlendMix)>,
                    stroke: Option<StrokeStyle>| PaintPrimitive::Path {
            node_id,
            x,
            y,
            width,
            height,
            normalize_to_origin: normalize,
            text_outline: outline,
            fill_paths: fp,
            stroke_paths: sp,
            svg_base64: svg,
            fills,
            stroke,
            effects: merged_effects.clone(),
            blend: primitive_blend.clone(),
            rotation,
            authored,
        };

        let own_start = self.out.len();
        match (class(&node.kind), &node.kind) {
            // Text with exported glyph outlines: paint the outlines (the
            // exporter's own shaping), at raw transform-chain coordinates.
            (Class::Text, NodeKind::Text(text)) if has_native_geometry => {
                let mut outline_fills = fills;
                if outline_fills.is_empty() {
                    outline_fills = text_outline_fills(text, scene, content_opacity);
                }
                if stroke_paths.is_empty()
                    && !fill_paths.is_empty()
                    && outline_fills.len() == fill_paths.len()
                {
                    for (d, fill) in fill_paths.into_iter().zip(outline_fills) {
                        self.out.push(path(
                            chain_x,
                            chain_y,
                            true,
                            true,
                            vec![d],
                            Vec::new(),
                            None,
                            vec![fill],
                            stroke.clone(),
                        ));
                    }
                } else {
                    self.out.push(path(
                        chain_x,
                        chain_y,
                        true,
                        true,
                        fill_paths,
                        stroke_paths,
                        svg_base64,
                        outline_fills,
                        stroke,
                    ));
                }
            }
            (Class::Text, NodeKind::Text(text)) => {
                if !text.characters.is_empty() {
                    let color = fills
                        .first()
                        .and_then(|(f, _)| first_color(f))
                        .unwrap_or(Rgba {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: content_opacity,
                        });
                    self.out.push(PaintPrimitive::Text {
                        node_id,
                        x,
                        y,
                        text: text.characters.clone(),
                        color,
                        font_size: text.font_size.max(1.0),
                        line_height: text.line_height,
                        letter_spacing: text.letter_spacing,
                        text_case: text_case(text.text_case),
                        font: text_font(text),
                        style_runs: text_style_runs(text, content_opacity),
                        blend: primitive_blend.clone(),
                        width,
                        align: match text.align {
                            doc::TextAlign::Center => TextAlignment::Center,
                            doc::TextAlign::Right => TextAlignment::Right,
                            doc::TextAlign::Left | doc::TextAlign::Justified => TextAlignment::Left,
                        },
                        effects: effects
                            .iter()
                            .filter(|e| match e.kind {
                                EffectKind::Glow => true,
                                EffectKind::DropShadow | EffectKind::InnerShadow => authored,
                                EffectKind::LayerBlur | EffectKind::BackgroundBlur => false,
                            })
                            .cloned()
                            .collect(),
                    });
                }
            }
            // A detached instance (main component not in this scene) or a
            // boolean with only an exporter snapshot: draw the snapshot whole.
            _ if svg_base64.is_some()
                && !has_native_geometry
                && node.children.is_empty()
                && (instance_component == Some(None) || is_boolean) =>
            {
                atomic_svg = true;
                self.out.push(path(
                    x,
                    y,
                    false,
                    false,
                    fill_paths,
                    stroke_paths,
                    svg_base64,
                    fills,
                    stroke,
                ));
            }
            _ if instance_component.is_some() => {}
            (Class::Shape, _) if has_vector_geometry => {
                self.out.push(path(
                    x,
                    y,
                    false,
                    false,
                    fill_paths,
                    stroke_paths,
                    svg_base64,
                    fills,
                    stroke,
                ));
            }
            _ => {
                if width > 0.0 && height > 0.0 {
                    self.out.push(PaintPrimitive::Rect {
                        node_id,
                        x,
                        y,
                        width,
                        height,
                        fills,
                        stroke,
                        corner_radii: radii,
                        effects: merged_effects.clone(),
                        blend: primitive_blend,
                        rotation,
                        authored,
                    });
                }
            }
        }

        if self.plan.is_some()
            && self.in_mask_def == 0
            && let Some(em) = &node.emission
            && em.intensity > 0.0
            && effective_opacity > 0.0
        {
            let color = Rgba {
                a: 1.0,
                ..rgba(&em.color, 1.0)
            };
            let mut primitives: Vec<PaintPrimitive> = self.out[own_start..]
                .iter()
                .filter_map(|p| recolor(p, &color))
                .collect();
            if primitives.is_empty() && width > 0.0 && height > 0.0 {
                primitives.push(PaintPrimitive::Rect {
                    node_id,
                    x,
                    y,
                    width,
                    height,
                    fills: vec![(FillPaint::Solid(color.clone()), BlendMix::Normal)],
                    stroke: None,
                    corner_radii: radii,
                    effects: Vec::new(),
                    blend: BlendMix::Normal,
                    rotation,
                    authored,
                });
            }
            if !primitives.is_empty() {
                let world = self
                    .groups
                    .last()
                    .copied()
                    .unwrap_or_else(AffineTransform2D::identity);
                let flicker = self.flicker.clone();
                if let Some(plan) = &mut self.plan {
                    plan.emissive.push(EmissiveDraw {
                        node_id,
                        primitives,
                        world,
                        clips: ancestor_clips,
                        color,
                        strength: em.intensity * effective_opacity,
                        radius: em.radius.max(0.0),
                        flicker,
                    });
                }
            }
        }

        if open_clip && authored {
            self.open_clip(node_id, x, y, width, height, radii);
        }
        if !atomic_svg {
            self.children(node, x, y, content_opacity, &child_blurs);
            // An instance exported without its subtree: draw the main
            // component's children in its place.
            if node.children.is_empty()
                && let Some(Some(component)) = instance_component
                && let Some(main) = scene.node(component)
            {
                for c in &main.children {
                    self.node(*c, x, y, true, content_opacity, &child_blurs);
                }
            }
        }

        if open_clip {
            if self.plan.is_some() {
                self.clips.pop();
            }
            self.out.push(PaintPrimitive::ClipEnd { node_id });
        }
        if needs_layer {
            self.out.push(PaintPrimitive::LayerEnd { node_id });
        }
        if group.is_some() {
            self.out.push(PaintPrimitive::GroupEnd { node_id });
            self.groups.pop();
        }

        self.flicker = saved_flicker;
        if shader_here {
            self.in_shader = false;
            if let (Some(fx), Some(plan)) = (node.shader.as_ref(), self.plan.as_mut())
                && self.out.len() > region_start
            {
                // Node-local box → world: the container's linear part (its
                // group) or the leaf's own rotation, about its box origin.
                let local = if group.is_some() {
                    let t = &node.transform;
                    AffineTransform2D {
                        a: t.a,
                        b: t.b,
                        tx: x,
                        c: t.c,
                        d: t.d,
                        ty: y,
                    }
                } else {
                    let (sin, cos) = rotation.sin_cos();
                    AffineTransform2D {
                        a: cos,
                        b: -sin,
                        tx: x,
                        c: sin,
                        d: cos,
                        ty: y,
                    }
                };
                plan.shaders.push(ShaderRegion {
                    node_id,
                    range: region_start..self.out.len(),
                    world: outer_world * local,
                    width,
                    height,
                    shader: fx.shader.clone(),
                    params: fx.params.clone(),
                    control: nearest_control(scene, node_id),
                    inputs: fx.inputs.clone(),
                    opacity: effective_opacity,
                });
            }
        }
    }

    fn open_clip(
        &mut self,
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        corner_radii: doc::CornerRadii,
    ) {
        let clip = PaintPrimitive::ClipStart {
            node_id,
            x,
            y,
            width,
            height,
            corner_radii,
        };
        if self.plan.is_some() {
            let world = self
                .groups
                .last()
                .copied()
                .unwrap_or_else(AffineTransform2D::identity);
            self.clips.push((world, clip.clone()));
        }
        self.out.push(clip);
    }

    /// Children in order; a mask child masks the siblings after it.
    fn children(&mut self, node: &doc::Node, x: f64, y: f64, opacity: f64, blurs: &[NodeEffect]) {
        let mut active_mask: Option<NodeId> = None;
        for &child_id in &node.children {
            let Some(child) = self.scene.node(child_id) else {
                continue;
            };
            let supported_mask = matches!(
                child.mask,
                Some(doc::MaskKind::Alpha | doc::MaskKind::Vector)
            );
            if supported_mask {
                if let Some(m) = active_mask.take() {
                    self.out.push(PaintPrimitive::MaskApplyEnd { node_id: m });
                }
                let (mut mx, mut my, mut mw, mut mh) = self.child_box(child_id, x, y);
                if !is_translation(&child.transform) {
                    // A transformed mask covers its transformed box.
                    let g = group_transform(&child.transform, mx, my);
                    let pts = [(mx, my), (mx + mw, my), (mx, my + mh), (mx + mw, my + mh)]
                        .map(|(px, py)| g.transform_point(px, py));
                    let (x0, y0) = pts
                        .iter()
                        .fold((f64::INFINITY, f64::INFINITY), |(a, b), p| {
                            (a.min(p.0), b.min(p.1))
                        });
                    let (x1, y1) = pts
                        .iter()
                        .fold((f64::NEG_INFINITY, f64::NEG_INFINITY), |(a, b), p| {
                            (a.max(p.0), b.max(p.1))
                        });
                    (mx, my, mw, mh) = (x0, y0, x1 - x0, y1 - y0);
                }
                if mw > 0.0 && mh > 0.0 {
                    self.out.push(PaintPrimitive::MaskDefStart {
                        node_id: child_id,
                        x: mx,
                        y: my,
                        width: mw,
                        height: mh,
                        luminance: false,
                    });
                    self.in_mask_def += 1;
                    self.node(child_id, x, y, false, opacity, blurs);
                    self.in_mask_def -= 1;
                    self.out
                        .push(PaintPrimitive::MaskDefEnd { node_id: child_id });
                    self.out.push(PaintPrimitive::MaskApplyStart {
                        node_id: child_id,
                        x: mx,
                        y: my,
                        width: mw,
                        height: mh,
                    });
                    active_mask = Some(child_id);
                }
                continue;
            }
            self.node(child_id, x, y, false, opacity, blurs);
        }
        if let Some(m) = active_mask {
            self.out.push(PaintPrimitive::MaskApplyEnd { node_id: m });
        }
    }
}

/// Bounds of SVG path data, parsed once: asked for every authored node on
/// every frame (culling), and parsing the path again each time was most of
/// a still face's cost to render.
fn cached_path_bounds(path_data: &String) -> Option<kurbo::Rect> {
    use kurbo::Shape;
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::{LazyLock, Mutex};

    static CACHE: LazyLock<Mutex<HashMap<u64, Option<kurbo::Rect>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path_data.hash(&mut hasher);
    let key = hasher.finish();
    if let Ok(cache) = CACHE.lock()
        && let Some(cached) = cache.get(&key)
    {
        return *cached;
    }
    let bounds = kurbo::BezPath::from_svg(path_data)
        .ok()
        .map(|p| p.bounding_box());
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, bounds);
    }
    bounds
}
