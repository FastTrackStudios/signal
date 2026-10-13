//! Macro curve system for one-knob parameter control.
//!
//! Allows a single "macro" control to modulate multiple parameters,
//! each with its own range and response curve.

use std::collections::HashMap;

/// Response curve types for parameter mapping.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CurveType {
    /// Linear response (y = x)
    #[default]
    Linear,
    /// Logarithmic response (slow start, fast end)
    /// Good for: frequency, gain
    Logarithmic,
    /// Exponential response (fast start, slow end)
    /// Good for: attack times, some EQ
    Exponential,
    /// S-Curve (slow start and end, fast middle)
    /// Good for: crossfades, smooth transitions
    SCurve,
    /// Inverse S-Curve (fast start and end, slow middle)
    InverseSCurve,
    /// Step function (quantized levels)
    Stepped { steps: u8 },
    /// Custom bezier curve defined by control points
    Bezier {
        cx1: f32,
        cy1: f32,
        cx2: f32,
        cy2: f32,
    },
}

impl CurveType {
    /// Apply the curve to a normalized input (0.0 - 1.0).
    #[must_use]
    pub fn apply(&self, input: f32) -> f32 {
        let x = input.clamp(0.0, 1.0);

        match self {
            Self::Linear => x,
            Self::Logarithmic => {
                // log2(x + 1) scaled to 0-1
                (x + 1.0).log2()
            }
            Self::Exponential => {
                // 2^x - 1 scaled to 0-1
                2.0_f32.powf(x) - 1.0
            }
            Self::SCurve => {
                // Smoothstep: 3x² - 2x³
                x * x * (3.0 - 2.0 * x)
            }
            Self::InverseSCurve => {
                // Attempt inverse of smoothstep approximation
                if x < 0.5 {
                    (2.0 * x).sqrt() * 0.5
                } else {
                    1.0 - (2.0 * (1.0 - x)).sqrt() * 0.5
                }
            }
            Self::Stepped { steps } => {
                let steps = *steps as f32;
                (x * steps).floor() / (steps - 1.0).max(1.0)
            }
            Self::Bezier { cx1, cy1, cx2, cy2 } => {
                // Cubic bezier approximation
                // P0 = (0, 0), P1 = (cx1, cy1), P2 = (cx2, cy2), P3 = (1, 1)
                cubic_bezier_y(x, *cx1, *cy1, *cx2, *cy2)
            }
        }
    }

    /// Get the inverse of this curve (for display purposes).
    #[must_use]
    pub fn inverse(&self, output: f32) -> f32 {
        let y = output.clamp(0.0, 1.0);

        match self {
            Self::Linear => y,
            Self::Logarithmic => 2.0_f32.powf(y) - 1.0,
            Self::Exponential => (y + 1.0).log2(),
            Self::SCurve => {
                // Newton-Raphson approximation for inverse smoothstep
                inverse_smoothstep(y)
            }
            _ => y, // Fallback to linear for complex curves
        }
    }
}

/// Approximate cubic bezier y value for given x.
fn cubic_bezier_y(x: f32, cx1: f32, cy1: f32, cx2: f32, cy2: f32) -> f32 {
    // Use Newton-Raphson to find t for given x, then calculate y
    let mut t = x; // Initial guess

    for _ in 0..8 {
        let t2 = t * t;
        let t3 = t2 * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let _mt3 = mt2 * mt;

        // Bezier x(t) = 3*mt²*t*cx1 + 3*mt*t²*cx2 + t³
        let bx = 3.0 * mt2 * t * cx1 + 3.0 * mt * t2 * cx2 + t3;
        let dx = x - bx;

        if dx.abs() < 0.0001 {
            break;
        }

        // Derivative of x(t)
        let dbx = 3.0 * mt2 * cx1 + 6.0 * mt * t * (cx2 - cx1) + 3.0 * t2 * (1.0 - cx2);

        if dbx.abs() > 0.0001 {
            t += dx / dbx;
            t = t.clamp(0.0, 1.0);
        }
    }

    // Calculate y at found t
    let t2 = t * t;
    let t3 = t2 * t;
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let _mt3 = mt2 * mt;

    3.0 * mt2 * t * cy1 + 3.0 * mt * t2 * cy2 + t3
}

