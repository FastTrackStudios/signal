//! The guitar rig in a browser tab.
//!
//! An AudioWorklet runs the patch's chain ([`runner`]); the NAM models it
//! cannot afford run on Web Workers ([`remote`]), placed by [`plan`] so the
//! chain keeps up with zero added latency where the budget allows, and one
//! quantum per model where it does not. The wasm entry points — the worklet
//! side and the worker side of one module — are in `web`.
//!
//! See `crates/signal/docs/browser-guitar-rig.md`.

pub mod plan;
pub mod remote;
pub mod runner;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(test)]
mod tests;
