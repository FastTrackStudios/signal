//! Transport module — canonical home for everything transport-related.
//!
//! Data types live in `transport.rs`. The service trait lives in
//! `service.rs` decorated with `#[architect::rpc]`. The data struct
//! `Transport` and the trait `Transport` share a name but live at
//! different paths (struct: `daw_proto::Transport` /
//! `daw_proto::transport::Transport`; trait:
//! `daw_proto::Transport`).

pub mod error;
pub mod event;
// `service` is `pub mod` so the trait can be reached as
// `crate::transport::service::Transport` from sync/mod.rs without
// colliding with the `Transport` struct re-exported below.
pub mod service;
#[allow(clippy::module_inception)]
pub mod transport;

pub use error::TransportError;
pub use event::{PositionTick, TransportEvent, TransportStreamEvent, TransportSubscription};
pub use transport::{
    AllProjectsTransport, LoopRegion, PlayState, ProjectTransportState, RecordMode, Transport,
};

// architect-emitted RPC face from `service.rs`. The trait itself
// (`Transport`) is re-exported from `crate::sync` to avoid the name
// collision with the `Transport` struct re-exported above.
pub use service::*;