/// Newton-Raphson approximation for inverse smoothstep.
fn inverse_smoothstep(y: f32) -> f32 {
    let mut x = y;
    for _ in 0..8 {
        let fx = x * x * (3.0 - 2.0 * x) - y;
        let dfx = 6.0 * x * (1.0 - x);
        if dfx.abs() > 0.0001 {
            x -= fx / dfx;
            x = x.clamp(0.0, 1.0);
        }
    }
    x
}

/// Mapping from a macro control to a target parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterMapping {
    /// Unique identifier for the target parameter.
    pub param_id: String,
    /// Human-readable name for display.
    pub name: String,
    /// Where on the macro curve this param starts responding (0.0 - 1.0).
    pub macro_min: f32,
    /// Where on the macro curve this param stops responding (0.0 - 1.0).
    pub macro_max: f32,
    /// The parameter's output value when macro is at `macro_min`.
    pub param_min: f32,
    /// The parameter's output value when macro is at `macro_max`.
    pub param_max: f32,
    /// The response curve for this mapping.
    pub curve: CurveType,
    /// Whether the mapping is currently active.
    pub enabled: bool,
}

impl ParameterMapping {
    /// Create a new linear mapping with full range.
    #[must_use]
    pub fn new(param_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            param_id: param_id.into(),
            name: name.into(),
            macro_min: 0.0,
            macro_max: 1.0,
            param_min: 0.0,
            param_max: 1.0,
            curve: CurveType::Linear,
            enabled: true,
        }
    }

    /// Set the macro range (where on the macro curve this param responds).
    #[must_use]
    pub const fn with_macro_range(mut self, min: f32, max: f32) -> Self {
        self.macro_min = min;
        self.macro_max = max;
        self
    }

    /// Set the parameter output range.
    #[must_use]
    pub const fn with_param_range(mut self, min: f32, max: f32) -> Self {
        self.param_min = min;
        self.param_max = max;
        self
    }

    /// Set the response curve.
    #[must_use]
    pub const fn with_curve(mut self, curve: CurveType) -> Self {
        self.curve = curve;
        self
    }

    /// Calculate the output value for this parameter given a macro position.
    #[must_use]
    pub fn calculate(&self, macro_value: f32) -> Option<f32> {
        if !self.enabled {
            return None;
        }

        let macro_value = macro_value.clamp(0.0, 1.0);

        // Check if macro is within our response range
        if macro_value < self.macro_min || macro_value > self.macro_max {
            // Return clamped edge value
            return Some(if macro_value < self.macro_min {
                self.param_min
            } else {
                self.param_max
            });
        }

        // Normalize macro position within our range
        let range = self.macro_max - self.macro_min;
        let normalized = if range > 0.0 {
            (macro_value - self.macro_min) / range
        } else {
            0.5
        };

        // Apply the curve
        let curved = self.curve.apply(normalized);

        // Map to parameter range
        let output = self.param_min + curved * (self.param_max - self.param_min);
        Some(output)
    }
}

/// A macro control that drives multiple parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroControl {
    /// Unique identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Current macro value (0.0 - 1.0).
    pub value: f32,
    /// Default value for reset.
    pub default: f32,
    /// Parameter mappings.
    pub mappings: Vec<ParameterMapping>,
}

impl MacroControl {
    /// Create a new macro control.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            value: 0.5,
            default: 0.5,
            mappings: Vec::new(),
        }
    }

    /// Add a parameter mapping.
    pub fn add_mapping(&mut self, mapping: ParameterMapping) {
        self.mappings.push(mapping);
    }

    /// Builder: add a mapping.
    #[must_use]
    pub fn with_mapping(mut self, mapping: ParameterMapping) -> Self {
        self.add_mapping(mapping);
        self
    }

    /// Set the macro value and return all calculated parameter values.
    pub fn set_value(&mut self, value: f32) -> HashMap<String, f32> {
        self.value = value.clamp(0.0, 1.0);
        self.calculate_all()
    }

    /// Calculate all parameter values for the current macro position.
    #[must_use]
    pub fn calculate_all(&self) -> HashMap<String, f32> {
        self.mappings
            .iter()
            .filter_map(|m| m.calculate(self.value).map(|v| (m.param_id.clone(), v)))
            .collect()
    }

    /// Get a specific parameter value.
    #[must_use]
    pub fn get_param_value(&self, param_id: &str) -> Option<f32> {
        self.mappings
            .iter()
            .find(|m| m.param_id == param_id)
            .and_then(|m| m.calculate(self.value))
    }
}

