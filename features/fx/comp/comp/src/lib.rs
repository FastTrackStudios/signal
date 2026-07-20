//! `comp` — the built-in Comp FX for signal.
//!
//! Facade over the DSP core [`comp_dsp`]; downstream crates depend on `comp`
//! (never `comp-dsp` directly). The plugin UI and preset profiles are opt-in
//! (`features = ["ui"]` / `["profiles"]`) and wire in once the plugin shell
//! lands — the DSP is always available.

pub use comp_dsp as dsp;
pub use comp_dsp::*;

/// Limiter engine surface for the FTS Limiter shell (`apps/plugins/limiter`).
///
/// Intended wiring is `pub use limiter_dsp as limiter;` — the `LimiterChain`
/// over the Airwindows-derived AdClip / ClipSoftly / BlockParty / Loud stages
/// at `features/fx/comp/limiter-dsp`. That crate is still an all-stub skeleton
/// (every algorithm module is a TODO and its manifest inherits a nonexistent
/// `fts-dsp` workspace dep), so until it is implemented this module re-exports
/// the real Airwindows clip primitives from `audiocore-dsp` that the limiter
/// ceiling stage builds on. Downstream crates depend on `comp::limiter`
/// (never `limiter-dsp` directly), so the swap is source-compatible at the
/// facade boundary.
pub mod limiter {
    /// Golden-ratio interpolated hard clip (ClipOnly2 / ADClip8 lineage) and
    /// the ClipSoftly sine waveshaper.
    pub use audiocore_dsp::soft_clip::{sin_clip, GoldenClip};
}
