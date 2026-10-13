//! Block abstraction for modular audio effect units.
//!
//! A Block represents a self-contained audio effect with:
//! - Multiple parameters
//! - A macro control for one-knob operation
//! - Support for different form factors and LOD levels
//! - Bypass functionality

use super::{
    layout::{FormFactor, LevelOfDetail},
    macro_curve::{MacroControl, ParameterMapping},
};
use std::collections::HashMap;

/// A parameter definition within a block.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockParameter {
    /// Unique identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Short name for compact displays.
    pub short_name: String,
    /// Current value (normalized 0.0 - 1.0).
    pub value: f32,
    /// Default value.
    pub default: f32,
    /// Unit label (dB, Hz, ms, %).
    pub unit: String,
    /// Display format function (normalized -> display string).
    pub format: ParameterFormat,
    /// Priority for LOD culling (lower = more important).
    pub priority: u8,
}

/// How to format a parameter value for display.
#[derive(Debug, Clone, PartialEq)]
pub enum ParameterFormat {
    /// Percentage (0-100%)
    Percent,
    /// Decibels with range
    Decibels { min: f32, max: f32 },
    /// Frequency (Hz/kHz)
    Frequency { min: f32, max: f32 },
    /// Time in milliseconds
    Milliseconds { min: f32, max: f32 },
    /// Time in seconds
    Seconds { min: f32, max: f32 },
    /// Simple float with precision
    Float { min: f32, max: f32, precision: u8 },
    /// Integer range
    Integer { min: i32, max: i32 },
    /// Enumerated options
    Enum { options: Vec<String> },
}

impl ParameterFormat {
    /// Format a normalized value (0.0 - 1.0) to a display string.
    #[must_use]
    pub fn format(&self, normalized: f32) -> String {
        match self {
            Self::Percent => format!("{:.0}%", normalized * 100.0),
            Self::Decibels { min, max } => {
                let db = min + normalized * (max - min);
                if db > -0.05 && db < 0.05 {
                    "0 dB".to_string()
                } else {
                    format!("{db:.1} dB")
                }
            }
            Self::Frequency { min, max } => {
                // Logarithmic scaling for frequency
                let log_min = min.ln();
                let log_max = max.ln();
                let freq = (log_min + normalized * (log_max - log_min)).exp();
                if freq >= 1000.0 {
                    format!("{:.1} kHz", freq / 1000.0)
                } else {
                    format!("{freq:.0} Hz")
                }
            }
            Self::Milliseconds { min, max } => {
                let ms = min + normalized * (max - min);
                format!("{ms:.0} ms")
            }
            Self::Seconds { min, max } => {
                let s = min + normalized * (max - min);
                format!("{s:.2} s")
            }
            Self::Float {
                min,
                max,
                precision,
            } => {
                let val = min + normalized * (max - min);
                format!("{val:.prec$}", prec = *precision as usize)
            }
            Self::Integer { min, max } => {
                let val = *min + (normalized * (*max - *min) as f32).round() as i32;
                format!("{val}")
            }
            Self::Enum { options } => {
                let idx = (normalized * (options.len() - 1) as f32).round() as usize;
                options.get(idx).cloned().unwrap_or_default()
            }
        }
    }

    /// Parse a display string back to normalized value.
    #[must_use]
    pub fn parse(&self, _input: &str) -> Option<f32> {
        // TODO: Implement parsing for each format type
        None
    }
}

impl BlockParameter {
    /// Create a new parameter with percent format.
    #[must_use]
    pub fn percent(id: impl Into<String>, name: impl Into<String>) -> Self {
        let name_str = name.into();
        Self {
            id: id.into(),
            name: name_str.clone(),
            short_name: name_str.chars().take(4).collect(),
            value: 0.5,
            default: 0.5,
            unit: "%".to_string(),
            format: ParameterFormat::Percent,
            priority: 5,
        }
    }

