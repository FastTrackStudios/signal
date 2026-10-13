//! Faces that move: shape-provider artwork and the presets faces share.

use std::collections::BTreeMap;

use document_proto::motion::{Motion, Num, Target};
use document_proto::{Node, NodeKind, Size, Transform};
use motion_runtime::{ShapeCtx, ShapeRegistry};

/// A vector spanning a `w × h` parent whose outline is shape provider
/// `provider`'s `part`, redrawn as its inputs change: `fixed` numbers,
/// and `live` expressions drawn at rest with `rest` (the same names).
/// Paint and stroke it like any vector.
#[must_use]
pub fn shape_node(name: &str, provider: &str, part: &str, (w, h): (f64, f64), fixed: &[(&str, f64)], live: &[(&str, &str)], rest: &[(&str, f64)]) -> Node {
    let args: Vec<(&str, f64)> = fixed.iter().chain(rest).copied().collect();
    let ctx = ShapeCtx { part, width: w, height: h, args: &args };
    let geometry = ShapeRegistry::builtin().get(provider).and_then(|f| f(&ctx)).unwrap_or_default();
    let mut inputs: BTreeMap<String, Num> = fixed.iter().map(|(k, v)| ((*k).to_string(), Num::Value(*v))).collect();
    inputs.extend(live.iter().map(|(k, e)| ((*k).to_string(), Num::Expr((*e).to_string()))));
    let mut n = Node::new(NodeKind::Vector, name);
    n.transform = Transform::IDENTITY;
    n.size = Size { width: w, height: h };
    n.fill_geometry = geometry;
    n.motion = vec![Motion::new(Target::Shape { provider: provider.into(), part: Some(part.into()), inputs })];
    n
}