/// Preset macro configurations for common effect types.
pub mod presets {
    use super::*;

    /// Create a Drive macro that controls gain, tone, and saturation.
    #[must_use]
    pub fn drive_macro() -> MacroControl {
        MacroControl::new("drive", "Drive")
            .with_mapping(
                ParameterMapping::new("gain", "Gain")
                    .with_param_range(0.0, 0.8)
                    .with_curve(CurveType::Exponential),
            )
            .with_mapping(
                ParameterMapping::new("tone", "Tone")
                    .with_macro_range(0.0, 0.7) // Only responds to first 70% of macro
                    .with_param_range(0.3, 0.7)
                    .with_curve(CurveType::SCurve),
            )
            .with_mapping(
                ParameterMapping::new("saturation", "Saturation")
                    .with_macro_range(0.3, 1.0) // Kicks in after 30%
                    .with_param_range(0.0, 1.0)
                    .with_curve(CurveType::Logarithmic),
            )
    }

    /// Create a Mix macro for wet/dry effects.
    #[must_use]
    pub fn mix_macro() -> MacroControl {
        MacroControl::new("mix", "Mix")
            .with_mapping(ParameterMapping::new("wet", "Wet").with_param_range(0.0, 1.0))
            .with_mapping(
                ParameterMapping::new("dry", "Dry").with_param_range(1.0, 0.0), // Inverse
            )
    }

    /// Create a Space macro for reverb/delay.
    #[must_use]
    pub fn space_macro() -> MacroControl {
        MacroControl::new("space", "Space")
            .with_mapping(
                ParameterMapping::new("mix", "Mix")
                    .with_param_range(0.0, 0.6)
                    .with_curve(CurveType::SCurve),
            )
            .with_mapping(
                ParameterMapping::new("decay", "Decay")
                    .with_macro_range(0.2, 1.0)
                    .with_param_range(0.5, 4.0) // 0.5s to 4s
                    .with_curve(CurveType::Logarithmic),
            )
            .with_mapping(
                ParameterMapping::new("predelay", "Pre-Delay")
                    .with_macro_range(0.0, 0.5)
                    .with_param_range(0.0, 100.0) // 0 to 100ms
                    .with_curve(CurveType::Linear),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_linear() {
        let curve = CurveType::Linear;
        assert!((curve.apply(0.0) - 0.0).abs() < 0.001);
        assert!((curve.apply(0.5) - 0.5).abs() < 0.001);
        assert!((curve.apply(1.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn curve_scurve_midpoint() {
        let curve = CurveType::SCurve;
        // S-curve should pass through midpoint
        assert!((curve.apply(0.5) - 0.5).abs() < 0.001);
        // Should be below linear at 0.25
        assert!(curve.apply(0.25) < 0.25);
        // Should be above linear at 0.75
        assert!(curve.apply(0.75) > 0.75);
    }

    #[test]
    fn mapping_range() {
        let mapping = ParameterMapping::new("test", "Test")
            .with_macro_range(0.2, 0.8)
            .with_param_range(100.0, 200.0);

        // Below range should clamp to min
        assert!((mapping.calculate(0.0).unwrap() - 100.0).abs() < 0.001);
        // Above range should clamp to max
        assert!((mapping.calculate(1.0).unwrap() - 200.0).abs() < 0.001);
        // Middle of range
        assert!((mapping.calculate(0.5).unwrap() - 150.0).abs() < 0.001);
    }

    #[test]
    fn macro_control() {
        let macro_ctrl = presets::drive_macro();
        let values = macro_ctrl.calculate_all();

        assert!(values.contains_key("gain"));
        assert!(values.contains_key("tone"));
        assert!(values.contains_key("saturation"));
    }
}
