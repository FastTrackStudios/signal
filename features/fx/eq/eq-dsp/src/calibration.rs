//! Frequency-response calibration helpers for hardware-style EQ models.
//!
//! The calibration layer is intentionally data-oriented: a profile provides
//! response snapshots and a small set of tunable model parameters, then this
//! module evaluates fit error and runs a bounded coordinate search.

use crate::biquad::Coeffs;
use crate::response::compute_magnitude_response;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResponsePoint {
    pub freq_hz: f64,
    pub magnitude_db: f64,
    pub weight: f64,
}

impl ResponsePoint {
    pub const fn new(freq_hz: f64, magnitude_db: f64) -> Self {
        Self {
            freq_hz,
            magnitude_db,
            weight: 1.0,
        }
    }

    pub const fn weighted(freq_hz: f64, magnitude_db: f64, weight: f64) -> Self {
        Self {
            freq_hz,
            magnitude_db,
            weight,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResponseTarget {
    pub id: String,
    pub sample_rate: f64,
    pub points: Vec<ResponsePoint>,
}

impl ResponseTarget {
    pub fn new(id: impl Into<String>, sample_rate: f64, points: Vec<ResponsePoint>) -> Self {
        Self {
            id: id.into(),
            sample_rate,
            points,
        }
    }

    pub fn frequencies(&self) -> Vec<f64> {
        self.points.iter().map(|point| point.freq_hz).collect()
    }

    pub fn weighted_error_db(&self, predicted_db: &[f64]) -> CalibrationError {
        assert_eq!(
            self.points.len(),
            predicted_db.len(),
            "target point count must match predicted magnitude count"
        );

        let mut weighted_sse = 0.0;
        let mut weight_sum = 0.0;
        let mut max_abs_db = 0.0_f64;

        for (point, &predicted) in self.points.iter().zip(predicted_db) {
            let weight = point.weight.max(0.0);
            let error = predicted - point.magnitude_db;
            weighted_sse += weight * error * error;
            weight_sum += weight;
            max_abs_db = max_abs_db.max(error.abs());
        }

        let rms_db = if weight_sum > 0.0 {
            (weighted_sse / weight_sum).sqrt()
        } else {
            0.0
        };

        CalibrationError { rms_db, max_abs_db }
    }

    pub fn evaluate_sections(&self, sections: &[Coeffs]) -> CalibrationError {
        let freqs = self.frequencies();
        let predicted = compute_magnitude_response(sections, &freqs, self.sample_rate);
        self.weighted_error_db(&predicted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalibrationError {
    pub rms_db: f64,
    pub max_abs_db: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalibratedScalar {
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

impl CalibratedScalar {
    pub const fn new(value: f64, min: f64, max: f64, step: f64) -> Self {
        Self {
            value,
            min,
            max,
            step,
        }
    }

    pub fn clamp_value(&self, value: f64) -> f64 {
        value.clamp(self.min, self.max)
    }
}

pub trait CalibrationParameters: Clone {
    fn len(&self) -> usize;
    fn scalar(&self, index: usize) -> CalibratedScalar;
    fn set_scalar(&mut self, index: usize, value: f64);
    fn set_step(&mut self, index: usize, step: f64);

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitOptions {
    pub iterations: usize,
    pub shrink: f64,
    pub min_step: f64,
}

impl Default for FitOptions {
    fn default() -> Self {
        Self {
            iterations: 24,
            shrink: 0.55,
            min_step: 1.0e-4,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitReport<P> {
    pub params: P,
    pub error: CalibrationError,
    pub iterations: usize,
}

pub fn fit_response<P, F>(
    initial: P,
    target: &ResponseTarget,
    options: FitOptions,
    build_sections: F,
) -> FitReport<P>
where
    P: CalibrationParameters,
    F: Fn(&P) -> Vec<Coeffs>,
{
    let mut current = initial;
    let mut current_error = target.evaluate_sections(&build_sections(&current));
    let mut iterations_run = 0;

    for iteration in 0..options.iterations {
        iterations_run = iteration + 1;
        let mut improved = false;

        for index in 0..current.len() {
            let scalar = current.scalar(index);
            if scalar.step <= options.min_step {
                continue;
            }

            let mut best_value = scalar.value;
            let mut best_error = current_error;

            for direction in [-1.0, 1.0] {
                let candidate_value = scalar.clamp_value(scalar.value + scalar.step * direction);
                if (candidate_value - scalar.value).abs() <= f64::EPSILON {
                    continue;
                }

                let mut candidate = current.clone();
                candidate.set_scalar(index, candidate_value);
                let candidate_error = target.evaluate_sections(&build_sections(&candidate));
                if candidate_error.rms_db < best_error.rms_db {
                    best_value = candidate_value;
                    best_error = candidate_error;
                }
            }

            if best_error.rms_db < current_error.rms_db {
                current.set_scalar(index, best_value);
                current_error = best_error;
                improved = true;
            } else {
                let next_step = (scalar.step * options.shrink).max(options.min_step);
                current.set_step(index, next_step);
            }
        }

        if !improved {
            break;
        }
    }

    FitReport {
        params: current,
        error: current_error,
        iterations: iterations_run,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::{self, FilterType};

    #[derive(Clone)]
    struct GainParam {
        gain: f64,
        step: f64,
    }

    impl CalibrationParameters for GainParam {
        fn len(&self) -> usize {
            1
        }

        fn scalar(&self, _index: usize) -> CalibratedScalar {
            CalibratedScalar::new(self.gain, -12.0, 12.0, self.step)
        }

        fn set_scalar(&mut self, _index: usize, value: f64) {
            self.gain = value;
        }

        fn set_step(&mut self, _index: usize, step: f64) {
            self.step = step;
        }
    }

    #[test]
    fn response_target_reports_zero_error_for_matching_sections() {
        let sections = design::design_filter(FilterType::Peak, 1000.0, 1.0, 6.0, 48_000.0, 2);
        let freqs = vec![100.0, 1000.0, 10_000.0];
        let mags = compute_magnitude_response(&sections, &freqs, 48_000.0);
        let target = ResponseTarget::new(
            "matching",
            48_000.0,
            freqs
                .iter()
                .zip(mags.iter())
                .map(|(&freq_hz, &magnitude_db)| ResponsePoint::new(freq_hz, magnitude_db))
                .collect(),
        );

        let err = target.evaluate_sections(&sections);
        assert!(err.rms_db < 1.0e-12);
        assert!(err.max_abs_db < 1.0e-12);
    }

    #[test]
    fn weighted_error_respects_point_weights() {
        let target = ResponseTarget::new(
            "weights",
            48_000.0,
            vec![
                ResponsePoint::weighted(100.0, 0.0, 0.0),
                ResponsePoint::weighted(1000.0, 10.0, 1.0),
            ],
        );

        let err = target.weighted_error_db(&[100.0, 8.0]);
        assert!((err.rms_db - 2.0).abs() < 1.0e-12);
        assert!((err.max_abs_db - 100.0).abs() < 1.0e-12);
    }

    #[test]
    fn fit_response_reduces_simple_gain_error() {
        let target_sections =
            design::design_filter(FilterType::Peak, 1000.0, 1.0, 4.0, 48_000.0, 2);
        let freqs = vec![500.0, 1000.0, 2000.0];
        let target_mags = compute_magnitude_response(&target_sections, &freqs, 48_000.0);
        let target = ResponseTarget::new(
            "peak-gain",
            48_000.0,
            freqs
                .iter()
                .zip(target_mags.iter())
                .map(|(&freq_hz, &magnitude_db)| ResponsePoint::new(freq_hz, magnitude_db))
                .collect(),
        );

        let initial = GainParam {
            gain: 0.0,
            step: 1.0,
        };
        let before = target.evaluate_sections(&design::design_filter(
            FilterType::Peak,
            1000.0,
            1.0,
            initial.gain,
            48_000.0,
            2,
        ));
        let report = fit_response(initial, &target, FitOptions::default(), |params| {
            design::design_filter(FilterType::Peak, 1000.0, 1.0, params.gain, 48_000.0, 2)
        });

        assert!(report.error.rms_db < before.rms_db);
        assert!((report.params.gain - 4.0).abs() <= 1.0);
    }
}
