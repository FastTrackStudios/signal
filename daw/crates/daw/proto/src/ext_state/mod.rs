//! Persistent key-value storage (REAPER ext-state API) — service trait,
//! plus the namespaced project-scoped side store built on it.

mod service;
pub mod side_store;

pub use service::*;
