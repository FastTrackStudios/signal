//! Static EQ response evaluated from the DSP's prepared coefficients.
//! Prepare once per graph update, then evaluate the complete frequency grid.

use super::eq_graph_model::{EqBand, EqBandShape, StereoMode};
use eq_dsp::{
    BandConfig, CutSlope, EqConfig, Filter, Placement, PreparedEq, PreparedFilter, ProcessSpec,
    Steepness,
};

fn config(band: &EqBand) -> BandConfig {
    let frequency_hz = f64::from(band.frequency);
    let gain_db = f64::from(band.gain);
    let q = f64::from(band.q);
    let steepness = match band.slope.map(|v| v.round()) {
        Some(0.0 | 1.0) => {
            if matches!(band.shape, EqBandShape::Bell | EqBandShape::Notch) {
                Steepness::Order2
            } else {
                Steepness::Order1
            }
        }
        Some(3.0) => Steepness::Order3,
        Some(4.0) => Steepness::Order4,
        Some(5.0) => Steepness::Order5,
        Some(6.0) => Steepness::Order6,
        Some(7.0) => Steepness::Order8,
        Some(8.0) => Steepness::Order12,
        Some(9.0 | 10.0) => Steepness::Order16,
        _ => Steepness::Order2,
    };
    let cut_slope = match band.slope.map(f64::from) {
        Some(raw) if (0.0..6.0).contains(&raw) => CutSlope::DbPerOctave(raw * 6.0),
        Some(raw) => match raw.round() {
            6.0 => CutSlope::DbPerOctave(36.0),
            7.0 => CutSlope::DbPerOctave(48.0),
            8.0 => CutSlope::DbPerOctave(72.0),
            9.0 => CutSlope::DbPerOctave(96.0),
            10.0 => CutSlope::Brickwall,
            _ => CutSlope::DbPerOctave(12.0),
        },
        None => CutSlope::DbPerOctave(12.0),
    };
    let filter = match band.shape {
        EqBandShape::Bell => Filter::Bell {
            frequency_hz,
            gain_db,
            q,
            steepness,
        },
        EqBandShape::LowShelf => Filter::LowShelf {
            frequency_hz,
            gain_db,
            q,
            steepness,
        },
        EqBandShape::HighShelf => Filter::HighShelf {
            frequency_hz,
            gain_db,
            q,
            steepness,
        },
        EqBandShape::LowCut => Filter::HighPass {
            frequency_hz,
            q,
            slope: cut_slope,
        },
        EqBandShape::HighCut => Filter::LowPass {
            frequency_hz,
            q,
            slope: cut_slope,
        },
        EqBandShape::Notch => Filter::Notch {
            frequency_hz,
            q,
            steepness,
        },
        EqBandShape::BandPass => Filter::BandPass {
            frequency_hz,
            q,
            steepness,
        },
        EqBandShape::TiltShelf => Filter::Tilt {
            frequency_hz,
            gain_db,
            q,
            steepness,
        },
        EqBandShape::FlatTilt => Filter::FlatTilt {
            frequency_hz,
            gain_db,
            q,
            steepness,
        },
        EqBandShape::AllPass => Filter::AllPass {
            frequency_hz,
            q,
            steepness,
        },
    };
    BandConfig::new(filter)
        .enabled(band.used && band.enabled)
        .placement(match band.stereo_mode {
            StereoMode::Stereo => Placement::Stereo,
            StereoMode::Left => Placement::Left,
            StereoMode::Right => Placement::Right,
            StereoMode::Mid => Placement::Mid,
            StereoMode::Side => Placement::Side,
        })
}

pub fn prepare_band(band: &EqBand, sample_rate: f64) -> Result<PreparedFilter, eq_dsp::Error> {
    config(band).filter.prepare(sample_rate)
}

pub fn prepare_graph(bands: &[EqBand], sample_rate: f64) -> Result<PreparedEq, eq_dsp::Error> {
    let mut eq = EqConfig::with_capacity(bands.len());
    for band in bands.iter().filter(|b| b.used && b.enabled) {
        eq.add_band(config(band))?;
    }
    eq.prepare(ProcessSpec::new(sample_rate, 1)?)
}

/// Power response for equal-power uncorrelated stereo input. This keeps one
/// graph line meaningful even when bands mix left/right and mid/side placement.
#[must_use]
pub fn graph_magnitude(prepared: &PreparedEq, hz: f64) -> f64 {
    prepared.base_response(hz).map_or(f64::NAN, |h| {
        10.0 * ((h.ll.mag_sq() + h.lr.mag_sq() + h.rl.mag_sq() + h.rr.mag_sq()) * 0.5)
            .max(1e-30)
            .log10()
    })
}

/// Convenience evaluation for a single point; renderers should prepare once.
#[must_use]
pub fn calculate_combined_response(bands: &[EqBand], freq: f64, sample_rate: f64) -> f64 {
    prepare_graph(bands, sample_rate).map_or(f64::NAN, |eq| graph_magnitude(&eq, freq))
}

