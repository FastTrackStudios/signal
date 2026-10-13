//! The browser rigs' AudioWorklet entry: one wasm crate, each rig's runner behind its feature.
//!
//! Build one rig per wasm bundle (`--features guitar` or `--features keys`): each
//! module has its own `#[wasm_bindgen(start)]` and JS glue.

#[cfg(feature = "guitar")]
pub mod guitar;
#[cfg(feature = "keys")]
pub mod keys;
