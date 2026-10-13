//! Chart parsers RPC contract — sync trait + auto-derived RPC face.

mod service;

pub use service::{ChartParsers, ChartParsersRpc};

#[cfg(feature = "vox")]
pub use service::{
    ChartParsersClient, ChartParsersRpcDispatcher as Dispatcher, Service,
    chart_parsers_rpc_service_descriptor as descriptor, layer, serve,
};
