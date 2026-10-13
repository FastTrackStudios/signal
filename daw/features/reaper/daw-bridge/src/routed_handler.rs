//! Bridge-side glue: just the [`DawConnectionAcceptor`] now.
//!
//! Method-ID dispatch was promoted into [`architect::LayerRouter`] —
//! `daw-bridge`'s old in-tree `RoutedHandler` had identical shape and was
//! retired alongside the architect-rpc port. Use
//! `daw_proto::service_set::daw_layers(backend).serve()` to build a
//! fully populated `LayerRouter` and pass it to [`DawConnectionAcceptor::new`].

use std::sync::Arc;

use architect::LayerRouter;

// ============================================================================
// DawConnectionAcceptor — lane-based service routing (vox 0.10)
// ============================================================================

/// Holds the [`architect::LayerRouter`] handed to every inbound lane.
///
/// Clients open lanes with metadata identifying themselves. All lanes
/// currently get the full DAW service set (the router dispatches by method
/// id). Future: metadata-driven sub-bundles for role-restricted views.
#[derive(Clone)]
pub struct DawConnectionAcceptor {
    handler: Arc<LayerRouter>,
}

impl DawConnectionAcceptor {
    pub fn new(handler: LayerRouter) -> Self {
        Self {
            handler: Arc::new(handler),
        }
    }

    /// Clone the shared `LayerRouter` for handing to a vox lane.
    pub fn router(&self) -> LayerRouter {
        self.handler.as_ref().clone()
    }
}
