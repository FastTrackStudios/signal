// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! Session Protocol - Shared types and service definitions for Session cell
//!
//! This crate provides:
//! - Service trait definitions (`SetlistService`, `SongService`, `SessionService`)
//! - Domain types (`Setlist`, `Song`, `Section`, etc.)
//! - Generated service clients and dispatchers
//!
//! For UI components, see the `session-ui` crate.

// ─── Typed IDs ─────────────────────────────────────────────────

// `typed_uuid_id!` derives `Deserialize` on a newtype that also derives
// `Facet`, whose `Reborrow` impl is `unsafe` but does not touch the
// deserialized bytes — nothing for `unsafe_derive_deserialize` to protect
// against here.
#[allow(clippy::unsafe_derive_deserialize)]
mod typed_ids {
    utils::typed_uuid_id!(
        /// Unique identifier for a song in a session setlist.
        SongId
    );

    utils::typed_uuid_id!(
        /// Unique identifier for a section within a song.
        SectionId
    );
}
pub use typed_ids::{SectionId, SongId};

// Domain modules
pub mod color;
pub mod guide;
pub mod key;
pub mod keyflow_actions;
pub mod keyflow_scaffold;
pub mod mode;
pub mod navigation_actions;
pub mod offset_map;
pub mod playback;
pub mod review;
pub mod routing_project;
pub mod ruler_lanes;
pub mod services;
pub mod setlist;
pub mod setlist_actions;
pub mod song;
// Action contracts (`#[architect::actions]`), traits only — see
// `track_manager` for the rationale; implementations live in `session`.
pub mod guitar_grow;
pub mod track_manager;
pub mod track_structure;
pub mod track_template_actions;
pub mod watch;

// Re-export common types
pub use offset_map::{SetlistOffsetMap, SongOffset};
pub use routing_project::{LoopbackConfig, RoutingChannel, RoutingGroup};
pub use services::{
    AudioLatencyInfo, MeasureInfo, SessionServiceError, SetlistEvent, SetlistService,
    SetlistServiceClient, SetlistServiceDispatcher, SongService, SongServiceClient,
    SongServiceDispatcher, SongTransportState, serve_setlist_service, serve_song_service,
    setlist_service_service_descriptor, song_service_service_descriptor,
};
pub use setlist::{ActiveIndices, AdvanceMode, QueuedTarget, Setlist};
pub use song::{Comment, Section, SectionType, Song, SongChartHydration, SongDetectedChord};
pub use track_structure::{
    GuideTrackRole, ReferenceStructure, SetlistTrackStructure, SongTrackMapping, TrackEntry,
    TrackIdentity,
};
// Typed IDs are defined at crate root and re-exported here for convenience
// (SongId and SectionId are already in scope from the definitions above)

// Re-export position types from daw-proto for convenience
pub use daw_proto::MusicalPosition;

/// Session service - provides session-specific functionality
#[architect::rpc]
pub trait SessionService {
    /// Get session cell status
    async fn get_status(&self) -> String;
}
