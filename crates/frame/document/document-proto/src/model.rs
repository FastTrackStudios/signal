//! The frame document model.
//!
//! A `Scene` is a tree of typed `Node`s plus a content-addressed asset table.
//! This is frame's own model: importers (Figma REST, `.fig`) translate into
//! it once, and everything downstream (render, surface runtime, AI tools,
//! the `.frame` bundle) reads only this.
//!
//! Values are stored *as authored* (paint opacity, stroke weight, node
//! opacity…); resolution into draw calls (opacity inheritance, layers,
//! clip decisions) is the renderer's job.

use std::collections::BTreeMap;

use facet::Facet;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── ids ──────────────────────────────────────────────────────────────────

/// A node's identity. Stable across saves, edits and replicas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Facet)]
#[facet(transparent)]
#[serde(transparent)]
pub struct NodeId(pub Uuid);

impl NodeId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An asset's identity: the lowercase hex blake3 of its bytes, so the same
/// image imported twice is stored once.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Facet)]
#[facet(transparent)]
#[serde(transparent)]
pub struct AssetId(pub String);

impl AssetId {
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).to_hex().to_string())
    }
}

impl std::fmt::Display for AssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── geometry ─────────────────────────────────────────────────────────────

/// Row-major 2×3 affine: `x' = a·x + b·y + tx`, `y' = c·x + d·y + ty`
/// (the same layout Figma's `relativeTransform` uses).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub tx: f64,
    pub c: f64,
    pub d: f64,
    pub ty: f64,
}

impl Transform {
    pub const IDENTITY: Self = Self { a: 1.0, b: 0.0, tx: 0.0, c: 0.0, d: 1.0, ty: 0.0 };

    #[must_use]
    pub const fn translate(tx: f64, ty: f64) -> Self {
        Self { a: 1.0, b: 0.0, tx, c: 0.0, d: 1.0, ty }
    }

