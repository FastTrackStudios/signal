//! Resolving a node target — the path that replaces `resolve_service`.
//!
//! [`resolve_service`](crate::resolve_service) walks five repositories to turn
//! a five-level target into a [`ResolvedGraph`](signal_proto::resolve::ResolvedGraph).
//! That graph is shaped around the hierarchy it came from — `rig_id`,
//! `rig_scene_id`, `engines` — so it cannot carry a node tree without
//! flattening it back into levels that no longer exist.
//!
//! So this is not a translation of that stack. It is the small thing that
//! replaces it: load the library, resolve the node.
//!
//! # Why it is this small
//!
//! Resolution is already written, in `signal_proto::node_resolve`, and it
//! needs no repository at all — it is a pure walk over a
//! [`NodeLibrary`](signal_proto::node::NodeLibrary). All that is left is
//! fetching the library, which is why the old stack's 1,143 lines become a
//! handful here. The five levels were the complexity, not the resolving.
//!
//! # Migrating a consumer
//!
//! One at a time, and each move is small: where a caller asks
//! `resolve_target` for a `ResolvedGraph`, it asks [`resolve_node`] for a
//! [`Resolved`] instead. When the last one has moved, `resolve_service`, the
//! five repositories, the ten entity types and the five level-specific
//! `PatchTarget` variants all go together — and "Scene" comes free for the
//! Song entry.

use signal_proto::node::{NodeId, VariantId};
use signal_proto::node_resolve::{Report, ResolveError, Resolved, resolve};
use signal_storage::node_repo::NodeRepo;

/// Why a node target could not be played.
#[derive(Debug, Clone)]
pub enum NodeResolveError {
    /// The library could not be read.
    Storage(String),
    /// The node is absent, or the tree references itself.
    Resolve(ResolveError),
}

impl std::fmt::Display for NodeResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(e) => write!(f, "node library: {e}"),
            Self::Resolve(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for NodeResolveError {}

/// Resolve one node with one of its variants, ready to render.
///
/// The [`Report`] comes back alongside rather than being folded into the
/// error, because a rig missing one block should still play: the caller
/// renders what resolved and shows what did not. Only a missing root or a
/// reference cycle is fatal.
///
/// # Errors
///
/// [`NodeResolveError::Storage`] if the library cannot be read, or
/// [`NodeResolveError::Resolve`] if the node is absent or self-referencing.
pub async fn resolve_node<R: NodeRepo + ?Sized>(
    repo: &R,
    node: &NodeId,
    variant: Option<&VariantId>,
) -> Result<(Resolved, Report), NodeResolveError> {
    let library = repo
        .load_library()
        .await
        .map_err(|e| NodeResolveError::Storage(e.to_string()))?;
    resolve(&library, node, variant).map_err(NodeResolveError::Resolve)
}

/// Resolve whatever a patch points at, when it points at a node.
///
/// Returns `Ok(None)` for a patch targeting one of the five level-specific
/// variants — those still belong to `resolve_service`, and a caller migrating
/// incrementally needs to be told "not mine" rather than handed an error.
///
/// # Errors
///
/// As [`resolve_node`].
pub async fn resolve_patch_target<R: NodeRepo + ?Sized>(
    repo: &R,
    target: &signal_proto::profile::PatchTarget,
) -> Result<Option<(Resolved, Report)>, NodeResolveError> {
    match target {
        signal_proto::profile::PatchTarget::Node { node, variant } => {
            resolve_node(repo, node, Some(variant)).await.map(Some)
        }
        _ => Ok(None),
    }
}
