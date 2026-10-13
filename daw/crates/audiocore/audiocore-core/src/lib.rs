//! FTS Plugin Core - Abstraction layer for FastTrackStudio audio plugins.
//!
//! Provides a unified interface for building FTS audio plugins with
//! consistent UI, state management, and plugin configuration.
//!
//! # Usage
//!
//! ```ignore
//! use audiocore_core::prelude::*;  // nice_plug + nice_plug_dioxus re-exports
//! use audiocore_core::ui::prelude::*;  // FTS UI components (Toggle, Section, etc.)
//! ```

// Re-export core dependencies
pub use nice_plug;
#[allow(
    ambiguous_glob_reexports,
    reason = "nice_plug::prelude and nice_plug_dioxus::prelude both export `Modifiers`; \
              this crate re-exports both for convenience and never re-exports the \
              ambiguous name itself, so the conflict is harmless"
)]
pub use nice_plug::prelude::*;

#[cfg(feature = "gui")]
pub use nice_plug_dioxus;
#[cfg(feature = "gui")]
#[allow(
    ambiguous_glob_reexports,
    reason = "nice_plug::prelude and nice_plug_dioxus::prelude both export `Modifiers`; \
              this crate re-exports both for convenience and never re-exports the \
              ambiguous name itself, so the conflict is harmless"
)]
pub use nice_plug_dioxus::prelude::*;

#[cfg(feature = "gui")]
pub mod ui;

/// Re-export audiocore-gui crate for shared audio UI components.
#[cfg(feature = "gui")]
pub use audiocore_gui;

/// Prelude for convenient imports.
pub mod prelude {
    #[allow(
        ambiguous_glob_reexports,
        reason = "nice_plug::prelude and nice_plug_dioxus::prelude both export `Modifiers`; \
                  this crate re-exports both for convenience and never re-exports the \
                  ambiguous name itself, so the conflict is harmless"
    )]
    pub use nice_plug::prelude::*;

    #[cfg(feature = "gui")]
    #[allow(
        ambiguous_glob_reexports,
        reason = "nice_plug::prelude and nice_plug_dioxus::prelude both export `Modifiers`; \
                  this crate re-exports both for convenience and never re-exports the \
                  ambiguous name itself, so the conflict is harmless"
    )]
    pub use nice_plug_dioxus::prelude::*;
}

/// Standard window size for FTS plugins (16:9 aspect ratio).
pub const DEFAULT_WINDOW_SIZE: (u32, u32) = (640, 360);

/// Create a standard editor state with FTS defaults.
#[cfg(feature = "gui")]
#[must_use]
pub fn default_editor_state() -> std::sync::Arc<DioxusState> {
    DioxusState::new(|| DEFAULT_WINDOW_SIZE)
}
