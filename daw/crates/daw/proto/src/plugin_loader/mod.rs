//! Plugin loader — types + service trait.

mod service;
mod types;

pub use service::*;
pub use types::{LoadedPluginInfo, PluginLoadResult};