    /// Create a gain parameter in dB.
    #[must_use]
    pub fn gain(id: impl Into<String>, name: impl Into<String>, min: f32, max: f32) -> Self {
        let name_str = name.into();
        Self {
            id: id.into(),
            name: name_str.clone(),
            short_name: name_str.chars().take(4).collect(),
            value: 0.5,
            default: 0.5,
            unit: "dB".to_string(),
            format: ParameterFormat::Decibels { min, max },
            priority: 1,
        }
    }

    /// Create a frequency parameter.
    #[must_use]
    pub fn frequency(id: impl Into<String>, name: impl Into<String>, min: f32, max: f32) -> Self {
        let name_str = name.into();
        Self {
            id: id.into(),
            name: name_str.clone(),
            short_name: name_str.chars().take(4).collect(),
            value: 0.5,
            default: 0.5,
            unit: "Hz".to_string(),
            format: ParameterFormat::Frequency { min, max },
            priority: 3,
        }
    }

    /// Create a time parameter in milliseconds.
    #[must_use]
    pub fn time_ms(id: impl Into<String>, name: impl Into<String>, min: f32, max: f32) -> Self {
        let name_str = name.into();
        Self {
            id: id.into(),
            name: name_str.clone(),
            short_name: name_str.chars().take(4).collect(),
            value: 0.5,
            default: 0.5,
            unit: "ms".to_string(),
            format: ParameterFormat::Milliseconds { min, max },
            priority: 4,
        }
    }

    /// Set the default value.
    #[must_use]
    pub const fn with_default(mut self, default: f32) -> Self {
        self.default = default;
        self.value = default;
        self
    }

    /// Set the priority (lower = shown at lower LOD).
    #[must_use]
    pub const fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// Set the short name.
    #[must_use]
    pub fn with_short_name(mut self, short_name: impl Into<String>) -> Self {
        self.short_name = short_name.into();
        self
    }
}

/// Block category for organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockCategory {
    /// Input/gain staging
    Input,
    /// Dynamics (compressor, limiter, gate)
    Dynamics,
    /// EQ and filters
    Eq,
    /// Distortion/saturation
    Drive,
    /// Amp simulation
    Amp,
    /// Cabinet/speaker simulation
    Cabinet,
    /// Modulation effects (chorus, flanger, phaser)
    Modulation,
    /// Time-based effects (delay)
    Delay,
    /// Reverb
    Reverb,
    /// Utility (volume, pan, routing)
    Utility,
    /// Output/master
    Output,
}

impl BlockCategory {
    /// Get a display name for this category.
    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Input => "Input",
            Self::Dynamics => "Dynamics",
            Self::Eq => "EQ",
            Self::Drive => "Drive",
            Self::Amp => "Amp",
            Self::Cabinet => "Cabinet",
            Self::Modulation => "Modulation",
            Self::Delay => "Delay",
            Self::Reverb => "Reverb",
            Self::Utility => "Utility",
            Self::Output => "Output",
        }
    }

    /// Get a short icon/emoji for this category.
    #[must_use]
    pub const fn icon(&self) -> &'static str {
        match self {
            Self::Input => "IN",
            Self::Dynamics => "DYN",
            Self::Eq => "EQ",
            Self::Drive => "DRV",
            Self::Amp => "AMP",
            Self::Cabinet => "CAB",
            Self::Modulation => "MOD",
            Self::Delay => "DLY",
            Self::Reverb => "VRB",
            Self::Utility => "UTL",
            Self::Output => "OUT",
        }
    }
}

/// Definition of an audio effect block.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockDefinition {
    /// Unique type identifier.
    pub type_id: String,
    /// Display name.
    pub name: String,
    /// Category.
    pub category: BlockCategory,
    /// All parameters.
    pub parameters: Vec<BlockParameter>,
    /// Macro control for one-knob operation.
    pub macro_control: MacroControl,
    /// Supported form factors.
    pub supported_form_factors: Vec<FormFactor>,
    /// Minimum LOD this block supports.
    pub min_lod: LevelOfDetail,
}

