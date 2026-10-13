use document_proto::NodeId;
pub use render_fonts::FontSpec;

pub use super::transform::AffineTransform2D;

#[derive(Debug, Clone, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    pub(super) fn with_opacity(&self, opacity: f64) -> Self {
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a: (self.a * opacity).clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GradientStop {
    pub offset: f64,
    pub color: Rgba,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FillPaint {
    Solid(Rgba),
    GradientLinear {
        transform: Option<AffineTransform2D>,
        stops: Vec<GradientStop>,
    },
    GradientRadial {
        transform: Option<AffineTransform2D>,
        stops: Vec<GradientStop>,
    },
    GradientAngular {
        transform: Option<AffineTransform2D>,
        stops: Vec<GradientStop>,
    },
    Image {
        image_hash: Option<String>,
        /// The image file, base64-encoded — shared: encoded once per asset
        /// ([`asset_base64`](super::scene_collect::asset_base64)), so a
        /// collect clones a pointer, and painters cache their decode by it.
        data_base64: std::sync::Arc<str>,
        alpha: f64,
        scale_mode: ImageScaleMode,
        image_transform: Option<AffineTransform2D>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageScaleMode {
    #[default]
    Fill,
    Fit,
    Crop,
    Tile,
    Stretch,
}

impl FillPaint {
    pub(super) fn with_opacity(self, opacity: f64) -> Self {
        match self {
            FillPaint::Solid(color) => FillPaint::Solid(color.with_opacity(opacity)),
            FillPaint::GradientLinear { transform, stops } => FillPaint::GradientLinear {
                transform,
                stops: stops
                    .into_iter()
                    .map(|s| GradientStop {
                        offset: s.offset,
                        color: s.color.with_opacity(opacity),
                    })
                    .collect(),
            },
            FillPaint::GradientRadial { transform, stops } => FillPaint::GradientRadial {
                transform,
                stops: stops
                    .into_iter()
                    .map(|s| GradientStop {
                        offset: s.offset,
                        color: s.color.with_opacity(opacity),
                    })
                    .collect(),
            },
            FillPaint::GradientAngular { transform, stops } => FillPaint::GradientAngular {
                transform,
                stops: stops
                    .into_iter()
                    .map(|s| GradientStop {
                        offset: s.offset,
                        color: s.color.with_opacity(opacity),
                    })
                    .collect(),
            },
            FillPaint::Image {
                image_hash,
                data_base64,
                alpha,
                scale_mode,
                image_transform,
            } => FillPaint::Image {
                image_hash,
                data_base64,
                alpha: (alpha * opacity).clamp(0.0, 1.0),
                scale_mode,
                image_transform,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EffectKind {
    DropShadow,
    InnerShadow,
    LayerBlur,
    BackgroundBlur,
    /// Light a node gives off (its `emission`), drawn without the GPU effects
    /// stage: a blurred halo in the emission colour behind the node, like a
    /// drop shadow with no offset.
    Glow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeEffect {
    pub kind: EffectKind,
    pub color: Rgba,
    pub offset_x: f64,
    pub offset_y: f64,
    pub radius: f64,
    pub spread: f64,
}

pub use document_proto::{StrokeAlign, StrokeCap, StrokeJoin};

/// The box a gradient stroke is laid across: the node's `(x, y, w, h)`,
/// with a side of zero length (a horizontal or vertical line) widened to
/// the stroke's width about the line, so the gradient still has an extent.
#[must_use]
pub fn stroke_gradient_box(s: &StrokeStyle, x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
    let grow = |at: f64, len: f64| if len > 1e-9 { (at, len) } else { (at - s.width * 0.5, s.width.max(1e-6)) };
    let ((x, w), (y, h)) = (grow(x, w), grow(y, h));
    (x, y, w, h)
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrokeStyle {
    /// The stroke's colour; for a gradient stroke its first stop (what
    /// effects and blurs that need one colour use).
    pub color: Rgba,
    /// A gradient paint (linear, radial or angular) laid across the node's
    /// box as a fill's would be; `None` for a solid stroke.
    pub gradient: Option<FillPaint>,
    pub width: f64,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub align: StrokeAlign,
    pub miter_limit: f64,
    pub dash_pattern: Vec<f64>,
    pub dash_offset: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlendMix {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextCase {
    Original,
    Upper,
    Lower,
    Title,
    SmallCaps,
    SmallCapsForced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextStyleRun {
    pub text: String,
    pub color: Option<Rgba>,
    pub font_size: Option<f64>,
    pub letter_spacing: Option<f64>,
    pub font_style: Option<String>,
    pub font_weight: Option<f64>,
    pub font_family: Option<String>,
    pub font_width: Option<f64>,
}

impl TextStyleRun {
    /// The run's own font settings (unset fields inherit the node's).
    #[must_use]
    pub fn font(&self) -> FontSpec {
        FontSpec {
            family: self.font_family.clone(),
            weight: self.font_weight,
            style: self.font_style.clone(),
            width: self.font_width,
        }
    }
}

/// Paint primitives, in paint order.
///
/// Coordinates are in *flattened scene space*: a child sits at its parent's
/// box origin plus its own `tx`/`ty`, in the parent's unrotated frame. A
/// container whose transform has a non-translation part (rotation, scale,
/// skew, flip) wraps its own primitives and its whole subtree in a
/// [`PaintPrimitive::GroupStart`]/[`PaintPrimitive::GroupEnd`] pair; painters
/// apply the group's matrix on top of everything between them, so children
/// inherit the parent's transform (world = parent world ∘ local). Scenes
/// without such containers emit no groups, so their primitives are exactly
/// what they were before groups existed.
#[derive(Debug, Clone, PartialEq)]
pub enum PaintPrimitive {
    /// Apply `transform` (flattened scene space → flattened scene space,
    /// pivot included) to every primitive until the matching `GroupEnd`,
    /// composing with any enclosing group (`outer * transform`).
    GroupStart {
        node_id: NodeId,
        transform: AffineTransform2D,
    },
    GroupEnd {
        node_id: NodeId,
    },
    LayerStart {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        blend: BlendMix,
        opacity: f64,
    },
    LayerEnd {
        node_id: NodeId,
    },
    ClipStart {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        corner_radii: document_proto::CornerRadii,
    },
    ClipEnd {
        node_id: NodeId,
    },
    MaskDefStart {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        luminance: bool,
    },
    MaskDefEnd {
        node_id: NodeId,
    },
    MaskApplyStart {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    MaskApplyEnd {
        node_id: NodeId,
    },
    Rect {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        fills: Vec<(FillPaint, BlendMix)>,
        stroke: Option<StrokeStyle>,
        corner_radii: document_proto::CornerRadii,
        effects: Vec<NodeEffect>,
        blend: BlendMix,
        rotation: f64,
        /// Drawn as authored rather than as a Figma import (see `authored`
        /// on [`PaintPrimitive::Path`]): aligned strokes are exact and
        /// shadows follow non-uniform corners.
        authored: bool,
    },
    Text {
        node_id: NodeId,
        x: f64,
        y: f64,
        text: String,
        color: Rgba,
        font_size: f64,
        line_height: Option<f64>,
        letter_spacing: f64,
        text_case: TextCase,
        /// The node's family, weight, style and width; runs override it.
        font: FontSpec,
        style_runs: Vec<TextStyleRun>,
        blend: BlendMix,
        /// The text box width, for alignment.
        width: f64,
        align: TextAlignment,
        /// Shadows and glows that follow the glyph outlines (drop shadows,
        /// inner shadows and the emission [`EffectKind::Glow`]). Figma
        /// imports draw text effects from their exported outlines instead,
        /// so theirs carry only a glow.
        effects: Vec<NodeEffect>,
    },
    Path {
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        normalize_to_origin: bool,
        text_outline: bool,
        fill_paths: Vec<String>,
        stroke_paths: Vec<String>,
        /// The node's SVG snapshot, base64-encoded (shared, like image data).
        svg_base64: Option<std::sync::Arc<str>>,
        fills: Vec<(FillPaint, BlendMix)>,
        stroke: Option<StrokeStyle>,
        effects: Vec<NodeEffect>,
        blend: BlendMix,
        rotation: f64,
        /// The node was authored in frame (markup, the surface kit, code)
        /// rather than imported from Figma, so its geometry is taken
        /// literally: path data is drawn in node-local coordinates as written
        /// (never fitted into the node's box), a stroke without
        /// `stroke_paths` strokes the fill geometry (open subpaths are
        /// stroked, not filled), shadows and blurs follow the outline, and
        /// aligned strokes are exact. Figma imports (`false`) keep the
        /// exporter-compatible behaviour the parity corpus is measured
        /// against.
        authored: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderDiagnostics {
    pub total_nodes: usize,
    pub non_normal_blend_nodes: usize,
    pub gradient_fill_nodes: usize,
    pub image_fill_nodes: usize,
    pub drop_shadow_nodes: usize,
    pub inner_shadow_nodes: usize,
    pub layer_blur_nodes: usize,
    pub layer_blur_approx_nodes: usize,
    pub background_blur_nodes: usize,
    pub background_blur_approx_nodes: usize,
    pub alpha_mask_nodes: usize,
    pub luminance_mask_nodes: usize,
}

impl RenderDiagnostics {
    pub fn format_summary(&self) -> String {
        format!(
            "Render diag: nodes {} | blend {} | grad {} | image {} | drop {} | inner {} | blur {} (approx {}) | bg-blur {} (approx {}) | alpha-mask {} | luminance-mask {}*",
            self.total_nodes,
            self.non_normal_blend_nodes,
            self.gradient_fill_nodes,
            self.image_fill_nodes,
            self.drop_shadow_nodes,
            self.inner_shadow_nodes,
            self.layer_blur_nodes,
            self.layer_blur_approx_nodes,
            self.background_blur_nodes,
            self.background_blur_approx_nodes,
            self.alpha_mask_nodes,
            self.luminance_mask_nodes
        )
    }
}
