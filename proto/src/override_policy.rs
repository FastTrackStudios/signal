//! Override policy enforcement by domain scope.
//!
//! This encodes the rule set:
//! - Snapshot scopes: parameter-only edits (no topology/switching)
//! - Scene scopes: may switch refs, but no signal-flow mutation ops
//! - Patch/Section scopes: full override freedom

use crate::overrides::{NodeOverrideOp, NodePath, NodePathSegment, Override};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverridePolicyError {
    InvalidPath { index: usize },
    OperationNotAllowed { index: usize },
    ParameterTargetRequired { index: usize },
    NonParameterTargetRequired { index: usize },
}

pub trait OverridePolicy {
    /// # Errors
    ///
    /// Returns an error if the override is invalid for this policy.
    fn validate_override(index: usize, ov: &Override) -> Result<(), OverridePolicyError>;
}

pub struct SnapshotPolicy;
pub struct ScenePolicy;
pub struct FreePolicy;

fn is_parameter_target(path: &NodePath) -> bool {
    matches!(
        path.segments().last(),
        Some(NodePathSegment::Parameter { id: _ })
    )
}

const fn is_flow_mutation(op: &NodeOverrideOp) -> bool {
    matches!(
        op,
        NodeOverrideOp::InsertBefore { id: _ }
            | NodeOverrideOp::InsertAfter { id: _ }
            | NodeOverrideOp::Remove
    )
}

fn is_path_valid(path: &NodePath) -> bool {
    path.validate().is_ok()
}

impl OverridePolicy for SnapshotPolicy {
    fn validate_override(index: usize, ov: &Override) -> Result<(), OverridePolicyError> {
        if !is_path_valid(&ov.path) {
            return Err(OverridePolicyError::InvalidPath { index });
        }
        if !matches!(ov.op, NodeOverrideOp::Set { value: _ }) {
            return Err(OverridePolicyError::OperationNotAllowed { index });
        }
        if !is_parameter_target(&ov.path) {
            return Err(OverridePolicyError::ParameterTargetRequired { index });
        }
        Ok(())
    }
}

impl OverridePolicy for ScenePolicy {
    fn validate_override(index: usize, ov: &Override) -> Result<(), OverridePolicyError> {
        if !is_path_valid(&ov.path) {
            return Err(OverridePolicyError::InvalidPath { index });
        }
        if is_flow_mutation(&ov.op) {
            return Err(OverridePolicyError::OperationNotAllowed { index });
        }

        match ov.op {
            NodeOverrideOp::Set { value: _ } => {
                if !is_parameter_target(&ov.path) {
                    return Err(OverridePolicyError::ParameterTargetRequired { index });
                }
            }
            NodeOverrideOp::ReplaceRef { id: _ } => {
                if is_parameter_target(&ov.path) {
                    return Err(OverridePolicyError::NonParameterTargetRequired { index });
                }
            }
            NodeOverrideOp::Bypass { bypassed: _ } | NodeOverrideOp::Enable { enabled: _ } => {}
            NodeOverrideOp::InsertBefore { id: _ }
            | NodeOverrideOp::InsertAfter { id: _ }
            | NodeOverrideOp::Remove => {
                return Err(OverridePolicyError::OperationNotAllowed { index });
            }
        }

        Ok(())
    }
}

impl OverridePolicy for FreePolicy {
    fn validate_override(index: usize, ov: &Override) -> Result<(), OverridePolicyError> {
        if !is_path_valid(&ov.path) {
            return Err(OverridePolicyError::InvalidPath { index });
        }
        Ok(())
    }
}

/// # Errors
///
/// Returns an error if any override fails validation under policy `P`.
pub fn validate_overrides<P: OverridePolicy>(
    overrides: &[Override],
) -> Result<(), OverridePolicyError> {
    for (index, ov) in overrides.iter().enumerate() {
        P::validate_override(index, ov)?;
    }
    Ok(())
}
