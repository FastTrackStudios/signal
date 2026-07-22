//! Energy-to-velocity mapping with configurable curves.
//!
//! Converts the raw peak level from the detector into a velocity value
//! (0.0 to 1.0) using a dynamics parameter and curve shaping.
//!
//! Based on LSP Trigger's velocity computation:
//! `velocity = 0.5 * (level / threshold)^dynamics`
//! followed by dynamic range windowing and curve shaping.

// r[impl trigger.velocity.energy]
// r[impl trigger.velocity.curve]

/// Velocity curve shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VelocityCurve {
    /// Linear mapping.
    Linear,
    /// Logarithmic (more sensitive to soft hits).
    Logarithmic,
    /// Exponential (more sensitive to hard hits).
    Exponential,
    /// Fixed velocity (ignores input level).
    Fixed,
}

/// Velocity mapper — converts detected peak level to velocity (0.0-1.0).
pub struct VelocityMapper {
    /// Dynamics amount (0.0 = fixed velocity, 1.0 = full dynamic range).
    pub dynamics: f64,
    /// Fixed velocity output when dynamics = 0 or curve = Fixed (0.0-1.0).
    pub fixed_velocity: f64,
    /// Velocity curve shape.
    pub curve: VelocityCurve,
    /// Minimum output velocity (0.0-1.0).
    pub min_velocity: f64,
    /// Maximum output velocity (0.0-1.0).
    pub max_velocity: f64,
}

impl VelocityMapper {
    pub fn new() -> Self {
        Self {
            dynamics: 0.5,
            fixed_velocity: 0.5,
            curve: VelocityCurve::Linear,
            min_velocity: 0.0,
            max_velocity: 1.0,
        }
    }

    /// Map a detected peak level to a velocity value.
    ///
    /// # Parameters
    /// - `peak_level`: peak amplitude from the detector (linear, > 0)
    /// - `threshold`: the detect threshold (linear, > 0)
    ///
    /// # Returns
    /// Velocity in range [min_velocity, max_velocity].
    pub fn map(&self, peak_level: f64, threshold: f64) -> f64 {
        if matches!(self.curve, VelocityCurve::Fixed) || self.dynamics <= 0.0 {
            return self
                .fixed_velocity
                .clamp(self.min_velocity, self.max_velocity);
        }

        if peak_level <= 0.0 || threshold <= 0.0 {
            return self.min_velocity;
        }

        // LSP-style: velocity = 0.5 * (level / threshold)^dynamics
        let ratio = peak_level / threshold;
        let raw = 0.5 * ratio.powf(self.dynamics);

        // Apply curve shaping
        let shaped = match self.curve {
            VelocityCurve::Linear => raw,
            VelocityCurve::Logarithmic => {
                // Log curve: more sensitivity at low levels
                if raw <= 0.0 {
                    0.0
                } else {
                    (1.0 + raw.ln().max(-5.0) / 5.0).clamp(0.0, 1.0) * raw.min(1.0).max(raw)
                    // Simpler: use log scaling within 0-1
                }
            }
            VelocityCurve::Exponential => {
                // Exponential: more sensitivity at high levels
                raw * raw
            }
            VelocityCurve::Fixed => unreachable!(),
        };

        // For logarithmic, use a cleaner formulation
        let shaped = match self.curve {
            VelocityCurve::Logarithmic => {
                // Map 0..~2 raw range to 0..1 with log curve
                // log(1 + x) / log(1 + max) normalized
                let x = raw.clamp(0.0, 2.0);
                (1.0 + x).ln() / (3.0_f64).ln()
            }
            _ => shaped,
        };

        shaped.clamp(self.min_velocity, self.max_velocity)
    }

    /// Convert velocity (0.0-1.0) to MIDI velocity (1-127).
    pub fn to_midi(velocity: f64) -> u8 {
        (1.0 + velocity.clamp(0.0, 1.0) * 126.0).round() as u8
    }

    /// Convert MIDI velocity (1-127) to velocity (0.0-1.0).
    pub fn from_midi(midi_vel: u8) -> f64 {
        ((midi_vel as f64) - 1.0) / 126.0
    }
}

impl Default for VelocityMapper {
    fn default() -> Self {
        Self::new()
    }
}
