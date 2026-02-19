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
    fn validate_override(index: usize, ov: &Override) -> Result<(), OverridePolicyError>;
}

pub struct SnapshotPolicy;
pub struct ScenePolicy;
pub struct FreePolicy;

fn is_parameter_target(path: &NodePath) -> bool {
    matches!(path.segments().last(), Some(NodePathSegment::Parameter(_)))
}

fn is_flow_mutation(op: &NodeOverrideOp) -> bool {
    matches!(
        op,
        NodeOverrideOp::InsertBefore(_) | NodeOverrideOp::InsertAfter(_) | NodeOverrideOp::Remove
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
        if !matches!(ov.op, NodeOverrideOp::Set(_)) {
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
            NodeOverrideOp::Set(_) => {
                if !is_parameter_target(&ov.path) {
                    return Err(OverridePolicyError::ParameterTargetRequired { index });
                }
            }
            NodeOverrideOp::ReplaceRef(_) => {
                if is_parameter_target(&ov.path) {
                    return Err(OverridePolicyError::NonParameterTargetRequired { index });
                }
            }
            NodeOverrideOp::Bypass(_) | NodeOverrideOp::Enable(_) => {}
            NodeOverrideOp::InsertBefore(_)
            | NodeOverrideOp::InsertAfter(_)
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

pub fn validate_overrides<P: OverridePolicy>(
    overrides: &[Override],
) -> Result<(), OverridePolicyError> {
    for (index, ov) in overrides.iter().enumerate() {
        P::validate_override(index, ov)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overrides::NodeOverrideOp;
    use crate::ParameterValue;

    #[test]
    fn snapshot_allows_only_parameter_set() {
        let ok = Override::set(
            NodePath::module("m").with_block("b").with_parameter("p"),
            0.5,
        );
        assert!(validate_overrides::<SnapshotPolicy>(&[ok]).is_ok());

        let not_set = Override::bypass(NodePath::module("m").with_block("b"), true);
        assert!(matches!(
            validate_overrides::<SnapshotPolicy>(&[not_set]),
            Err(OverridePolicyError::OperationNotAllowed { .. })
        ));
    }

    #[test]
    fn scene_disallows_flow_mutation_but_allows_replace_ref() {
        let replace = Override {
            path: NodePath::engine("e").with_layer("l"),
            op: NodeOverrideOp::ReplaceRef("alt-layer".into()),
        };
        assert!(validate_overrides::<ScenePolicy>(&[replace]).is_ok());

        let mutate = Override {
            path: NodePath::engine("e").with_layer("l"),
            op: NodeOverrideOp::InsertBefore("x".into()),
        };
        assert!(matches!(
            validate_overrides::<ScenePolicy>(&[mutate]),
            Err(OverridePolicyError::OperationNotAllowed { .. })
        ));
    }

    #[test]
    fn free_policy_allows_topology_ops() {
        let ov = Override {
            path: NodePath::engine("e").with_layer("l"),
            op: NodeOverrideOp::InsertAfter("new-layer".into()),
        };
        assert!(validate_overrides::<FreePolicy>(&[ov]).is_ok());

        let set = Override {
            path: NodePath::engine("e").with_layer("l").with_parameter("x"),
            op: NodeOverrideOp::Set(ParameterValue::new(0.4)),
        };
        assert!(validate_overrides::<FreePolicy>(&[set]).is_ok());
    }
}
