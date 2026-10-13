//! Batch operations — composition over the `#[architect::rpc(ops)]`
//! generated per-service enums.
//!
//! Each covered service trait carries `ops(...)` substitution pairs,
//! so the macro emits its `<Trait>Op` / `<Trait>OpOutput` enums with
//! deferred argument types (`ProjectArg`, `TrackArg`, `FxChainArg`)
//! in place of the literal parameter types. This module only:
//!
//! - wraps those per-service enums into the top-level [`BatchOp`] /
//!   [`BatchOpOutput`] wire enums,
//! - implements the [`architect::ops`] resolver over prior step
//!   outputs ([`StepOutputs`]), and
//! - drives a whole program ([`run`]).
//!
//! Adding a method to a covered trait needs **no edit here** — the
//! variant, its application, and its output all regenerate.

use architect::ops::{OpResolver, ResolveArg};
use facet::Facet;

use super::args::{FxChainArg, ProjectArg, TrackArg};
use super::program::{BatchRequest, BatchResponse, StepOutcome, StepResult};
use crate::ext_state::{ExtStateOp, ExtStateOpOutput};
use crate::fx::{EffectsOp, EffectsOpOutput, FxChainContext};
use crate::item::{ItemsOp, ItemsOpOutput};
use crate::marker::{MarkersOp, MarkersOpOutput};
use crate::project::{ProjectContext, ProjectsOp, ProjectsOpOutput};
use crate::region::{RegionsOp, RegionsOpOutput};
use crate::routing::{RoutingOp, RoutingOpOutput};
use crate::take::{TakesOp, TakesOpOutput};
use crate::tempo_map::{TempoMapOp, TempoMapOpOutput};
use crate::track::{TrackRef, TracksOp, TracksOpOutput};
use crate::transport::{TransportOp, TransportOpOutput};

/// Top-level batch operation — one variant per covered service,
/// wrapping that service's macro-generated op enum.
#[repr(u8)]
#[derive(Clone, Debug, Facet)]
pub enum BatchOp {
    Transport(TransportOp),
    Project(ProjectsOp),
    Track(TracksOp),
    Marker(MarkersOp),
    Fx(EffectsOp),
    Routing(RoutingOp),
    Region(RegionsOp),
    TempoMap(TempoMapOp),
    ExtState(ExtStateOp),
    Item(ItemsOp),
    Take(TakesOp),
}

/// Output of one applied [`BatchOp`] — mirrors [`BatchOp`]'s shape,
/// wrapping the macro-generated per-service output enums.
// Wire/domain type: variant size asymmetry is inherent.
#[allow(clippy::large_enum_variant)]
#[repr(u8)]
#[derive(Clone, Debug, Facet)]
pub enum BatchOpOutput {
    Transport(TransportOpOutput),
    Project(ProjectsOpOutput),
    Track(TracksOpOutput),
    Marker(MarkersOpOutput),
    Fx(EffectsOpOutput),
    Routing(RoutingOpOutput),
    Region(RegionsOpOutput),
    TempoMap(TempoMapOpOutput),
    ExtState(ExtStateOpOutput),
    Item(ItemsOpOutput),
    Take(TakesOpOutput),
}

/// Everything a backend must implement to execute batch programs.
/// Blanket-implemented — backends just implement the service traits.
pub trait BatchBackend:
    crate::transport::prelude::Transport
    + crate::project::Projects
    + crate::track::Tracks
    + crate::marker::Markers
    + crate::fx::Effects
    + crate::routing::Routing
    + crate::region::Regions
    + crate::tempo_map::prelude::TempoMap
    + crate::ext_state::prelude::ExtState
    + crate::item::Items
    + crate::take::Takes
{
}

impl<B> BatchBackend for B where
    B: crate::transport::prelude::Transport
        + crate::project::Projects
        + crate::track::Tracks
        + crate::marker::Markers
        + crate::fx::Effects
        + crate::routing::Routing
        + crate::region::Regions
        + crate::tempo_map::prelude::TempoMap
        + crate::ext_state::prelude::ExtState
        + crate::item::Items
        + crate::take::Takes
{
}

