//! Guitar-rig Dioxus components — the wasm-clean UI half of the detachable
//! GUI (see `docs/detachable-gui.md` in the repo root).
//!
//! Renders purely from `signal-guitar-proto` types via the generated vox
//! clients (provided through Dioxus context by the host app). The same
//! components mount inside the desktop `signal-ui` shell and the browser
//! `apps/web` shell.

mod chain;
mod comp_surface;
mod control;
/// Painted delay + reverb visualisers — native only (a painted scene needs a
/// Blitz host).
pub mod fx_viz;
/// The modulation visualisers live with the effect that owns them, so the
/// plugins draw the same pictures — see `modulation_ui::viz`.
pub use modulation_ui::viz as mod_viz;
/// The plugin's own vello EQ editor — native only (a painted scene needs a
/// Blitz host natively, a canvas in a browser).
mod eq_vello;
mod grid;
/// The shared audio-gui knob (moved to signal-widgets).
pub use signal_widgets::knob;
mod icons;
mod indicators;
mod kit;
mod library;
mod macro_bar;
mod module_sidebar;
mod meters;
mod palette;
pub mod param_writer;
mod perform;
mod preset_bar;
mod preset_look;
mod remote;
mod setlist_bar;
mod settings;
mod sidebars;
mod stable;
mod state;
mod theme;
mod wire_param;

pub use chain::ChainStrip;
pub use comp_surface::CompSurface;
pub use control::{ControlView, MidiIndicator, ZoomPanel};
pub use grid::RigGraph;
pub use icons::module_icon;
pub use library::{Kind as LibraryKind, LibraryPicker};
pub use macro_bar::{MacroBar, MacroPanelOpen, MacroShotStatus, MacroTuneMode};
pub use module_sidebar::{InitialSelection, Selection as ModuleSelection};
pub use meters::{CpuMeter, DspReadout, MeterBar, MeterPair, meter_level};
pub use perform::PerformGrid;
pub use preset_bar::PresetSidebar;
pub use remote::GuitarRigRemote;
pub use setlist_bar::SetlistSidebar;
pub use settings::{AudioSettingsBridge, AudioSettingsModal};
pub use sidebars::{LeftSidebar, LevellingChip};
/// The node/preset tree (moved to signal-widgets — both rigs draw it).
pub use signal_widgets::PresetTree;
pub use signal_widgets::{Knob, KnobSize, Picker, PickerSize};
pub use state::{RigViewState, use_rig_state};
pub use wire_param::{EditSink, WireParam, use_wire_params};

// The wire contract, re-exported for convenience.
pub use signal_guitar_proto as proto;
