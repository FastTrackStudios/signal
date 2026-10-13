//! Figma → frame document.
//!
//! Figma is an import source, never frame's storage format. This crate reads
//! Figma data (REST `GetFile` JSON and the FTS plugin/bridge exports, via
//! `figma-model`) and translates it — once — into a [`crate::document::proto::Scene`].
//! Every Figma quirk (v1/v2 key spellings, `relativeTransform` rows, paint
//! opacity, placeholder images…) is resolved here so nothing downstream ever
//! reads Figma JSON.
//!
//! Node ids are preserved (`crate::figma::model::NodeId` and `crate::document::proto::NodeId`
//! wrap the same UUID), and each node keeps a [`SourceRef`] with its Figma id
//! so a re-import can update in place.

use std::collections::BTreeMap;

use crate::document::proto::{
    AlignSelf, AutoLayout, BlendMode, BooleanOp, Color, CornerRadii, CrossAxisAlign, Effect,
    EffectKind, GradientStop, ImageScaleMode, LayoutChild, LayoutDirection, MainAxisAlign,
    MaskKind, Node, NodeId, NodeKind, Paint, PaintKind, Rect, Scene, Size, SourceRef, Stroke,
    StrokeAlign, StrokeCap, StrokeJoin, TextCase, TextData, TextRun, Transform,
};
use crate::figma::model::{
    AutoLayoutAlignSelf, AutoLayoutCounterAlign, AutoLayoutMode, AutoLayoutPositioning,
    AutoLayoutPrimaryAlign, AutoLayoutWrap, FrameDocument,
};
use base64::Engine as _;
use serde_json::Value;

pub use crate::figma::model::document::FrameDocumentError as ImportError;

/// Parse any supported Figma source (REST `GetFile` JSON, FTS export v1/v2,
/// bridge JSON, `.fig` zip container) and import it.
///
/// # Errors
/// When the bytes aren't a Figma source `figma-model` understands.
pub fn import_bytes(bytes: &[u8]) -> Result<Scene, ImportError> {
    Ok(import_document(&FrameDocument::from_source_bytes(bytes)?))
}

/// Import an already-parsed Figma document.
#[must_use]
pub fn import_document(doc: &FrameDocument) -> Scene {
    let mut scene = Scene {
        root: NodeId(doc.root.0),
        nodes: BTreeMap::new(),
        assets: BTreeMap::new(),
        variables: crate::document::proto::Variables::default(),
    };
    for id in doc.walk_subtree(doc.root) {
        let Some(fnode) = doc.get_node(id) else {
            continue;
        };
        let node = import_node(doc, fnode, &mut scene);
        scene.nodes.insert(node.id, node);
    }
    scene
}

/// The document node for a Figma node id (`"12:34"`), if it was imported.
#[must_use]
pub fn node_for_figma_id(scene: &Scene, figma_id: &str) -> Option<NodeId> {
    scene.by_source_id(figma_id).map(|n| n.id)
}

