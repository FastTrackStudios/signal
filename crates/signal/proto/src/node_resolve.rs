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
use crate::node::{Combine, Content, NodeId, NodeLibrary, Role, VariantId};
use crate::overrides::{NodeOverrideOp, NodePath, NodePathSegment, Override};

/// A node with every choice made: one variant picked, references followed,
/// overrides applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub id: NodeId,
    pub name: String,
    pub role: Role,
    pub combine: Combine,
    /// Bypassed by an override from somewhere above.
    pub bypassed: bool,
    pub content: ResolvedContent,
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
    let resolved = resolve_node(library, root, variant, &mut path, &mut report)?;
    Ok((resolved, report))
}

fn resolve_node(
    library: &NodeLibrary,
    id: &NodeId,
    variant: Option<&VariantId>,
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
            let mut children = Vec::with_capacity(nodes.len());
            for child in nodes {
                // `ReplaceRef` swaps *which node* sits here, so it has to be
                // read before the child is resolved — this is how one patch
                // recalls the chain with a different amp in it.
                let child = replacement_for(library, chosen, child).unwrap_or(child);
                // A child the variant does not name uses its own default.
                let child_variant = chosen.and_then(|v| v.selection_for(child));
                match resolve_node(library, child, child_variant, path, report) {
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

    let mut resolved = Resolved {
        id: node.id.clone(),
        name: node.name.clone(),
        role: node.role,
        combine: node.combine,
        bypassed: false,
        content,
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
    variant: Option<&'a crate::node::Variant>,
    child: &'a NodeId,
) -> Option<&'a NodeId> {
    let variant = variant?;
    variant.overrides.iter().find_map(|ov| {
        let NodeOverrideOp::ReplaceRef { id: with } = &ov.op else {
            return None;
        };
        // One segment, naming this child by id or by name.
        let [segment] = ov.path.segments() else {
            return None;
        };
        let wanted = segment_id(segment)?;
        let names_child = child.as_str() == wanted
            || library.get(child).is_some_and(|n| &n.name == wanted);
        if !names_child {
            return None;
        }
        library.nodes.iter().find_map(|n| {
            (n.id.as_str() == with || &n.name == with).then_some(&n.id)
        })
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
fn walk<'a>(root: &'a mut Resolved, path: &[NodePathSegment]) -> Option<&'a mut Resolved> {
    let mut current = root;
    for segment in path {
        let Some(wanted) = segment_id(segment) else {
            continue;
        };
        let ResolvedContent::Children(children) = &mut current.content else {
            return None;
        };
        current = children
            .iter_mut()
            .find(|c| c.id.as_str() == wanted || &c.name == wanted)?;
    }
    Some(current)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{Node, Variant};
    use signal_macromod::BlockParameter;

    /// A block with one named parameter, so overrides have something to hit.
    fn block_with(param: &str, value: f32) -> Block {
        Block::from_parameters(vec![BlockParameter::new(param, param, value)])
    }

    fn param_of(resolved: &Resolved, node: &NodeId, param: &str) -> Option<f32> {
        match &resolved.find(node)?.content {
            ResolvedContent::Leaf { block, .. } => block
                .parameters()
                .iter()
                .find(|p| p.id() == param)
                .map(|p| p.value().get()),
            ResolvedContent::Children(_) => None,
        }
    }

    #[test]
    fn a_tree_resolves_to_its_leaves_in_order() {
        let mut lib = NodeLibrary::new();
        let comp = Node::leaf("Comp", crate::block::BlockType::Compressor, block_with("threshold", -40.0));
        let amp = Node::leaf("AC30", crate::block::BlockType::Amp, block_with("gain", 0.5));
        let chain = Node::container("Worship", Role::Preset, Combine::Serial)
            .with_child(&comp)
            .with_child(&amp);
        let root = chain.id.clone();
        for n in [comp, amp, chain] {
            lib.insert(n);
        }

        let (resolved, report) = resolve(&lib, &root, None).expect("resolves");
        assert!(report.is_clean());
        let names: Vec<_> = resolved.leaves().iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Comp", "AC30"]);
    }

    #[test]
    fn an_override_reaches_one_parameter_of_one_block() {
        let mut lib = NodeLibrary::new();
        let verb = Node::leaf("VERB 1", crate::block::BlockType::Reverb, block_with("mix", 0.08));
        let verb_id = verb.id.clone();

        // "Ambient": the same reverb, wetter. The block itself is untouched.
        let ambient = Variant::new("Ambient");
        let ambient_id = ambient.id.clone();
        let mut ambient = ambient;
        ambient.overrides.push(Override::set(
            NodePath::new(vec![
                NodePathSegment::Block { id: "VERB 1".into() },
                NodePathSegment::Parameter { id: "mix".into() },
            ]),
            0.35,
        ));

        let patch = Node::container("Clean", Role::Preset, Combine::Serial)
            .with_child(&verb)
            .with_variant(ambient);
        let root = patch.id.clone();
        lib.insert(verb);
        lib.insert(patch);

        let (plain, _) = resolve(&lib, &root, None).expect("resolves");
        assert_eq!(param_of(&plain, &verb_id, "mix"), Some(0.08));

        let (wet, report) = resolve(&lib, &root, Some(&ambient_id)).expect("resolves");
        assert_eq!(param_of(&wet, &verb_id, "mix"), Some(0.35));
        assert!(report.is_clean());

        // And the stored node is unchanged — one thing on disk, many voicings.
        assert_eq!(
            lib.get(&verb_id).map(|n| matches!(n.content, Content::Leaf { .. })),
            Some(true)
        );
        let (again, _) = resolve(&lib, &root, None).expect("resolves");
        assert_eq!(param_of(&again, &verb_id, "mix"), Some(0.08));
    }

    #[test]
    fn a_variant_picks_which_variant_each_child_uses() {
        let mut lib = NodeLibrary::new();

        // A pedal with two captures. Its "High" variant pushes the drive.
        let pedal = Node::leaf("Drive", crate::block::BlockType::Drive, block_with("drive", 0.2));
        let pedal_id = pedal.id.clone();
        let mut high = Variant::new("High");
        high.overrides.push(Override::set(
            NodePath::new(vec![NodePathSegment::Parameter { id: "drive".into() }]),
            0.9,
        ));
        let high_id = high.id.clone();
        let pedal = pedal.with_variant(high);

        let board = Node::container("Board", Role::Module, Combine::Serial).with_child(&pedal);
        // "Lead" recalls the board, but with the pedal on its High capture.
        let lead = Variant::new("Lead").selecting(pedal_id.clone(), high_id);
        let lead_id = lead.id.clone();
        let board = board.with_variant(lead);
        let root = board.id.clone();
        lib.insert(pedal);
        lib.insert(board);

        let (plain, _) = resolve(&lib, &root, None).expect("resolves");
        assert_eq!(param_of(&plain, &pedal_id, "drive"), Some(0.2));

        let (lead, _) = resolve(&lib, &root, Some(&lead_id)).expect("resolves");
        assert_eq!(
            param_of(&lead, &pedal_id, "drive"),
            Some(0.9),
            "selecting a child's variant applies that variant's own overrides"
        );
    }

    #[test]
    fn a_cycle_is_refused_rather_than_hung() {
        let mut lib = NodeLibrary::new();
        let mut a = Node::container("A", Role::Module, Combine::Serial);
        let mut b = Node::container("B", Role::Module, Combine::Serial);
        // A -> B -> A. Reachable from a hand-edited styx file.
        a.content = Content::Children { nodes: vec![b.id.clone()] };
        b.content = Content::Children { nodes: vec![a.id.clone()] };
        let root = a.id.clone();
        lib.insert(a);
        lib.insert(b);

        assert!(matches!(
            resolve(&lib, &root, None),
            Err(ResolveError::Cycle { .. })
        ));
    }

    #[test]
    fn a_missing_child_is_a_hole_not_a_failure() {
        let mut lib = NodeLibrary::new();
        let present = Node::leaf("Amp", crate::block::BlockType::Amp, block_with("gain", 0.5));
        let absent = Node::leaf("Gone", crate::block::BlockType::Amp, block_with("x", 0.0));
        let chain = Node::container("P", Role::Preset, Combine::Serial)
            .with_child(&present)
            .with_child(&absent);
        let root = chain.id.clone();
        let absent_id = absent.id.clone();
        lib.insert(present);
        lib.insert(chain); // `absent` is deliberately never inserted.

        let (resolved, report) = resolve(&lib, &root, None).expect("still resolves");
        assert_eq!(resolved.leaves().len(), 1, "the rest of the rig still plays");
        assert_eq!(report.missing, vec![absent_id]);
    }

    /// "Recall this patch, but with a different amp in it." The chain is one
    /// node; what changes is which amp node sits in it.
    #[test]
    fn replace_ref_swaps_which_node_sits_at_a_position() {
        let mut lib = NodeLibrary::new();
        let fender = Node::leaf("Fender Clean", crate::block::BlockType::Amp, block_with("gain", 0.3));
        let ac30 = Node::leaf("AC30", crate::block::BlockType::Amp, block_with("gain", 0.7));
        let fender_id = fender.id.clone();
        let ac30_id = ac30.id.clone();

        // The chain holds the Fender by default.
        let mut ambient = Variant::new("Ambient");
        ambient.overrides.push(Override {
            path: NodePath::new(vec![NodePathSegment::Block { id: "Fender Clean".into() }]),
            op: NodeOverrideOp::ReplaceRef { id: "AC30".into() },
        });
        let ambient_id = ambient.id.clone();
        let chain = Node::container("Worship", Role::Preset, Combine::Serial)
            .with_child(&fender)
            .with_variant(ambient);
        let root = chain.id.clone();
        lib.insert(fender);
        lib.insert(ac30);
        lib.insert(chain);

        let (plain, _) = resolve(&lib, &root, None).expect("resolves");
        assert_eq!(plain.leaves()[0].name, "Fender Clean");
        assert_eq!(param_of(&plain, &fender_id, "gain"), Some(0.3));

        let (swapped, report) = resolve(&lib, &root, Some(&ambient_id)).expect("resolves");
        assert_eq!(swapped.leaves()[0].name, "AC30");
        assert_eq!(param_of(&swapped, &ac30_id, "gain"), Some(0.7));
        assert!(report.is_clean(), "the swap is not an unmatched override");
    }

    #[test]
    fn a_replacement_the_library_lacks_leaves_the_original_playing() {
        let mut lib = NodeLibrary::new();
        let amp = Node::leaf("Amp", crate::block::BlockType::Amp, block_with("gain", 0.5));
        let mut broken = Variant::new("Broken");
        broken.overrides.push(Override {
            path: NodePath::new(vec![NodePathSegment::Block { id: "Amp".into() }]),
            op: NodeOverrideOp::ReplaceRef { id: "a-node-that-was-deleted".into() },
        });
        let broken_id = broken.id.clone();
        let chain = Node::container("P", Role::Preset, Combine::Serial)
            .with_child(&amp)
            .with_variant(broken);
        let root = chain.id.clone();
        lib.insert(amp);
        lib.insert(chain);

        let (resolved, _) = resolve(&lib, &root, Some(&broken_id)).expect("resolves");
        assert_eq!(
            resolved.leaves().len(),
            1,
            "a dangling replacement must not leave a hole where the amp was"
        );
        assert_eq!(resolved.leaves()[0].name, "Amp");
    }

    /// The rig library is hand-editable text, so the round trip is a
    /// property to prove rather than assume.
    #[test]
    fn a_library_round_trips_through_styx() {
        let mut lib = NodeLibrary::new();
        let amp = Node::leaf("AC30", crate::block::BlockType::Amp, block_with("gain", 0.7));
        let mut lead = Variant::new("Lead");
        lead.overrides.push(Override::set(
            NodePath::new(vec![
                NodePathSegment::Block { id: "AC30".into() },
                NodePathSegment::Parameter { id: "gain".into() },
            ]),
            0.9,
        ));
        let lead_id = lead.id.clone();
        let chain = Node::container("Worship", Role::Preset, Combine::Serial)
            .with_child(&amp)
            .with_variant(lead);
        let root = chain.id.clone();
        let amp_id = amp.id.clone();
        lib.insert(amp);
        lib.insert(chain);

        let text = facet_styx::to_string(&lib).expect("serializes to styx");
        let back: NodeLibrary = facet_styx::from_str(&text).expect("parses back");

        // Ids survive, which is the whole point — a reference that does not
        // round-trip is a reference that breaks on the next start.
        assert_eq!(back.nodes.len(), 2);
        assert!(back.get(&amp_id).is_some());
        let (resolved, report) = resolve(&back, &root, Some(&lead_id)).expect("resolves");
        assert!(report.is_clean());
        assert_eq!(param_of(&resolved, &amp_id, "gain"), Some(0.9));
    }

    #[test]
    fn an_override_that_matches_nothing_is_reported() {
        let mut lib = NodeLibrary::new();
        let amp = Node::leaf("Amp", crate::block::BlockType::Amp, block_with("gain", 0.5));
        let mut stale = Variant::new("Stale");
        // The name this points at was renamed — the silent-miss hazard,
        // surfaced instead of swallowed.
        stale.overrides.push(Override::set(
            NodePath::new(vec![
                NodePathSegment::Block { id: "Shimmer".into() },
                NodePathSegment::Parameter { id: "mix".into() },
            ]),
            0.5,
        ));
        let stale_id = stale.id.clone();
        let chain = Node::container("P", Role::Preset, Combine::Serial)
            .with_child(&amp)
            .with_variant(stale);
        let root = chain.id.clone();
        lib.insert(amp);
        lib.insert(chain);

        let (_, report) = resolve(&lib, &root, Some(&stale_id)).expect("resolves");
        assert_eq!(report.unmatched_overrides.len(), 1);
        assert!(!report.is_clean());
    }
}
