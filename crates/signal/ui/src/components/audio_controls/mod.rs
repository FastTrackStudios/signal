//! Audio control widgets — knobs, sliders, and 2D pads for parameter editing.
//!
//! All values use a 0.0–1.0 normalized range with display formatting
//! handled by the consuming view.

mod pattern_editor;
mod pitch_editor;
mod xy_pad;

pub use signal_grid_ui::{Knob, KnobSize};
pub use pattern_editor::PatternEditor;
pub use pitch_editor::{BlobEdit, BlobView, PitchEditor};
pub use xy_pad::XYPad;
