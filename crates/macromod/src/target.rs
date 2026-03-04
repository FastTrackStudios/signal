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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn eq_and_hash() {
        let a = ParamTarget::new("amp", "gain");
        let b = ParamTarget::new("amp", "gain");
        let c = ParamTarget::new("amp", "tone");
        assert_eq!(a, b);
        assert_ne!(a, c);

        let mut map = HashMap::new();
        map.insert(a.clone(), 42);
        assert_eq!(map.get(&b), Some(&42));
        assert_eq!(map.get(&c), None);
    }

    #[test]
    fn serde_round_trip() {
        let target = ParamTarget::new("drive", "level");
        let json = serde_json::to_string(&target).unwrap();
        let parsed: ParamTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(target, parsed);
    }
}
