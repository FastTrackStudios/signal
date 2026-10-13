//! Styx-parseable types for keybind config files.
//!
//! The types themselves live in `input-config-proto` (the wire contract
//! for keybind config editing) so wasm clients — the web editor, the
//! site, the hub — can use them without linking this crate. Re-exported
//! here at their historical paths.

pub use input_config_proto::types::*;
