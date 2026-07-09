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
            PatchApplyError::NoTarget(msg) => write!(f, "no target: {msg}"),
            PatchApplyError::DawError(msg) => write!(f, "DAW error: {msg}"),
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
}
