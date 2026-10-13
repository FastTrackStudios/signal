//! Static EQ response evaluated from the DSP's prepared coefficients.
//! Prepare once per graph update, then evaluate the complete frequency grid.

use super::eq_graph_model::{EqBand, EqBandShape, StereoMode};
use eq_dsp::{
    BandConfig, CutSlope, EqConfig, Filter, Placement, PreparedEq, PreparedFilter, ProcessSpec,
    Steepness,
};

/// The highest frequency a filter can actually be designed at, as a fraction
/// of the sample rate.
///
/// A biquad's centre has to sit below Nyquist; at exactly Nyquist the design
/// degenerates. Sessions cross sample rates — one saved at 96 kHz and opened
/// at 44.1 has bands the new rate cannot realise — so the graph designs them
/// where they can be designed rather than refusing to draw.
const DESIGN_CEILING: f64 = 0.499;

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

/// The band as the graph will design it: its own frequency, or the highest
/// this sample rate can realise.
fn realisable(band: &EqBand, sample_rate: f64) -> EqBand {
    let ceiling = (sample_rate * DESIGN_CEILING) as f32;
    if band.frequency <= ceiling {
        return band.clone();
    }
    EqBand {
        frequency: ceiling,
        ..band.clone()
    }
}

pub fn prepare_band(band: &EqBand, sample_rate: f64) -> Result<PreparedFilter, eq_dsp::Error> {
    config(&realisable(band, sample_rate))
        .filter
        .prepare(sample_rate)
}

