//! Macro binding — maps a macro knob to a target parameter with range and curve.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::response::ResponseCurve;
use crate::target::ParamTarget;

/// Maps a macro knob to a target parameter with range scaling and response curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct MacroBinding {
    /// The parameter this binding controls.
    pub target: ParamTarget,
    /// Parameter value when macro = 0.
    pub min: f32,
    /// Parameter value when macro = 1.
    pub max: f32,
    /// Response curve for the mapping.
    pub curve: ResponseCurve,
}

impl MacroBinding {
    #[must_use]
    pub fn new(target: ParamTarget, min: f32, max: f32) -> Self {
        Self {
            target,
            min,
            max,
            curve: ResponseCurve::default(),
        }
    }

    /// Convenience: create from `block_id` + `param_id` strings (backward-compatible API).
    pub fn from_ids(
        target_block_id: impl Into<String>,
        target_param_id: impl Into<String>,
        min: f32,
        max: f32,
    ) -> Self {
        Self::new(ParamTarget::new(target_block_id, target_param_id), min, max)
    }

    #[must_use]
    pub fn with_curve(mut self, curve: impl Into<ResponseCurve>) -> Self {
        self.curve = curve.into();
        self
    }

    /// Returns a copy with min/max swapped (inverts the binding direction).
    ///
    /// Instead of a separate `invert: bool` field, inversion is expressed by
    /// having `min > max`. This method is a convenience for creating the swapped version.
    #[must_use]
    pub fn inverted(&self) -> Self {
        Self {
            target: self.target.clone(),
            min: self.max,
            max: self.min,
            curve: self.curve,
        }
    }

    /// Whether this binding runs in reverse (min > max).
    #[must_use]
    pub fn is_inverted(&self) -> bool {
        self.min > self.max
    }
}