    /// Rotation in radians, from the matrix columns (`atan2(c, a)`).
    #[must_use]
    pub fn rotation(&self) -> f64 {
        self.c.atan2(self.a)
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct CornerRadii {
    pub top_left: f64,
    pub top_right: f64,
    pub bottom_right: f64,
    pub bottom_left: f64,
}

impl CornerRadii {
    #[must_use]
    pub const fn uniform(r: f64) -> Self {
        Self { top_left: r, top_right: r, bottom_right: r, bottom_left: r }
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.top_left == 0.0 && self.top_right == 0.0 && self.bottom_right == 0.0 && self.bottom_left == 0.0
    }
}

// ── paint ────────────────────────────────────────────────────────────────

/// Straight (non-premultiplied) RGBA, each channel 0..=1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    #[must_use]
    pub const fn rgba(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum BlendMode {
    #[default]
    Normal,
    PassThrough,
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
    LinearBurn,
    LinearDodge,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct GradientStop {
    pub position: f64,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum ImageScaleMode {
    #[default]
    Fill,
    Fit,
    Crop,
    Tile,
    Stretch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum PaintKind {
    Solid { color: Color },
    LinearGradient { stops: Vec<GradientStop>, transform: Option<Transform> },
    RadialGradient { stops: Vec<GradientStop>, transform: Option<Transform> },
    AngularGradient { stops: Vec<GradientStop>, transform: Option<Transform> },
    Image {
        /// The image bytes, in the scene's asset table.
        asset: AssetId,
        /// The source tool's own reference (Figma `imageHash`), for re-import.
        source_ref: Option<String>,
        scale_mode: ImageScaleMode,
        transform: Option<Transform>,
    },
}

/// One entry of a fill or stroke paint stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Paint {
    pub kind: PaintKind,
    /// Paint opacity, multiplied into the paint's colors when drawn.
    pub opacity: f64,
    pub blend: BlendMode,
    pub visible: bool,
}

impl Paint {
    #[must_use]
    pub const fn solid(color: Color) -> Self {
        Self { kind: PaintKind::Solid { color }, opacity: 1.0, blend: BlendMode::Normal, visible: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum StrokeCap {
    #[default]
    None,
    Round,
    Square,
    LineArrow,
    TriangleArrow,
    DiamondFilled,
    CircleFilled,
    TriangleFilled,
    WashiTape1,
    WashiTape2,
    WashiTape3,
    WashiTape4,
    WashiTape5,
    WashiTape6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum StrokeJoin {
    #[default]
    Miter,
    Bevel,
    Round,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum StrokeAlign {
    Inside,
    Outside,
    #[default]
    Center,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Stroke {
    pub paints: Vec<Paint>,
    pub weight: f64,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub align: StrokeAlign,
    pub miter_limit: f64,
    pub dash_pattern: Vec<f64>,
    pub dash_offset: f64,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            paints: Vec::new(),
            weight: 1.0,
            cap: StrokeCap::None,
            join: StrokeJoin::Miter,
            align: StrokeAlign::Center,
            miter_limit: 4.0,
            dash_pattern: Vec::new(),
            dash_offset: 0.0,
        }
    }
}

// ── effects ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum EffectKind {
    DropShadow,
    InnerShadow,
    LayerBlur,
    BackgroundBlur,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Effect {
    pub kind: EffectKind,
    pub visible: bool,
    pub color: Color,
    pub offset_x: f64,
    pub offset_y: f64,
    pub radius: f64,
    pub spread: f64,
}

// ── text ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum TextCase {
    #[default]
    Original,
    Upper,
    Lower,
    Title,
    SmallCaps,
    SmallCapsForced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justified,
}

/// A styled span of a text node. Unset fields inherit the node's style.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct TextRun {
    pub text: String,
    pub fills: Vec<Paint>,
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub font_weight: Option<f64>,
    /// A style name: `"italic"`, Figma's `"Semi Bold Italic"`,
    /// `"condensed"`. Weight and width words apply where `font_weight` /
    /// `font_width` are unset.
    pub font_style: Option<String>,
    /// Width, percent of normal (CSS `font-stretch`): 75 condensed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub font_width: Option<f64>,
    pub letter_spacing: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct TextData {
    pub characters: String,
    /// A family name (`"Oswald"`, `"DSEG7 Classic"`) or a generic one
    /// (`"monospace"`); `None` = the host's UI face. Missing families and
    /// glyphs fall back through the host's font book.
    pub font_family: Option<String>,
    pub font_size: f64,
    /// 1–1000 (400 regular, 700 bold); `None` = regular.
    pub font_weight: Option<f64>,
    /// A style name: `"italic"`, `"Bold Italic"`, `"condensed"` (see
    /// [`TextRun::font_style`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub font_style: Option<String>,
    /// Width, percent of normal (CSS `font-stretch`): 75 condensed, 125
    /// expanded. Variable fonts with a `wdth` axis draw it exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub font_width: Option<f64>,
    /// Line height in px; `None` = the font's natural line height.
    pub line_height: Option<f64>,
    /// Letter spacing in px.
    pub letter_spacing: f64,
    pub text_case: TextCase,
    pub align: TextAlign,
    pub runs: Vec<TextRun>,
}

impl Default for TextData {
    fn default() -> Self {
        Self {
            characters: String::new(),
            font_family: None,
            font_size: 14.0,
            font_weight: None,
            font_style: None,
            font_width: None,
            line_height: None,
            letter_spacing: 0.0,
            text_case: TextCase::Original,
            align: TextAlign::Left,
            runs: Vec::new(),
        }
    }
}

// ── layout ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum LayoutDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum MainAxisAlign {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum CrossAxisAlign {
    #[default]
    Start,
    Center,
    End,
    Baseline,
}

/// Flex-style auto layout on a container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct AutoLayout {
    pub direction: LayoutDirection,
    pub gap: f64,
    pub padding_left: f64,
    pub padding_right: f64,
    pub padding_top: f64,
    pub padding_bottom: f64,
    pub wrap: bool,
    pub main_align: Option<MainAxisAlign>,
    pub cross_align: Option<CrossAxisAlign>,
    /// Size to content along the layout direction ("hug contents"). An
    /// authoring rule: editors recompute the size when children change;
    /// the renderer uses the stored size.
    #[serde(default)]
    #[facet(default)]
    pub hug_main: bool,
    /// Size to content across the layout direction.
    #[serde(default)]
    #[facet(default)]
    pub hug_cross: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum AlignSelf {
    Start,
    Center,
    End,
    Stretch,
}

/// How a node follows its parent's width when the parent is laid out
/// wider or narrower than it was drawn — a responsive host fitting a
/// surface to a box of other proportions ([`Scene::resize_width`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum ResizeX {
    /// Keeps its distance from the parent's left edge.
    Left,
    /// Keeps its distance from the parent's right edge.
    Right,
    /// Keeps its offset from the parent's centre.
    Center,
    /// Keeps both distances: its width takes the change.
    Stretch,
    /// Drawn in proportion: its position scaled and the node itself
    /// squeezed or stretched with the change (its transform, so its
    /// outlines and children scale with it) — for a drawing that must hold
    /// together, not for a control.
    Scale,
}

/// How a node follows its parent's height changing — [`ResizeX`] on the
/// other axis. A mixer strip is laid out for a height and lives at many:
/// what hangs from the top stays, what sits on the floor follows it down,
/// and the fader's travel takes the difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum ResizeY {
    /// Keeps its distance from the parent's top edge.
    Top,
    /// Keeps its distance from the parent's bottom edge.
    Bottom,
    /// Keeps its offset from the parent's centre.
    Center,
    /// Keeps both distances: its height takes the change.
    Stretch,
    /// Drawn in proportion (see [`ResizeX::Scale`]).
    Scale,
}

/// How a node sits inside an auto-layout parent.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct LayoutChild {
    pub grow: Option<f64>,
    pub align_self: Option<AlignSelf>,
    /// Taken out of flow and placed at its own transform.
    pub absolute: bool,
    pub min_width: Option<f64>,
    pub min_height: Option<f64>,
    pub max_width: Option<f64>,
    pub max_height: Option<f64>,
    /// How it follows its parent's width changing ([`ResizeX`]). `None`:
    /// it stretches when it spans the parent, else keeps its size and
    /// moves in proportion (a knob, a label never distorts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub resize_x: Option<ResizeX>,
    /// How it follows its parent's height changing ([`ResizeY`]). `None`:
    /// the same default as `resize_x`, on the vertical axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub resize_y: Option<ResizeY>,
}

// ── nodes ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum NodeKind {
    /// The scene root. Its children are pages.
    Document,
    Page,
    Frame,
    Group,
    Component,
    ComponentSet,
    Instance {
        /// The main component, when it is part of this scene.
        component: Option<NodeId>,
    },
    Rectangle,
    Ellipse,
    Line,
    Polygon,
    Star,
    Vector,
    BooleanOperation { op: Option<BooleanOp> },
    Text(TextData),
    /// Non-rendering helpers (slices, embeds, widgets).
    Utility,
    /// A node type this model has no dedicated kind for yet; `tag` names it.
    Other { tag: String },
}

/// Light a node emits (see [`Node::emission`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Emission {
    pub color: Color,
    /// Brightness multiplier of the bloom (1 = the node's own brightness).
    pub intensity: f64,
    /// How far the light spreads, px.
    pub radius: f64,
}

/// A shader post-effect (see [`Node::shader`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Facet)]
pub struct ShaderFx {
    /// A built-in effect id (`glass`, `crt`, `lighting`, `flicker`, …) or
    /// `asset:<id>` for custom WGSL stored in the document's asset table.
    pub shader: String,
    /// Named numeric uniforms (`strength`, `tint_r`, …). Unknown names are
    /// ignored by the effect.
    #[serde(default)]
    #[facet(default)]
    pub params: BTreeMap<String, f64>,
    /// Live data the shader reads each frame (feeds, params, curves),
    /// by name — `IN_<NAME>` in WGSL. See [`ShaderInput`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[facet(default)]
    pub inputs: Vec<crate::shader_input::ShaderInput>,
}

/// Where a node came from, so re-imports can update in place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct SourceRef {
    /// e.g. `"figma"`.
    pub tool: String,
    /// The source tool's node id (Figma `"12:34"`).
    pub id: String,
    /// The source tool's node type (Figma `"FRAME"`).
    pub node_type: String,
    /// The node's absolute, axis-aligned bounds as the source tool computed
    /// them at import (Figma `absoluteBoundingBox`). A snapshot for exporters
    /// that reproduce the source's own output; edits don't maintain it.
    pub bounds: Option<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum MaskKind {
    Alpha,
    Vector,
    Luminance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub visible: bool,
    pub locked: bool,

    /// Placement relative to the parent.
    pub transform: Transform,
    pub size: Size,
    pub opacity: f64,
    pub blend: BlendMode,
    pub corner_radii: CornerRadii,
    pub clips_content: bool,
    /// When set, this node masks the siblings painted after it.
    pub mask: Option<MaskKind>,

    pub fills: Vec<Paint>,
    pub stroke: Stroke,
    pub effects: Vec<Effect>,

    /// Vector outlines as SVG path data, in node-local coordinates.
    pub fill_geometry: Vec<String>,
    pub stroke_geometry: Vec<String>,
    /// A pre-rendered SVG of the whole node (an exporter's snapshot), used
    /// where native geometry can't reproduce it (booleans, detached instances).
    pub svg_snapshot: Option<AssetId>,

    pub layout: Option<AutoLayout>,
    pub layout_child: LayoutChild,

    /// Makes this node a control (knob, fader, footswitch…): its binding to
    /// an engine and the descendant nodes it drives.
    pub control: Option<surface_proto::Control>,

    /// Light this node gives off (VU lamps, jewels, lit caps, LEDs). The GPU
    /// effects stage blooms it onto its surroundings; renderers without one
    /// fall back to a soft blurred glow. Scales with the node's opacity, so
    /// an LED slot dimming a lamp dims its light too.
    #[serde(default)]
    #[facet(default)]
    pub emission: Option<Emission>,
    /// A post-effect on this node's subtree, run by the GPU effects stage
    /// (glass, CRT, lighting, …). Optional by design: renderers without it
    /// draw the subtree plainly, so a surface must read without it.
    #[serde(default)]
    #[facet(default)]
    pub shader: Option<ShaderFx>,
    /// Animations on this node: each drives one property from params,
    /// feeds, time or gestures through physical dynamics (springs,
    /// struck resonators, LFOs, noise…). Run by the surface runtime every
    /// frame on the render copy; the authored node is what shows at rest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[facet(default)]
    pub motion: Vec<motion_proto::Motion>,
    /// Properties computed from expressions instead of stored: a theme's
    /// tokens (`$strip.ground`), the box the node is laid out in (`w`,
    /// `h`), and the host's data (`track.name`, `track.volume`). See
    /// [`Bind`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[facet(default)]
    pub bind: Vec<Bind>,

    /// On an instance: the component it places, by name — resolved by
    /// whatever expands instances (the theme runtime, across a theme's
    /// files), not by id, so a theme can swap the component it names.
    /// `None` for an instance that tracks its component by id alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub of: Option<String>,

    /// A component's arguments: on a component, each one it takes and its
    /// default; on an instance, the value it is given. Expressions in the
    /// theme language, read inside the component as `arg.<name>`.
    /// See [`Arg`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[facet(default)]
    pub args: Vec<Arg>,

    pub source: Option<SourceRef>,
}

/// One argument of a component (`arg:<name>="<expr>"` in markup).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct Arg {
    pub name: String,
    pub expr: String,
}

/// One property of a node computed from an expression.
///
/// The expression language and what each property accepts belong to the
/// theme runtime (`theme-expr`); the document only carries the source, so
/// a document with bindings still loads, prints and diffs everywhere. The
/// stored property is what shows where nothing evaluates the binding (an
/// editor, an SVG export): author it as the binding's typical value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct Bind {
    /// The property, by its markup attribute name: `fill`, `stroke`,
    /// `opacity`, `visible`, `text`, `x`, `y`, `w`, `h`, `radius`,
    /// `rotation` (degrees clockwise, about the node's origin), `dash`
    /// (how much of the stroke is drawn, px from its start), `inset-top`
    /// `-bottom` `-left` `-right` (the node's own box shrunk, its children
    /// left where they are: a meter's lit part revealing a fixed
    /// gradient), `size` (a text's font size).
    pub prop: String,
    /// The expression's source.
    pub expr: String,
}

