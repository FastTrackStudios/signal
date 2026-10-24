//! Electrostatic pickup model — time-varying RC circuit.
//!
//! The Wurlitzer 200A pickup is a capacitive sensor: reed vibration modulates
//! the capacitance between the reed and a charged metal plate (+147V DC).
//!
//! The reed-plate capacitance varies nonlinearly with displacement:
//!   C(y) = C_0 / (1 - y)
//! where y = x/d_0 is the normalized displacement (fraction of rest gap,
//! positive toward the plate).
//!
//! Unlike the old model which applied y/(1-y) then a separate HPF, this model
//! discretizes the actual RC circuit with bilinear transform, coupling the
//! nonlinearity and filtering into a single physical system:
//!
//!   R_total * C(y) * dV/dt + V = V_hv
//!
//! Normalized charge q = Q/(C_0 * V_eq), equilibrium at q=1. The bilinear
//! discretization with time-varying capacitance c_n = 1/(1-y):
//!
//!   alpha = beta / c_n = beta * (1 - y)
//!   q_next = (q * (1 - alpha) + 2*beta) / (1 + alpha)
//!   output = (1 - q_next/c_n) * SENSITIVITY
//!
//! This produces:
//! - Identical small-signal HPF at f_c = 1/(2π*R*C_0) = 2312 Hz
//! - Coupled nonlinear harmonic generation (H2 from capacitance modulation)
//! - Frequency-dependent nonlinearity (stronger near/below RC corner)
//! - Correct asymmetry (positive y amplified more than negative)

/// RC time constant: R_total * C_0
/// R_total = R_feed (1M) || (R-1 + R-2||R-3) = 1M || 402K = 287K
/// C_0 = 240 pF (rest capacitance)
const TAU: f64 = 287.0e3 * 240.0e-12; // 68.88 µs → f_c = 2312 Hz

/// Pickup sensitivity: V_hv * C_0 / (C_0 + C_p) = 147 * 3/240 = 1.8375 V
/// Applied to the AC voltage perturbation from charge dynamics.
pub const PICKUP_SENSITIVITY: f64 = 1.8375;

/// Asymptotic displacement-fraction limit. The reed physically cannot touch
/// the plate (y=1.0 is a singularity in c_n=1/(1-y)). With the smooth-saturation
/// curve below, |y_out| approaches but never reaches this value.
///
/// The old static model needed a tight clamp (0.90) because y/(1-y) at 0.90 = 9.0
/// produced huge intermediate signals. The time-varying RC model self-limits via
/// charge dynamics — output is bounded at ~±SENSITIVITY regardless of y, so we
/// can safely allow y close to 1.0. At y=0.98, c_n=50, alpha=0.008 — numerically
/// well-behaved.
pub const PICKUP_MAX_Y: f64 = 0.98;

/// Knee where the smooth saturation begins. Below this, `pickup_soft_saturate`
/// is the identity function (no compression, no distortion). Above this, it
/// smoothly bends toward `PICKUP_MAX_Y` so the upper-envelope tip never crosses
/// a hard corner. Picked to leave the entire normal operating range untouched
/// (typical y_peak at v=127 is ~0.85 with DS at NEW values 0.85/0.88).
pub const PICKUP_KNEE_Y: f64 = 0.85;