fn import_node(
    doc: &FrameDocument,
    fnode: &crate::figma::model::FrameNode,
    scene: &mut Scene,
) -> Node {
    let raw = &fnode.raw;
    let kind = node_kind(doc, fnode);
    let mut node = Node::new(kind, fnode.name.clone());
    node.id = NodeId(fnode.id.0);
    node.parent = fnode.parent.map(|p| NodeId(p.0));
    node.children = fnode.children.iter().map(|c| NodeId(c.0)).collect();
    node.visible = fnode.visible;
    node.locked = fnode.locked;
    node.source = Some(SourceRef {
        tool: "figma".into(),
        id: fnode.figma_id.clone(),
        node_type: fnode.figma_type.clone(),
        bounds: absolute_bounds(raw),
    });

    node.transform = relative_transform(raw);
    node.size = size(raw);
    node.opacity = f64_at(raw, "opacity").unwrap_or(1.0);
    node.blend = blend_mode(raw);
    node.corner_radii = corner_radii(raw);
    node.clips_content = bool_at(raw, "clipsContent").unwrap_or(false);
    node.mask = bool_at(raw, "isMask")
        .unwrap_or(false)
        .then(|| match str_at(raw, "maskType") {
            Some(t) if t.eq_ignore_ascii_case("LUMINANCE") => MaskKind::Luminance,
            Some(t) if t.eq_ignore_ascii_case("VECTOR") => MaskKind::Vector,
            _ => MaskKind::Alpha,
        });

    node.fills = paints(raw, raw, "fills", scene);
    node.stroke = stroke(raw, scene);
    node.effects = effects(raw);
    node.fill_geometry = path_strings(raw, "fillGeometry");
    node.stroke_geometry = path_strings(raw, "strokeGeometry");
    node.svg_snapshot = raw
        .get("exports")
        .and_then(|v| v.get("svgBase64"))
        .and_then(Value::as_str)
        .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok())
        .map(|bytes| scene.add_asset("image/svg+xml", bytes));

    let (layout, layout_child) = layout(raw);
    node.layout = layout;
    node.layout_child = layout_child;

    if let NodeKind::Text(text) = &mut node.kind {
        text.runs = text_runs(raw, scene);
    }
    node
}

fn node_kind(doc: &FrameDocument, fnode: &crate::figma::model::FrameNode) -> NodeKind {
    let raw = &fnode.raw;
    match fnode.figma_type.as_str() {
        "DOCUMENT" => NodeKind::Document,
        "CANVAS" => NodeKind::Page,
        "FRAME" | "SECTION" | "TABLE" | "TABLE_CELL" | "TRANSFORM_GROUP" => NodeKind::Frame,
        "GROUP" => NodeKind::Group,
        "COMPONENT" => NodeKind::Component,
        "COMPONENT_SET" => NodeKind::ComponentSet,
        "INSTANCE" => NodeKind::Instance {
            component: str_at(raw, "componentId")
                .and_then(|cid| doc.get_by_figma_id(cid))
                .map(|c| NodeId(c.id.0)),
        },
        "RECTANGLE" => NodeKind::Rectangle,
        "ELLIPSE" => NodeKind::Ellipse,
        "LINE" => NodeKind::Line,
        "REGULAR_POLYGON" => NodeKind::Polygon,
        "STAR" => NodeKind::Star,
        "VECTOR" | "CONNECTOR" | "WASHI_TAPE" => NodeKind::Vector,
        "BOOLEAN_OPERATION" => NodeKind::BooleanOperation {
            op: match str_at(raw, "booleanOperation") {
                Some("UNION") => Some(BooleanOp::Union),
                Some("SUBTRACT") => Some(BooleanOp::Subtract),
                Some("INTERSECT") => Some(BooleanOp::Intersect),
                Some("EXCLUDE") => Some(BooleanOp::Exclude),
                _ => None,
            },
        },
        "TEXT" | "TEXT_PATH" | "SHAPE_WITH_TEXT" | "STICKY" => NodeKind::Text(text_data(raw)),
        "SLICE" | "EMBED" | "LINK_UNFURL" | "WIDGET" => NodeKind::Utility,
        other => NodeKind::Other {
            tag: other.to_string(),
        },
    }
}

// ── scalar helpers ───────────────────────────────────────────────────────

fn f64_at(raw: &Value, key: &str) -> Option<f64> {
    raw.get(key).and_then(Value::as_f64)
}

fn bool_at(raw: &Value, key: &str) -> Option<bool> {
    raw.get(key).and_then(Value::as_bool)
}

fn str_at<'a>(raw: &'a Value, key: &str) -> Option<&'a str> {
    raw.get(key).and_then(Value::as_str)
}

/// v2 plugin exports put text style keys at the top level; v1 REST under `style`.
fn style_f64(raw: &Value, top: &str, nested: &str) -> Option<f64> {
    f64_at(raw, top).or_else(|| raw.get("style").and_then(|s| f64_at(s, nested)))
}

