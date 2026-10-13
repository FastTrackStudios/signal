// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! Standalone DAW Implementation
//!
//! This is a minimal DAW implementation that runs standalone without any external DAW.
//! It serves as both the reference implementation and the mock for testing.
//!
//! The implementations in this module (`StandaloneTransport`, `StandaloneProject`) can be
//! used directly in tests without spawning a separate cell process.
//!
//! ## Mock Data
//!
//! The standalone implementation includes mock data for testing:
//! - **3 songs** with markers (SONGSTART/SONGEND)
//! - **Sections** as regions (Intro, Verse, Chorus, Bridge, Outro, Solo)
//! - **Tempo/time signature changes** throughout the timeline
//!
//! This allows testing the full fts-control-web experience without a real DAW.

#![deny(unsafe_code)]
// Stylistic lints kept tolerant on this crate; standalone is the
// reference/mock implementation and uses ergonomic patterns
// (`let mut x = Default::default(); x.field = ...`) extensively.
#![allow(
    clippy::field_reassign_with_default,
    clippy::manual_clamp,
    clippy::needless_range_loop
)]

mod action_registry;
mod audio_accessor;
#[cfg(any(
    feature = "audio",
    feature = "decode",
    feature = "clap-host",
    feature = "vst3-host"
))]
pub mod audio_engine;
mod audio_engine_svc;
mod automation;
mod automation_touch;
pub mod grouping;
#[cfg(feature = "rpp-save")]
pub mod save;
/// Save / load a live project as a `.session` (FastTrackStudio's native
/// project format). Native-only: pulls the `.rpp` parser and the format.
#[cfg(feature = "session-file")]
pub mod session_file;
pub mod steps;
mod stretch_marker;
#[cfg(any(feature = "audio", feature = "decode"))]
mod take_reader;
pub use automation_touch::TouchableParam;
mod batch;
#[cfg(feature = "bootstrap")]
pub mod bootstrap;
mod dawfile_service;
mod diagnostics;
pub mod digest;
mod event_bus;
mod ext_state;
mod fx;
mod fx_chains;
mod fx_params;
mod health;
mod input;
mod item;
mod live_midi;
mod marker;
pub mod media_bay;
/// Seed a project with real on-disk media (grouped tracks + mmap'd stems).
#[cfg(any(feature = "audio", feature = "decode"))]
pub mod media_seed;
pub mod metering;
mod midi;
/// Hardware MIDI into the tracks armed for it.
#[cfg(feature = "midi-input")]
pub mod midi_input;
/// Offline effect rendering for A/B work against reference plugins.
#[cfg(not(target_arch = "wasm32"))]
pub mod offline_fx;
mod peak;
/// Persistent source-level peaks: REAPER `.reapeaks` sidecars next to
/// each on-disk media file, shared with REAPER where projects overlap.
#[cfg(all(feature = "reapeaks", any(feature = "audio", feature = "decode")))]
mod peak_store;
pub(crate) mod platform;
pub mod plugin;
mod plugin_loader;
mod position_conversion;
mod project;
#[cfg(any(feature = "rpp-project", feature = "rpp-project-wasm"))]
pub mod project_loader;
/// Recording: armed tracks' inputs captured to WAV files while the
/// transport records, each pass placed as a new take on a fixed lane.
#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "audio", feature = "decode")
))]
pub mod recording;
mod region;
mod resource;
mod routing;
mod routing_sync;
pub mod rpp_state;
mod screenset;
mod services;
// A peer streaming the song in: its folder's files by range.
mod shared_state;
mod song_files;
pub mod sync;
mod take;
mod tempo_map;
mod toolbar;
mod track;
mod transport;
pub mod transport_engine;
/// The engine as a multi-machine transport-sync backend
/// ([`transport_sync::SyncBackend`], via [`Standalone::sync_backend`]).
pub mod transport_sync;
mod ui;
mod window_geometry;
mod window_manager;

// All per-service impls are on `Standalone` directly post-port.
// Old `Standalone*` per-service structs retired.
pub use project::project_guids;
pub use shared_state::SharedProjectState;
pub use sync::Standalone;

/// REAPER `.reapeaks` peak-cache reading (the `reapeaks` feature) — draw
/// waveforms from REAPER's own peak mipmaps.
#[cfg(feature = "reapeaks")]
pub use dawfile_reaper::reapeaks;

/// Check a GUID handed to a create-with-guid call (`add_with_guid`,
/// `add_item_with_guid`).
///
/// The engine keeps it verbatim, and a `.session` save writes it as a
/// bare token (`TRACKID`, `IGUID`, a `MARKER` line's GUID field), so it
/// must be non-empty and free of whitespace and quotes to come back from
/// a load unchanged.
pub(crate) fn check_new_guid(kind: &str, guid: &str) -> daw_proto::DawResult<()> {
    if guid.is_empty() || guid.chars().any(|c| c.is_whitespace() || c == '"') {
        return Err(daw_proto::DawError::operation_failed(format!(
            "{kind} guid {guid:?} is empty or has whitespace or quotes"
        )));
    }
    Ok(())
}

/// A fresh GUID in REAPER's spelling, `{UPPERCASE-UUID}` — what a marker
/// or region gets when it is created without one, and what the
/// `.session` writer mints for one that has none.
pub(crate) fn new_braced_guid() -> String {
    format!("{{{}}}", uuid::Uuid::new_v4().to_string().to_uppercase())
}

/// A colour as the service surface receives it, as this backend stores it
/// (`0xRRGGBB`, `None` = default).
///
/// Callers written for REAPER pass its native form — `0x01BBGGRR`, the
/// top flag meaning "custom colour set" — which is what
/// `color_palette::Color::to_reaper_native` produces and what the `.rpp`
/// loader already converts from. A value carrying that flag is read that
/// way; one without it is taken as plain `0xRRGGBB`. Zero is no colour.
pub(crate) fn color_from_service(c: u32) -> Option<u32> {
    if c == 0 {
        return None;
    }
    if c & 0x0100_0000 != 0 {
        let (r, g, b) = (c & 0xff, (c >> 8) & 0xff, (c >> 16) & 0xff);
        return Some((r << 16) | (g << 8) | b);
    }
    Some(c & 0x00ff_ffff)
}
