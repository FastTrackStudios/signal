//! # RPP Primitives
//!
//! Core parsing primitives for the RPP file format. These modules provide the fundamental
//! building blocks for parsing RPP files without any REAPER-specific interpretation.
//!
//! ## Modules
//!
//! - **`token`**: Token-level parsing (strings, numbers, MIDI events, etc.)
//! - **`block`**: Block structure parsing (`<BLOCK>` and `>`)
//! - **`project`**: Top-level project parsing and coordination
//!
//! ## Architecture
//!
//! These primitives form the foundation layer of RPP parsing:
//! 1. **Token parsing** breaks down lines into meaningful units
//! 2. **Block parsing** handles the nested structure of RPP files
//! 3. **Project parsing** coordinates the overall file parsing
//!
//! REAPER-specific data structures (tracks, items, envelopes, FX chains) should be handled
//! by separate adapter modules that consume these primitives.

pub mod block;
pub mod fast_project;
pub mod project;
pub mod token;

// Re-export the main types for convenience
pub use block::{BlockType, RppBlock, RppBlockContent, parse_blocks};
pub use fast_project::parse_rpp_fast;
pub use project::{RppProject, parse_project_header, parse_rpp};
pub use token::{QuoteType, Token};
