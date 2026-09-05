//! Biquad filters the metrics need: octave-band splitting for per-band decay,
//! and the ITU-R BS.1770 K-weighting pair for loudness.
//!
//! These are measurement filters, not audio-path DSP — they run offline over a
//! captured buffer, so clarity wins over the hot-path constraints that govern
//! `features/fx/*-dsp`.

use std::f64::consts::PI;

/// A transposed-direct-form-II biquad section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Biquad {
    /// Run the filter over a whole buffer, starting from rest.
    #[must_use]
    pub fn apply(&self, x: &[f32]) -> Vec<f32> {
        let (mut z1, mut z2) = (0.0f64, 0.0f64);
        x.iter()
            .map(|&s| {
                let s = s as f64;
                let y = self.b0 * s + z1;
                z1 = self.b1 * s - self.a1 * y + z2;
                z2 = self.b2 * s - self.a2 * y;
                y as f32
            })
            .collect()
    }

    /// Normalize by `a0` — every design below builds unnormalized coefficients.
    fn normalized(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Constant-Q bandpass (RBJ cookbook), unity gain at the centre.
    #[must_use]
    pub fn bandpass(centre_hz: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * centre_hz / sample_rate;
        let (sin, cos) = (w0.sin(), w0.cos());
        let alpha = sin / (2.0 * q);
        Self::normalized(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
    }

    /// High-shelf (RBJ cookbook), `gain_db` above the corner.
    #[must_use]
    pub fn high_shelf(f0: f64, q: f64, gain_db: f64, sample_rate: f64) -> Self {
        let a = 10.0f64.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / sample_rate;
        let (sin, cos) = (w0.sin(), w0.cos());
        let alpha = sin / (2.0 * q);
        let sqrt_a = a.sqrt();
        Self::normalized(
            a * ((a + 1.0) + (a - 1.0) * cos + 2.0 * sqrt_a * alpha),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - 2.0 * sqrt_a * alpha),
            (a + 1.0) - (a - 1.0) * cos + 2.0 * sqrt_a * alpha,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - 2.0 * sqrt_a * alpha,
        )
    }

    /// Second-order highpass (RBJ cookbook).
    #[must_use]
    pub fn highpass(f0: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * f0 / sample_rate;
        let (sin, cos) = (w0.sin(), w0.cos());
        let alpha = sin / (2.0 * q);
        Self::normalized(
            (1.0 + cos) / 2.0,
            -(1.0 + cos),
            (1.0 + cos) / 2.0,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        )
    }
}

/// The octave-band centre frequencies decay is reported over.
///
/// Stops at 8 kHz: the next band up (16 kHz) is above Nyquist for a 32 kHz
/// project and carries almost no reverb energy anyway.
pub const OCTAVE_CENTRES_HZ: [f64; 8] = [62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];

/// Q for a one-octave-wide constant-Q bandpass section.
const OCTAVE_Q: f64 = 1.414;

/// Bandpass sections cascaded per band.
///
/// A constant-Q bandpass has only 6 dB/octave skirts, which is nowhere near
/// enough isolation for decay measurement: a reverb's energy is concentrated
/// in the low-mids, and when those decay faster than the top (a tamed low
/// end, say) their leakage into a high band decays with them and drags that
/// band's fitted RT60 down. A 300 Hz low-shelf cut appeared to halve the
/// 4 kHz band's decay — that was leakage, not the reverb.
///
/// Three sections give 18 dB/octave, so a tone four octaves away lands ~72 dB
/// down and cannot influence the fit.
const OCTAVE_SECTIONS: usize = 3;

/// Split a buffer into octave bands, in [`OCTAVE_CENTRES_HZ`] order.
///
/// Bands whose centre is at or above Nyquist are returned as silence rather
/// than being dropped, so the band index always lines up with the frequency
/// table regardless of sample rate.
#[must_use]
pub fn octave_bands(x: &[f32], sample_rate: f64) -> Vec<Vec<f32>> {
    OCTAVE_CENTRES_HZ
        .iter()
        .map(|&c| {
            if c >= sample_rate / 2.0 {
                return vec![0.0; x.len()];
            }
            let bp = Biquad::bandpass(c, OCTAVE_Q, sample_rate);
            let mut out = bp.apply(x);
            for _ in 1..OCTAVE_SECTIONS {
                out = bp.apply(&out);
            }
            out
        })
        .collect()
}

/// ITU-R BS.1770 K-weighting: a high-shelf "head" filter followed by an RLB
/// highpass.
///
/// The standard tabulates coefficients for 48 kHz; these are the analog
/// prototype parameters the table is derived from, re-designed at the actual
/// sample rate so the weighting stays correct off 48 kHz.
#[must_use]
pub fn k_weighting(sample_rate: f64) -> [Biquad; 2] {
    const SHELF_F0: f64 = 1_681.974_450_955_533;
    const SHELF_Q: f64 = 0.707_175_236_955_419_6;
    const SHELF_GAIN_DB: f64 = 3.999_843_853_973_347;
    const HP_F0: f64 = 38.135_470_876_024_44;
    const HP_Q: f64 = 0.500_327_037_323_877_3;

    [
        Biquad::high_shelf(SHELF_F0, SHELF_Q, SHELF_GAIN_DB, sample_rate),
        Biquad::highpass(HP_F0, HP_Q, sample_rate),
    ]
}

