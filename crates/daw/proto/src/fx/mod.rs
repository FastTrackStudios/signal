//! Effects (FX) — types + events + service trait.
//!
//! "FX" is REAPER's term for plugins. `Effects` is the architect::rpc
//! service trait for managing FX instances in chains (add, remove,
//! parameters, presets, state, containers, tree). For loading plugin
//! binaries off disk, see `PluginLoading` (`crate::plugin_loader`).
//! For per-chain ops see `FxChains`; for per-param ops see `FxParams`.

mod error;
mod event;
#[allow(clippy::module_inception)]
mod service;
pub mod tree;
mod types;

pub use error::*;
pub use event::*;
pub use service::*;
pub use tree::*;
pub use types::*;
