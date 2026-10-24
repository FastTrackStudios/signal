//! Pure waveform evaluation for LFO modulation sources.
//!
//! All waveforms take a phase in `[0, 1)` and return a bipolar value in `[-1, 1]`.
//! The function is stateless — Sample & Hold and Step Sequence receive their held
//! value as a parameter so the caller can manage randomness and stepping.

use crate::sources::lfo::LfoWaveform;
use std::f64::consts::TAU;

/// Evaluate an LFO waveform at the given phase.
///
/// # Arguments
/// - `waveform` — which shape to evaluate
/// - `phase` — position within the cycle, `[0.0, 1.0)`
/// - `pulse_width` — duty cycle for `Square` (0.0–1.0, 0.5 = symmetric)
/// - `held_value` — current held output for `SampleAndHold` / `StepSequence` (bipolar)
///
/// # Returns
/// Bipolar value in `[-1.0, 1.0]`.
#[must_use]
pub fn evaluate_waveform(
    waveform: LfoWaveform,
    phase: f64,
    pulse_width: f64,
    held_value: f64,
) -> f64 {
    match waveform {
        LfoWaveform::Sine => (phase * TAU).sin(),

        LfoWaveform::Triangle => {
            // Rising 0→1 in first half, falling 1→-1 in second half
            if phase < 0.5 {
                4.0f64.mul_add(phase, -1.0)
            } else {
                4.0f64.mul_add(-phase, 3.0)
            }
        }

        LfoWaveform::Square => {
            let pw = pulse_width.clamp(0.01, 0.99);
            if phase < pw { 1.0 } else { -1.0 }
        }

        LfoWaveform::Sawtooth => 2.0f64.mul_add(phase, -1.0),

        LfoWaveform::InverseSawtooth => 2.0f64.mul_add(-phase, 1.0),

        LfoWaveform::SampleAndHold | LfoWaveform::StepSequence => held_value,
    }
}
