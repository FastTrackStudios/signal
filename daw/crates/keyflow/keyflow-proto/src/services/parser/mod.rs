//! Parsers RPC contract — sync trait + auto-derived RPC face.

mod service;

pub use service::{ParseRequest, ParseResponse, Parsers, ParsersRpc};

#[cfg(feature = "vox")]
pub use service::{
    ParsersClient, ParsersRpcDispatcher as Dispatcher, Service, layer,
    parsers_rpc_service_descriptor as descriptor, serve,
};