impl BlockDefinition {
    /// Create a new block definition.
    #[must_use]
    pub fn new(
        type_id: impl Into<String>,
        name: impl Into<String>,
        category: BlockCategory,
    ) -> Self {
        let name_str = name.into();
        Self {
            type_id: type_id.into(),
            name: name_str.clone(),
            category,
            parameters: Vec::new(),
            macro_control: MacroControl::new("macro", &name_str),
            supported_form_factors: vec![
                FormFactor::Fullscreen,
                FormFactor::Square,
                FormFactor::Series500,
                FormFactor::Mini,
            ],
            min_lod: LevelOfDetail::Mini,
        }
    }

    /// Add a parameter.
    pub fn add_parameter(&mut self, param: BlockParameter) {
        self.parameters.push(param);
    }

    /// Builder: add a parameter.
    #[must_use]
    pub fn with_parameter(mut self, param: BlockParameter) -> Self {
        self.add_parameter(param);
        self
    }

    /// Add a macro mapping for a parameter.
    pub fn add_macro_mapping(&mut self, mapping: ParameterMapping) {
        self.macro_control.add_mapping(mapping);
    }

    /// Builder: add a macro mapping.
    #[must_use]
    pub fn with_macro_mapping(mut self, mapping: ParameterMapping) -> Self {
        self.add_macro_mapping(mapping);
        self
    }

    /// Get parameters that should be visible at a given LOD.
    #[must_use]
    pub fn parameters_for_lod(&self, lod: LevelOfDetail) -> Vec<&BlockParameter> {
        let max_priority = match lod {
            LevelOfDetail::Full => u8::MAX,
            LevelOfDetail::Standard => 5,
            LevelOfDetail::Compact => 2,
            LevelOfDetail::Mini => 0, // No individual params, only macro
        };

        self.parameters
            .iter()
            .filter(|p| p.priority <= max_priority)
            .collect()
    }
}

/// An instance of a block in a chain.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockInstance {
    /// Unique instance ID.
    pub id: String,
    /// Reference to the block definition type.
    pub type_id: String,
    /// Instance name (can be customized).
    pub name: String,
    /// Whether the block is bypassed.
    pub bypassed: bool,
    /// Whether the block is soloed.
    pub soloed: bool,
    /// Current parameter values (keyed by param ID).
    pub param_values: HashMap<String, f32>,
    /// Current macro value.
    pub macro_value: f32,
    /// Current form factor being displayed.
    pub form_factor: FormFactor,
    /// Current LOD.
    pub lod: LevelOfDetail,
}

impl BlockInstance {
    /// Create a new instance from a definition.
    #[must_use]
    pub fn from_definition(definition: &BlockDefinition, instance_id: impl Into<String>) -> Self {
        let param_values = definition
            .parameters
            .iter()
            .map(|p| (p.id.clone(), p.default))
            .collect();

        Self {
            id: instance_id.into(),
            type_id: definition.type_id.clone(),
            name: definition.name.clone(),
            bypassed: false,
            soloed: false,
            param_values,
            macro_value: definition.macro_control.default,
            form_factor: FormFactor::Square,
            lod: LevelOfDetail::Standard,
        }
    }

    /// Set a parameter value.
    pub fn set_param(&mut self, param_id: &str, value: f32) {
        self.param_values
            .insert(param_id.to_string(), value.clamp(0.0, 1.0));
    }

    /// Get a parameter value.
    #[must_use]
    pub fn get_param(&self, param_id: &str) -> f32 {
        self.param_values.get(param_id).copied().unwrap_or(0.5)
    }

    /// Apply macro control to update all mapped parameters.
    pub fn apply_macro(&mut self, definition: &BlockDefinition) {
        let mut macro_ctrl = definition.macro_control.clone();
        let values = macro_ctrl.set_value(self.macro_value);

        for (param_id, value) in values {
            self.param_values.insert(param_id, value);
        }
    }
}

/// Predefined block definitions for common effects.
pub mod definitions {
    use super::*;
    use crate::core::macro_curve::CurveType;