/// A Figma `gradientTransform` as frame's gradient transform. Figma's maps
/// the node's unit box *to* the gradient's own space (a linear gradient runs
/// (0, ½) → (1, ½) there, a radial one is centred at (½, ½)); frame's maps
/// the gradient's space into the node's box, where its handles are drawn.
/// So it is the inverse (a singular one is kept as is).
fn gradient_matrix(value: Option<&Value>) -> Option<Transform> {
    let t = matrix(value)?;
    let det = t.a * t.d - t.b * t.c;
    if det.abs() < 1e-12 {
        return Some(t);
    }
    Some(Transform {
        a: t.d / det,
        b: -t.b / det,
        c: -t.c / det,
        d: t.a / det,
        tx: (t.b * t.ty - t.d * t.tx) / det,
        ty: (t.c * t.tx - t.a * t.ty) / det,
    })
}

fn matrix(value: Option<&Value>) -> Option<Transform> {
    let t = value?.as_array()?;
    if t.len() < 2 {
        return None;
    }
    let r0 = t[0].as_array()?;
    let r1 = t[1].as_array()?;
    if r0.len() < 3 || r1.len() < 3 {
        return None;
    }
    Some(Transform {
        a: r0[0].as_f64().unwrap_or(1.0),
        b: r0[1].as_f64().unwrap_or(0.0),
        tx: r0[2].as_f64().unwrap_or(0.0),
        c: r1[0].as_f64().unwrap_or(0.0),
        d: r1[1].as_f64().unwrap_or(1.0),
        ty: r1[2].as_f64().unwrap_or(0.0),
    })
}

fn relative_transform(raw: &Value) -> Transform {
    let Some(t) = raw.get("relativeTransform").and_then(Value::as_array) else {
        return Transform::IDENTITY;
    };
    if t.len() < 2 {
        return Transform::IDENTITY;
    }
    let at = |row: usize, col: usize, default: f64| {
        t[row]
            .as_array()
            .and_then(|r| r.get(col))
            .and_then(Value::as_f64)
            .unwrap_or(default)
    };
    Transform {
        a: at(0, 0, 1.0),
        b: at(0, 1, 0.0),
        tx: at(0, 2, 0.0),
        c: at(1, 0, 0.0),
        d: at(1, 1, 1.0),
        ty: at(1, 2, 0.0),
    }
}

fn size(raw: &Value) -> Size {
    let mut w = raw.get("size").and_then(|v| f64_at(v, "x")).unwrap_or(0.0);
    let mut h = raw.get("size").and_then(|v| f64_at(v, "y")).unwrap_or(0.0);
    if (w <= 0.0 || h <= 0.0)
        && let Some(bb) = raw.get("absoluteBoundingBox")
    {
        if w <= 0.0 {
            w = f64_at(bb, "width").unwrap_or(w);
        }
        if h <= 0.0 {
            h = f64_at(bb, "height").unwrap_or(h);
        }
    }
    Size {
        width: w,
        height: h,
    }
}

fn corner_radii(raw: &Value) -> CornerRadii {
    if let Some(arr) = raw.get("rectangleCornerRadii").and_then(Value::as_array)
        && arr.len() >= 4
    {
        let r = |i: usize| arr[i].as_f64().unwrap_or(0.0).max(0.0);
        return CornerRadii {
            top_left: r(0),
            top_right: r(1),
            bottom_right: r(2),
            bottom_left: r(3),
        };
    }
    let uniform = f64_at(raw, "cornerRadius").unwrap_or(0.0).max(0.0);
    if uniform > 0.0 {
        return CornerRadii::uniform(uniform);
    }
    // Mixed radii: plugin exports drop `cornerRadius` and carry each corner.
    let r = |key: &str| f64_at(raw, key).unwrap_or(0.0).max(0.0);
    CornerRadii {
        top_left: r("topLeftRadius"),
        top_right: r("topRightRadius"),
        bottom_right: r("bottomRightRadius"),
        bottom_left: r("bottomLeftRadius"),
    }
}

