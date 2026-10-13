//! Typed document edits.
//!
//! Every edit to a [`Scene`] is an [`Op`]. Applying an op returns the ops
//! that undo it, so undo/redo, AI tool transactions and rollback-on-error
//! all run on one mechanism ([`Scene::apply`], [`Scene::transact`]).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::model::{
    Asset, AssetId, AutoLayout, BlendMode, CornerRadii, Effect, LayoutChild, MaskKind, Node,
    NodeId, NodeKind, Paint, Scene, Size, Stroke, Transform,
};

/// One property assignment on one node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Prop {
    Name(String),
    Kind(NodeKind),
    Visible(bool),
    Locked(bool),
    Transform(Transform),
    Size(Size),
    Opacity(f64),
    Blend(BlendMode),
    CornerRadii(CornerRadii),
    ClipsContent(bool),
    Mask(Option<MaskKind>),
    Fills(Vec<Paint>),
    Stroke(Stroke),
    Effects(Vec<Effect>),
    FillGeometry(Vec<String>),
    StrokeGeometry(Vec<String>),
    SvgSnapshot(Option<AssetId>),
    Layout(Option<AutoLayout>),
    LayoutChild(LayoutChild),
    Control(Option<surface_proto::Control>),
    Emission(Option<crate::model::Emission>),
    Shader(Option<crate::model::ShaderFx>),
    Motion(Vec<motion_proto::Motion>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(u8)]
pub enum Op {
    /// Insert `node` (its `children` are ignored) under `parent` at `index`
    /// (clamped; `u32::MAX` = append).
    Insert {
        node: Node,
        parent: NodeId,
        index: u32,
    },
    /// Remove a node and its whole subtree.
    Remove {
        id: NodeId,
    },
    /// Reparent / reorder.
    Move {
        id: NodeId,
        parent: NodeId,
        index: u32,
    },
    Set {
        id: NodeId,
        prop: Prop,
    },
    PutAsset {
        asset: Asset,
    },
    DropAsset {
        id: AssetId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet, thiserror::Error)]
#[repr(u8)]
pub enum OpError {
    #[error("no node {0}")]
    NoSuchNode(NodeId),
    #[error("node {0} already exists")]
    DuplicateNode(NodeId),
    #[error("the root can't be removed or moved")]
    Root,
    #[error("moving {0} under its own descendant")]
    Cycle(NodeId),
}

impl Scene {
    /// Apply one op; returns the ops that revert it, in apply order.
    ///
    /// # Errors
    /// Leaves the scene untouched and returns why, when the op doesn't fit.
    pub fn apply(&mut self, op: Op) -> Result<Vec<Op>, OpError> {
        match op {
            Op::Insert {
                mut node,
                parent,
                index,
            } => {
                if self.nodes.contains_key(&node.id) {
                    return Err(OpError::DuplicateNode(node.id));
                }
                let id = node.id;
                let siblings = &mut self
                    .nodes
                    .get_mut(&parent)
                    .ok_or(OpError::NoSuchNode(parent))?
                    .children;
                let at = (index as usize).min(siblings.len());
                siblings.insert(at, id);
                node.parent = Some(parent);
                node.children.clear();
                self.nodes.insert(id, node);
                Ok(vec![Op::Remove { id }])
            }
            Op::Remove { id } => {
                if id == self.root {
                    return Err(OpError::Root);
                }
                let node = self.node(id).ok_or(OpError::NoSuchNode(id))?;
                let parent = node.parent.ok_or(OpError::Root)?;
                let index = self.index_in_parent(id).unwrap_or(0);
                // Inverse: re-insert the subtree top-down, children appended in order.
                let mut inverse = Vec::new();
                for (n, (p, i)) in self.subtree_with_slots(id, parent, index) {
                    inverse.push(Op::Insert {
                        node: n,
                        parent: p,
                        index: i,
                    });
                }
                for n in self.walk(id) {
                    self.nodes.remove(&n);
                }
                if let Some(p) = self.nodes.get_mut(&parent) {
                    p.children.retain(|c| *c != id);
                }
                Ok(inverse)
            }
            Op::Move { id, parent, index } => {
                if id == self.root {
                    return Err(OpError::Root);
                }
                if !self.nodes.contains_key(&parent) {
                    return Err(OpError::NoSuchNode(parent));
                }
                if self.walk(id).contains(&parent) {
                    return Err(OpError::Cycle(id));
                }
                let old_parent = self
                    .node(id)
                    .ok_or(OpError::NoSuchNode(id))?
                    .parent
                    .ok_or(OpError::Root)?;
                let old_index = self.index_in_parent(id).unwrap_or(0);
                if let Some(p) = self.nodes.get_mut(&old_parent) {
                    p.children.retain(|c| *c != id);
                }
                let siblings = &mut self
                    .nodes
                    .get_mut(&parent)
                    .ok_or(OpError::NoSuchNode(parent))?
                    .children;
                let at = (index as usize).min(siblings.len());
                siblings.insert(at, id);
                if let Some(n) = self.nodes.get_mut(&id) {
                    n.parent = Some(parent);
                }
                Ok(vec![Op::Move {
                    id,
                    parent: old_parent,
                    index: u32::try_from(old_index).unwrap_or(u32::MAX),
                }])
            }
            Op::Set { id, prop } => {
                let node = self.nodes.get_mut(&id).ok_or(OpError::NoSuchNode(id))?;
                let old = set_prop(node, prop);
                Ok(vec![Op::Set { id, prop: old }])
            }
            Op::PutAsset { asset } => {
                let id = asset.id.clone();
                match self.assets.insert(id.clone(), asset) {
                    Some(previous) => Ok(vec![Op::PutAsset { asset: previous }]),
                    None => Ok(vec![Op::DropAsset { id }]),
                }
            }
            Op::DropAsset { id } => Ok(self
                .assets
                .remove(&id)
                .map(|asset| vec![Op::PutAsset { asset }])
                .unwrap_or_default()),
        }
    }

