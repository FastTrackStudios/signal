//! Batch instruction set for DAW RPC — collapses N operations into
//! one round-trip with cross-step dependency resolution server-side.

mod args;
mod op;
mod program;
mod service;

pub use args::*;
pub use op::*;
pub use program::*;
pub use service::*;
