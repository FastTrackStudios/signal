//! Shared Blitz-safe Dioxus widgets for signal rig UIs.
//!
//! Every rig remote (guitar, keys, synth, drums) and the signal desktop UI
//! draws the same audio-gui control vocabulary — rotary knobs on a 270° arc,
//! faders, meters. This crate is the one home for those components and their
//! geometry, below every rig UI crate in the graph (signal-ui itself depends
//! on rig UIs, so shared widgets cannot live there).
//!
//! Rendering rules (Blitz parity — standalone, VST3/CLAP, REAPER-embedded):
//! inline styles for everything layout-critical; Tailwind classes are
//! additive only; no external stylesheets.

pub mod arc;
pub mod drag_bus;
pub mod popup;
pub mod knob;
pub mod picker;
pub mod preset_tree;

pub use drag_bus::{DragBus, DragEvent};
pub use popup::{PopupHost, PopupLayer};
pub use knob::{FmtFn, Knob, KnobSize};
pub use picker::{Picker, PickerSize};

pub use preset_tree::PresetTree;
