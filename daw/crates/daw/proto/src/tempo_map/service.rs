//! Tempo map service trait.
//!
//! Sync, decorated with `#[architect::rpc]`. `ProjectContext` per
//! method; backends impl directly on their singleton type. Retired
//! surfaces (time_to_qn / qn_to_time helpers + subscribe streaming)
//! land on follow-on sibling traits if needed.

use super::TempoPoint;
use super::event::TempoMapStreamEvent;
use crate::batch::ProjectArg;
use crate::{DawResult, ProjectContext};

#[architect::rpc(ops(ProjectContext as ProjectArg))]
pub trait TempoMap {
    // ── Queries ─────────────────────────────────────────────────────

    fn get_tempo_points(&self, project: ProjectContext) -> Vec<TempoPoint>;

    fn get_tempo_point(&self, project: ProjectContext, index: u32) -> Option<TempoPoint>;

    fn tempo_point_count(&self, project: ProjectContext) -> u32;

    fn get_tempo_at(&self, project: ProjectContext, seconds: f64) -> f64;

    #[ops(skip)]
    fn get_time_signature_at(&self, project: ProjectContext, seconds: f64) -> (i32, i32);

    #[ops(skip)]
    fn time_to_musical(&self, project: ProjectContext, seconds: f64) -> (i32, i32, f64);

    /// `(measure, beat, fraction)` → seconds.
    fn musical_to_time(
        &self,
        project: ProjectContext,
        measure: i32,
        beat: i32,
        fraction: f64,
    ) -> f64;

    // ── Mutation ────────────────────────────────────────────────────

    fn add_tempo_point(&self, project: ProjectContext, seconds: f64, bpm: f64) -> DawResult<u32>;

    fn remove_tempo_point(&self, project: ProjectContext, index: u32) -> DawResult<()>;

    fn set_tempo_at_point(&self, project: ProjectContext, index: u32, bpm: f64) -> DawResult<()>;

    fn set_time_signature_at_point(
        &self,
        project: ProjectContext,
        index: u32,
        numerator: i32,
        denominator: i32,
    ) -> DawResult<()>;

    fn move_tempo_point(&self, project: ProjectContext, index: u32, seconds: f64) -> DawResult<()>;

    // ── Project defaults ────────────────────────────────────────────

    fn get_default_tempo(&self, project: ProjectContext) -> f64;

    fn set_default_tempo(&self, project: ProjectContext, bpm: f64) -> DawResult<()>;

    #[ops(skip)]
    fn get_default_time_signature(&self, project: ProjectContext) -> (i32, i32);

    fn set_default_time_signature(
        &self,
        project: ProjectContext,
        numerator: i32,
        denominator: i32,
    ) -> DawResult<()>;

    /// Tempo-map changes across all open projects, as they happen.
    /// Subscribers filter by `project_guid` on the envelope.
    #[subscribe]
    fn events(&self) -> TempoMapStreamEvent;
}
