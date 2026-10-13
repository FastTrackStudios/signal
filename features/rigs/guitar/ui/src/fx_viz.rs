//! Where the rig gets its pictures from.
//!
//! It does not draw them. A delay's taps are drawn by `delay-ui`, a reverb's
//! decay by `reverb-ui`, a compressor's curve by `comp-ui`, a chorus by
//! `modulation-ui` — each by the effect that owns it, in the processor repo,
//! so the plugin and the rig show the same picture of the same block. A
//! second copy here would be a second answer to "what does this delay look
//! like", and the two would drift.
//!
//! What is left is the seam: the re-exports the rig's panels mount, and the
//! repaint clock they share.

pub use fts_audio_ui::paint::lane::rgb;

pub use delay_ui::viz::DelayViz;
pub use reverb_ui::viz::ReverbViz;

/// Mark this scope dirty ~40 times a second, for as long as it lives.
///
/// Blitz repaints when the document changes, and an animation changes nothing
/// in the DOM — the movement is inside a widget's scene. Any of the effect
/// crates' clocks would do; this is the rig's name for one, so a panel that
/// mounts no visualiser of its own can still ask to be redrawn.
pub use delay_ui::viz::use_repaint_clock;
