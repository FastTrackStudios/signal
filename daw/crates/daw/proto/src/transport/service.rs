//! Transport service trait.
//!
//! Sync, decorated with `#[architect::rpc]`. The data types
//! (`Transport`, `PlayState`, `LoopRegion`, etc.) live alongside in
//! `transport.rs`; this file owns the verbs.
//!
//! Retired surfaces (subscribe_state / subscribe_all_projects for
//! streaming; set_position_musical / goto_measure for musical
//! position; play_from_last_start_position) land on follow-on sibling
//! traits if/when a real consumer needs them.
//!
//! Naming note: this trait is `Transport`, same as the data struct.
//! They share the name because they always have — backend code
//! addresses them by path (`daw_proto::Transport` for the
//! trait, `daw_proto::Transport` / `daw_proto::transport::Transport`
//! for the struct).

use crate::batch::ProjectArg;
use crate::transport::event::TransportStreamEvent;
use crate::transport::transport::{LoopRegion, PlayState, Transport as TransportState};
use crate::{DawResult, ProjectContext, TimeSignature};

/// Operations on the transport of a project. `ProjectContext` flows
/// through each call so a single backend instance serves every project.
#[architect::rpc(ops(ProjectContext as ProjectArg))]
pub trait Transport {
    // ── Playback ────────────────────────────────────────────────────

    fn play(&self, project: ProjectContext) -> DawResult<()>;
    fn pause(&self, project: ProjectContext) -> DawResult<()>;
    fn stop(&self, project: ProjectContext) -> DawResult<()>;
    fn play_pause(&self, project: ProjectContext) -> DawResult<()>;
    fn play_stop(&self, project: ProjectContext) -> DawResult<()>;

    // ── Recording ───────────────────────────────────────────────────

    fn record(&self, project: ProjectContext) -> DawResult<()>;
    fn stop_recording(&self, project: ProjectContext) -> DawResult<()>;
    fn toggle_recording(&self, project: ProjectContext) -> DawResult<()>;

    // ── Position ────────────────────────────────────────────────────

    fn set_position(&self, project: ProjectContext, seconds: f64) -> DawResult<()>;
    fn get_position(&self, project: ProjectContext) -> f64;
    fn goto_start(&self, project: ProjectContext) -> DawResult<()>;
    fn goto_end(&self, project: ProjectContext) -> DawResult<()>;

    // ── State queries ───────────────────────────────────────────────

    fn get_state(&self, project: ProjectContext) -> TransportState;
    fn get_play_state(&self, project: ProjectContext) -> PlayState;
    fn is_playing(&self, project: ProjectContext) -> bool;
    fn is_recording(&self, project: ProjectContext) -> bool;

    // ── Tempo ───────────────────────────────────────────────────────

    fn get_tempo(&self, project: ProjectContext) -> f64;
    fn set_tempo(&self, project: ProjectContext, bpm: f64) -> DawResult<()>;

    // ── Loop / time selection ───────────────────────────────────────

    fn toggle_loop(&self, project: ProjectContext) -> DawResult<()>;

    /// Metronome / click on or off.
    fn set_metronome(&self, project: ProjectContext, enabled: bool) -> DawResult<()>;
    fn metronome_enabled(&self, project: ProjectContext) -> bool;
    fn is_looping(&self, project: ProjectContext) -> bool;
    fn set_loop(&self, project: ProjectContext, enabled: bool) -> DawResult<()>;
    fn get_time_selection(&self, project: ProjectContext) -> Option<LoopRegion>;
    fn set_time_selection(
        &self,
        project: ProjectContext,
        start_seconds: f64,
        end_seconds: f64,
    ) -> DawResult<()>;
    fn clear_time_selection(&self, project: ProjectContext) -> DawResult<()>;

    // ── Playrate / time signature ───────────────────────────────────

    fn get_playrate(&self, project: ProjectContext) -> f64;
    fn set_playrate(&self, project: ProjectContext, rate: f64) -> DawResult<()>;
    fn get_time_signature(&self, project: ProjectContext) -> TimeSignature;

    // ── Streaming ───────────────────────────────────────────────────

    /// Transport events across all open projects, as they happen —
    /// discrete state changes and continuous ~30 Hz position ticks,
    /// multiplexed as `TransportStreamEvent::{State, Position}`.
    /// Subscribers filter by `project_guid` on the envelope. This is the
    /// architect `#[subscribe]` stream replacing the legacy `subscribe`
    /// above (per-subscriber Tx pump); served from each backend's
    /// `TransportStreamSource` hub.
    #[subscribe]
    fn events(&self) -> TransportStreamEvent;
}
