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
                4.0 * phase - 1.0
            } else {
                3.0 - 4.0 * phase
            }
        }

        LfoWaveform::Square => {
            let pw = pulse_width.clamp(0.01, 0.99);
            if phase < pw {
                1.0
            } else {
                -1.0
            }
        }

        LfoWaveform::Sawtooth => 2.0 * phase - 1.0,

        LfoWaveform::InverseSawtooth => 1.0 - 2.0 * phase,

        LfoWaveform::SampleAndHold => held_value,

        LfoWaveform::StepSequence => held_value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PW: f64 = 0.5; // default pulse width
    const HELD: f64 = 0.0; // unused for most waveforms

    #[test]
    fn sine_at_cardinal_phases() {
        let eps = 1e-10;
        assert!((evaluate_waveform(LfoWaveform::Sine, 0.0, PW, HELD) - 0.0).abs() < eps);
        assert!((evaluate_waveform(LfoWaveform::Sine, 0.25, PW, HELD) - 1.0).abs() < eps);
        assert!((evaluate_waveform(LfoWaveform::Sine, 0.5, PW, HELD) - 0.0).abs() < eps);
        assert!((evaluate_waveform(LfoWaveform::Sine, 0.75, PW, HELD) - (-1.0)).abs() < eps);
    }

    #[test]
    fn triangle_at_cardinal_phases() {
        let eps = 1e-10;
        // phase=0 → 4*0-1 = -1
        assert!((evaluate_waveform(LfoWaveform::Triangle, 0.0, PW, HELD) - (-1.0)).abs() < eps);
        // phase=0.25 → 4*0.25-1 = 0
        assert!((evaluate_waveform(LfoWaveform::Triangle, 0.25, PW, HELD) - 0.0).abs() < eps);
        // phase=0.5 → 3-4*0.5 = 1
        assert!((evaluate_waveform(LfoWaveform::Triangle, 0.5, PW, HELD) - 1.0).abs() < eps);
        // phase=0.75 → 3-4*0.75 = 0
        assert!((evaluate_waveform(LfoWaveform::Triangle, 0.75, PW, HELD) - 0.0).abs() < eps);
    }

    #[test]
    fn square_symmetric() {
        assert_eq!(evaluate_waveform(LfoWaveform::Square, 0.0, 0.5, HELD), 1.0);
        assert_eq!(
            evaluate_waveform(LfoWaveform::Square, 0.25, 0.5, HELD),
            1.0
        );
        assert_eq!(
            evaluate_waveform(LfoWaveform::Square, 0.5, 0.5, HELD),
            -1.0
        );
        assert_eq!(
            evaluate_waveform(LfoWaveform::Square, 0.75, 0.5, HELD),
            -1.0
        );
    }

    #[test]
    fn square_asymmetric_pulse_width() {
        // 75% duty cycle: high for first 75%, low for last 25%
        assert_eq!(evaluate_waveform(LfoWaveform::Square, 0.5, 0.75, HELD), 1.0);
        assert_eq!(
            evaluate_waveform(LfoWaveform::Square, 0.8, 0.75, HELD),
            -1.0
        );
    }

    #[test]
    fn sawtooth_ramp() {
        let eps = 1e-10;
        assert!((evaluate_waveform(LfoWaveform::Sawtooth, 0.0, PW, HELD) - (-1.0)).abs() < eps);
        assert!((evaluate_waveform(LfoWaveform::Sawtooth, 0.5, PW, HELD) - 0.0).abs() < eps);
        // Just before 1.0
        assert!(evaluate_waveform(LfoWaveform::Sawtooth, 0.999, PW, HELD) > 0.99);
    }

    #[test]
    fn inverse_sawtooth_ramp() {
        let eps = 1e-10;
        assert!(
            (evaluate_waveform(LfoWaveform::InverseSawtooth, 0.0, PW, HELD) - 1.0).abs() < eps
        );
        assert!(
            (evaluate_waveform(LfoWaveform::InverseSawtooth, 0.5, PW, HELD) - 0.0).abs() < eps
        );
        assert!(evaluate_waveform(LfoWaveform::InverseSawtooth, 0.999, PW, HELD) < -0.99);
    }

    #[test]
    fn sample_and_hold_returns_held_value() {
        assert_eq!(
            evaluate_waveform(LfoWaveform::SampleAndHold, 0.3, PW, 0.42),
            0.42
        );
        assert_eq!(
            evaluate_waveform(LfoWaveform::SampleAndHold, 0.7, PW, -0.8),
            -0.8
        );
    }

    #[test]
    fn step_sequence_returns_held_value() {
        assert_eq!(
            evaluate_waveform(LfoWaveform::StepSequence, 0.1, PW, 0.55),
            0.55
        );
    }

    #[test]
    fn all_waveforms_within_bipolar_range() {
        let phases = [0.0, 0.1, 0.25, 0.33, 0.5, 0.66, 0.75, 0.9, 0.99];
        for waveform in LfoWaveform::ALL {
            for &phase in &phases {
                let val = evaluate_waveform(*waveform, phase, PW, 0.5);
                assert!(
                    (-1.0..=1.0).contains(&val),
                    "{:?} at phase {phase} = {val}",
                    waveform
                );
            }
        }
    }
}
