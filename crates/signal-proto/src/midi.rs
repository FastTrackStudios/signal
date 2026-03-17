//! MIDI CC mapping and learn types for external controller integration.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// Response curve for a MIDI CC → parameter mapping.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum CcCurve {
    /// Direct 1:1 mapping.
    #[default]
    Linear,
    /// Slow start, fast end — finer control at low CC values.
    Logarithmic,
    /// Fast start, slow end — finer control at high CC values.
    Exponential,
    /// On/off toggle at midpoint (CC >= 64 = on).
    Toggle,
}

impl CcCurve {
    /// Apply the curve to a normalized CC value (0.0–1.0).
    pub fn apply(self, value: f64) -> f64 {
        let v = value.clamp(0.0, 1.0);
        match self {
            Self::Linear => v,
            Self::Logarithmic => {
                // log curve: slower response at low values
                let base = 10.0_f64;
                (base.powf(v) - 1.0) / (base - 1.0)
            }
            Self::Exponential => {
                // exp curve: faster response at low values
                let base = 10.0_f64;
                v.log(base) * (base - 1.0) + 1.0 // inverse of log
            }
            Self::Toggle => {
                if v >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    pub const ALL: &'static [CcCurve] = &[
        Self::Linear,
        Self::Logarithmic,
        Self::Exponential,
        Self::Toggle,
    ];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Logarithmic => "Logarithmic",
            Self::Exponential => "Exponential",
            Self::Toggle => "Toggle",
        }
    }
}

/// What a MIDI CC controls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum MidiTarget {
    /// Controls the morph slider position (0.0–1.0).
    MorphSlider,
    /// Controls a specific block parameter.
    Parameter {
        /// Block ID within the module/rig.
        block_id: String,
        /// Parameter ID within the block.
        parameter_id: String,
    },
    /// Triggers scene/snapshot switching (CC value selects scene index).
    SceneSelect,
}

/// A single MIDI CC → target mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct MidiCcMapping {
    /// MIDI channel (0–15, None = omni).
    pub channel: Option<u8>,
    /// CC number (0–127).
    pub cc_number: u8,
    /// What this CC controls.
    pub target: MidiTarget,
    /// Response curve.
    pub curve: CcCurve,
    /// Minimum output value (default 0.0).
    pub range_min: f32,
    /// Maximum output value (default 1.0).
    pub range_max: f32,
    /// Whether this mapping is active.
    pub enabled: bool,
}

impl MidiCcMapping {
    pub fn new(cc_number: u8, target: MidiTarget) -> Self {
        Self {
            channel: None,
            cc_number,
            target,
            curve: CcCurve::default(),
            range_min: 0.0,
            range_max: 1.0,
            enabled: true,
        }
    }

    /// Apply curve and range to a raw CC value (0–127).
    pub fn map_value(&self, cc_value: u8) -> f32 {
        let normalized = cc_value as f64 / 127.0;
        let curved = self.curve.apply(normalized);
        let range = self.range_max - self.range_min;
        self.range_min + (curved as f32 * range)
    }
}

/// State of the MIDI learn process.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum MidiLearnState {
    /// Not learning — normal operation.
    #[default]
    Idle,
    /// Waiting for a CC message to assign to the target.
    Learning { target: MidiTarget },
    /// A CC was received and assigned.
    Assigned {
        target: MidiTarget,
        cc_number: u8,
        channel: Option<u8>,
    },
}

/// Collection of MIDI CC mappings for a rig.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Facet)]
pub struct MidiMappingSet {
    pub mappings: Vec<MidiCcMapping>,
}

impl MidiMappingSet {
    pub fn new() -> Self {
        Self {
            mappings: Vec::new(),
        }
    }

    /// Find mappings that match a given CC message.
    pub fn find_mappings(&self, channel: u8, cc_number: u8) -> Vec<&MidiCcMapping> {
        self.mappings
            .iter()
            .filter(|m| {
                m.enabled && m.cc_number == cc_number && m.channel.map_or(true, |ch| ch == channel)
            })
            .collect()
    }

    pub fn add(&mut self, mapping: MidiCcMapping) {
        self.mappings.push(mapping);
    }

    pub fn remove_by_cc(&mut self, cc_number: u8) {
        self.mappings.retain(|m| m.cc_number != cc_number);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cc_curve_linear_identity() {
        assert_eq!(CcCurve::Linear.apply(0.0), 0.0);
        assert_eq!(CcCurve::Linear.apply(0.5), 0.5);
        assert_eq!(CcCurve::Linear.apply(1.0), 1.0);
    }

    #[test]
    fn toggle_curve_threshold() {
        assert_eq!(CcCurve::Toggle.apply(0.0), 0.0);
        assert_eq!(CcCurve::Toggle.apply(0.49), 0.0);
        assert_eq!(CcCurve::Toggle.apply(0.5), 1.0);
        assert_eq!(CcCurve::Toggle.apply(1.0), 1.0);
    }

    #[test]
    fn mapping_value_range() {
        let mapping = MidiCcMapping {
            channel: None,
            cc_number: 1,
            target: MidiTarget::MorphSlider,
            curve: CcCurve::Linear,
            range_min: 0.2,
            range_max: 0.8,
            enabled: true,
        };
        let val = mapping.map_value(127);
        assert!((val - 0.8).abs() < 0.01);
        let val = mapping.map_value(0);
        assert!((val - 0.2).abs() < 0.01);
    }

    #[test]
    fn find_mappings_omni_channel() {
        let mut set = MidiMappingSet::new();
        set.add(MidiCcMapping::new(1, MidiTarget::MorphSlider));
        set.add(MidiCcMapping::new(2, MidiTarget::SceneSelect));

        let found = set.find_mappings(0, 1);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].cc_number, 1);
    }

    #[test]
    fn serde_round_trip() {
        let mapping = MidiCcMapping::new(11, MidiTarget::MorphSlider);
        let json = serde_json::to_string(&mapping).unwrap();
        let parsed: MidiCcMapping = serde_json::from_str(&json).unwrap();
        assert_eq!(mapping, parsed);
    }
}
