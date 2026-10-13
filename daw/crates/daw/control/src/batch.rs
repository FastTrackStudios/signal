//! Batch builder for constructing batch programs with type-safe step handles.
//!
//! Ops are the `#[architect::rpc(ops)]`-generated per-service enums —
//! every covered trait method is expressible; the named builder
//! methods below are conveniences for the common flows, and
//! [`BatchBuilder::push_raw`] covers the rest.
//!
//! # Example
//!
//! ```no_run
//! use daw_control::{BatchBuilder, BatchResponseExt, Daw};
//!
//! # async fn example(daw: &Daw) -> daw_control::Result<()> {
//! let mut b = BatchBuilder::new().with_undo("Setup routing");
//! let project = b.current_project();
//! let tracks = b.get_tracks(&project);
//! let transport = b.get_transport(&project);
//!
//! let response = daw.execute_batch(b.build()).await?;
//! let tracks: Vec<daw_proto::Track> = response.get(&tracks).unwrap();
//! let transport: daw_proto::transport::transport::Transport = response.get(&transport).unwrap();
//! # Ok(())
//! # }
//! ```

use daw_proto::batch::*;
use daw_proto::fx::{EffectsOp, EffectsOpOutput};
use daw_proto::marker::{MarkersOp, MarkersOpOutput};
use daw_proto::project::{ProjectsOp, ProjectsOpOutput};
use daw_proto::routing::{RoutingOp, RoutingOpOutput};
use daw_proto::track::{TracksOp, TracksOpOutput};
use daw_proto::transport::{TransportOp, TransportOpOutput};
use daw_proto::*;

/// Extraction function baked into a [`StepHandle`] by the builder
/// method that created it — each method knows exactly which output
/// variant its op produces.
type Extractor<T> = fn(&BatchOpOutput, u32) -> Result<T, BatchExtractError>;

/// A typed handle to a step in a batch program.
///
/// Created by [`BatchBuilder`] methods; redeem it against the
/// [`BatchResponse`] with [`BatchResponseExt::get`].
pub struct StepHandle<T> {
    index: u32,
    extract: Extractor<T>,
}

impl<T> StepHandle<T> {
    /// Get the step index.
    pub fn index(&self) -> u32 {
        self.index
    }
}

fn mismatch<T>(step: u32, expected: &'static str) -> Result<T, BatchExtractError> {
    Err(BatchExtractError::TypeMismatch { step, expected })
}

/// Flatten an application-level `DawResult` into the extraction
/// error channel.
fn flatten<T: Clone>(r: &DawResult<T>) -> Result<T, BatchExtractError> {
    r.clone()
        .map_err(|e| BatchExtractError::StepFailed(e.to_string()))
}

/// Builder for constructing batch programs with automatic step numbering.
pub struct BatchBuilder {
    instructions: Vec<BatchInstruction>,
    options: BatchOptions,
}