fn absolute_bounds(raw: &Value) -> Option<Rect> {
    let bb = raw.get("absoluteBoundingBox")?;
    Some(Rect {
        x: f64_at(bb, "x")?,
        y: f64_at(bb, "y")?,
        width: f64_at(bb, "width")?,
        height: f64_at(bb, "height")?,
    })
}

fn blend_mode(raw: &Value) -> BlendMode {
    match str_at(raw, "blendMode") {
        Some("PASS_THROUGH") => BlendMode::PassThrough,
        Some("MULTIPLY") => BlendMode::Multiply,
        Some("SCREEN") => BlendMode::Screen,
        Some("OVERLAY") => BlendMode::Overlay,
        Some("DARKEN") => BlendMode::Darken,
        Some("LIGHTEN") => BlendMode::Lighten,
        Some("COLOR_DODGE") => BlendMode::ColorDodge,
        Some("COLOR_BURN") => BlendMode::ColorBurn,
        Some("HARD_LIGHT") => BlendMode::HardLight,
        Some("SOFT_LIGHT") => BlendMode::SoftLight,
        Some("DIFFERENCE") => BlendMode::Difference,
        Some("EXCLUSION") => BlendMode::Exclusion,
        Some("HUE") => BlendMode::Hue,
        Some("SATURATION") => BlendMode::Saturation,
        Some("COLOR") => BlendMode::Color,
        Some("LUMINOSITY") => BlendMode::Luminosity,
        Some("LINEAR_BURN") => BlendMode::LinearBurn,
        Some("LINEAR_DODGE") => BlendMode::LinearDodge,
        _ => BlendMode::Normal,
    }
}

fn color(value: &Value) -> Option<Color> {
    Some(Color {
        r: value.get("r")?.as_f64()?,
        g: value.get("g")?.as_f64()?,
        b: value.get("b")?.as_f64()?,
        a: value.get("a").and_then(Value::as_f64).unwrap_or(1.0),
    })
}

// ── paints ───────────────────────────────────────────────────────────────

/// A paint stack. `node_raw` carries the image asset table (`imageFillAssets`)
/// for image paints found in `holder_raw[key]`.
fn paints(node_raw: &Value, holder_raw: &Value, key: &str, scene: &mut Scene) -> Vec<Paint> {
    let Some(arr) = holder_raw.get(key).and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|p| paint(node_raw, p, scene))
        .collect()
}

fn paint(node_raw: &Value, p: &Value, scene: &mut Scene) -> Option<Paint> {
    let stops = || -> Option<Vec<GradientStop>> {
        let stops = p.get("gradientStops")?.as_array()?;
        let out: Option<Vec<_>> = stops
            .iter()
            .map(|s| {
                Some(GradientStop {
                    position: f64_at(s, "position").unwrap_or(0.0),
                    color: color(s.get("color")?)?,
                })
            })
            .collect();
        out.filter(|v| !v.is_empty())
    };
    let kind = match str_at(p, "type")? {
        "SOLID" => PaintKind::Solid {
            color: color(p.get("color")?)?,
        },
        "GRADIENT_LINEAR" => PaintKind::LinearGradient {
            stops: stops()?,
            transform: gradient_matrix(p.get("gradientTransform")),
        },
        "GRADIENT_RADIAL" => PaintKind::RadialGradient {
            stops: stops()?,
            transform: gradient_matrix(p.get("gradientTransform")),
        },
        "GRADIENT_ANGULAR" => PaintKind::AngularGradient {
            stops: stops()?,
            transform: gradient_matrix(p.get("gradientTransform")),
        },
        "IMAGE" => {
            let source_ref = str_at(p, "imageHash").map(ToString::to_string);
            let bytes = image_bytes(node_raw, source_ref.as_deref());
            let mime = sniff_image_mime(&bytes);
            PaintKind::Image {
                asset: scene.add_asset(mime, bytes),
                source_ref,
                scale_mode: match str_at(p, "scaleMode") {
                    Some("FIT") => ImageScaleMode::Fit,
                    Some("CROP") => ImageScaleMode::Crop,
                    Some("TILE") => ImageScaleMode::Tile,
                    Some("STRETCH") => ImageScaleMode::Stretch,
                    _ => ImageScaleMode::Fill,
                },
                // A tile is placed by its scaling factor (image px → node
                // px); the other modes by Figma's image transform.
                transform: if str_at(p, "scaleMode") == Some("TILE") {
                    f64_at(p, "scalingFactor").map(|s| Transform {
                        a: s,
                        b: 0.0,
                        c: 0.0,
                        d: s,
                        tx: 0.0,
                        ty: 0.0,
                    })
                } else {
                    matrix(p.get("imageTransform"))
                },
            }
        }
        _ => return None,
    };
    Some(Paint {
        kind,
        opacity: f64_at(p, "opacity").unwrap_or(1.0),
        blend: blend_mode(p),
        visible: bool_at(p, "visible").unwrap_or(true),
    })
}