/// Apply K-weighting to a buffer.
#[must_use]
pub fn k_weight(x: &[f32], sample_rate: f64) -> Vec<f32> {
    let [shelf, hp] = k_weighting(sample_rate);
    hp.apply(&shelf.apply(x))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    /// Magnitude response of a biquad at `f`, by evaluating H(z) on the unit
    /// circle. Independent of `apply`, so it checks the coefficients directly.
    fn magnitude(bq: &Biquad, f: f64, sr: f64) -> f64 {
        use std::f64::consts::TAU;
        let w = TAU * f / sr;
        let (c1, s1) = ((-w).cos(), (-w).sin());
        let (c2, s2) = ((-2.0 * w).cos(), (-2.0 * w).sin());
        let num = (bq.b0 + bq.b1 * c1 + bq.b2 * c2, bq.b1 * s1 + bq.b2 * s2);
        let den = (1.0 + bq.a1 * c1 + bq.a2 * c2, bq.a1 * s1 + bq.a2 * s2);
        (num.0.hypot(num.1)) / (den.0.hypot(den.1))
    }

    fn db(x: f64) -> f64 {
        20.0 * x.log10()
    }

    #[test]
    fn bandpass_peaks_at_its_centre_and_rolls_off() {
        let bq = Biquad::bandpass(1000.0, 1.414, SR);
        let at_centre = magnitude(&bq, 1000.0, SR);
        assert!((db(at_centre)).abs() < 0.1, "unity at centre");
        // An octave either side should be well down.
        assert!(db(magnitude(&bq, 500.0, SR)) < -5.0);
        assert!(db(magnitude(&bq, 2000.0, SR)) < -5.0);
    }

    #[test]
    fn highpass_rejects_dc_and_passes_the_top() {
        let bq = Biquad::highpass(100.0, 0.707, SR);
        assert!(db(magnitude(&bq, 1.0, SR)) < -60.0);
        assert!(db(magnitude(&bq, 10_000.0, SR)).abs() < 0.5);
        // -3 dB at the corner.
        assert!((db(magnitude(&bq, 100.0, SR)) + 3.0).abs() < 0.5);
    }

    #[test]
    fn high_shelf_reaches_its_gain() {
        let bq = Biquad::high_shelf(1000.0, 0.707, 6.0, SR);
        assert!(db(magnitude(&bq, 20.0, SR)).abs() < 0.5, "unity below");
        assert!(
            (db(magnitude(&bq, 20_000.0, SR)) - 6.0).abs() < 0.5,
            "+6 above"
        );
    }

    #[test]
    fn k_weighting_matches_the_standards_shape() {
        let [shelf, hp] = k_weighting(SR);
        let resp = |f: f64| db(magnitude(&shelf, f, SR) * magnitude(&hp, f, SR));

        // BS.1770: ~0 dB through the midrange, ~+4 dB at the top, steep
        // low-frequency rolloff.
        assert!(
            resp(1000.0).abs() < 0.5,
            "flat at 1 kHz, got {}",
            resp(1000.0)
        );
        assert!(
            resp(10_000.0) > 3.0,
            "lifted at 10 kHz, got {}",
            resp(10_000.0)
        );
        assert!(
            resp(20.0) < -10.0,
            "rolled off at 20 Hz, got {}",
            resp(20.0)
        );
    }

    #[test]
    fn k_weighting_is_redesigned_per_sample_rate() {
        // The same physical frequency must weight the same at 44.1 and 96 kHz.
        for sr in [44_100.0, 96_000.0] {
            let [shelf, hp] = k_weighting(sr);
            let at_1k = db(magnitude(&shelf, 1000.0, sr) * magnitude(&hp, 1000.0, sr));
            assert!(at_1k.abs() < 0.5, "1 kHz at {sr} Hz: {at_1k}");
        }
    }

    #[test]
    fn octave_bands_reject_a_distant_tone() {
        // The isolation that matters for decay measurement: a loud 250 Hz
        // tone must not show up in the 4 kHz band.
        let tone = crate::generators::sine(250.0, SR, 48_000);
        let bands = octave_bands(&tone, SR);
        let energy = |b: &[f32]| b.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>();
        let at_250 = energy(&bands[2]); // 250 Hz
        let at_4k = energy(&bands[6]); // 4 kHz
        let rejection_db = 10.0 * (at_4k / at_250).log10();
        assert!(
            rejection_db < -60.0,
            "4 kHz band should reject a 250 Hz tone, got {rejection_db:.1} dB"
        );
    }

    #[test]
    fn octave_bands_pass_their_own_centre() {
        let tone = crate::generators::sine(1000.0, SR, 48_000);
        let bands = octave_bands(&tone, SR);
        let energy = |b: &[f32]| b.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>();
        let src = energy(&tone);
        let at_1k = energy(&bands[4]);
        assert!(
            10.0 * (at_1k / src).log10() > -3.0,
            "the 1 kHz band must pass a 1 kHz tone"
        );
    }

    #[test]
    fn octave_bands_are_silent_above_nyquist_not_missing() {
        // At 16 kHz the 8 kHz band centre is at Nyquist.
        let bands = octave_bands(&vec![1.0; 256], 16_000.0);
        assert_eq!(bands.len(), OCTAVE_CENTRES_HZ.len());
        assert!(bands[7].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn apply_starts_from_rest() {
        // A filter fed silence stays silent — no state leaking in.
        let bq = Biquad::bandpass(1000.0, 1.0, SR);
        assert!(bq.apply(&vec![0.0; 64]).iter().all(|&s| s == 0.0));
    }
}
