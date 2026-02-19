//! Dioxus hooks for the signal UI layer.

use dioxus::prelude::*;
use signal::Signal;

/// Access the [`Signal`] service from Dioxus context.
///
/// The app must call `provide_context(signal)` before any component
/// that uses this hook renders. Panics if the context is missing.
pub fn use_signal_service() -> Signal {
    consume_context::<Signal>()
}