/// Smooth saturation on the reed-displacement fraction `y = x / d_0`.
///
/// Below `±PICKUP_KNEE_Y` the function is the identity — no flavour change vs.
/// the old hard clamp on quiet-to-medium content. Above the knee it follows
/// `knee + (limit-knee) * tanh((|y|-knee)/(limit-knee))`, which is C¹-continuous
/// at the knee (slope = 1) and asymptotes to `±PICKUP_MAX_Y` from below. This
/// removes the derivative discontinuity at the old `clamp(±0.98)` corner that
/// was producing broadband HF "tear" hash whenever bass-heavy / chord-ff
/// content grazed the limit (~6–7× more click-band energy at NEW DS values).
///
/// Bark character is preserved: the upper-velocity range still spends most of
/// its time in the steep `1/(1-y)` zone (y in [0.85, 0.95]), where the
/// saturation barely deviates from identity. The change is concentrated at the
/// very top of the velocity range where the corner used to live.
#[inline]
fn pickup_soft_saturate(y: f64) -> f64 {
    let abs_y = y.abs();
    if abs_y < PICKUP_KNEE_Y {
        return y;
    }
    let range = PICKUP_MAX_Y - PICKUP_KNEE_Y;
    let saturated = PICKUP_KNEE_Y + range * ((abs_y - PICKUP_KNEE_Y) / range).tanh();
    saturated.copysign(y)
}

/// Convert reed model displacement units to physical y = x/d_0.
///
/// NOTE: This default is overridden per-note by tables::pickup_displacement_scale()
/// in voice.rs. Only used if set_displacement_scale() is never called.
const DISPLACEMENT_SCALE: f64 = 0.85;

pub struct Pickup {
    /// Normalized charge state (equilibrium = 1.0).
    q: f64,
    /// Precomputed: dt / (2 * TAU). Bilinear integration coefficient.
    beta: f64,
    displacement_scale: f64,
}

impl Pickup {
    pub fn new(sample_rate: f64) -> Self {
        Self::new_with_scale(sample_rate, DISPLACEMENT_SCALE)
    }

    /// Construct with explicit displacement scale (for bark-audit/calibrate tools).
    pub fn new_with_scale(sample_rate: f64, displacement_scale: f64) -> Self {
        let dt = 1.0 / sample_rate;
        let beta = dt / (2.0 * TAU);
        Self {
            q: 1.0,
            beta,
            displacement_scale,
        }
    }

    /// Override the displacement scale (default: 0.85).
    /// Higher = tighter gap = more nonlinearity = more bark.
    pub fn set_displacement_scale(&mut self, scale: f64) {
        self.displacement_scale = scale;
    }

    /// Process a buffer of reed displacement samples in-place.
    ///
    /// Input: reed displacement in normalized model units.
    /// Output: pickup voltage in volts (millivolt-scale signals).
    ///
    /// The time-varying RC circuit couples the 1/(1-y) capacitance nonlinearity
    /// with the charge dynamics, producing frequency-dependent harmonic generation.
    /// At frequencies well below the RC corner (2312 Hz), the circuit generates
    /// H2 proportional to displacement² (same as the static y/(1-y) model).
    /// At frequencies near/above the corner, the charge can't follow the fast
    /// capacitance changes, reducing the nonlinear contribution — physically
    /// correct behavior that the old separated model couldn't capture.
    pub fn process(&mut self, buffer: &mut [f64]) {
        let scale = self.displacement_scale;
        let beta = self.beta;
        for sample in buffer.iter_mut() {
            // Smooth saturation: identity below ±PICKUP_KNEE_Y, asymptotic to
            // ±PICKUP_MAX_Y above. Replaces the old hard clamp whose derivative
            // discontinuity at the limit was producing audible HF distortion.
            let y = pickup_soft_saturate(*sample * scale);
            // Eliminate c_n = 1/(1-y) division: use (1-y) directly.
            // alpha = beta / c_n = beta * (1-y)
            let one_minus_y = 1.0 - y;
            let alpha = beta * one_minus_y;
            // Bilinear (trapezoidal) integration of: TAU * dq/dt = 1 - q/c_n
            // Driving term is 2*beta (from the constant V_hv source), NOT 2*alpha
            let q_next = (self.q * (1.0 - alpha) + 2.0 * beta) / (1.0 + alpha);
            self.q = q_next;
            // Output: (q/c_n - 1) = (q*(1-y) - 1) — no division needed
            *sample = (q_next * one_minus_y - 1.0) * PICKUP_SENSITIVITY;
        }
    }

    pub fn reset(&mut self) {
        self.q = 1.0;
    }
}