impl BatchBuilder {
    /// Create a new empty batch builder.
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            options: BatchOptions::default(),
        }
    }

    /// Set the undo label — all mutations will be grouped in a single undo block.
    pub fn with_undo(mut self, label: impl Into<String>) -> Self {
        self.options.undo_label = Some(label.into());
        self
    }

    /// Enable fail-fast mode — stop on first error.
    pub fn with_fail_fast(mut self) -> Self {
        self.options.fail_fast = true;
        self
    }

    /// Build the batch request.
    pub fn build(self) -> BatchRequest {
        BatchRequest {
            instructions: self.instructions,
            options: self.options,
        }
    }

    /// Add an instruction and return a typed handle carrying its
    /// output extractor.
    fn push<T>(&mut self, op: BatchOp, extract: Extractor<T>) -> StepHandle<T> {
        let index = self.instructions.len() as u32;
        self.instructions.push(BatchInstruction { step: index, op });
        StepHandle { index, extract }
    }

    // =========================================================================
    // Project operations
    // =========================================================================

    /// Get the current project.
    pub fn current_project(&mut self) -> StepHandle<Option<ProjectInfo>> {
        self.push(BatchOp::Project(ProjectsOp::Current), |o, step| match o {
            BatchOpOutput::Project(ProjectsOpOutput::Current(v)) => Ok(v.clone()),
            _ => mismatch(step, "Option<ProjectInfo>"),
        })
    }

    /// Get a specific project by GUID.
    pub fn get_project(&mut self, guid: impl Into<String>) -> StepHandle<Option<ProjectInfo>> {
        self.push(
            BatchOp::Project(ProjectsOp::Get {
                project_id: guid.into(),
            }),
            |o, step| match o {
                BatchOpOutput::Project(ProjectsOpOutput::Get(v)) => Ok(v.clone()),
                _ => mismatch(step, "Option<ProjectInfo>"),
            },
        )
    }

    /// List all open projects.
    pub fn list_projects(&mut self) -> StepHandle<Vec<ProjectInfo>> {
        self.push(BatchOp::Project(ProjectsOp::List), |o, step| match o {
            BatchOpOutput::Project(ProjectsOpOutput::List(v)) => Ok(v.clone()),
            _ => mismatch(step, "Vec<ProjectInfo>"),
        })
    }

    // =========================================================================
    // Transport operations
    // =========================================================================

    /// Play the project.
    pub fn play(&mut self, project: &StepHandle<Option<ProjectInfo>>) -> StepHandle<()> {
        self.push(
            BatchOp::Transport(TransportOp::Play {
                project: ProjectArg::FromStep(project.index),
            }),
            |o, step| match o {
                BatchOpOutput::Transport(TransportOpOutput::Play(r)) => flatten(r),
                _ => mismatch(step, "()"),
            },
        )
    }

    /// Stop the project.
    pub fn stop(&mut self, project: &StepHandle<Option<ProjectInfo>>) -> StepHandle<()> {
        self.push(
            BatchOp::Transport(TransportOp::Stop {
                project: ProjectArg::FromStep(project.index),
            }),
            |o, step| match o {
                BatchOpOutput::Transport(TransportOpOutput::Stop(r)) => flatten(r),
                _ => mismatch(step, "()"),
            },
        )
    }

    /// Get transport state.
    pub fn get_transport(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
    ) -> StepHandle<transport::transport::Transport> {
        self.push(
            BatchOp::Transport(TransportOp::GetState {
                project: ProjectArg::FromStep(project.index),
            }),
            |o, step| match o {
                BatchOpOutput::Transport(TransportOpOutput::GetState(v)) => Ok(v.clone()),
                _ => mismatch(step, "Transport"),
            },
        )
    }

    /// Get tempo.
    pub fn get_tempo(&mut self, project: &StepHandle<Option<ProjectInfo>>) -> StepHandle<f64> {
        self.push(
            BatchOp::Transport(TransportOp::GetTempo {
                project: ProjectArg::FromStep(project.index),
            }),
            |o, step| match o {
                BatchOpOutput::Transport(TransportOpOutput::GetTempo(v)) => Ok(*v),
                _ => mismatch(step, "f64"),
            },
        )
    }

    /// Set tempo.
    pub fn set_tempo(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        bpm: f64,
    ) -> StepHandle<()> {
        self.push(
            BatchOp::Transport(TransportOp::SetTempo {
                project: ProjectArg::FromStep(project.index),
                bpm,
            }),
            |o, step| match o {
                BatchOpOutput::Transport(TransportOpOutput::SetTempo(r)) => flatten(r),
                _ => mismatch(step, "()"),
            },
        )
    }

    // =========================================================================
    // Track operations
    // =========================================================================

    /// Get all tracks in a project.
    pub fn get_tracks(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
    ) -> StepHandle<Vec<Track>> {
        self.push(
            BatchOp::Track(TracksOp::All {
                project: ProjectArg::FromStep(project.index),
            }),
            |o, step| match o {
                BatchOpOutput::Track(TracksOpOutput::All(v)) => Ok(v.clone()),
                _ => mismatch(step, "Vec<Track>"),
            },
        )
    }

    /// Get a specific track by reference.
    pub fn get_track(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        track: TrackRef,
    ) -> StepHandle<Option<Track>> {
        self.push(
            BatchOp::Track(TracksOp::Get {
                project: ProjectArg::FromStep(project.index),
                track: TrackArg::Literal(track),
            }),
            |o, step| match o {
                BatchOpOutput::Track(TracksOpOutput::Get(v)) => Ok(v.clone()),
                _ => mismatch(step, "Option<Track>"),
            },
        )
    }

    /// Get track count.
    pub fn track_count(&mut self, project: &StepHandle<Option<ProjectInfo>>) -> StepHandle<u32> {
        self.push(
            BatchOp::Track(TracksOp::Count {
                project: ProjectArg::FromStep(project.index),
            }),
            |o, step| match o {
                BatchOpOutput::Track(TracksOpOutput::Count(v)) => Ok(*v),
                _ => mismatch(step, "u32"),
            },
        )
    }

    /// Add a track and get its GUID.
    pub fn add_track(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        name: impl Into<String>,
        at_index: Option<u32>,
    ) -> StepHandle<String> {
        self.push(
            BatchOp::Track(TracksOp::Add {
                project: ProjectArg::FromStep(project.index),
                name: name.into(),
                at_index,
            }),
            |o, step| match o {
                BatchOpOutput::Track(TracksOpOutput::Add(r)) => flatten(r),
                _ => mismatch(step, "String"),
            },
        )
    }

    /// Set track muted using a step handle from get_tracks (by index).
    pub fn set_track_muted_by_index(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        tracks: &StepHandle<Vec<Track>>,
        track_index: u32,
        muted: bool,
    ) -> StepHandle<()> {
        self.push(
            BatchOp::Track(TracksOp::SetMuted {
                project: ProjectArg::FromStep(project.index),
                track: TrackArg::FromStepIndex(tracks.index, track_index),
                muted,
            }),
            |o, step| match o {
                BatchOpOutput::Track(TracksOpOutput::SetMuted(r)) => flatten(r),
                _ => mismatch(step, "()"),
            },
        )
    }

    /// Set track muted using a literal TrackRef.
    pub fn set_track_muted(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        track: TrackRef,
        muted: bool,
    ) -> StepHandle<()> {
        self.push(
            BatchOp::Track(TracksOp::SetMuted {
                project: ProjectArg::FromStep(project.index),
                track: TrackArg::Literal(track),
                muted,
            }),
            |o, step| match o {
                BatchOpOutput::Track(TracksOpOutput::SetMuted(r)) => flatten(r),
                _ => mismatch(step, "()"),
            },
        )
    }

    /// Set track volume.
    pub fn set_track_volume(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        track: TrackRef,
        volume: f64,
    ) -> StepHandle<()> {
        self.push(
            BatchOp::Track(TracksOp::SetVolume {
                project: ProjectArg::FromStep(project.index),
                track: TrackArg::Literal(track),
                volume,
            }),
            |o, step| match o {
                BatchOpOutput::Track(TracksOpOutput::SetVolume(r)) => flatten(r),
                _ => mismatch(step, "()"),
            },
        )
    }

    /// Rename a track.
    pub fn rename_track(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        track: TrackRef,
        name: impl Into<String>,
    ) -> StepHandle<()> {
        self.push(
            BatchOp::Track(TracksOp::Rename {
                project: ProjectArg::FromStep(project.index),
                track: TrackArg::Literal(track),
                name: name.into(),
            }),
            |o, step| match o {
                BatchOpOutput::Track(TracksOpOutput::Rename(r)) => flatten(r),
                _ => mismatch(step, "()"),
            },
        )
    }

    // =========================================================================
    // FX operations
    // =========================================================================

    /// Get the FX list for a track (using step handle for track).
    pub fn get_fx_list_from_track(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        track: &StepHandle<String>,
    ) -> StepHandle<Vec<Fx>> {
        self.push(
            BatchOp::Fx(EffectsOp::List {
                project: ProjectArg::FromStep(project.index),
                chain: FxChainArg::TrackFromStep(track.index),
            }),
            |o, step| match o {
                BatchOpOutput::Fx(EffectsOpOutput::List(v)) => Ok(v.clone()),
                _ => mismatch(step, "Vec<Fx>"),
            },
        )
    }

    /// Get the FX list using a literal chain context.
    pub fn get_fx_list(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        chain: FxChainContext,
    ) -> StepHandle<Vec<Fx>> {
        self.push(
            BatchOp::Fx(EffectsOp::List {
                project: ProjectArg::FromStep(project.index),
                chain: FxChainArg::Literal(chain),
            }),
            |o, step| match o {
                BatchOpOutput::Fx(EffectsOpOutput::List(v)) => Ok(v.clone()),
                _ => mismatch(step, "Vec<Fx>"),
            },
        )
    }

    /// Get FX parameters.
    pub fn get_fx_parameters(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        target: FxTarget,
    ) -> StepHandle<Vec<FxParameter>> {
        self.push(
            BatchOp::Fx(EffectsOp::Parameters {
                project: ProjectArg::FromStep(project.index),
                target,
            }),
            |o, step| match o {
                BatchOpOutput::Fx(EffectsOpOutput::Parameters(v)) => Ok(v.clone()),
                _ => mismatch(step, "Vec<FxParameter>"),
            },
        )
    }

    /// Set FX enabled state.
    pub fn set_fx_enabled(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        target: FxTarget,
        enabled: bool,
    ) -> StepHandle<()> {
        self.push(
            BatchOp::Fx(EffectsOp::SetEnabled {
                project: ProjectArg::FromStep(project.index),
                target,
                enabled,
            }),
            |o, step| match o {
                BatchOpOutput::Fx(EffectsOpOutput::SetEnabled(r)) => flatten(r),
                _ => mismatch(step, "()"),
            },
        )
    }

    /// Add an FX plugin to a chain.
    pub fn add_fx(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        chain: FxChainContext,
        name: impl Into<String>,
    ) -> StepHandle<Option<String>> {
        self.push(
            BatchOp::Fx(EffectsOp::Add {
                project: ProjectArg::FromStep(project.index),
                chain: FxChainArg::Literal(chain),
                name: name.into(),
            }),
            |o, step| match o {
                BatchOpOutput::Fx(EffectsOpOutput::Add(v)) => Ok(v.clone()),
                _ => mismatch(step, "Option<String>"),
            },
        )
    }

    /// Set an FX parameter value.
    pub fn set_fx_parameter(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        request: SetParameterRequest,
    ) -> StepHandle<()> {
        self.push(
            BatchOp::Fx(EffectsOp::SetParameter {
                project: ProjectArg::FromStep(project.index),
                request,
            }),
            |o, step| match o {
                BatchOpOutput::Fx(EffectsOpOutput::SetParameter(r)) => flatten(r),
                _ => mismatch(step, "()"),
            },
        )
    }

    // =========================================================================
    // Routing operations
    // =========================================================================

    /// Get sends for a track.
    pub fn get_sends(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        track: TrackRef,
    ) -> StepHandle<Vec<TrackRoute>> {
        self.push(
            BatchOp::Routing(RoutingOp::Sends {
                project: ProjectArg::FromStep(project.index),
                track: TrackArg::Literal(track),
            }),
            |o, step| match o {
                BatchOpOutput::Routing(RoutingOpOutput::Sends(v)) => Ok(v.clone()),
                _ => mismatch(step, "Vec<TrackRoute>"),
            },
        )
    }

    /// Add a send between two tracks.
    pub fn add_send(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        source: TrackRef,
        dest: TrackRef,
    ) -> StepHandle<Option<u32>> {
        self.push(
            BatchOp::Routing(RoutingOp::AddSend {
                project: ProjectArg::FromStep(project.index),
                source: TrackArg::Literal(source),
                dest: TrackArg::Literal(dest),
            }),
            |o, step| match o {
                BatchOpOutput::Routing(RoutingOpOutput::AddSend(v)) => Ok(*v),
                _ => mismatch(step, "Option<u32>"),
            },
        )
    }

    // =========================================================================
    // Marker operations
    // =========================================================================

    /// Get all markers.
    pub fn get_markers(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
    ) -> StepHandle<Vec<Marker>> {
        self.push(
            BatchOp::Marker(MarkersOp::All {
                project: ProjectArg::FromStep(project.index),
            }),
            |o, step| match o {
                BatchOpOutput::Marker(MarkersOpOutput::All(v)) => Ok(v.clone()),
                _ => mismatch(step, "Vec<Marker>"),
            },
        )
    }

    /// Add a marker.
    pub fn add_marker(
        &mut self,
        project: &StepHandle<Option<ProjectInfo>>,
        position: f64,
        name: impl Into<String>,
    ) -> StepHandle<u32> {
        self.push(
            BatchOp::Marker(MarkersOp::Add {
                project: ProjectArg::FromStep(project.index),
                position,
                name: name.into(),
            }),
            |o, step| match o {
                BatchOpOutput::Marker(MarkersOpOutput::Add(r)) => flatten(r),
                _ => mismatch(step, "u32"),
            },
        )
    }

    // =========================================================================
    // Raw op — for ops not covered by convenience methods
    // =========================================================================

    /// Push a raw batch operation with a custom extractor. Use this
    /// for operations not covered by convenience methods above; pass
    /// an extractor matching the op's output variant (or one that
    /// always succeeds for fire-and-forget steps).
    pub fn push_raw<T>(&mut self, op: BatchOp, extract: Extractor<T>) -> StepHandle<T> {
        self.push(op, extract)
    }

    /// Push a raw batch operation whose result the caller never
    /// redeems (fire-and-forget mutation).
    pub fn push_unchecked(&mut self, op: BatchOp) -> StepHandle<()> {
        self.push(op, |_, _| Ok(()))
    }
}

