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
pub mod hooks;
pub mod infer_adapter;
pub mod panel_registration;
pub mod views;

// Convenience re-exports
pub use hooks::use_signal_service;
pub use panel_registration::register_panels;
pub use views::SignalSlider;