    /// Create a Drive/Overdrive block definition.
    #[must_use]
    pub fn drive() -> BlockDefinition {
        BlockDefinition::new("drive", "Drive", BlockCategory::Drive)
            .with_parameter(
                BlockParameter::gain("gain", "Gain", 0.0, 24.0)
                    .with_priority(1)
                    .with_default(0.3),
            )
            .with_parameter(
                BlockParameter::percent("tone", "Tone")
                    .with_priority(2)
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::percent("mix", "Mix")
                    .with_priority(3)
                    .with_default(1.0),
            )
            .with_parameter(
                BlockParameter::percent("output", "Output")
                    .with_priority(2)
                    .with_default(0.5),
            )
            .with_macro_mapping(
                ParameterMapping::new("gain", "Gain")
                    .with_param_range(0.0, 0.8)
                    .with_curve(CurveType::Exponential),
            )
            .with_macro_mapping(
                ParameterMapping::new("tone", "Tone")
                    .with_macro_range(0.0, 0.7)
                    .with_param_range(0.3, 0.7)
                    .with_curve(CurveType::SCurve),
            )
    }

    /// Create an Amp block definition.
    /// Parameters: Input, Volume, Bass, Middle, Treble, Output
    #[must_use]
    pub fn amp() -> BlockDefinition {
        BlockDefinition::new("amp", "Amp", BlockCategory::Amp)
            .with_parameter(
                BlockParameter::gain("input", "Input", -12.0, 12.0)
                    .with_priority(1)
                    .with_short_name("In")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::percent("volume", "Volume")
                    .with_priority(1)
                    .with_short_name("Vol")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::percent("bass", "Bass")
                    .with_priority(2)
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::percent("mid", "Middle")
                    .with_priority(2)
                    .with_short_name("Mid")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::percent("treble", "Treble")
                    .with_priority(2)
                    .with_short_name("Treb")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::gain("output", "Output", -12.0, 12.0)
                    .with_priority(1)
                    .with_short_name("Out")
                    .with_default(0.5),
            )
            .with_macro_mapping(
                ParameterMapping::new("volume", "Volume")
                    .with_param_range(0.2, 0.9)
                    .with_curve(CurveType::Logarithmic),
            )
    }

    /// Create a Cabinet block definition.
    #[must_use]
    pub fn cabinet() -> BlockDefinition {
        BlockDefinition::new("cabinet", "Cabinet", BlockCategory::Cabinet)
            .with_parameter(
                BlockParameter::percent("mic_position", "Mic Position")
                    .with_priority(2)
                    .with_short_name("Mic")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::percent("room", "Room")
                    .with_priority(3)
                    .with_default(0.3),
            )
            .with_parameter(
                BlockParameter::frequency("low_cut", "Low Cut", 20.0, 500.0)
                    .with_priority(3)
                    .with_short_name("LC")
                    .with_default(0.2),
            )
            .with_parameter(
                BlockParameter::frequency("high_cut", "High Cut", 2000.0, 20000.0)
                    .with_priority(3)
                    .with_short_name("HC")
                    .with_default(0.7),
            )
    }

    /// Create a Delay block definition.
    #[must_use]
    pub fn delay() -> BlockDefinition {
        BlockDefinition::new("delay", "Delay", BlockCategory::Delay)
            .with_parameter(
                BlockParameter::time_ms("time", "Time", 1.0, 2000.0)
                    .with_priority(1)
                    .with_default(0.3),
            )
            .with_parameter(
                BlockParameter::percent("feedback", "Feedback")
                    .with_priority(2)
                    .with_short_name("FB")
                    .with_default(0.3),
            )
            .with_parameter(
                BlockParameter::percent("mix", "Mix")
                    .with_priority(1)
                    .with_default(0.3),
            )
            .with_parameter(
                BlockParameter::frequency("tone", "Tone", 200.0, 8000.0)
                    .with_priority(3)
                    .with_default(0.6),
            )
            .with_parameter(
                BlockParameter::percent("mod_depth", "Mod Depth")
                    .with_priority(4)
                    .with_short_name("Mod")
                    .with_default(0.2),
            )
            .with_macro_mapping(
                ParameterMapping::new("mix", "Mix")
                    .with_param_range(0.0, 0.6)
                    .with_curve(CurveType::SCurve),
            )
            .with_macro_mapping(
                ParameterMapping::new("feedback", "Feedback")
                    .with_macro_range(0.3, 1.0)
                    .with_param_range(0.2, 0.7)
                    .with_curve(CurveType::Linear),
            )
    }