// ── variables ────────────────────────────────────────────────────────────

/// A document's design tokens: named values a binding reads as `$name`,
/// each with a value per mode. Switching mode is switching theme without
/// touching a node.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct Variables {
    /// The modes, in order; the first is the default.
    pub modes: Vec<String>,
    /// Each variable, by name (`strip.ground`, `meter.hot`).
    pub vars: BTreeMap<String, Variable>,
}

impl Variables {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modes.is_empty() && self.vars.is_empty()
    }
}

/// One variable: an expression per mode, and one for modes that don't say.
/// Values are expressions in the binding language, so one variable can
/// alias another (`$accent`) or derive from it (`mix($ground, $ink, 0.2)`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct Variable {
    pub value: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[facet(default)]
    pub modes: BTreeMap<String, String>,
}

impl Variable {
    /// The expression this variable has in `mode`.
    #[must_use]
    pub fn in_mode(&self, mode: &str) -> &str {
        self.modes.get(mode).map_or(self.value.as_str(), String::as_str)
    }
}

impl Node {
    /// A fresh node of `kind` with neutral defaults.
    #[must_use]
    pub fn new(kind: NodeKind, name: impl Into<String>) -> Self {
        Self {
            id: NodeId::new(),
            name: name.into(),
            kind,
            parent: None,
            children: Vec::new(),
            visible: true,
            locked: false,
            transform: Transform::IDENTITY,
            size: Size::default(),
            opacity: 1.0,
            blend: BlendMode::Normal,
            corner_radii: CornerRadii::default(),
            clips_content: false,
            mask: None,
            fills: Vec::new(),
            stroke: Stroke::default(),
            effects: Vec::new(),
            fill_geometry: Vec::new(),
            stroke_geometry: Vec::new(),
            svg_snapshot: None,
            layout: None,
            layout_child: LayoutChild::default(),
            control: None,
            emission: None,
            shader: None,
            motion: Vec::new(),
            bind: Vec::new(),
            of: None,
            args: Vec::new(),
            source: None,
        }
    }

