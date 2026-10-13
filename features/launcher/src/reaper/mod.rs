//! Reaper-specific providers powered by the `daw` crate.
//!
//! All providers use the `daw` API for safe, async DAW interaction.
//! No direct reaper-rs calls — the daw crate handles all that.
//!
//! # Integration modes
//!
//! - **Embedded (CLAP plugin)**: `daw::init(host_context)` bootstraps in-process
//! - **Standalone**: Uses `daw` standalone feature with mock/demo data
//!
//! The providers don't care which mode — they use `daw::get()` or `daw::block_on()`.

mod actions;
mod fx;
mod markers;
mod tags;
mod tracks;
mod transport;

pub use actions::DawActionsProvider;
pub use fx::DawFxProvider;
pub use markers::DawMarkersProvider;
pub use tags::register_reaper_tags;
pub use tracks::DawTracksProvider;
pub use transport::DawTransportProvider;

/// Helper: run an async DAW operation synchronously.
/// Uses the DAW runtime if available, otherwise creates a temporary one.
#[allow(dead_code)]
pub(crate) fn daw_block_on<F: std::future::Future>(f: F) -> Option<F::Output> {
    daw::block_on(f)
}
