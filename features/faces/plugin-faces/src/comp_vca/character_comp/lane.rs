//! The character compressor in a host's FX column (see [`crate::lane`]).
//!
//! The unit already has both layouts, drawn for the rig: its phone lane
//! ([`super::unit_phone`], 760 × 164 — the ladder, the name and the ratio
//! lamps, then INPUT as the big dial, ATTACK, RELEASE, OUTPUT and IN) and
//! its front laid out to fill a box ([`super::unit_box`] — the meter and
//! switch sections across the top, the dials across the bottom). Here
//! they are bound to the rack face's own `charcomp/…` addresses, without
//! what the rig adds round them: its nameplate and preset browser, its
//! flip arrow (the comp catalog has no such params) and the maker's mark
//! (no face in this set prints a maker).

use frame::document::proto::{NodeId, NodeKind, Op, Scene};
use frame::surface::proto::{ParamInfo, Source};

use crate::face::Face;
use crate::lane::Shape;

pub fn faces() -> Vec<Face> {
    let rack = super::face();
    Shape::ALL
        .into_iter()
        .map(|shape| {
            let (w, h) = shape.size();
            let scene = match shape {
                Shape::Lane => super::unit_phone(
                    "charcomp",
                    "Character",
                    "charcomp/input",
                    "charcomp/output",
                    "charcomp/in",
                    false,
                ),
                Shape::Tall => super::unit_box(
                    "charcomp",
                    "Character",
                    "charcomp/input",
                    "charcomp/output",
                    "charcomp/in",
                    w,
                    h,
                ),
            };
            let scene = without_rig_parts(scene, &rack.params);
            let params = bound(&scene, &rack.params);
            Face {
                file: shape.file("10-character-comp"),
                plugin: "comp",
                scene,
                params,
                streams: rack.streams.clone(),
            }
        })
        .collect()
}

/// The rack face's params that `scene` binds a control to.
fn bound(scene: &frame::document::proto::Scene, params: &[ParamInfo]) -> Vec<ParamInfo> {
    let addrs: Vec<String> = scene
        .walk(scene.root)
        .into_iter()
        .filter_map(|id| scene.node(id))
        .filter_map(|n| n.control.as_ref()?.binding.as_ref())
        .filter_map(|b| match &b.source {
            Source::Param { addr } => Some(addr.0.clone()),
            _ => None,
        })
        .collect();
    params
        .iter()
        .filter(|p| addrs.contains(&p.addr.0))
        .cloned()
        .collect()
}

/// `scene` without the parts the rig draws round the unit: every top-level
/// part of the panel holding a control bound to a param the rack face
/// doesn't declare, and the maker's mark.
fn without_rig_parts(mut scene: Scene, params: &[ParamInfo]) -> Scene {
    let ours = |addr: &str| params.iter().any(|p| p.addr.0 == addr);
    let mut drop: Vec<NodeId> = Vec::new();
    for id in scene.walk(scene.root) {
        let Some(n) = scene.node(id) else { continue };
        let foreign = n
            .control
            .as_ref()
            .and_then(|c| c.binding.as_ref())
            .is_some_and(|b| matches!(&b.source, Source::Param { addr } if !ours(&addr.0)));
        let maker = matches!(&n.kind, NodeKind::Text(t) if ["Empirical", "Labs", "Empirical Labs"].contains(&t.characters.as_str()));
        if foreign || maker {
            drop.push(part_of(&scene, id));
        }
    }
    drop.sort_by_key(|id| id.0);
    drop.dedup();
    for id in drop {
        if scene.node(id).is_some() {
            let _ = scene.apply(Op::Remove { id });
        }
    }
    scene
}

/// The ancestor of `id` that is a direct child of the face's panel (the
/// first child of the page's top frame), or `id` itself.
fn part_of(scene: &Scene, id: NodeId) -> NodeId {
    let page = scene.pages()[0];
    let panel = scene
        .node(page)
        .and_then(|p| p.children.first().copied())
        .and_then(|top| scene.node(top))
        .and_then(|t| t.children.first().copied());
    let mut at = id;
    while let Some(parent) = scene.node(at).and_then(|n| n.parent) {
        if Some(parent) == panel {
            return at;
        }
        at = parent;
    }
    id
}