/// Prepare the visible EQ.
///
/// A band the sample rate cannot realise is designed at Nyquist, and one that
/// still will not design is left out. Neither may take the curve with it: the
/// whole combined response used to come back `NaN` if any single band was
/// out of range, so one band parked above Nyquist blanked the display for all
/// eight — and at 48 kHz the frequency control could reach 30 kHz, so getting
/// there was a drag, not an edge case.
pub fn prepare_graph(bands: &[EqBand], sample_rate: f64) -> Result<PreparedEq, eq_dsp::Error> {
    let mut eq = EqConfig::with_capacity(bands.len());
    for band in bands.iter().filter(|b| b.used && b.enabled) {
        let _ = eq.add_band(config(&realisable(band, sample_rate)));
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

#[cfg(test)]
mod nyquist_tests {
    use super::*;

    fn bell(hz: f32) -> EqBand {
        EqBand {
            index: 0,
            used: true,
            enabled: true,
            frequency: hz,
            gain: 6.0,
            q: 1.0,
            slope: None,
            shape: EqBandShape::Bell,
            focus: false,
            stereo_mode: StereoMode::Stereo,
            name: String::new(),
        }
    }

    /// One band the sample rate cannot realise must not take the curve with
    /// it.
    ///
    /// `prepare_graph` returned `Err` if *any* band failed to design, and the
    /// callers turn that into `NaN` — so a single band above Nyquist blanked
    /// the whole display, every other band's response included. At 48 kHz the
    /// frequency control could reach 30 kHz, which made getting there a drag
    /// rather than an edge case; sessions crossing sample rates get there
    /// without touching anything.
    #[test]
    fn a_band_above_nyquist_does_not_erase_the_curve() {
        for hz in [25_000.0_f32, 29_000.0] {
            let at_1k = calculate_combined_response(&[bell(hz)], 1000.0, 48_000.0);
            assert!(
                at_1k.is_finite(),
                "a band at {hz} Hz blanked the response at 1 kHz",
            );
        }
    }

    /// And it must not erase its neighbours either — the case that actually
    /// looks like the bug, because seven bands vanish along with the one you
    /// dragged.
    #[test]
    fn a_band_above_nyquist_leaves_its_neighbours_alone() {
        let alone = calculate_combined_response(&[bell(1000.0)], 1000.0, 48_000.0);
        let with_stray =
            calculate_combined_response(&[bell(1000.0), bell(29_000.0)], 1000.0, 48_000.0);
        assert!(
            (with_stray - alone).abs() < 0.2,
            "a stray band above Nyquist moved its neighbour from {alone} to {with_stray} dB",
        );
    }
}

#[cfg(test)]
mod default_state_tests {
    use super::*;
    use crate::params::{FtsEqParams, NUM_BANDS};

    /// The band shapes, by the engine's canonical order. A local copy: the
    /// two in the editor are private, and this test is about the numbers the
    /// parameters ship with, not about the editor.
    fn shape_of(v: i32) -> EqBandShape {
        match v {
            1 => EqBandShape::LowShelf,
            2 => EqBandShape::LowCut,
            3 => EqBandShape::HighShelf,
            4 => EqBandShape::HighCut,
            5 => EqBandShape::Notch,
            6 => EqBandShape::BandPass,
            7 => EqBandShape::TiltShelf,
            8 => EqBandShape::FlatTilt,
            9 => EqBandShape::AllPass,
            _ => EqBandShape::Bell,
        }
    }

    /// Loading the plugin must not change the audio.
    ///
    /// Bands 0 and 1 ship enabled — a low shelf at 400 Hz and a high shelf at
    /// 2.5 kHz, both at 0 dB, so they sit ready to pull. "Ready to pull" is
    /// only defensible if it is inaudible, which is what this asserts.
    #[test]
    fn the_default_state_is_unity() {
        let params = FtsEqParams::default();
        let bands: Vec<EqBand> = (0..NUM_BANDS)
            .map(|i| {
                let bp = &params.bands[i];
                EqBand {
                    index: i,
                    used: bp.enabled.value() > 0.5,
                    enabled: bp.enabled.value() > 0.5,
                    frequency: bp.freq_hz.value(),
                    gain: bp.gain_db.value(),
                    q: bp.q.value() * std::f32::consts::FRAC_1_SQRT_2,
                    slope: Some(bp.slope.value()),
                    shape: shape_of(bp.filter_type.value()),
                    focus: bp.focus.value() > 0.5,
                    stereo_mode: StereoMode::Stereo,
                    name: String::new(),
                }
            })
            .collect();

        assert!(
            !bands.iter().any(|b| b.focus),
            "a band ships focused, which mutes everything else",
        );

        for hz in [30.0, 100.0, 400.0, 1000.0, 2500.0, 8000.0, 18_000.0] {
            let db = calculate_combined_response(&bands, hz, 48_000.0);
            assert!(
                db.abs() < 0.01,
                "the default state is {db:+.3} dB at {hz} Hz — loading the \
                 plugin changes the sound",
            );
        }
    }
}

#[cfg(test)]
mod q_range_tests {
    use super::*;
    use crate::params::FtsEqParams;

    /// Both ends of the Q control have to be settings the engine will design.
    ///
    /// A control that can reach past what the DSP accepts is the same fault
    /// as the frequency one: the band drops out of the curve at the very
    /// settings — the widest and the most surgical — you reached for it to
    /// get. The editor shows Q at 1/√2 of the parameter, which is where the
    /// two ends came apart.
    #[test]
    fn the_q_controls_extremes_are_designable() {
        let params = FtsEqParams::default();
        let range = params.bands[0].q.range();
        let shown_at = |n: f32| range.unnormalize(n) * std::f32::consts::FRAC_1_SQRT_2;

        for (label, normalized) in [("narrowest", 1.0_f32), ("widest", 0.0)] {
            let shown = shown_at(normalized);
            let band = EqBand {
                index: 0,
                used: true,
                enabled: true,
                frequency: 1000.0,
                gain: 6.0,
                q: shown,
                slope: None,
                shape: EqBandShape::Bell,
                focus: false,
                stereo_mode: StereoMode::Stereo,
                name: String::new(),
            };
            let db = calculate_combined_response(&[band], 1000.0, 48_000.0);
            assert!(
                db.is_finite(),
                "the {label} Q the control can reach (shown as {shown:.3}) is \
                 one the engine will not design",
            );
        }

        // And the shown range is Pro-Q's, which is the point of the scaling.
        let widest = shown_at(0.0);
        let narrowest = shown_at(1.0);
        assert!((widest - 0.025).abs() < 0.001, "widest Q shows as {widest}");
        assert!(
            (narrowest - 40.0).abs() < 0.01,
            "narrowest Q shows as {narrowest}"
        );
    }
}
