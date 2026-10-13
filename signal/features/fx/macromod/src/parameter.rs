//! Normalized parameter value and block parameter types.

use facet::Facet;

use crate::range::ParameterRange;
use serde::{Deserialize, Serialize};

/// A normalized parameter value clamped to `[0.0, 1.0]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct ParameterValue(f32);

impl ParameterValue {
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Default for ParameterValue {
    fn default() -> Self {
        Self(0.5)
    }
}

/// A named parameter within a processing block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct BlockParameter {
    id: String,
    name: String,
    value: ParameterValue,
    /// Original DAW plugin parameter name, when it differs from `name`.
    ///
    /// Some importers rename parameters for display (e.g. `"Band 1 Frequency"` → `"B1 Freq"`).
    /// This field preserves the original name so `set_parameter_by_name` can use it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    daw_name: Option<String>,
    /// What this parameter's normalized position means — its span, its taper
    /// and its unit. See [`ParameterRange`].
    ///
    /// Defaults to the unit interval, so a parameter nobody has given a range
    /// behaves exactly as it did before ranges existed: normalized in,
    /// normalized out.
    #[serde(default)]
    #[facet(default)]
    range: ParameterRange,
}

impl BlockParameter {
    /// A parameter holding a **normalized** value, with no range of its own.
    pub fn new(id: impl Into<String>, name: impl Into<String>, value: f32) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            value: ParameterValue::new(value),
            daw_name: None,
            range: ParameterRange::default(),
        }
    }

    /// A parameter holding a **real** value in its own units — 5500 Hz, −40 dB.
    ///
    /// The value is normalized through `range` on the way in, which is the
    /// thing that was impossible before: `new(.., 5500.0)` silently clamped to
    /// 1.0, because a bare `ParameterValue` has nowhere to put the scale.
    pub fn ranged(
        id: impl Into<String>,
        name: impl Into<String>,
        real: f32,
        range: ParameterRange,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            value: ParameterValue::new(range.normalize(real)),
            daw_name: None,
            range,
        }
    }

    /// Give an existing parameter a range, keeping its normalized position.
    #[must_use]
    pub const fn with_range(mut self, range: ParameterRange) -> Self {
        self.range = range;
        self
    }

    #[must_use]
    pub const fn range(&self) -> &ParameterRange {
        &self.range
    }

    /// The value in its own units — what a DSP wants and a player reads.
    #[must_use]
    pub fn real(&self) -> f32 {
        self.range.denormalize(self.value.get())
    }

    /// Set from a real value, normalizing through the range.
    pub fn set_real(&mut self, real: f32) {
        self.value = ParameterValue::new(self.range.normalize(real));
    }

    /// The value as a player reads it, units included — `"5.50 kHz"`.
    #[must_use]
    pub fn display(&self) -> String {
        self.range.display(self.value.get())
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn value(&self) -> ParameterValue {
        self.value
    }

    pub const fn set_value(&mut self, value: f32) {
        self.value = ParameterValue::new(value);
    }

    /// Set the original DAW parameter name (builder pattern).
    #[must_use]
    pub fn with_daw_name(mut self, name: impl Into<String>) -> Self {
        self.daw_name = Some(name.into());
        self
    }

    /// The original DAW parameter name, if it differs from the display name.
    #[must_use]
    pub fn daw_name(&self) -> Option<&str> {
        self.daw_name.as_deref()
    }

    /// The name to use when setting parameters in the DAW.
    ///
    /// Returns `daw_name` if set, otherwise falls back to `name`.
    #[must_use]
    pub fn effective_daw_name(&self) -> &str {
        self.daw_name.as_deref().unwrap_or(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_value_clamps() {
        assert_eq!(ParameterValue::new(1.5).get(), 1.0);
        assert_eq!(ParameterValue::new(-0.5).get(), 0.0);
        assert_eq!(ParameterValue::new(0.7).get(), 0.7);
    }

    #[test]
    fn block_parameter_accessors() {
        let mut p = BlockParameter::new("gain", "Gain", 0.8);
        assert_eq!(p.id(), "gain");
        assert_eq!(p.name(), "Gain");
        assert!((p.value().get() - 0.8).abs() < 1e-6);
        p.set_value(0.3);
        assert!((p.value().get() - 0.3).abs() < 1e-6);
    }

    #[test]
    fn serde_round_trip() {
        let p = BlockParameter::new("tone", "Tone", 0.5);
        let json = serde_json::to_string(&p).unwrap();
        let parsed: BlockParameter = serde_json::from_str(&json).unwrap();
        assert_eq!(p, parsed);
    }
}
