// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! DAW REAPER Implementation
//!
//! This crate provides REAPER-specific implementations of the DAW Protocol.
//! It is designed to be used as an in-process library within the reaper-extension,
//! not as a standalone cell binary.
//!
//! # Main Thread Safety
//!
//! REAPER APIs can only be called from the main thread. This crate uses `TaskSupport`
//! from `reaper-high` to dispatch operations to the main thread.
//!
//! # Usage
//!
//! During extension initialization, call `set_task_support()` with a reference to
//! the global TaskSupport instance:
//!
//! ```rust,ignore
//! // In extension initialization
//! daw_reaper::set_task_support(Global::task_support());
//! ```
//!
//! Then create the dispatchers:
//!
//! ```rust,ignore
//! let transport = daw_reaper::ReaperTransport::new();
//! let project = daw_reaper::ReaperProject::new();
//! let dispatcher = RoutedDispatcher::new(
//!     TransportServiceDispatcher::new(transport),
//!     ProjectsDispatcher::new(project),
//! );
//! ```

pub mod batch;
pub mod bootstrap;
pub mod local_caller;
pub mod plugin_services;
pub mod renumber;
pub mod services;

pub mod extension_setup;
pub mod safe_wrappers;
pub mod socket_publisher;
pub mod sync_api;
pub use local_caller::LocalCaller;
pub use sync_api::DawMainThread;

pub mod action_registry;
pub mod audio_accessor;
pub mod audio_engine;
pub mod automation;
pub mod dawfile_service;
pub mod ext_state;
pub mod fx;
pub mod fx_chains;
pub mod fx_params;
pub mod health;
pub mod input;
pub mod item;
pub mod lanes;
pub mod live_midi;
pub mod main_thread;
pub mod marker;
pub mod midi;
pub mod peak;
// Pro Tools plugin-state → Reaper VST-chunk bridge. PT-specific.
#[cfg(feature = "protools")]
pub mod plugin_bridge;
pub mod plugin_loader;
pub mod position_conversion;
pub mod project;
pub mod project_context;
// RPP → Pro Tools domain export (inverse of the protools importer). PT-specific.
#[cfg(feature = "protools")]
pub mod project_export;
pub mod project_import;
pub mod ptr_validation;
pub mod region;
pub mod resource;
pub mod routing;
pub mod screenset;
pub mod song_files;
pub mod stretch_marker;
mod take;
pub mod tempo_map;
pub mod toolbar;
pub mod track;
pub mod transport;
pub mod transport_sync;
pub mod ui;
pub mod window_geometry;
pub mod window_manager;

// Push-based change detection via REAPER's IReaperControlSurface
// callbacks. Parallel fast path alongside the 30Hz pollers — see
// `control_surface.rs` for the variant-by-variant mapping.
pub mod control_surface;
pub mod diagnostics;
pub mod event_bus;
pub use control_surface::{DawControlSurface, Mode as CsurfMode};

// Per-service `Reaper*` structs retired with the architect::rpc port —
// every backend trait now impls on the `crate::Reaper` singleton.
// Mount via `<service>::serve(Reaper)` / `<service>::descriptor()`.
//
// Free helpers (broadcasters / extension menu / project importer)
// kept; they don't belong to any one service.
pub use action_registry::{register_extension_menu, subscribe_action_broadcasts};
pub use marker::{Reaper, ReaperMainThreadDispatcher};
pub use plugin_loader::{eager_load_fx_plugins, set_plugin_context};
#[cfg(feature = "protools")]
pub use project_export::{reaper_track_to_pt_playlists, reaper_track_to_pt_playlists_at_rate};
pub use project_import::register_project_importer;
pub use region::get_regions_on_main_thread;
pub use tempo_map::{
    get_tempo_and_time_sig_at_on_main_thread, qn_to_time_on_main_thread, time_to_qn_on_main_thread,
};
pub use track::add_track_on_main_thread;

// Re-export the main thread bridge and transport broadcaster functions
pub use main_thread::set_task_support;

// FX broadcaster + polling retired with architect::rpc port.
// Streaming subscriptions live on a sibling trait when revived.

// Re-export track broadcaster functions

// Tempo map broadcaster + polling retired with architect::rpc port.
// Stubs kept on `tempo_map` module so existing call sites remain.
pub use tempo_map::{init_tempo_map_broadcaster, poll_and_broadcast_tempo_map};

// Re-export item broadcaster functions
pub use item::{init_item_broadcaster, poll_and_broadcast_items, subscribe_items};

// FX / routing / take streaming — own-broadcaster pattern (sibling to
// item.rs). Each module has init / subscribe / poll_and_broadcast.
pub mod fx_stream;
pub mod routing_stream;
pub mod take_stream;
pub use fx_stream::{init_fx_broadcaster, poll_and_broadcast_fx, subscribe_fx};
pub use routing_stream::{init_routing_broadcaster, poll_and_broadcast_routing, subscribe_routing};
pub use take_stream::{init_take_broadcaster, poll_and_broadcast_takes, subscribe_takes};

// Central streaming event hub — owns one broadcast::Sender per
// streaming domain. See docs/streaming-design.md.
pub mod event_hub;
pub use event_hub::{DawEventHub, hub as event_hub, init_event_hub};

// Transport state + position polling. Called from the bridge's
// main-thread timer tick alongside the surviving item / tempo_map
// pollers.
pub use transport::poll_and_broadcast_transport;

// Markers polling. Same shape — diffs against per-project cache,
// emits Added/Changed/Removed events. Called from the bridge timer.
pub use marker::poll_and_broadcast_markers;

// Regions polling — sibling of markers.
pub use region::poll_and_broadcast_regions;

// Tracks polling — coarse Added/Removed only for Phase 2.
pub use track::poll_and_broadcast_grouping;
pub use track::poll_and_broadcast_tracks;

// Live meter frames — reads active-project track peaks per tick and
// publishes one MeterFrame on the hub (`Peaks::meters` stream).
pub use peak::poll_and_broadcast_meters;

// Re-export toolbar deferred ops processor
pub use toolbar::process_deferred_ops as process_toolbar_ops;

// Re-export extension bootstrap convenience
pub use bootstrap::{build_extension_daw, build_extension_daw_with};
pub use plugin_services::create_daw_handler;
// `RoutedHandler` retired — use `architect::LayerRouter` directly.