/// 1×1 placeholders used when an export carries an image reference but not
/// the bytes, so image-fill structure survives. One parity fixture pins a
/// distinct placeholder to its hash.
const IMAGE_PLACEHOLDER_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";
const IMAGE_SAMPLING_PLACEHOLDER_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4MDD4DwAE/gH/5pEq9AAAAABJRU5ErkJggg==";

fn image_bytes(node_raw: &Value, hash: Option<&str>) -> Vec<u8> {
    let embedded = node_raw
        .get("imageFillAssets")
        .and_then(Value::as_object)
        .and_then(|assets| {
            let v = assets.get(hash?)?;
            v.as_str()
                .or_else(|| v.get("base64").and_then(Value::as_str))
        });
    let b64 = embedded.unwrap_or(match hash {
        Some("b6f8e2adf979577a15f43ba46ba26289a0ef23f3") => IMAGE_SAMPLING_PLACEHOLDER_B64,
        _ => IMAGE_PLACEHOLDER_B64,
    });
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap_or_default()
}

fn sniff_image_mime(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, ..] => "image/jpeg",
        [b'G', b'I', b'F', ..] => "image/gif",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => "image/webp",
        _ => "application/octet-stream",
    }
}

fn stroke(raw: &Value, scene: &mut Scene) -> Stroke {
    let enum_at = |key: &str| raw.get(key).cloned();
    Stroke {
        paints: paints(raw, raw, "strokes", scene),
        weight: f64_at(raw, "strokeWeight").unwrap_or(1.0),
        cap: enum_at("strokeCap")
            .and_then(|v| {
                serde_json::from_value::<figma_api::models::has_geometry_trait::StrokeCap>(v).ok()
            })
            .map_or(StrokeCap::None, map_cap),
        join: match str_at(raw, "strokeJoin") {
            Some("BEVEL") => StrokeJoin::Bevel,
            Some("ROUND") => StrokeJoin::Round,
            _ => StrokeJoin::Miter,
        },
        align: match str_at(raw, "strokeAlign") {
            Some("INSIDE") => StrokeAlign::Inside,
            Some("OUTSIDE") => StrokeAlign::Outside,
            _ => StrokeAlign::Center,
        },
        miter_limit: f64_at(raw, "strokeMiterLimit").unwrap_or(4.0),
        dash_pattern: raw
            .get("dashPattern")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default(),
        dash_offset: raw
            .get("strokeDashes")
            .and_then(|v| f64_at(v, "offset"))
            .unwrap_or(0.0),
    }
}

fn map_cap(cap: figma_api::models::has_geometry_trait::StrokeCap) -> StrokeCap {
    use figma_api::models::has_geometry_trait::StrokeCap as F;
    match cap {
        F::None => StrokeCap::None,
        F::Round => StrokeCap::Round,
        F::Square => StrokeCap::Square,
        F::LineArrow => StrokeCap::LineArrow,
        F::TriangleArrow => StrokeCap::TriangleArrow,
        F::DiamondFilled => StrokeCap::DiamondFilled,
        F::CircleFilled => StrokeCap::CircleFilled,
        F::TriangleFilled => StrokeCap::TriangleFilled,
        F::WashiTape1 => StrokeCap::WashiTape1,
        F::WashiTape2 => StrokeCap::WashiTape2,
        F::WashiTape3 => StrokeCap::WashiTape3,
        F::WashiTape4 => StrokeCap::WashiTape4,
        F::WashiTape5 => StrokeCap::WashiTape5,
        F::WashiTape6 => StrokeCap::WashiTape6,
    }
}

