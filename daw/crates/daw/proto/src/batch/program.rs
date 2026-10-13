//! Batch program types — request, response, and instruction containers.

use super::{BatchOp, BatchOpOutput};
use facet::Facet;

/// A batch program to execute as a single RPC call.
#[derive(Clone, Debug, Facet)]
pub struct BatchRequest {
    /// Ordered list of instructions to execute.
    pub instructions: Vec<BatchInstruction>,
    /// Execution options.
    pub options: BatchOptions,
}

/// A single instruction in a batch program.
#[derive(Clone, Debug, Facet)]
pub struct BatchInstruction {
    /// Step index (must be unique and sequential starting from 0).
    pub step: u32,
    /// The operation to perform.
    pub op: BatchOp,
}

/// Options controlling batch execution behavior.
#[derive(Clone, Debug, Default, Facet)]
pub struct BatchOptions {
    /// If set, wrap all mutations in a single REAPER undo block with this label.
    pub undo_label: Option<String>,
    /// Stop execution on the first error (default: false, execute all independent steps).
    pub fail_fast: bool,
}

/// Response from a batch execution.
#[derive(Clone, Debug, Facet)]
pub struct BatchResponse {
    /// Results for each step, in order.
    pub results: Vec<StepResult>,
}

/// Result of a single step in a batch.
#[derive(Clone, Debug, Facet)]
pub struct StepResult {
    /// The step index this result corresponds to.
    pub step: u32,
    /// The outcome of the step.
    pub outcome: StepOutcome,
}

/// Outcome of a single batch step.
// Wire/domain type: variant size asymmetry is inherent (the success
// payload dwarfs the error string) and boxing would ripple through
// every constructor/match across the workspace.
#[allow(clippy::large_enum_variant)]
#[repr(u8)]
#[derive(Clone, Debug, Facet)]
pub enum StepOutcome {
    /// Step completed successfully with this output.
    Ok(BatchOpOutput),
    /// Step failed with this error message.
    Error(String),
    /// Step was skipped because dependency step `n` failed.
    Skipped(u32),
}

impl BatchResponse {
    /// Get the output of a specific step, if it succeeded.
    pub fn step_output(&self, step: u32) -> Option<&BatchOpOutput> {
        self.results.iter().find_map(|r| {
            if r.step == step {
                if let StepOutcome::Ok(ref output) = r.outcome {
                    Some(output)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }
}
