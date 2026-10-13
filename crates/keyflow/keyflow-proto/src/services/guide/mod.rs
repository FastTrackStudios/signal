//! Guides RPC contract — sync trait + auto-derived RPC face.

mod service;

pub use service::{GuideRequest, Guides, GuidesRpc};

#[cfg(feature = "vox")]
pub use service::{
    GuidesClient, GuidesRpcDispatcher as Dispatcher, Service,
    guides_rpc_service_descriptor as descriptor, layer, serve,
};
