//! Turning a [`Node`] tree into something playable.
//!
//! A node tree in a [`NodeLibrary`] is *potential*: nodes reference other
//! nodes by id, each has several variants, and variants carry overrides that
//! reach down into children. Resolving picks one variant at every level,
//! follows the references, and applies the overrides — producing a
//! [`Resolved`] tree where every choice has been made.
//!
//! That is the whole performance model in one function. A Patch pointing at a
//! node-and-variant, a Scene doing the same, a Snapshot voicing one level:
//! they all mean "resolve this node with this variant".
//!
//! # Two things that must not happen on a stage
//!
//! **A cycle must not hang.** References are ids, so a node can reference an
//! ancestor — by mistake, or by a hand-edited styx file. Resolution tracks
//! the path it is on and refuses rather than recursing forever. The audio
//! thread cannot afford to find out the hard way.
//!
//! **A missing reference must not be fatal.** A library that lost a node —
//! an edit, a half-finished sync — still has to make sound. A dangling child
//! is dropped with a reason recorded, not propagated as an error that takes
//! the whole rig down.

use crate::model::Block;
use crate::node::{Combine, Content, NodeId, NodeLibrary, Role, Selection, VariantId};
use crate::node_routing::{AudioSend, ModRoute, Setting, Zone};
use crate::overrides::{NodeOverrideOp, NodePath, NodePathSegment, Override};

/// A node with every choice made: one variant picked, references followed,
/// overrides applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub id: NodeId,
    pub name: String,
    pub role: Role,
    pub combine: Combine,
    /// Bypassed — on the node itself, or by an override from above.
    pub bypassed: bool,
    pub content: ResolvedContent,

    // ─── Carried through verbatim ────────────────────────────────
    //
    // A renderer gets the whole node, not just its shape: the fader, the
    // zone and the routing axis are as much a part of "what this sounds
    // like" as the blocks are. Dropping them here is what made `to_chain`
    // lossy for the keys rig.
    /// Which kind of playable thing, when [`role`](Self::role) is an Engine.
    pub engine_type: crate::model::EngineType,
    pub input_db: f32,
    pub output_db: f32,
    /// Control-rate modulators, resolved like any other node. Off the audio
    /// path — they reach parameters through [`mod_routes`](Self::mod_routes).
    pub modulators: Vec<Resolved>,
    pub sends: Vec<AudioSend>,
    pub mod_routes: Vec<ModRoute>,
    pub settings: Vec<Setting>,
    /// The keyboard window that reaches this subtree, as the node declares
    /// it. Nested zones multiply — that is the router's job, not the
    /// resolver's.
    pub zone: Zone,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedContent {
    Leaf {
        block_type: crate::block::BlockType,
        block: Block,
    },
    Children(Vec<Resolved>),
}

/// Why a resolution could not be completed at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// The root is not in the library.
    NoSuchNode(NodeId),
    /// A node referenced itself, directly or through its descendants.
    Cycle { at: NodeId },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSuchNode(id) => write!(f, "no node {}", id.as_str()),
            Self::Cycle { at } => write!(f, "node {} references itself", at.as_str()),
        }
    }
}

impl std::error::Error for ResolveError {}

/// What was dropped or ignored along the way.
///
/// Resolution succeeds with a report rather than failing: a rig missing one
/// block should play the rest, and a UI should be able to say what is wrong
/// instead of showing nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Children that named a node the library does not hold.
    pub missing: Vec<NodeId>,
    /// Overrides whose path matched nothing in the resolved tree.
    pub unmatched_overrides: Vec<String>,
}

impl Report {
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.unmatched_overrides.is_empty()
    }
}

/// Resolve `root` with `variant` (its default when `None`).
///
/// # Errors
///
/// [`ResolveError::NoSuchNode`] if the root is absent, [`ResolveError::Cycle`]
/// if the tree references itself. Everything else is reported, not failed.
pub fn resolve(
    library: &NodeLibrary,
    root: &NodeId,
    variant: Option<&VariantId>,
) -> Result<(Resolved, Report), ResolveError> {
    let mut report = Report::default();
    let mut path = Vec::new();
    let resolved = resolve_node(
        library,
        root,
        variant,
        &Inherited::default(),
        &mut path,
        &mut report,
    )?;
    Ok((resolved, report))
}