    #[must_use]
    pub fn text(&self) -> Option<&TextData> {
        match &self.kind {
            NodeKind::Text(t) => Some(t),
            _ => None,
        }
    }
}

// ── assets + scene ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct Asset {
    pub id: AssetId,
    /// e.g. `image/png`, `image/svg+xml`, `font/ttf`.
    pub mime: String,
    pub bytes: Vec<u8>,
}

impl Asset {
    #[must_use]
    pub fn new(mime: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self { id: AssetId::of(&bytes), mime: mime.into(), bytes }
    }
}

/// A whole document: the node tree and the assets it references.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Scene {
    pub root: NodeId,
    pub nodes: BTreeMap<NodeId, Node>,
    pub assets: BTreeMap<AssetId, Asset>,
    /// The document's design tokens.
    #[serde(default, skip_serializing_if = "Variables::is_empty")]
    #[facet(default)]
    pub variables: Variables,
}

impl Scene {
    /// Lay `node` out `width` wide instead of what it was drawn, its
    /// descendants following by their [`ResizeX`]: what a responsive host
    /// does to fit a surface to a box of other proportions without
    /// distorting it.
    pub fn resize_width(&mut self, node: NodeId, width: f64) {
        let Some(n) = self.nodes.get_mut(&node) else { return };
        let old = n.size.width;
        if old <= 0.0 || (width - old).abs() < 1e-6 {
            return;
        }
        n.size.width = width;
        self.follow_width(node, old, width);
    }

