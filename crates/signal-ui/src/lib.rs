//! Signal2 UI crate.
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

pub mod components;
pub mod hooks;
pub mod panel_registration;
pub mod views;

// Convenience re-exports
pub use hooks::use_signal_service;
pub use panel_registration::register_panels;
pub use views::SignalSlider;