/// What an ancestor's variant decided that still applies further down.
///
/// Both halves address a node **by id**, and ids are unique, so neither needs
/// to say how deep the node sits — and therefore both should reach however
/// deep it actually is. See [`effective_selections`] for what went wrong when
/// they reached exactly one level.
#[derive(Default, Clone)]
struct Inherited {
    selections: Vec<Selection>,
    /// `ReplaceRef` overrides whose path is a single segment: "wherever this
    /// node is below me, use that one instead".
    replacements: Vec<Override>,
}

fn resolve_node(
    library: &NodeLibrary,
    id: &NodeId,
    variant: Option<&VariantId>,
    inherited: &Inherited,
    path: &mut Vec<NodeId>,
    report: &mut Report,
) -> Result<Resolved, ResolveError> {
    if path.contains(id) {
        return Err(ResolveError::Cycle { at: id.clone() });
    }
    let node = library
        .get(id)
        .ok_or_else(|| ResolveError::NoSuchNode(id.clone()))?;

    // The variant asked for, else the node's own default. `default_variant`
    // already falls back to the first when the default id has gone missing.
    let chosen = variant
        .and_then(|v| node.variant(v))
        .or_else(|| node.default_variant());

    let content = match &node.content {
        Content::Leaf { block_type, block } => ResolvedContent::Leaf {
            block_type: *block_type,
            block: block.clone(),
        },
        Content::Children { nodes } => {
            path.push(id.clone());
            let scope = Inherited {
                selections: effective_selections(chosen, &inherited.selections),
                replacements: effective_replacements(chosen, &inherited.replacements),
            };
            let mut children = Vec::with_capacity(nodes.len());
            for child in nodes {
                // `ReplaceRef` swaps *which node* sits here, so it has to be
                // read before the child is resolved — this is how one patch
                // recalls the chain with a different amp in it.
                let child = replacement_for(library, &scope.replacements, child).unwrap_or(child);
                // A child nothing has named uses its own default.
                let child_variant = scope
                    .selections
                    .iter()
                    .find(|s| &s.node == child)
                    .map(|s| &s.variant);
                match resolve_node(library, child, child_variant, &scope, path, report) {
                    Ok(resolved) => children.push(resolved),
                    // A dangling reference is a hole, not a failure: play the
                    // rest of the rig and say what is missing.
                    Err(ResolveError::NoSuchNode(missing)) => report.missing.push(missing),
                    Err(cycle) => {
                        path.pop();
                        return Err(cycle);
                    }
                }
            }
            path.pop();
            ResolvedContent::Children(children)
        }
    };

    // Modulators resolve like children, but off the audio path, so they are
    // resolved beside the content rather than inside it. A missing one is a
    // hole in the mod matrix, reported the same way.
    let mut modulators = Vec::with_capacity(node.modulators.len());
    path.push(id.clone());
    for modulator in &node.modulators {
        match resolve_node(library, modulator, None, inherited, path, report) {
            Ok(resolved) => modulators.push(resolved),
            Err(ResolveError::NoSuchNode(missing)) => report.missing.push(missing),
            Err(cycle) => {
                path.pop();
                return Err(cycle);
            }
        }
    }
    path.pop();

    let mut resolved = Resolved {
        id: node.id.clone(),
        name: node.name.clone(),
        role: node.role,
        combine: node.combine,
        bypassed: node.bypassed,
        content,
        engine_type: node.engine_type,
        input_db: node.input_db,
        output_db: node.output_db,
        modulators,
        sends: node.sends.clone(),
        mod_routes: node.mod_routes.clone(),
        settings: node.settings.clone(),
        zone: node.zone,
    };

    // Overrides are applied after the subtree exists, because they reach
    // *into* it — a path names descendants that resolution has just built.
    if let Some(chosen) = chosen {
        for ov in &chosen.overrides {
            if !apply_override(&mut resolved, ov) {
                report.unmatched_overrides.push(describe(&ov.path));
            }
        }
    }
    Ok(resolved)
}

/// The selections in force inside a node: its chosen variant's own, plus
/// everything an ancestor's variant selected that it has not overruled.
///
/// A [`Selection`] names a node **by id**, and ids are unique, so it does not
/// need to say how deep that node sits — which means it should reach however
/// deep the node actually is. Without this it reached exactly one level, and
/// the depth a node happened to sit at silently decided whether a variant
/// applied: the guitar rig's drive pedals stopped obeying their patch the
/// moment they were grouped into a Drive module, and played their first
/// capture instead of the selected one. Nothing said so; it was just the
/// wrong pedal setting.
///
/// The nearer variant wins, so a Layer can pick a capture its Engine also
/// picked differently — the same precedence overrides have.
fn effective_selections(
    variant: Option<&crate::node::Variant>,
    inherited: &[Selection],
) -> Vec<Selection> {
    let own = variant.map_or(&[][..], |v| v.selections.as_slice());
    let mut out = own.to_vec();
    for selection in inherited {
        if !out.iter().any(|s| s.node == selection.node) {
            out.push(selection.clone());
        }
    }
    out
}