impl Default for BatchBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Response extraction
// =============================================================================

/// Error type for batch response extraction.
#[derive(Debug)]
pub enum BatchExtractError {
    /// The step was not found in the response.
    StepNotFound(u32),
    /// The step failed with an error.
    StepFailed(String),
    /// The step was skipped because a dependency failed.
    StepSkipped(u32),
    /// The output type didn't match the expected type.
    TypeMismatch { step: u32, expected: &'static str },
}

impl std::fmt::Display for BatchExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StepNotFound(s) => write!(f, "step {} not found in response", s),
            Self::StepFailed(msg) => write!(f, "step failed: {}", msg),
            Self::StepSkipped(dep) => write!(f, "step skipped due to dependency {} failing", dep),
            Self::TypeMismatch { step, expected } => {
                write!(
                    f,
                    "step {} output type mismatch, expected {}",
                    step, expected
                )
            }
        }
    }
}

impl std::error::Error for BatchExtractError {}

/// Extension trait for extracting typed results from a `BatchResponse`.
pub trait BatchResponseExt {
    /// Extract a typed result from a batch response using a step handle.
    fn get<T>(&self, handle: &StepHandle<T>) -> Result<T, BatchExtractError>;
}

impl BatchResponseExt for BatchResponse {
    fn get<T>(&self, handle: &StepHandle<T>) -> Result<T, BatchExtractError> {
        let result = self
            .results
            .iter()
            .find(|r| r.step == handle.index)
            .ok_or(BatchExtractError::StepNotFound(handle.index))?;

        match &result.outcome {
            StepOutcome::Ok(output) => (handle.extract)(output, handle.index),
            StepOutcome::Error(msg) => Err(BatchExtractError::StepFailed(msg.clone())),
            StepOutcome::Skipped(dep) => Err(BatchExtractError::StepSkipped(*dep)),
        }
    }
}
