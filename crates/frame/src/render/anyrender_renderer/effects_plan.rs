//! What the effects stage needs beyond the paint primitives: emissive draws
//! (light a node gives off) and shader regions (post-effects on a subtree).
//!
//! Collected alongside the primitives by
//! [`build_scene_primitives_and_effects`](super::build_scene_primitives_and_effects);
//! a scene without `emission`/`shader` yields an empty plan and exactly the
//! primitives [`build_scene_primitives_with`](super::build_scene_primitives_with)
//! would without a glow.

use std::collections::BTreeMap;
use std::ops::Range;

use crate::document::proto::NodeId;

use super::types::{AffineTransform2D, PaintPrimitive, Rgba};

/// One emitting node, ready to bloom.
#[derive(Debug, Clone, PartialEq)]
pub struct EmissiveDraw {
    pub node_id: NodeId,
    /// The node's own geometry, recoloured to the (opaque) emission colour,
    /// in flattened scene space (like every primitive).
    pub primitives: Vec<PaintPrimitive>,
    /// The enclosing groups' transform (flattened scene → world), already
    /// composed; paint `primitives` under `root · world`.
    pub world: AffineTransform2D,
    /// The ancestors' open clips (`ClipStart`s), outermost first, each with
    /// the world transform it was opened under: the light is cut to them
    /// like the node itself is, so a needle's glow stays inside its window.
    pub clips: Vec<(AffineTransform2D, PaintPrimitive)>,
    /// The emission colour (alpha 1).
    pub color: Rgba,
    /// Light strength: emission intensity × the node's effective opacity.
    pub strength: f64,
    /// Blur radius in scene px.
    pub radius: f64,
    /// Params of the `flicker` shader on this node or an ancestor, when there
    /// is one: its light flickers with it.
    pub flicker: Option<BTreeMap<String, f64>>,
}

/// One node whose subtree runs through a shader.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderRegion {
    pub node_id: NodeId,
    /// The node's primitives (its own and its subtree's, including its own
    /// group/layer/clip wrappers) as an index range into the primitive list.
    pub range: Range<usize>,
    /// Node-local box space (`0..w × 0..h`) → world scene space.
    pub world: AffineTransform2D,
    pub width: f64,
    pub height: f64,
    /// The shader id (`glass`, `crt`, …, or `asset:<id>`).
    pub shader: String,
    pub params: BTreeMap<String, f64>,
    /// The nearest control (this node or an ancestor) whose position feeds the
    /// shader's `value`, with that control's default position.
    pub control: Option<(NodeId, Option<f64>)>,
    /// The live data the shader reads (resolved per frame by the host).
    pub inputs: Vec<crate::document::proto::ShaderInput>,
    /// The node's effective opacity (its own × its ancestors'): the effect's
    /// result is laid down at this opacity, as the subtree it replaces
    /// would have been.
    pub opacity: f64,
}

/// Everything the effects stage runs for one frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectsPlan {
    pub emissive: Vec<EmissiveDraw>,
    pub shaders: Vec<ShaderRegion>,
}

impl EffectsPlan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.emissive.is_empty() && self.shaders.is_empty()
    }

    /// Whether frames differ over time (a `flicker`, or a shader — built-in
    /// or custom — that reads `time_s`): the host must keep redrawing.
    #[must_use]
    pub fn time_dependent(&self, source: impl Fn(&str) -> Option<String>) -> bool {
        self.emissive.iter().any(|e| e.flicker.is_some())
            || self.shaders.iter().any(|s| match s.shader.as_str() {
                "flicker" => true,
                id => source(id).is_some_and(|src| src.contains("time_s")),
            })
    }
}

/// The tube-lamp brightness of a `flicker` at `time_s` (1 = steady), shared by
/// the shader and the emission it modulates so both flicker together.
/// `strength` is the depth (0..1), `speed` the jitter rate (Hz).
#[must_use]
pub fn flicker_level(time_s: f64, params: &BTreeMap<String, f64>) -> f64 {
    let depth = params
        .get("strength")
        .copied()
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    let speed = params.get("speed").copied().unwrap_or(10.0).max(0.0);
    let hash = |x: f64| ((x * 12.9898).sin() * 43_758.545_3).rem_euclid(1.0);
    let x = time_s * speed;
    let (i, f) = (x.floor(), x - x.floor());
    let s = f * f * (3.0 - 2.0 * f);
    let noise = hash(i) * (1.0 - s) + hash(i + 1.0) * s;
    let dropout = if hash((x * 0.37).floor() + 7.0) > 0.9 {
        0.6
    } else {
        0.0
    };
    (1.0 - depth * (noise * 0.5 + dropout)).clamp(0.0, 1.0)
}
