//! Guitar-rig Dioxus components — the wasm-clean UI half of the detachable
//! GUI (see `docs/detachable-gui.md` in the repo root).
//!
//! Renders purely from `signal-guitar-proto` types via the generated vox
//! clients (provided through Dioxus context by the host app). The same
//! components mount inside the desktop `signal-ui` shell and the browser
//! `apps/web` shell.

mod chain;
mod grid;
mod meters;
mod perform;
mod remote;
mod settings;
mod state;

pub use chain::ChainStrip;
pub use grid::RigGraph;
pub use meters::{MeterBar, MeterPair, meter_level};
pub use perform::PerformGrid;
pub use remote::GuitarRigRemote;
pub use settings::{AudioSettingsBridge, AudioSettingsModal};
pub use state::{RigViewState, use_rig_state};

// The wire contract, re-exported for convenience.
pub use signal_guitar_proto as proto;
