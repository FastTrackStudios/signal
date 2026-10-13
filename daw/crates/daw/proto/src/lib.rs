//! DAW Protocol Definitions
//!
//! This crate defines the shared types and service interfaces for DAW cells.
//!
//! # Glob re-exports
//!
//! The per-RPC modules under this crate (`marker`, `track`, `audio_engine`,
//! `transport`, etc.) each re-export an architect-generated set of names —
//! `Service`, `Dispatcher`, `descriptor`, `layer`, `serve` — that are
//! intentionally namespaced per service. When glob-imported at the crate
//! root the same bare names appear from multiple modules, which the
//! compiler flags as ambiguous. The marker/region/track/take/tempo_map/
//! ext_state/window_manager/action_registry modules already use explicit
//! re-exports to avoid this. The remaining `pub use foo::*` imports below
//! also pull in these same architect aliases — but only one "wins" the
//! bare-name binding at the crate root, and no caller relies on
//! `daw_proto::Service` (they always use the qualified path
//! `daw_proto::marker::Service`). The lint is therefore harmless and
//! silenced for the crate.

#![deny(unsafe_code)]
#![allow(ambiguous_glob_reexports)]

pub mod action_registry;
pub mod actions;
pub mod audio_accessor;
pub mod audio_engine;
pub mod automation;
pub mod batch;
pub mod capability;
pub mod daw;
pub mod dawfile_service;
pub mod diagnostics;
pub mod dock_host;
#[cfg(feature = "test-utils")]
pub mod dock_host_mock;
pub mod error;
pub mod event_bus;
pub mod ext_state;
pub mod fx;
pub mod fx_chains;
pub mod fx_params;
pub mod groups;
pub mod handle;
pub mod health;
pub mod input;
pub mod item;
pub mod live_midi;
pub mod main_thread;
pub mod marker;
pub mod midi;
pub mod peak;
pub mod plugin_loader;
pub mod position_conversion;
pub mod preroll;
pub mod primitives;
pub mod project;
pub mod record_control;
pub mod region;
pub mod resource;
pub mod routing;
pub mod screenset;
pub mod song_files;
pub mod stretch_marker;
pub mod take;
pub mod take_ranking;
pub mod tempo_map;
pub mod toolbar;
pub mod track;
pub mod transport;
pub mod transport_sync;
pub mod ui;
pub mod undo;
pub mod window_geometry;
pub mod window_manager;

// Explicit re-exports rather than glob — the `service` submodule emits
// architect `Service`/`Dispatcher`/`descriptor`/`layer`/`serve` names
// which collide with the same names in other RPC modules. Callers reach
// those via the fully-qualified `daw_proto::action_registry::*`.
#[cfg(feature = "vox")]
pub use action_registry::ActionRegistrationClient;
pub use action_registry::{
    ActionEvent, ActionExecutionResult, ActionInfo, ActionListFilter, ActionListRequest,
    ActionListResponse, ActionOrigin, ActionRegistration, ActionRegistrationRpc, ActionSection,
};
pub use actions::*;
pub use audio_accessor::*;
pub use audio_engine::*;
pub use automation::*;
pub use batch::*;
pub use capability::*;
pub use daw::Daw;
pub use dawfile_service::*;
pub use dock_host::*;
pub use error::*;
pub use fx::*;
pub use health::*;
pub use input::*;
pub use item::*;
pub use live_midi::*;
pub use stretch_marker::*;
// Explicit re-exports rather than glob: the architect-emitted
// `serve` / `descriptor` / `Dispatcher` aliases in `marker::*` and
// `track::*` collide when glob-imported at the crate root. Callers
// reach those via the fully qualified paths `daw_proto::marker::*`.
#[cfg(feature = "vox")]
pub use marker::MarkersClient;
pub use marker::{Marker, MarkerError, MarkerEvent, Markers, MarkersRpc};
pub use midi::*;
pub use peak::*;
pub use plugin_loader::*;
pub use position_conversion::*;
pub use primitives::*;
pub use project::*;
// Explicit re-exports (see marker / track for the rationale).
#[cfg(feature = "vox")]
pub use region::RegionsClient;
pub use region::{AddRegionInLaneRequest, Region, RegionError, RegionEvent, Regions, RegionsRpc};
pub use resource::*;
pub use routing::*;
pub use screenset::*;
// Explicit re-exports (see marker / track / region for rationale —
// architect-emitted `serve` / `descriptor` / `Dispatcher` aliases
// can't be glob-imported at the crate root.)
#[cfg(feature = "vox")]
pub use ext_state::ExtStateClient;
pub use ext_state::{ExtState, ExtStateRpc};
#[cfg(feature = "vox")]
pub use fx_chains::FxChainsClient;
pub use fx_chains::{FxChains, FxChainsRpc};
#[cfg(feature = "vox")]
pub use fx_params::FxParamsClient;
pub use fx_params::{FxParams, FxParamsRpc};
#[cfg(feature = "vox")]
pub use record_control::RecordControlServiceClient;
pub use record_control::{
    RecordControlService, RecordControlServiceDispatcher,
    record_control_service_service_descriptor, serve_record_control_service,
};
#[cfg(feature = "vox")]
pub use take::TakesClient;
pub use take::{Takes, TakesRpc};
#[cfg(feature = "vox")]
pub use take_ranking::TakeRankingServiceClient;
pub use take_ranking::{
    TakeRankLevel, TakeRankScope, TakeRankingService, TakeRankingServiceDispatcher,
    serve_take_ranking_service, take_ranking_service_service_descriptor,
};
#[cfg(feature = "vox")]
pub use tempo_map::TempoMapClient;
pub use tempo_map::{TempoMap, TempoMapError, TempoMapEvent, TempoMapRpc, TempoPoint};
pub use toolbar::*;
// Explicit re-exports (see marker::* note above for the rationale).
#[cfg(feature = "vox")]
pub use track::TracksClient;
pub use track::{
    AddChildren, Comp, CompArea, FolderDepthChange, InputMonitoringMode, LaneComping, LaneDisplay,
    RecordInput, ReorderTracksBehavior, Track, TrackError, TrackEvent, TrackExtEntry,
    TrackExtStateRequest, TrackGroup, TrackGrouping, TrackHierarchy, TrackHierarchyBuilder,
    TrackNode, TrackRef, TrackShape, TrackStructureBuilder, TrackTree, Tracks, TracksExt,
    TracksRpc, assert_tracks_equal, display_tracklist, format_tracklist,
};
pub use transport::*;
// Explicit re-exports (see marker / track for the rationale).
#[cfg(feature = "vox")]
pub use song_files::SongFilesClient;
pub use song_files::{SongFile, SongFiles};
pub use transport_sync::{StampedPosition, TransportSync};
#[cfg(feature = "vox")]
pub use transport_sync::{
    TransportSyncClient, TransportSyncStreamClient, TransportSyncStreamSource,
};
pub use ui::*;
pub use undo::*;
pub use window_geometry::*;
// Explicit re-exports (see action_registry note above for the rationale).
#[cfg(feature = "vox")]
pub use window_manager::WindowManagerClient;
pub use window_manager::{
    LayoutPlacement, LayoutToolbar, ModeDockerLayout, MonitorRect, WindowLayout,
    WindowLayoutOptions, WindowLayoutResult, WindowLayoutSummary, WindowManager, WindowManagerRpc,
};
