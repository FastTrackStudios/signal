//! Unified parameter targeting — identifies a specific parameter within a block.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// Identifies a specific parameter within a processing block.
///
/// Used by both macro bindings and modulation routes as the unified
/// targeting mechanism. Derives `Eq + Hash` for use as map keys.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
pub struct ParamTarget {
    /// Block ID within the module/rig.
    pub block_id: String,
    /// Parameter ID within the block.
    pub param_id: String,
}

impl ParamTarget {
    pub fn new(block_id: impl Into<String>, param_id: impl Into<String>) -> Self {
        Self {
            block_id: block_id.into(),
            param_id: param_id.into(),
        }
    }
}

/// Legacy alias — modulation routes previously used this name.
pub type ModulationTarget = ParamTarget;