impl BatchOp {
    /// Apply this op against a backend, resolving `FromStep` arguments
    /// from `outputs`.
    pub fn apply<B: BatchBackend + ?Sized>(
        self,
        backend: &B,
        outputs: &StepOutputs,
    ) -> Result<BatchOpOutput, BatchArgError> {
        Ok(match self {
            BatchOp::Transport(op) => BatchOpOutput::Transport(op.apply(backend, outputs)?),
            BatchOp::Project(op) => BatchOpOutput::Project(op.apply(backend, outputs)?),
            BatchOp::Track(op) => BatchOpOutput::Track(op.apply(backend, outputs)?),
            BatchOp::Marker(op) => BatchOpOutput::Marker(op.apply(backend, outputs)?),
            BatchOp::Fx(op) => BatchOpOutput::Fx(op.apply(backend, outputs)?),
            BatchOp::Routing(op) => BatchOpOutput::Routing(op.apply(backend, outputs)?),
            BatchOp::Region(op) => BatchOpOutput::Region(op.apply(backend, outputs)?),
            BatchOp::TempoMap(op) => BatchOpOutput::TempoMap(op.apply(backend, outputs)?),
            BatchOp::ExtState(op) => BatchOpOutput::ExtState(op.apply(backend, outputs)?),
            BatchOp::Item(op) => BatchOpOutput::Item(op.apply(backend, outputs)?),
            BatchOp::Take(op) => BatchOpOutput::Take(op.apply(backend, outputs)?),
        })
    }
}

/// A deferred (`FromStep`) argument could not be resolved. Never
/// crosses the wire itself — [`run`] folds it into
/// [`StepOutcome::Error`] / [`StepOutcome::Skipped`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatchArgError {
    /// The referenced step has no successful output (never ran,
    /// errored, or was skipped).
    MissingStep(u32),
    /// The referenced step produced an output that cannot feed this
    /// argument kind (e.g. a tempo query feeding a track argument).
    WrongKind { step: u32, wanted: &'static str },
    /// A list output was referenced with an out-of-range index.
    IndexOutOfRange { step: u32, index: u32 },
}

impl core::fmt::Display for BatchArgError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingStep(step) => {
                write!(f, "step {step} has no output to resolve from")
            }
            Self::WrongKind { step, wanted } => {
                write!(f, "step {step} output cannot resolve a {wanted}")
            }
            Self::IndexOutOfRange { step, index } => {
                write!(f, "step {step} list output has no element {index}")
            }
        }
    }
}

impl std::error::Error for BatchArgError {}

/// Successful outputs of already-executed steps, indexed by step id —
/// the resolver behind `FromStep` arguments.
#[derive(Default)]
pub struct StepOutputs {
    slots: Vec<Option<BatchOpOutput>>,
}

impl StepOutputs {
    pub fn with_capacity(steps: usize) -> Self {
        Self {
            slots: vec![None; steps],
        }
    }

    pub fn record(&mut self, step: u32, output: BatchOpOutput) {
        let idx = step as usize;
        if idx >= self.slots.len() {
            self.slots.resize(idx + 1, None);
        }
        self.slots[idx] = Some(output);
    }

    pub fn get(&self, step: u32) -> Option<&BatchOpOutput> {
        self.slots.get(step as usize).and_then(|s| s.as_ref())
    }

    fn require(&self, step: u32) -> Result<&BatchOpOutput, BatchArgError> {
        self.get(step).ok_or(BatchArgError::MissingStep(step))
    }