    fn follow_width(&mut self, parent: NodeId, old: f64, new: f64) {
        let kids = self.nodes.get(&parent).map(|p| p.children.clone()).unwrap_or_default();
        let (dw, ratio) = (new - old, new / old);
        for id in kids {
            let Some(n) = self.nodes.get_mut(&id) else { continue };
            let (x, w) = (n.transform.tx, n.size.width);
            let spans = w >= old * 0.95 && x <= old * 0.03;
            let rule = n.layout_child.resize_x.unwrap_or(if spans { ResizeX::Stretch } else { ResizeX::Center });
            let proportional = n.layout_child.resize_x.is_none() && !spans;
            let (nx, nw) = match rule {
                ResizeX::Left => (x, w),
                ResizeX::Right => (x + dw, w),
                // The default: its centre where the change puts it.
                ResizeX::Center if proportional => ((x + w / 2.0) * ratio - w / 2.0, w),
                ResizeX::Center => (x + dw / 2.0, w),
                ResizeX::Stretch => (x, (w + dw).max(0.0)),
                ResizeX::Scale => {
                    // Squeezed whole: its drawing scales, its size stays.
                    n.transform.a *= ratio;
                    n.transform.b *= ratio;
                    (x * ratio, w)
                }
            };
            n.transform.tx = nx;
            let resized = (nw - w).abs() > 1e-6;
            n.size.width = nw;
            if resized && w > 0.0 {
                self.follow_width(id, w, nw);
            }
        }
    }
}