/// The single-segment `ReplaceRef` overrides in force inside a node: its
/// chosen variant's, plus an ancestor's.
///
/// Inherited ones come last, so a nearer variant's replacement of the same
/// node wins — the same precedence as selections.
fn effective_replacements(
    variant: Option<&crate::node::Variant>,
    inherited: &[Override],
) -> Vec<Override> {
    let mut out: Vec<Override> = variant
        .map(|v| {
            v.overrides
                .iter()
                .filter(|ov| {
                    matches!(ov.op, NodeOverrideOp::ReplaceRef { id: _ })
                        && ov.path.segments().len() == 1
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    out.extend(inherited.iter().cloned());
    out
}

/// The node a variant wants in `child`'s place, if it names one.
///
/// A `ReplaceRef` whose path is a single segment naming this child swaps the
/// reference outright. That is "recall this patch, but with a different amp
/// loaded" — the referenced node is untouched, and every other patch using it
/// is unaffected.
///
/// A replacement the library does not hold is ignored rather than followed
/// into a hole: the original child still plays.
fn replacement_for<'a>(
    library: &'a NodeLibrary,
    replacements: &[Override],
    child: &'a NodeId,
) -> Option<&'a NodeId> {
    replacements.iter().find_map(|ov| {
        let NodeOverrideOp::ReplaceRef { id: with } = &ov.op else {
            return None;
        };
        // One segment, naming this child by id or by name.
        let [segment] = ov.path.segments() else {
            return None;
        };
        let wanted = segment_id(segment)?;
        let names_child =
            child.as_str() == wanted || library.get(child).is_some_and(|n| &n.name == wanted);
        if !names_child {
            return None;
        }
        library
            .nodes
            .iter()
            .find_map(|n| (n.id.as_str() == with || &n.name == with).then_some(&n.id))
    })
}

/// Apply one override to a resolved subtree. Returns whether it matched.
fn apply_override(root: &mut Resolved, ov: &Override) -> bool {
    // Already acted on, during child resolution — and it cannot be checked
    // here, because the node its path names is precisely the one it replaced.
    if matches!(ov.op, NodeOverrideOp::ReplaceRef { id: _ }) {
        return true;
    }
    let segments = ov.path.segments();
    let Some((last, container_path)) = segments.split_last() else {
        return false;
    };

    // A path ending in a parameter addresses the parameter of the block the
    // rest of the path walks to; any other path addresses the node itself.
    let (target_path, param) = match last {
        NodePathSegment::Parameter { id: name } => (container_path, Some(name.as_str())),
        _ => (segments, None),
    };

    let Some(node) = walk(root, target_path) else {
        return false;
    };

    match (&ov.op, param) {
        (NodeOverrideOp::Set { value }, Some(param)) => set_parameter(node, param, value.get()),
        (NodeOverrideOp::Bypass { bypassed: on }, None) => {
            node.bypassed = *on;
            true
        }
        (NodeOverrideOp::Enable { enabled: on }, None) => {
            node.bypassed = !*on;
            true
        }
        // Handled during child resolution, not here — it changes which node
        // sits at a position, so it must act before that node is resolved.
        (NodeOverrideOp::ReplaceRef { id: _ }, _) => true,
        // `Set` with no parameter, and the flow mutations, are refused by
        // `override_policy` before they ever reach here.
        _ => false,
    }
}

/// Follow a path of named segments down a resolved tree.
///
/// Matches a segment against a node's **id first, then its name**. Ids are
/// what a stored override should carry; names are accepted so a hand-written
/// styx file stays readable.
/// Walk a path from `root`, matching each segment against a **descendant**
/// rather than only a direct child.
///
/// So a path is relative, and its segments need not be consecutive: `block
/// VERB 1 -> param mix` reaches the reverb wherever it sits, and `module Time
/// -> block VERB 1` reaches it via the Time module without having to name
/// every container in between.
///
/// This is the same rule selections and `ReplaceRef` follow, and it is what
/// the rig itself has always done — the guitar rig's own override applier
/// finds a block by name anywhere in the chain. Requiring direct children
/// made an override's survival depend on how deeply the block happened to be
/// grouped: grouping the chain into Module containers silently stopped every
/// patch override from applying, which on stage is a patch that does not
/// change what it promised to change.
///
/// First match wins, depth-first in chain order. Two nodes sharing a name is
/// the ambiguity a name always carries — an id cannot be ambiguous, which is
/// why an id is the better thing to address with.
fn walk<'a>(root: &'a mut Resolved, path: &[NodePathSegment]) -> Option<&'a mut Resolved> {
    let mut current = root;
    for segment in path {
        let Some(wanted) = segment_id(segment) else {
            continue;
        };
        current = find_descendant(current, wanted)?;
    }
    Some(current)
}