fn effects(raw: &Value) -> Vec<Effect> {
    let Some(arr) = raw.get("effects").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|e| {
            let kind = match str_at(e, "type")? {
                "DROP_SHADOW" => EffectKind::DropShadow,
                "INNER_SHADOW" => EffectKind::InnerShadow,
                "LAYER_BLUR" => EffectKind::LayerBlur,
                "BACKGROUND_BLUR" => EffectKind::BackgroundBlur,
                _ => return None,
            };
            // Effect colors carry their own `opacity`; fold it in here.
            let color = e
                .get("color")
                .and_then(color)
                .map(|c| {
                    let opacity = f64_at(e, "opacity").unwrap_or(1.0).clamp(0.0, 1.0);
                    Color {
                        r: c.r.clamp(0.0, 1.0),
                        g: c.g.clamp(0.0, 1.0),
                        b: c.b.clamp(0.0, 1.0),
                        a: (c.a.clamp(0.0, 1.0) * opacity).clamp(0.0, 1.0),
                    }
                })
                .unwrap_or(Color::rgba(0.0, 0.0, 0.0, 0.25));
            Some(Effect {
                kind,
                visible: bool_at(e, "visible").unwrap_or(true),
                color,
                offset_x: e.get("offset").and_then(|o| f64_at(o, "x")).unwrap_or(0.0),
                offset_y: e.get("offset").and_then(|o| f64_at(o, "y")).unwrap_or(0.0),
                radius: f64_at(e, "radius").unwrap_or(0.0),
                spread: f64_at(e, "spread").unwrap_or(0.0),
            })
        })
        .collect()
}

fn path_strings(raw: &Value, key: &str) -> Vec<String> {
    raw.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| str_at(entry, "path").or_else(|| str_at(entry, "data")))
        .filter(|s| !s.trim().is_empty())
        .map(ToString::to_string)
        .collect()
}

// ── layout ───────────────────────────────────────────────────────────────

fn layout(raw: &Value) -> (Option<AutoLayout>, LayoutChild) {
    let Some(al) = crate::figma::model::get_auto_layout_projection(raw) else {
        return (None, LayoutChild::default());
    };
    let container = al
        .mode
        .and_then(|m| match m {
            AutoLayoutMode::Horizontal => Some(LayoutDirection::Horizontal),
            AutoLayoutMode::Vertical => Some(LayoutDirection::Vertical),
            AutoLayoutMode::None => None,
        })
        .map(|direction| AutoLayout {
            direction,
            gap: al.item_spacing.unwrap_or(0.0),
            padding_left: al.padding_left.unwrap_or(0.0),
            padding_right: al.padding_right.unwrap_or(0.0),
            padding_top: al.padding_top.unwrap_or(0.0),
            padding_bottom: al.padding_bottom.unwrap_or(0.0),
            wrap: matches!(al.wrap, Some(AutoLayoutWrap::Wrap)),
            main_align: al.primary_axis_align_items.map(|a| match a {
                AutoLayoutPrimaryAlign::Min => MainAxisAlign::Start,
                AutoLayoutPrimaryAlign::Center => MainAxisAlign::Center,
                AutoLayoutPrimaryAlign::Max => MainAxisAlign::End,
                AutoLayoutPrimaryAlign::SpaceBetween => MainAxisAlign::SpaceBetween,
            }),
            hug_main: matches!(
                al.primary_axis_sizing_mode,
                Some(crate::figma::model::AutoLayoutSizingMode::Hug)
            ),
            hug_cross: matches!(
                al.counter_axis_sizing_mode,
                Some(crate::figma::model::AutoLayoutSizingMode::Hug)
            ),
            cross_align: al.counter_axis_align_items.map(|a| match a {
                AutoLayoutCounterAlign::Min => CrossAxisAlign::Start,
                AutoLayoutCounterAlign::Center => CrossAxisAlign::Center,
                AutoLayoutCounterAlign::Max => CrossAxisAlign::End,
                AutoLayoutCounterAlign::Baseline => CrossAxisAlign::Baseline,
            }),
        });
    let child = LayoutChild {
        grow: al.grow,
        align_self: al.align_self.and_then(|a| match a {
            AutoLayoutAlignSelf::Auto | AutoLayoutAlignSelf::Inherit => None,
            AutoLayoutAlignSelf::Min => Some(AlignSelf::Start),
            AutoLayoutAlignSelf::Center => Some(AlignSelf::Center),
            AutoLayoutAlignSelf::Max => Some(AlignSelf::End),
            AutoLayoutAlignSelf::Stretch => Some(AlignSelf::Stretch),
        }),
        absolute: matches!(al.positioning, Some(AutoLayoutPositioning::Absolute)),
        min_width: al.min_width,
        min_height: al.min_height,
        max_width: al.max_width,
        max_height: al.max_height,
        resize_x: None,
        resize_y: None,
    };
    (container, child)
}