    /// Create a Reverb block definition.
    #[must_use]
    pub fn reverb() -> BlockDefinition {
        BlockDefinition::new("reverb", "Reverb", BlockCategory::Reverb)
            .with_parameter(
                BlockParameter::percent("mix", "Mix")
                    .with_priority(1)
                    .with_default(0.3),
            )
            .with_parameter(BlockParameter {
                id: "decay".to_string(),
                name: "Decay".to_string(),
                short_name: "Dec".to_string(),
                value: 0.4,
                default: 0.4,
                unit: "s".to_string(),
                format: ParameterFormat::Seconds {
                    min: 0.1,
                    max: 10.0,
                },
                priority: 1,
            })
            .with_parameter(
                BlockParameter::time_ms("predelay", "Pre-Delay", 0.0, 200.0)
                    .with_priority(3)
                    .with_short_name("Pre")
                    .with_default(0.1),
            )
            .with_parameter(
                BlockParameter::percent("damping", "Damping")
                    .with_priority(3)
                    .with_short_name("Damp")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::percent("size", "Size")
                    .with_priority(2)
                    .with_default(0.5),
            )
            .with_macro_mapping(
                ParameterMapping::new("mix", "Mix")
                    .with_param_range(0.0, 0.6)
                    .with_curve(CurveType::SCurve),
            )
            .with_macro_mapping(
                ParameterMapping::new("decay", "Decay")
                    .with_macro_range(0.2, 1.0)
                    .with_param_range(0.2, 0.8)
                    .with_curve(CurveType::Logarithmic),
            )
    }

    /// Create a Compressor block definition.
    #[must_use]
    pub fn compressor() -> BlockDefinition {
        BlockDefinition::new("compressor", "Compressor", BlockCategory::Dynamics)
            .with_parameter(
                BlockParameter::gain("threshold", "Threshold", -60.0, 0.0)
                    .with_priority(1)
                    .with_short_name("Thr")
                    .with_default(0.3),
            )
            .with_parameter(BlockParameter {
                id: "ratio".to_string(),
                name: "Ratio".to_string(),
                short_name: "Rat".to_string(),
                value: 0.16,
                default: 0.16,
                unit: ":1".to_string(),
                format: ParameterFormat::Float {
                    min: 1.0,
                    max: 20.0,
                    precision: 1,
                },
                priority: 1,
            })
            .with_parameter(
                BlockParameter::time_ms("attack", "Attack", 0.01, 100.0)
                    .with_priority(2)
                    .with_short_name("Att")
                    .with_default(0.1),
            )
            .with_parameter(
                BlockParameter::time_ms("release", "Release", 1.0, 1000.0)
                    .with_priority(2)
                    .with_short_name("Rel")
                    .with_default(0.3),
            )
            .with_parameter(
                BlockParameter::gain("makeup", "Makeup", -12.0, 24.0)
                    .with_priority(2)
                    .with_short_name("Make")
                    .with_default(0.33),
            )
            .with_parameter(
                BlockParameter::gain("knee", "Knee", 0.0, 24.0)
                    .with_priority(3)
                    .with_short_name("Knee")
                    .with_default(0.25),
            )
            .with_parameter(
                BlockParameter::percent("mix", "Mix")
                    .with_priority(3)
                    .with_default(1.0),
            )
            .with_macro_mapping(
                ParameterMapping::new("threshold", "Threshold")
                    .with_param_range(0.2, 0.8)
                    .with_curve(CurveType::Linear),
            )
    }