#[must_use]
pub fn calculate_band_response(band: &EqBand, freq: f64, sample_rate: f64) -> f64 {
    prepare_band(band, sample_rate)
        .and_then(|filter| filter.magnitude_db(freq))
        .unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_band_response_bell() {
        let band = EqBand {
            used: true,
            enabled: true,
            frequency: 1000.0,
            gain: 6.0,
            q: 1.0,
            shape: EqBandShape::Bell,
            ..Default::default()
        };

        let response_at_center = calculate_band_response(&band, 1000.0, 48000.0);
        assert!(
            (response_at_center - 6.0).abs() < 0.5,
            "Expected ~6.0 dB at center, got {response_at_center}"
        );

        let response_far = calculate_band_response(&band, 100.0, 48000.0);
        assert!(
            response_far.abs() < 2.0,
            "Expected near 0 dB far from center, got {response_far}"
        );
    }

    #[test]
    fn test_combined_response() {
        let bands = vec![
            EqBand {
                used: true,
                enabled: true,
                frequency: 100.0,
                gain: 3.0,
                q: 1.0,
                shape: EqBandShape::Bell,
                ..Default::default()
            },
            EqBand {
                used: true,
                enabled: true,
                frequency: 10000.0,
                gain: -3.0,
                q: 1.0,
                shape: EqBandShape::Bell,
                ..Default::default()
            },
        ];

        let mid_response = calculate_combined_response(&bands, 1000.0, 48000.0);
        assert!(mid_response.abs() < 1.0);
    }

    #[test]
    fn test_band_response_bell_negative_gain() {
        let band = EqBand {
            used: true,
            enabled: true,
            frequency: 1000.0,
            gain: -6.0,
            q: 1.0,
            shape: EqBandShape::Bell,
            ..Default::default()
        };

        let response_at_center = calculate_band_response(&band, 1000.0, 48000.0);
        assert!(
            (response_at_center - (-6.0)).abs() < 0.5,
            "Expected ~-6.0 dB at center for negative gain, got {response_at_center}"
        );

        let response_far = calculate_band_response(&band, 100.0, 48000.0);
        assert!(
            response_far.abs() < 2.0,
            "Expected near 0 dB far from center, got {response_far}"
        );
    }

    #[test]
    fn test_band_response_low_shelf() {
        let band = EqBand {
            used: true,
            enabled: true,
            frequency: 100.0,
            gain: 6.0,
            q: 0.7,
            shape: EqBandShape::LowShelf,
            ..Default::default()
        };

        let response_low = calculate_band_response(&band, 20.0, 48000.0);
        assert!(
            response_low > 3.0,
            "Expected boost below cutoff for low shelf, got {response_low}"
        );

        let response_high = calculate_band_response(&band, 1000.0, 48000.0);
        assert!(
            response_high.abs() < 2.0,
            "Expected ~0 dB above cutoff for low shelf, got {response_high}"
        );
    }

    #[test]
    fn test_band_response_high_shelf() {
        let band = EqBand {
            used: true,
            enabled: true,
            frequency: 8000.0,
            gain: 6.0,
            q: 0.7,
            shape: EqBandShape::HighShelf,
            ..Default::default()
        };

        let response_high = calculate_band_response(&band, 16000.0, 48000.0);
        assert!(
            response_high > 3.0,
            "Expected boost above cutoff for high shelf, got {response_high}"
        );

        let response_low = calculate_band_response(&band, 1000.0, 48000.0);
        assert!(
            response_low.abs() < 2.0,
            "Expected ~0 dB below cutoff for high shelf, got {response_low}"
        );
    }
}

#[cfg(test)]
mod parity_tests {
    use super::*;

    #[test]
    fn slope_and_sample_rate_reach_the_shared_design() {
        let band = EqBand {
            used: true,
            enabled: true,
            frequency: 8_000.0,
            q: 0.707,
            slope: Some(4.0),
            shape: EqBandShape::HighCut,
            ..EqBand::default()
        };
        let expected = Filter::LowPass {
            frequency_hz: 8_000.0,
            q: f64::from(band.q),
            slope: CutSlope::DbPerOctave(24.0),
        }
        .prepare(48_000.0)
        .unwrap();
        assert!(
            (calculate_band_response(&band, 12_000.0, 48_000.0)
                - expected.magnitude_db(12_000.0).unwrap())
            .abs()
                < 1e-10
        );
        assert!(
            (calculate_band_response(&band, 20_000.0, 48_000.0)
                - calculate_band_response(&band, 20_000.0, 96_000.0))
            .abs()
                > 0.1
        );
    }

    #[test]
    fn one_sided_band_has_a_stereo_power_response() {
        let band = EqBand {
            used: true,
            enabled: true,
            frequency: 1_000.0,
            gain: 6.0,
            q: 1.0,
            shape: EqBandShape::Bell,
            stereo_mode: StereoMode::Left,
            ..EqBand::default()
        };
        let response = calculate_combined_response(&[band], 1_000.0, 48_000.0);
        assert!((response - 3.962_927_980_447_141).abs() < 0.05);
    }
    #[test]
    fn absent_slope_uses_second_order_without_reinterpreting_q() {
        let band = EqBand {
            used: true,
            enabled: true,
            frequency: 1000.0,
            q: 4.0,
            shape: EqBandShape::LowCut,
            ..Default::default()
        };
        let explicit = EqBand {
            slope: Some(2.0),
            ..band.clone()
        };
        assert_eq!(
            calculate_combined_response(&[band.clone()], 700.0, 48000.0),
            calculate_combined_response(&[explicit], 700.0, 48000.0)
        );
        let different_q = EqBand {
            q: 0.707,
            ..band.clone()
        };
        assert!(
            (calculate_combined_response(&[band], 1000.0, 48000.0)
                - calculate_combined_response(&[different_q], 1000.0, 48000.0))
            .abs()
                > 1.0
        );
    }
}