    /// Apply ops atomically: all of them, or (on the first error) none.
    /// Returns the inverse transaction (apply it to undo).
    ///
    /// # Errors
    /// The first failing op's error; the scene is rolled back.
    pub fn transact(&mut self, ops: impl IntoIterator<Item = Op>) -> Result<Vec<Op>, OpError> {
        let mut undo: Vec<Vec<Op>> = Vec::new();
        for op in ops {
            match self.apply(op) {
                Ok(inv) => undo.push(inv),
                Err(e) => {
                    for inv in undo.into_iter().rev() {
                        for op in inv {
                            let _ = self.apply(op);
                        }
                    }
                    return Err(e);
                }
            }
        }
        Ok(undo.into_iter().rev().flatten().collect())
    }

    fn index_in_parent(&self, id: NodeId) -> Option<usize> {
        let parent = self.node(id)?.parent?;
        self.node(parent)?.children.iter().position(|c| *c == id)
    }

    /// `(node without children, (parent, index))` for `id`'s subtree in pre-order.
    fn subtree_with_slots(
        &self,
        id: NodeId,
        parent: NodeId,
        index: usize,
    ) -> Vec<(Node, (NodeId, u32))> {
        let mut out = Vec::new();
        let mut stack = vec![(id, parent, index)];
        while let Some((n, p, i)) = stack.pop() {
            let Some(node) = self.node(n) else { continue };
            let mut copy = node.clone();
            copy.children.clear();
            out.push((copy, (p, u32::try_from(i).unwrap_or(u32::MAX))));
            for (ci, c) in node.children.iter().enumerate().rev() {
                stack.push((*c, n, ci));
            }
        }
        out
    }
}

/// Assign `prop`, returning the previous value as a `Prop`.
fn set_prop(node: &mut Node, prop: Prop) -> Prop {
    use std::mem::replace;
    match prop {
        Prop::Name(v) => Prop::Name(replace(&mut node.name, v)),
        Prop::Kind(v) => Prop::Kind(replace(&mut node.kind, v)),
        Prop::Visible(v) => Prop::Visible(replace(&mut node.visible, v)),
        Prop::Locked(v) => Prop::Locked(replace(&mut node.locked, v)),
        Prop::Transform(v) => Prop::Transform(replace(&mut node.transform, v)),
        Prop::Size(v) => Prop::Size(replace(&mut node.size, v)),
        Prop::Opacity(v) => Prop::Opacity(replace(&mut node.opacity, v)),
        Prop::Blend(v) => Prop::Blend(replace(&mut node.blend, v)),
        Prop::CornerRadii(v) => Prop::CornerRadii(replace(&mut node.corner_radii, v)),
        Prop::ClipsContent(v) => Prop::ClipsContent(replace(&mut node.clips_content, v)),
        Prop::Mask(v) => Prop::Mask(replace(&mut node.mask, v)),
        Prop::Fills(v) => Prop::Fills(replace(&mut node.fills, v)),
        Prop::Stroke(v) => Prop::Stroke(replace(&mut node.stroke, v)),
        Prop::Effects(v) => Prop::Effects(replace(&mut node.effects, v)),
        Prop::FillGeometry(v) => Prop::FillGeometry(replace(&mut node.fill_geometry, v)),
        Prop::StrokeGeometry(v) => Prop::StrokeGeometry(replace(&mut node.stroke_geometry, v)),
        Prop::SvgSnapshot(v) => Prop::SvgSnapshot(replace(&mut node.svg_snapshot, v)),
        Prop::Layout(v) => Prop::Layout(replace(&mut node.layout, v)),
        Prop::LayoutChild(v) => Prop::LayoutChild(replace(&mut node.layout_child, v)),
        Prop::Control(v) => Prop::Control(replace(&mut node.control, v)),
        Prop::Emission(v) => Prop::Emission(replace(&mut node.emission, v)),
        Prop::Shader(v) => Prop::Shader(replace(&mut node.shader, v)),
        Prop::Motion(v) => Prop::Motion(replace(&mut node.motion, v)),
    }
}

/// Linear undo/redo over transactions.
#[derive(Debug, Default, Clone)]
pub struct History {
    undo: Vec<Vec<Op>>,
    redo: Vec<Vec<Op>>,
}

impl History {
    /// Run `ops` as one undoable step.
    ///
    /// # Errors
    /// See [`Scene::transact`]; nothing is recorded on error.
    pub fn commit(
        &mut self,
        scene: &mut Scene,
        ops: impl IntoIterator<Item = Op>,
    ) -> Result<(), OpError> {
        let inverse = scene.transact(ops)?;
        self.undo.push(inverse);
        self.redo.clear();
        Ok(())
    }

    /// Returns `false` when there was nothing to undo.
    ///
    /// # Errors
    /// Only if the scene was edited outside this history in a conflicting way.
    pub fn undo(&mut self, scene: &mut Scene) -> Result<bool, OpError> {
        let Some(tx) = self.undo.pop() else {
            return Ok(false);
        };
        let redo = scene.transact(tx)?;
        self.redo.push(redo);
        Ok(true)
    }

    /// Returns `false` when there was nothing to redo.
    ///
    /// # Errors
    /// Only if the scene was edited outside this history in a conflicting way.
    pub fn redo(&mut self, scene: &mut Scene) -> Result<bool, OpError> {
        let Some(tx) = self.redo.pop() else {
            return Ok(false);
        };
        let undo = scene.transact(tx)?;
        self.undo.push(undo);
        Ok(true)
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
