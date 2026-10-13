//! Audio settings + rig wire types — re-exported from the feature-scoped,
//! wasm-clean `signal-guitar-ui` / `signal-guitar-proto` crates so existing
//! `signal_ui::views::*` import paths keep working.
//!
//! The presentation (modal, pickers) lives in `signal-guitar-ui`; the data
//! types are the wire contract every remote GUI shares. signal-ui adds only
//! its desktop-shell composition on top.

pub use signal_guitar_proto::{
    AudioDevice, AudioDevices, AudioPrefs, LiveBlock, PerfStack, PerformanceModel,
};
pub use signal_guitar_ui::{AudioSettingsBridge, AudioSettingsModal};
