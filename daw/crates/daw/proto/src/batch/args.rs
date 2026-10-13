//! Resolvable argument types for batch instructions.
//!
//! These allow batch steps to reference outputs from previous steps,
//! enabling cross-step dependency chains without client round-trips.

use crate::{FxChainContext, ProjectContext, TrackRef};
use facet::Facet;

/// A project argument that can be either a literal value or resolved from a previous step.
#[repr(u8)]
#[derive(Clone, Debug, Facet)]
pub enum ProjectArg {
    /// Use this literal project context directly.
    Literal(ProjectContext),
    /// Resolve from the output of step `n`, which must produce `StepOutput::ProjectInfo`.
    FromStep(u32),
}

/// A track argument that can be either a literal value or resolved from a previous step.
#[repr(u8)]
#[derive(Clone, Debug, Facet)]
pub enum TrackArg {
    /// Use this literal track reference directly.
    Literal(TrackRef),
    /// Resolve from step `n`, which must produce `StepOutput::Track`.
    FromStep(u32),
    /// Resolve from step `n` (which produces `StepOutput::TrackList`), taking element at `index`.
    FromStepIndex(u32, u32),
}

/// An FX chain argument that can be either a literal value or resolved from a previous step.
#[repr(u8)]
#[derive(Clone, Debug, Facet)]
pub enum FxChainArg {
    /// Use this literal FX chain context directly.
    Literal(FxChainContext),
    /// Build `FxChainContext::Track(guid)` from step `n` which produced a `Track`.
    TrackFromStep(u32),
}