    /// A project produced by step `step`, as a context for later calls.
    fn project_from(&self, step: u32) -> Result<ProjectContext, BatchArgError> {
        let wrong = || BatchArgError::WrongKind {
            step,
            wanted: "project",
        };
        let info = match self.require(step)? {
            BatchOpOutput::Project(out) => match out {
                ProjectsOpOutput::Info(r) => r.as_ref().ok(),
                ProjectsOpOutput::Current(o)
                | ProjectsOpOutput::Get(o)
                | ProjectsOpOutput::GetBySlot(o)
                | ProjectsOpOutput::Open(o)
                | ProjectsOpOutput::Create(o) => o.as_ref(),
                _ => None,
            },
            _ => None,
        };
        Ok(ProjectContext::Project(
            info.ok_or_else(wrong)?.guid.clone(),
        ))
    }

    /// A track ref produced by step `step` (`index` picks from list
    /// outputs).
    fn track_from(&self, step: u32, index: Option<u32>) -> Result<TrackRef, BatchArgError> {
        let wrong = || BatchArgError::WrongKind {
            step,
            wanted: "track",
        };
        let out = match self.require(step)? {
            BatchOpOutput::Track(out) => out,
            _ => return Err(wrong()),
        };
        let guid = match (out, index) {
            (TracksOpOutput::Get(o) | TracksOpOutput::Master(o), None) => {
                o.as_ref().ok_or_else(wrong)?.guid.clone()
            }
            // `add` returns the new track's GUID directly.
            (TracksOpOutput::Add(r), None) => r.as_ref().map_err(|_| wrong())?.clone(),
            (TracksOpOutput::All(list) | TracksOpOutput::Selected(list), idx) => {
                let index = idx.unwrap_or(0);
                list.get(index as usize)
                    .ok_or(BatchArgError::IndexOutOfRange { step, index })?
                    .guid
                    .clone()
            }
            _ => return Err(wrong()),
        };
        Ok(TrackRef::Guid(guid))
    }
}

impl OpResolver for StepOutputs {
    type Error = BatchArgError;
}

impl ResolveArg<ProjectArg, ProjectContext> for StepOutputs {
    fn resolve_arg(&self, arg: ProjectArg) -> Result<ProjectContext, BatchArgError> {
        match arg {
            ProjectArg::Literal(ctx) => Ok(ctx),
            ProjectArg::FromStep(step) => self.project_from(step),
        }
    }
}

impl ResolveArg<TrackArg, TrackRef> for StepOutputs {
    fn resolve_arg(&self, arg: TrackArg) -> Result<TrackRef, BatchArgError> {
        match arg {
            TrackArg::Literal(t) => Ok(t),
            TrackArg::FromStep(step) => self.track_from(step, None),
            TrackArg::FromStepIndex(step, index) => self.track_from(step, Some(index)),
        }
    }
}

impl ResolveArg<FxChainArg, FxChainContext> for StepOutputs {
    fn resolve_arg(&self, arg: FxChainArg) -> Result<FxChainContext, BatchArgError> {
        match arg {
            FxChainArg::Literal(chain) => Ok(chain),
            FxChainArg::TrackFromStep(step) => {
                let TrackRef::Guid(guid) = self.track_from(step, None)? else {
                    return Err(BatchArgError::WrongKind {
                        step,
                        wanted: "track guid",
                    });
                };
                Ok(FxChainContext::Track(guid))
            }
        }
    }
}