/// The nearest descendant matching `wanted` by id or name, depth-first.
fn find_descendant<'a>(node: &'a mut Resolved, wanted: &str) -> Option<&'a mut Resolved> {
    let ResolvedContent::Children(children) = &mut node.content else {
        return None;
    };
    // Breadth first among siblings, so a direct child always beats a deeper
    // namesake — the nearer answer is the one an author meant.
    if let Some(index) = children
        .iter()
        .position(|c| c.id.as_str() == wanted || c.name == wanted)
    {
        return children.get_mut(index);
    }
    children
        .iter_mut()
        .find_map(|child| find_descendant(child, wanted))
}

/// A readable rendering of a path, for a report a person has to act on.
fn describe(path: &NodePath) -> String {
    path.segments()
        .iter()
        .map(|s| match s {
            NodePathSegment::Engine { id } => format!("engine {id}"),
            NodePathSegment::Layer { id } => format!("layer {id}"),
            NodePathSegment::Module { id } => format!("module {id}"),
            NodePathSegment::Block { id } => format!("block {id}"),
            NodePathSegment::Parameter { id } => format!("param {id}"),
            NodePathSegment::Raw { text: raw } => raw.clone(),
        })
        .collect::<Vec<_>>()
        .join(" -> ")
}

const fn segment_id(segment: &NodePathSegment) -> Option<&String> {
    match segment {
        NodePathSegment::Engine { id }
        | NodePathSegment::Layer { id }
        | NodePathSegment::Module { id }
        | NodePathSegment::Block { id } => Some(id),
        // A parameter is handled by the caller; a raw segment addresses
        // nothing structural.
        NodePathSegment::Parameter { id: _ } | NodePathSegment::Raw { text: _ } => None,
    }
}

fn set_parameter(node: &mut Resolved, param: &str, value: f32) -> bool {
    // A value the domain could not range is carried verbatim as a setting.
    // It is still a parameter, so an override still has to reach it — see
    // [`RAW_PARAM`](crate::node_routing::RAW_PARAM). Checked first because a
    // node carrying one does not have the ranged parameter to find.
    let raw = format!("{}{param}", crate::node_routing::RAW_PARAM);
    if let Some(setting) = node.settings.iter_mut().find(|s| s.name == raw) {
        setting.value = value.to_string();
        return true;
    }
    let ResolvedContent::Leaf { block, .. } = &mut node.content else {
        return false;
    };
    let Some(index) = block
        .parameters()
        .iter()
        .position(|p| p.id() == param || p.name() == param)
    else {
        return false;
    };
    block.set_parameter_value(index, value);
    true
}

impl Resolved {
    /// Every leaf in render order, flattened — what a serial chain plays.
    #[must_use]
    pub fn leaves(&self) -> Vec<&Self> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves<'a>(&'a self, out: &mut Vec<&'a Self>) {
        match &self.content {
            ResolvedContent::Leaf { .. } => out.push(self),
            ResolvedContent::Children(children) => {
                for child in children {
                    child.collect_leaves(out);
                }
            }
        }
    }

    /// A descendant by id, including self — mutably.
    ///
    /// The read-only [`find`](Self::find) answers what a tree *is*; this is
    /// for changing what it resolved to, which is what an editor does before
    /// saving the result as a preset.
    pub fn find_mut(&mut self, id: &NodeId) -> Option<&mut Self> {
        if &self.id == id {
            return Some(self);
        }
        match &mut self.content {
            ResolvedContent::Leaf { .. } => None,
            ResolvedContent::Children(children) => children.iter_mut().find_map(|c| c.find_mut(id)),
        }
    }

    /// A descendant by id, including self.
    #[must_use]
    pub fn find(&self, id: &NodeId) -> Option<&Self> {
        if &self.id == id {
            return Some(self);
        }
        match &self.content {
            ResolvedContent::Leaf { .. } => None,
            ResolvedContent::Children(children) => children.iter().find_map(|c| c.find(id)),
        }
    }
}
