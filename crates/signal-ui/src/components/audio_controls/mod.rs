//! Audio control widgets — knobs, sliders, and 2D pads for parameter editing.
//!
//! All values use a 0.0–1.0 normalized range with display formatting
//! handled by the consuming view.

mod knob;
mod xy_pad;

pub use knob::{Knob, KnobSize};
pub use xy_pad::XYPad;
