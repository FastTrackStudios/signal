//! Signal UI -- Dioxus components for the signal domain.
//!
//! Provides both domain-agnostic presentation components and domain-aware
//! smart views that compose them into full editor and browser interfaces.
//!
//! # Architecture position
//!
//! ```text
//! signal (facade) + signal-controller + signal-daw-bridge
//!                        |
//!                        v
//!                 signal-ui (this crate)
//!                        |
//!                        v
//!              fts-control-desktop (app)
//! ```
//!
//! **Depends on**: `signal` (facade), `signal-controller`, `signal-daw-bridge`
//!
//! **Depended on by**: `fts-control-desktop` (the desktop application)
//!
//! # Key modules
//!
//! ## `components` -- domain-agnostic presentation
//!
//! Pure Dioxus building blocks (entity editor, star ratings, scene tiles,
//! morph slider, etc.) that take all data via props and have zero knowledge
//! of signal domain types.
//!
//! ## `views` -- domain-aware smart components
//!
//! Components that use [`signal::Signal`] (via context) and signal domain types
//! to fetch data, manage state, and compose the dumb `components` into
//! full editor/browser views.
//!
//! ## [`hooks`] -- Dioxus hooks for signal services
//!
//! - [`use_signal_service`] -- access the `Signal` controller from Dioxus context
//!
//! ## [`panel_registration`] -- register signal UI panels with the app shell
//!
//! ## [`infer_adapter`] -- adapt DAW bridge inference results for UI display

pub mod components;
// The controller-bound surface (views, shell, panels, DAW-bridge inference)
// takes a live `signal::Signal` (the native `SignalController`, which links the
// cpal audio engine), so it's native-only. A wasm build (browser rig remote)
// keeps just the wasm-clean widgets in `components` (e.g. the piano).
#[cfg(not(target_arch = "wasm32"))]
pub mod hooks;
#[cfg(not(target_arch = "wasm32"))]
pub mod infer_adapter;
#[cfg(not(target_arch = "wasm32"))]
pub mod panel_registration;
#[cfg(not(target_arch = "wasm32"))]
pub mod processing_chain;
#[cfg(not(target_arch = "wasm32"))]
pub mod shell;
#[cfg(not(target_arch = "wasm32"))]
pub mod views;

// Convenience re-exports (native-only, mirroring the modules above).
#[cfg(not(target_arch = "wasm32"))]
pub use hooks::use_signal_service;
#[cfg(not(target_arch = "wasm32"))]
pub use panel_registration::register_panels;
#[cfg(not(target_arch = "wasm32"))]
pub use processing_chain::ProcessingChain;
#[cfg(not(target_arch = "wasm32"))]
pub use shell::SignalRoot;
#[cfg(not(target_arch = "wasm32"))]
pub use views::{
    AudioDevice, AudioDevices, AudioPrefs, AudioSettingsBridge, AudioSettingsModal, GuitarRigView,
    LiveBlock, PerfStack, PerformanceModel, SignalSlider,
};
// Generated vox clients for the rig services — the host app establishes the
// connection (in-process LocalServer or remote WebSocket) and provides these
// via Dioxus context; views consume them with `try_consume_context`.
pub use signal_guitar_proto::audio::AudioSettingsClient;
pub use signal_guitar_proto::rig::{RigClient, RigStreamClient};
