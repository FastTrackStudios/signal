//! Trait for applying resolved graphs to a live DAW instance.
//!
//! The controller calls [`DawPatchApplier::apply_graph`] after resolving a
//! patch. Implementations handle the actual DAW API calls (track discovery,
//! FX state loading, parameter setting).

use signal_proto::resolve::ResolvedGraph;
use std::future::Future;
use std::pin::Pin;

/// Error type for patch application failures.
#[derive(Debug)]
pub enum PatchApplyError {
    /// No target track configured or found.
    NoTarget(String),
    /// DAW communication failed.
    DawError(String),
}

impl std::fmt::Display for PatchApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoTarget(msg) => write!(f, "no target: {msg}"),
            Self::DawError(msg) => write!(f, "DAW error: {msg}"),
        }
    }
}

impl std::error::Error for PatchApplyError {}

/// Abstracts applying a resolved graph to a DAW.
///
/// Implementations should:
/// 1. Extract state chunks via [`graph_state_chunks()`](super::param_bridge::graph_state_chunks)
/// 2. If chunks present: call `fx.set_state_chunk()` on the target FX
/// 3. Else: build a snapshot via [`graph_to_snapshot()`](super::param_bridge::graph_to_snapshot) and apply params
///
/// Returns `true` if a state chunk was used, `false` for param-by-param.
pub trait DawPatchApplier: Send + Sync {
    fn apply_graph<'a>(
        &'a self,
        graph: &'a ResolvedGraph,
        patch_name: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<bool, PatchApplyError>> + Send + 'a>>;

    /// Apply a resolved **node tree** — the domain form.
    ///
    /// The same job as [`apply_graph`](Self::apply_graph) from the model that
    /// replaces `ResolvedGraph`. An implementation extracts its chunks with
    /// [`node_state_chunks`](super::param_bridge::node_state_chunks) and is
    /// otherwise identical: the graph was only ever read for its state
    /// chunks, and everything a DAW applier does around that — preloaded
    /// tracks, tails, mutes, splicing — never looked at the model.
    ///
    /// Defaults to refusing rather than silently doing nothing, so an
    /// applier that has not been taught nodes says so instead of reporting
    /// success for a rig the DAW never received.
    fn apply_node<'a>(
        &'a self,
        resolved: &'a signal_proto::node_resolve::Resolved,
        patch_name: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<bool, PatchApplyError>> + Send + 'a>> {
        let _ = (resolved, patch_name);
        Box::pin(async {
            Err(PatchApplyError::NoTarget(
                "this applier does not speak nodes".into(),
            ))
        })
    }
}
