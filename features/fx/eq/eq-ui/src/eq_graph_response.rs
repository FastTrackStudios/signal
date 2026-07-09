//! Approximate EQ graph response math used for UI rendering.

use super::eq_graph_model::{EqBand, EqBandShape};

pub fn calculate_combined_response(bands: &[EqBand], freq: f64, sample_rate: f64) -> f64 {
    let mut total_db = 0.0;

    for band in bands {
        if !band.used || !band.enabled {
            continue;
        }
        total_db += calculate_band_response(band, freq, sample_rate);
    }

    total_db
}

fn biquad_magnitude_squared(coeff: &[f64; 6], w: f64) -> f64 {
    let w2 = w * w;
    let denom_real = coeff[0] - coeff[2] * w2;
    let denom_imag = coeff[1] * w;
    let denominator = denom_real * denom_real + denom_imag * denom_imag;

    let numer_real = coeff[3] - coeff[5] * w2;
    let numer_imag = coeff[4] * w;
    let numerator = numer_real * numer_real + numer_imag * numer_imag;

    if denominator > 1e-30 {
        numerator / denominator
    } else {
        1.0
    }
}

fn lowpass_coeffs(w0: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    [1.0, w0 / q, w02, w02, 0.0, 0.0]
}

fn highpass_coeffs(w0: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    [1.0, w0 / q, w02, 0.0, 0.0, 1.0]
}

fn lowshelf_coeffs(w0: f64, gain_linear: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    let sqrt_g = gain_linear.sqrt();
    let g4 = gain_linear.sqrt().sqrt();

    [
        w02,
        w0 * g4 / q,
        1.0,
        gain_linear * w02,
        w0 * sqrt_g * g4 / q,
        1.0,
    ]
}

fn highshelf_coeffs(w0: f64, gain_linear: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    let sqrt_g = gain_linear.sqrt();
    let g4 = gain_linear.sqrt().sqrt();

    [
        w02,
        w0 * g4 / q,
        1.0,
        w02,
        w0 * sqrt_g * g4 / q,
        gain_linear,
    ]
}

fn peak_coeffs(w0: f64, gain_linear: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    let a = gain_linear.sqrt();
    [w02, w0 / (a * q), 1.0, w02, w0 * a / q, 1.0]
}

fn notch_coeffs(w0: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    [1.0, w0 / q, w02, w02, 0.0, 1.0]
}

fn bandpass_coeffs(w0: f64, q: f64) -> [f64; 6] {
    let w02 = w0 * w0;
    [1.0, w0 / q, w02, 0.0, w0 / q, 0.0]
}

fn cascaded_magnitude_db(freq: f64, f0: f64, order: usize, filter_type: &EqBandShape) -> f64 {
    if order == 0 {
        return 0.0;
    }

    let w = 2.0 * std::f64::consts::PI * freq;
    let w0 = 2.0 * std::f64::consts::PI * f0;

    if order == 1 {
        let mag_sq = match filter_type {
            EqBandShape::LowCut => {
                let w2 = w * w;
                let w02 = w0 * w0;
                w2 / (w2 + w02)
            }
            EqBandShape::HighCut => {
                let w2 = w * w;
                let w02 = w0 * w0;
                w02 / (w2 + w02)
            }
            _ => 1.0,
        };
        return 10.0 * mag_sq.max(1e-30).log10();
    }

    let num_sections = order / 2;
    let has_first_order = order % 2 == 1;
    let mut total_mag_sq = 1.0;

    if has_first_order {
        let first_order_mag = match filter_type {
            EqBandShape::LowCut => {
                let w2 = w * w;
                let w02 = w0 * w0;
                w2 / (w2 + w02)
            }
            EqBandShape::HighCut => {
                let w2 = w * w;
                let w02 = w0 * w0;
                w02 / (w2 + w02)
            }
            _ => 1.0,
        };
        total_mag_sq *= first_order_mag;
    }

    for i in 0..num_sections {
        let theta = std::f64::consts::PI * (2 * i + 1) as f64 / (2 * order) as f64;
        let section_q = 1.0 / (2.0 * theta.cos());

        let coeffs = match filter_type {
            EqBandShape::LowCut => highpass_coeffs(w0, section_q),
            EqBandShape::HighCut => lowpass_coeffs(w0, section_q),
            _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        };

        total_mag_sq *= biquad_magnitude_squared(&coeffs, w);
    }

    10.0 * total_mag_sq.max(1e-30).log10()
}

pub fn calculate_band_response(band: &EqBand, freq: f64, _sample_rate: f64) -> f64 {
    let f0 = band.frequency as f64;
    let gain = band.gain as f64;
    let q = band.q as f64;

    let w = 2.0 * std::f64::consts::PI * freq;
    let w0 = 2.0 * std::f64::consts::PI * f0;

    match band.shape {
        EqBandShape::Bell => {
            let gain_linear = 10.0_f64.powf(gain / 20.0);
            let coeffs = peak_coeffs(w0, gain_linear, q);
            let mag_sq = biquad_magnitude_squared(&coeffs, w);
            10.0 * mag_sq.max(1e-30).log10()
        }
        EqBandShape::LowShelf => {
            let gain_linear = 10.0_f64.powf(gain / 20.0);
            let coeffs = lowshelf_coeffs(w0, gain_linear, q.max(0.5));
            let mag_sq = biquad_magnitude_squared(&coeffs, w);
            10.0 * mag_sq.max(1e-30).log10()
        }
        EqBandShape::HighShelf => {
            let gain_linear = 10.0_f64.powf(gain / 20.0);
            let coeffs = highshelf_coeffs(w0, gain_linear, q.max(0.5));
            let mag_sq = biquad_magnitude_squared(&coeffs, w);
            10.0 * mag_sq.max(1e-30).log10()
        }
        EqBandShape::LowCut => {
            let order = (q * 2.0).round().max(1.0) as usize;
            cascaded_magnitude_db(freq, f0, order, &EqBandShape::LowCut)
        }
        EqBandShape::HighCut => {
            let order = (q * 2.0).round().max(1.0) as usize;
            cascaded_magnitude_db(freq, f0, order, &EqBandShape::HighCut)
        }
        EqBandShape::Notch => {
            let coeffs = notch_coeffs(w0, q.max(0.5));
            let mag_sq = biquad_magnitude_squared(&coeffs, w);
            10.0 * mag_sq.max(1e-30).log10()
        }
        EqBandShape::BandPass => {
            let coeffs = bandpass_coeffs(w0, q.max(0.5));
            let mag_sq = biquad_magnitude_squared(&coeffs, w);
            let peak_mag_sq = biquad_magnitude_squared(&coeffs, w0);
            let normalized = mag_sq / peak_mag_sq.max(1e-30);
            gain + 10.0 * normalized.max(1e-30).log10()
        }
        EqBandShape::TiltShelf | EqBandShape::FlatTilt => {
            let octaves = (freq / f0).log2();
            let slope_db_per_oct = gain / 3.0;
            octaves * slope_db_per_oct
        }
        EqBandShape::AllPass => 0.0,
    }
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
