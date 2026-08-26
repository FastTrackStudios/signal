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
mod eq_surface;
mod grid;
/// The shared audio-gui knob (moved to signal-widgets).
pub use signal_widgets::knob;
mod icons;
mod meters;
mod palette;
mod perform;
mod remote;
mod settings;
mod sidebars;
mod state;

pub use chain::ChainStrip;
pub use comp_surface::CompSurface;
pub use control::{ControlView, MidiMonitorButton, ZoomPanel};
pub use eq_surface::EqProSurface;
pub use grid::RigGraph;
pub use icons::module_icon;
pub use meters::{meter_level, MeterBar, MeterPair};
pub use perform::PerformGrid;
pub use remote::GuitarRigRemote;
pub use settings::{AudioSettingsBridge, AudioSettingsModal};
pub use sidebars::{LeftSidebar, RightSidebar};
pub use signal_widgets::{Knob, KnobSize};
pub use state::{use_rig_state, RigViewState};

// The wire contract, re-exported for convenience.
pub use signal_guitar_proto as proto;