    /// Create a Gate block definition.
    #[must_use]
    pub fn gate() -> BlockDefinition {
        BlockDefinition::new("gate", "Gate", BlockCategory::Dynamics)
            .with_parameter(
                BlockParameter::gain("threshold", "Threshold", -60.0, 0.0)
                    .with_priority(1)
                    .with_short_name("Thr")
                    .with_default(0.5),
            )
            .with_parameter(BlockParameter {
                id: "ratio".to_string(),
                name: "Ratio".to_string(),
                short_name: "Rat".to_string(),
                value: 0.5,
                default: 0.5,
                unit: ":1".to_string(),
                format: ParameterFormat::Float {
                    min: 1.0,
                    max: 20.0,
                    precision: 1,
                },
                priority: 2,
            })
            .with_parameter(
                BlockParameter::gain("range", "Range", -80.0, 0.0)
                    .with_priority(1)
                    .with_short_name("Rng")
                    .with_default(0.25),
            )
            .with_parameter(
                BlockParameter::gain("knee", "Knee", 0.0, 24.0)
                    .with_priority(3)
                    .with_short_name("Knee")
                    .with_default(0.25),
            )
            .with_parameter(
                BlockParameter::time_ms("attack", "Attack", 0.01, 100.0)
                    .with_priority(2)
                    .with_short_name("Att")
                    .with_default(0.005),
            )
            .with_parameter(
                BlockParameter::time_ms("release", "Release", 1.0, 1000.0)
                    .with_priority(2)
                    .with_short_name("Rel")
                    .with_default(0.1),
            )
            .with_macro_mapping(
                ParameterMapping::new("threshold", "Threshold")
                    .with_param_range(0.2, 0.8)
                    .with_curve(CurveType::Linear),
            )
    }

    /// Create an EQ block definition.
    #[must_use]
    pub fn eq() -> BlockDefinition {
        BlockDefinition::new("eq", "EQ", BlockCategory::Eq)
            .with_parameter(
                BlockParameter::frequency("high_freq", "High Frequency", 2000.0, 20000.0)
                    .with_priority(1)
                    .with_short_name("HFreq")
                    .with_default(0.8),
            )
            .with_parameter(
                BlockParameter::gain("high_gain", "High Gain", -12.0, 12.0)
                    .with_priority(1)
                    .with_short_name("HGain")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::frequency("mid_freq", "Mid Frequency", 200.0, 4000.0)
                    .with_priority(2)
                    .with_short_name("MFreq")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::gain("mid_gain", "Mid Gain", -12.0, 12.0)
                    .with_priority(2)
                    .with_short_name("MGain")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::frequency("low_freq", "Low Frequency", 20.0, 500.0)
                    .with_priority(1)
                    .with_short_name("LFreq")
                    .with_default(0.2),
            )
            .with_parameter(
                BlockParameter::gain("low_gain", "Low Gain", -12.0, 12.0)
                    .with_priority(1)
                    .with_short_name("LGain")
                    .with_default(0.5),
            )
            .with_macro_mapping(
                ParameterMapping::new("low_gain", "Low Gain")
                    .with_param_range(0.2, 0.8)
                    .with_curve(CurveType::SCurve),
            )
    }

    /// Create a Chorus block definition.
    #[must_use]
    pub fn chorus() -> BlockDefinition {
        BlockDefinition::new("chorus", "Chorus", BlockCategory::Modulation)
            .with_parameter(
                BlockParameter::frequency("rate", "Rate", 0.01, 10.0)
                    .with_priority(1)
                    .with_short_name("Rate")
                    .with_default(0.1),
            )
            .with_parameter(
                BlockParameter::percent("depth", "Depth")
                    .with_priority(2)
                    .with_short_name("Depth")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::time_ms("delay", "Delay", 1.0, 40.0)
                    .with_priority(2)
                    .with_short_name("Dly")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::percent("mix", "Mix")
                    .with_priority(1)
                    .with_default(0.5),
            )
            .with_macro_mapping(
                ParameterMapping::new("mix", "Mix")
                    .with_param_range(0.0, 0.8)
                    .with_curve(CurveType::SCurve),
            )
    }