/// Execute a whole batch program against a backend. This is the
/// canonical `BatchExecution::execute` body — backends delegate here.
///
/// Semantics:
/// - Steps run in instruction order; successful outputs become
///   resolvable by later `FromStep` arguments.
/// - A failed argument resolution reports `Skipped(dep)` when the
///   referenced step failed/never ran, `Error` otherwise.
/// - With `fail_fast`, every step after the first error reports
///   `Skipped(failed_step)`.
/// - With `undo_label`, the program is wrapped in one undo block on
///   the current project.
pub fn run<B: BatchBackend + ?Sized>(backend: &B, request: BatchRequest) -> BatchResponse {
    let undo_label = request.options.undo_label.clone();
    if let Some(label) = &undo_label {
        crate::project::Projects::begin_undo_block(backend, ProjectContext::Current, label);
    }

    let mut outputs = StepOutputs::with_capacity(request.instructions.len());
    let mut results = Vec::with_capacity(request.instructions.len());
    let mut failed_step: Option<u32> = None;

    for instruction in request.instructions {
        let step = instruction.step;
        if let Some(failed) = failed_step {
            results.push(StepResult {
                step,
                outcome: StepOutcome::Skipped(failed),
            });
            continue;
        }
        let outcome = match instruction.op.apply(backend, &outputs) {
            Ok(output) => {
                outputs.record(step, output.clone());
                StepOutcome::Ok(output)
            }
            Err(BatchArgError::MissingStep(dep)) => StepOutcome::Skipped(dep),
            Err(err) => StepOutcome::Error(err.to_string()),
        };
        if request.options.fail_fast && !matches!(outcome, StepOutcome::Ok(_)) {
            failed_step = Some(step);
        }
        results.push(StepResult { step, outcome });
    }

    if let Some(label) = &undo_label {
        crate::project::Projects::end_undo_block(backend, ProjectContext::Current, label, None);
    }

    BatchResponse { results }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::program::BatchInstruction;

    /// Lock the JSON wire shape agents use with `daw op` / `daw batch`:
    /// externally-tagged nesting service → method → named args.
    #[test]
    fn batch_op_json_round_trip() {
        let op = BatchOp::Marker(MarkersOp::Add {
            project: ProjectArg::Literal(ProjectContext::Current),
            position: 1.5,
            name: "verse".into(),
        });
        let json = facet_json::to_string(&op).expect("serialize");
        let back: BatchOp = facet_json::from_str(&json).expect("deserialize");
        let BatchOp::Marker(MarkersOp::Add { position, name, .. }) = back else {
            panic!("wrong variant after round trip: {json}");
        };
        assert_eq!(position, 1.5);
        assert_eq!(name, "verse");
        println!("wire JSON: {json}");
    }

    /// Every batch wire type must survive phon's schema-exchange
    /// decode path (what vox runs per method) — a tuple or foreign
    /// std type in any op signature poisons the whole batch schema.
    #[test]
    fn batch_wire_types_survive_phon_schema_exchange() {
        fn probe<T>(value: &T)
        where
            T: for<'a> facet::Facet<'a> + core::fmt::Debug,
        {
            let name = core::any::type_name::<T>();
            let bytes = vox_phon::to_vec(value).unwrap_or_else(|e| panic!("{name}: encode: {e:?}"));
            let schema =
                vox_phon::schema_bytes::<T>().unwrap_or_else(|e| panic!("{name}: schema: {e:?}"));
            let bundle = vox_phon::parse_schema_bytes(&schema)
                .unwrap_or_else(|e| panic!("{name}: parse: {e:?}"));
            let program = vox_phon::build_decode_program::<T>(&bundle)
                .unwrap_or_else(|e| panic!("{name}: compat program: {e:?}"));
            vox_phon::decode_owned_with_program::<T>(&program, &bytes)
                .unwrap_or_else(|e| panic!("{name}: compat decode: {e:?}"));
        }

        probe(&BatchRequest {
            instructions: vec![BatchInstruction {
                step: 0,
                op: BatchOp::Marker(MarkersOp::Add {
                    project: ProjectArg::Literal(ProjectContext::Current),
                    position: 1.0,
                    name: "x".into(),
                }),
            }],
            options: Default::default(),
        });
        probe(&BatchResponse {
            results: vec![StepResult {
                step: 0,
                outcome: StepOutcome::Ok(BatchOpOutput::Marker(MarkersOpOutput::Add(Ok(1)))),
            }],
        });
        // What actually crosses the wire: the vox caller-signature root.
        let wire: Result<BatchResponse, vox::VoxError<core::convert::Infallible>> =
            Ok(BatchResponse {
                results: vec![StepResult {
                    step: 0,
                    outcome: StepOutcome::Ok(BatchOpOutput::Marker(MarkersOpOutput::Add(Ok(1)))),
                }],
            });
        probe(&wire);
    }
}