impl Scene {
    /// An empty document with a root and one page.
    #[must_use]
    pub fn new(page_name: &str) -> Self {
        let mut root = Node::new(NodeKind::Document, "Document");
        let mut page = Node::new(NodeKind::Page, page_name);
        page.parent = Some(root.id);
        root.children.push(page.id);
        let root_id = root.id;
        let mut nodes = BTreeMap::new();
        nodes.insert(root.id, root);
        nodes.insert(page.id, page);
        Self { root: root_id, nodes, assets: BTreeMap::new(), variables: Variables::default() }
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    #[must_use]
    pub fn pages(&self) -> &[NodeId] {
        self.node(self.root).map_or(&[], |r| r.children.as_slice())
    }

    #[must_use]
    pub fn asset(&self, id: &AssetId) -> Option<&Asset> {
        self.assets.get(id)
    }

    /// Store `bytes` (deduplicated by content) and return its id.
    pub fn add_asset(&mut self, mime: impl Into<String>, bytes: Vec<u8>) -> AssetId {
        let asset = Asset::new(mime, bytes);
        let id = asset.id.clone();
        self.assets.entry(id.clone()).or_insert(asset);
        id
    }

    /// Pre-order walk of `id` and its descendants.
    #[must_use]
    pub fn walk(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let Some(node) = self.node(n) else { continue };
            out.push(n);
            stack.extend(node.children.iter().rev().copied());
        }
        out
    }

    /// Find a node by its import source id (e.g. a Figma node id).
    #[must_use]
    pub fn by_source_id(&self, source_id: &str) -> Option<&Node> {
        self.nodes.values().find(|n| n.source.as_ref().is_some_and(|s| s.id == source_id))
    }
}
