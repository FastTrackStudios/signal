//! Service trait definitions for session management.
//!
//! One module per domain; each holds its own wire types alongside the
//! `#[architect::rpc]` trait it serves.
//!
//! Take-ranking and record-control used to live here too — their
//! contracts now sit in `daw-proto` next to their `daw-actions`
//! implementations, since neither is session-specific.

pub mod error;
pub mod mode;
pub mod setlist;
pub mod song;

pub use error::SessionServiceError;
pub use mode::session_mode_service;
pub use mode::{
    SessionModeService, SessionModeServiceClient, SessionModeServiceDispatcher,
    SessionModeServiceLayer, serve_session_mode_service, session_mode_service_layer,
    session_mode_service_rpc_service_descriptor, session_mode_service_service_descriptor,
};
pub use setlist::setlist_service;
pub use setlist::{
    AudioLatencyInfo, MeasureInfo, SetlistEvent, SetlistService, SetlistServiceClient,
    SetlistServiceDispatcher, SetlistServiceLayer, SongTransportState, serve_setlist_service,
    setlist_service_layer, setlist_service_rpc_service_descriptor,
    setlist_service_service_descriptor,
};
pub use song::song_service;
pub use song::{
    SongService, SongServiceClient, SongServiceDispatcher, SongServiceLayer, serve_song_service,
    song_service_layer, song_service_rpc_service_descriptor, song_service_service_descriptor,
};