    /// Create a Flanger block definition.
    #[must_use]
    pub fn flanger() -> BlockDefinition {
        BlockDefinition::new("flanger", "Flanger", BlockCategory::Modulation)
            .with_parameter(
                BlockParameter::frequency("rate", "Rate", 0.01, 10.0)
                    .with_priority(1)
                    .with_short_name("Rate")
                    .with_default(0.05),
            )
            .with_parameter(
                BlockParameter::percent("depth", "Depth")
                    .with_priority(2)
                    .with_short_name("Depth")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::time_ms("delay", "Delay", 0.1, 10.0)
                    .with_priority(2)
                    .with_short_name("Dly")
                    .with_default(0.2),
            )
            .with_parameter(BlockParameter {
                id: "feedback".to_string(),
                name: "Feedback".to_string(),
                short_name: "FB".to_string(),
                value: 0.5,
                default: 0.5,
                unit: "".to_string(),
                format: ParameterFormat::Float {
                    min: -1.0,
                    max: 1.0,
                    precision: 2,
                },
                priority: 3,
            })
            .with_parameter(
                BlockParameter::percent("mix", "Mix")
                    .with_priority(1)
                    .with_default(0.5),
            )
            .with_macro_mapping(
                ParameterMapping::new("mix", "Mix")
                    .with_param_range(0.0, 0.8)
                    .with_curve(CurveType::SCurve),
            )
    }

    /// Create a Saturator block definition.
    #[must_use]
    pub fn saturator() -> BlockDefinition {
        BlockDefinition::new("saturator", "Saturator", BlockCategory::Drive)
            .with_parameter(
                BlockParameter::gain("drive", "Drive", 0.0, 48.0)
                    .with_priority(1)
                    .with_short_name("Drv")
                    .with_default(0.25),
            )
            .with_parameter(
                BlockParameter::percent("mix", "Mix")
                    .with_priority(1)
                    .with_default(1.0),
            )
            .with_parameter(
                BlockParameter::gain("output", "Output", -24.0, 6.0)
                    .with_priority(2)
                    .with_short_name("Out")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::percent("tone", "Tone")
                    .with_priority(3)
                    .with_short_name("Tone")
                    .with_default(0.5),
            )
            .with_macro_mapping(
                ParameterMapping::new("drive", "Drive")
                    .with_param_range(0.0, 0.8)
                    .with_curve(CurveType::Exponential),
            )
    }

    /// Create a Deesser block definition.
    #[must_use]
    pub fn deesser() -> BlockDefinition {
        BlockDefinition::new("deesser", "Deesser", BlockCategory::Eq)
            .with_parameter(
                BlockParameter::frequency("frequency", "Frequency", 2000.0, 16000.0)
                    .with_priority(1)
                    .with_short_name("Freq")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::gain("threshold", "Threshold", -60.0, 0.0)
                    .with_priority(1)
                    .with_short_name("Thr")
                    .with_default(0.4),
            )
            .with_parameter(
                BlockParameter::gain("range", "Range", 0.0, 24.0)
                    .with_priority(2)
                    .with_short_name("Rng")
                    .with_default(0.5),
            )
            .with_parameter(BlockParameter {
                id: "ratio".to_string(),
                name: "Ratio".to_string(),
                short_name: "Rat".to_string(),
                value: 0.4,
                default: 0.4,
                unit: ":1".to_string(),
                format: ParameterFormat::Float {
                    min: 1.0,
                    max: 10.0,
                    precision: 1,
                },
                priority: 2,
            })
            .with_macro_mapping(
                ParameterMapping::new("threshold", "Threshold")
                    .with_param_range(0.2, 0.8)
                    .with_curve(CurveType::Linear),
            )
    }

    /// Create a Tuner block definition.
    #[must_use]
    pub fn tuner() -> BlockDefinition {
        BlockDefinition::new("tuner", "Tuner", BlockCategory::Utility)
            .with_parameter(
                BlockParameter::percent("speed", "Speed")
                    .with_priority(1)
                    .with_short_name("Spd")
                    .with_default(0.5),
            )
            .with_parameter(
                BlockParameter::percent("humanize", "Humanize")
                    .with_priority(2)
                    .with_short_name("Hum")
                    .with_default(0.0),
            )
            .with_macro_mapping(
                ParameterMapping::new("speed", "Speed")
                    .with_param_range(0.3, 0.9)
                    .with_curve(CurveType::Logarithmic),
            )
    }
}
