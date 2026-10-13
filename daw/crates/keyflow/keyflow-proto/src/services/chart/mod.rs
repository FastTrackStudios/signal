//! Charts RPC contract — sync trait + auto-derived RPC face.

mod service;

pub use service::{Charts, ChartsRpc};

#[cfg(feature = "vox")]
pub use service::{
    ChartsClient, ChartsRpcDispatcher as Dispatcher, Service,
    charts_rpc_service_descriptor as descriptor, layer, serve,
};