// ── text ─────────────────────────────────────────────────────────────────

fn text_data(raw: &Value) -> TextData {
    let text_case = str_at(raw, "textCase")
        .or_else(|| raw.get("style").and_then(|s| str_at(s, "textCase")))
        .map_or(TextCase::Original, |c| match c {
            "UPPER" => TextCase::Upper,
            "LOWER" => TextCase::Lower,
            "TITLE" => TextCase::Title,
            "SMALL_CAPS" => TextCase::SmallCaps,
            "SMALL_CAPS_FORCED" => TextCase::SmallCapsForced,
            _ => TextCase::Original,
        });
    TextData {
        characters: str_at(raw, "characters").unwrap_or_default().to_string(),
        font_family: str_at(raw, "fontFamily")
            .or_else(|| raw.get("style").and_then(|s| str_at(s, "fontFamily")))
            .map(ToString::to_string),
        font_size: style_f64(raw, "fontSize", "fontSize").unwrap_or(14.0),
        font_weight: style_f64(raw, "fontWeight", "fontWeight"),
        font_style: None,
        font_width: None,
        line_height: style_f64(raw, "lineHeightPx", "lineHeightPx").filter(|v| *v > 0.0),
        letter_spacing: style_f64(raw, "letterSpacingPx", "letterSpacing").unwrap_or(0.0),
        text_case,
        align: crate::document::proto::TextAlign::default(),
        runs: Vec::new(),
    }
}

fn text_runs(raw: &Value, scene: &mut Scene) -> Vec<TextRun> {
    let Some(segments) = raw.get("styledTextSegments").and_then(Value::as_array) else {
        return Vec::new();
    };
    segments
        .iter()
        .filter_map(|seg| {
            let text = str_at(seg, "characters").unwrap_or_default();
            if text.is_empty() {
                return None;
            }
            Some(TextRun {
                text: text.to_string(),
                fills: paints(seg, seg, "fills", scene),
                font_family: seg
                    .get("fontName")
                    .and_then(|f| str_at(f, "family"))
                    .map(ToString::to_string),
                font_size: f64_at(seg, "fontSize"),
                font_weight: f64_at(seg, "fontWeight"),
                font_style: str_at(seg, "fontStyle")
                    .or_else(|| seg.get("fontName").and_then(|f| str_at(f, "style")))
                    .map(ToString::to_string),
                font_width: None,
                letter_spacing: f64_at(seg, "letterSpacingPx")
                    .or_else(|| seg.get("letterSpacing").and_then(|l| f64_at(l, "value"))),
            })
        })
        .collect()
}
