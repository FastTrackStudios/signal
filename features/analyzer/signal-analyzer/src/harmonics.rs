//! Saturation: what a processor adds that was not in the signal.
//!
//! [`crate::comp_probe`] measures the gain a compressor applies — how much,
//! how fast, at which frequency. That is the whole story for a clean digital
//! compressor and about half of it for anything modelled on hardware. An 1176
//! is not a gain element with a fast detector; it is a gain element with a
//! fast detector *and* a transformer and a Class-A output stage, and a great
//! deal of what people reach for it for is the harmonic structure it adds
//! well before the gain reduction becomes obvious. Model the envelope alone
//! and the result compresses correctly and sounds nothing like it.
//!
//! So: drive a steady sine through the processor and look at what comes out at
//! multiples of the input frequency. Sweep the input level and the harmonic
//! amplitudes trace out a saturation curve — how the distortion grows, whether
//! it is even or odd order, and where it starts.
//!
//! Even and odd order is the distinction worth keeping separate rather than
//! collapsing into one THD number. Odd-order harmonics (3rd, 5th) come from
//! symmetric compression of the waveform and read as hardness; even-order
//! (2nd, 4th) come from asymmetry and read as warmth. Two processors with
//! identical THD and opposite balance do not sound remotely alike, so
//! [`Harmonics`] reports the series, and the aggregates are derived from it.
//!
//! Like the rest of this crate, nothing here hosts a plugin — callers render
//! the buffer and hand it over.

use realfft::RealFftPlanner;
use serde::{Deserialize, Serialize};

/// Analysis window. Long enough that a 20 Hz fundamental still has many
/// cycles in it, which is what keeps the low-frequency bins clean.
pub const FFT: usize = 32_768;

/// How many harmonics above the fundamental to report by default.
pub const DEFAULT_HARMONICS: usize = 8;

/// A steady tone at a known frequency and level.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ToneSpec {
    pub freq_hz: f64,
    /// Amplitude in dBFS (peak, not RMS).
    pub level_db: f64,
    pub duration_s: f64,
}

impl Default for ToneSpec {
    fn default() -> Self {
        Self { freq_hz: 1000.0, level_db: -12.0, duration_s: 2.0 }
    }
}

/// Render the tone.
pub fn tone(spec: &ToneSpec, sample_rate: f64) -> Vec<f32> {
    let n = (spec.duration_s * sample_rate).max(1.0) as usize;
    let amp = 10.0f64.powf(spec.level_db / 20.0);
    (0..n)
        .map(|i| (amp * (std::f64::consts::TAU * spec.freq_hz * i as f64 / sample_rate).sin()) as f32)
        .collect()
}

/// What came out at the fundamental and its multiples.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Harmonics {
    /// Absolute level of the fundamental, dBFS.
    pub fundamental_db: f64,
    /// Harmonics 2..=n, each **relative to the fundamental** in dB. Negative.
    /// A harmonic below the measured noise floor is reported as the floor
    /// rather than as a spuriously precise tiny number.
    pub harmonics_db: Vec<f64>,
    /// Total harmonic distortion as a percentage of the fundamental.
    pub thd_percent: f64,
    /// The same, in dB relative to the fundamental.
    pub thd_db: f64,
    /// Even-order harmonics only (2nd, 4th, …), summed, dB rel. fundamental.
    pub even_db: f64,
    /// Odd-order harmonics only (3rd, 5th, …), summed, dB rel. fundamental.
    pub odd_db: f64,
    /// Median bin level away from any harmonic — everything the processor
    /// added that is *not* harmonically related, which is where a noisy or
    /// aliasing model shows itself.
    pub noise_floor_db: f64,
}

/// Measure the harmonic series of a rendered tone.
///
/// `skip` samples are discarded from the front before analysis; a compressor
/// needs its envelope to settle, and measuring across the attack transient
/// reads the transient rather than the steady-state distortion.
///
/// Harmonics above Nyquist are skipped, not folded: a 5 kHz fundamental has no
/// real 6th harmonic at 48 kHz, and reporting whatever lands at the aliased
/// frequency would invent one. They come back as `None`-shaped entries — the
/// noise floor — rather than as data.
pub fn analyze(
    rendered: &[f32],
    freq_hz: f64,
    sample_rate: f64,
    n_harmonics: usize,
    skip: usize,
) -> Harmonics {
    let start = skip.min(rendered.len());
    let tail = &rendered[start..];
    if tail.len() < FFT {
        return Harmonics {
            fundamental_db: f64::NEG_INFINITY,
            harmonics_db: vec![f64::NEG_INFINITY; n_harmonics.saturating_sub(1)],
            thd_percent: 0.0,
            thd_db: f64::NEG_INFINITY,
            even_db: f64::NEG_INFINITY,
            odd_db: f64::NEG_INFINITY,
            noise_floor_db: f64::NEG_INFINITY,
        };
    }

    // Hann window, and the coherent gain it costs (0.5) divided back out so
    // the fundamental reads at its true amplitude rather than 6 dB low.
    let window: Vec<f64> = (0..FFT)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / FFT as f64).cos())
        .collect();
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(FFT);
    let mut frame: Vec<f64> = (0..FFT).map(|i| tail[i] as f64 * window[i]).collect();
    let mut out = fft.make_output_vec();
    fft.process(&mut frame, &mut out).expect("fft");

    let mag: Vec<f64> = out.iter().map(|c| c.norm()).collect();
    let bin_hz = sample_rate / FFT as f64;
    let nyquist = sample_rate / 2.0;
    // Energy normalisation for the window, from Parseval: a windowed sine of
    // amplitude A puts (A²/4)·N·Σw² into its main lobe, whatever the window.
    let window_energy: f64 = window.iter().map(|w| w * w).sum();
    let norm = FFT as f64 * window_energy;

    // Amplitude of a tone near `hz`, by summing the **energy** in its main
    // lobe rather than reading the peak bin.
    //
    // Peak-bin reading is wrong whenever the tone does not land exactly on a
    // bin centre, which is the normal case: 1 kHz at 48 kHz over a 32k FFT
    // falls at bin 682.67, and the resulting scalloping loss measured a
    // -12 dBFS sine as -12.63 dBFS. Summing the lobe is independent of where
    // the tone sits between bins, and of which window is used.
    let amp_near = |hz: f64| -> f64 {
        let centre = (hz / bin_hz).round() as isize;
        let lo = (centre - 4).max(0) as usize;
        let hi = ((centre + 4) as usize).min(mag.len().saturating_sub(1));
        let energy: f64 = (lo..=hi).map(|i| mag[i] * mag[i]).sum();
        2.0 * (energy / norm).sqrt()
    };
    // The noise floor is a per-bin statistic, so it needs the same scaling a
    // single bin would carry rather than the lobe sum.
    let bin_amp = |i: usize| 2.0 * (mag[i] * mag[i] / norm).sqrt();

    let db = |v: f64| if v <= 1e-12 { -240.0 } else { 20.0 * v.log10() };

    let fundamental = amp_near(freq_hz);
    let fundamental_db = db(fundamental);

    // The noise floor: the median magnitude of bins that are not within a few
    // bins of any harmonic. Median, not mean, so a stray tone cannot lift it.
    let mut away: Vec<f64> = Vec::new();
    for (i, _m) in mag.iter().enumerate() {
        let hz = i as f64 * bin_hz;
        if hz < 20.0 || hz > nyquist * 0.95 {
            continue;
        }
        let near_harmonic = (1..=n_harmonics.max(1) * 2)
            .any(|k| (hz - k as f64 * freq_hz).abs() < bin_hz * 5.0);
        if !near_harmonic {
            away.push(bin_amp(i));
        }
    }
    away.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let floor = away.get(away.len() / 2).copied().unwrap_or(0.0);
    let floor_rel_db = if fundamental > 0.0 { db(floor / fundamental) } else { -240.0 };

    let mut harmonics_db = Vec::new();
    let mut even_power = 0.0f64;
    let mut odd_power = 0.0f64;
    let mut total_power = 0.0f64;
    for k in 2..=n_harmonics.max(2) {
        let hz = k as f64 * freq_hz;
        if hz >= nyquist {
            // No such harmonic exists in this band — do not invent one from
            // whatever aliased into its place.
            harmonics_db.push(floor_rel_db);
            continue;
        }
        let m = amp_near(hz);
        let rel = if fundamental > 0.0 { m / fundamental } else { 0.0 };
        // Below the floor there is nothing to report but the floor.
        let rel_db = db(rel).max(floor_rel_db);
        harmonics_db.push(rel_db);

        if m > floor {
            let p = rel * rel;
            total_power += p;
            if k % 2 == 0 {
                even_power += p;
            } else {
                odd_power += p;
            }
        }
    }

    let thd = total_power.sqrt();
    Harmonics {
        fundamental_db,
        harmonics_db,
        thd_percent: thd * 100.0,
        thd_db: db(thd),
        even_db: db(even_power.sqrt()),
        odd_db: db(odd_power.sqrt()),
        noise_floor_db: floor_rel_db,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn spec(freq: f64, level: f64) -> ToneSpec {
        ToneSpec { freq_hz: freq, level_db: level, duration_s: 2.0 }
    }

    #[test]
    fn a_clean_sine_has_essentially_no_distortion() {
        let s = spec(1000.0, -12.0);
        let x = tone(&s, SR);
        let h = analyze(&x, s.freq_hz, SR, 8, 0);
        assert!(h.thd_percent < 0.01, "clean sine measured {:.4}% THD", h.thd_percent);
        // -12 dBFS peak sine reads as -12 dBFS.
        assert!((h.fundamental_db + 12.0).abs() < 0.2, "{}", h.fundamental_db);
    }

    #[test]
    fn amplitude_is_right_wherever_the_tone_falls_between_bins() {
        // The regression this exists for: reading the peak bin instead of the
        // lobe energy measured a -12 dBFS 1 kHz sine as -12.63 dBFS, because
        // 1 kHz lands at bin 682.67 of a 32k FFT. Frequencies chosen to sit
        // on, near and far from bin centres.
        let bin = SR / FFT as f64;
        for freq in [1000.0, 440.0, 97.3, 5000.0, bin * 700.0, bin * 700.5] {
            for level in [-40.0, -18.0, -6.0, 0.0] {
                let s = ToneSpec { freq_hz: freq, level_db: level, duration_s: 2.0 };
                let h = analyze(&tone(&s, SR), freq, SR, 5, 0);
                assert!(
                    (h.fundamental_db - level).abs() < 0.1,
                    "{freq} Hz at {level} dBFS read as {:.2}",
                    h.fundamental_db
                );
            }
        }
    }

    #[test]
    fn a_cubic_nonlinearity_shows_up_as_third_harmonic() {
        // y = x - x^3/4 is symmetric, so it must produce odd harmonics only.
        let s = spec(1000.0, 0.0);
        let x = tone(&s, SR);
        let y: Vec<f32> = x.iter().map(|v| v - v * v * v / 4.0).collect();
        let h = analyze(&y, s.freq_hz, SR, 8, 0);

        let h2 = h.harmonics_db[0];
        let h3 = h.harmonics_db[1];
        assert!(h3 > h2 + 30.0, "expected odd-dominant: h2={h2:.1} h3={h3:.1}");
        assert!(h.odd_db > h.even_db + 30.0, "odd {:.1} vs even {:.1}", h.odd_db, h.even_db);
    }

    #[test]
    fn an_asymmetric_nonlinearity_shows_up_as_second_harmonic() {
        // y = x + x^2/4 is asymmetric, so even harmonics dominate — the
        // "warmth" side of the distinction this module keeps separate.
        let s = spec(1000.0, 0.0);
        let x = tone(&s, SR);
        let y: Vec<f32> = x.iter().map(|v| v + v * v / 4.0).collect();
        let h = analyze(&y, s.freq_hz, SR, 8, 0);
        assert!(h.even_db > h.odd_db + 20.0, "even {:.1} vs odd {:.1}", h.even_db, h.odd_db);
    }

    #[test]
    fn distortion_grows_with_level_which_is_the_saturation_curve() {
        let mut last = -240.0;
        for level in [-40.0, -30.0, -20.0, -10.0, 0.0] {
            let s = spec(1000.0, level);
            let x = tone(&s, SR);
            // A soft-clipper: distortion rises monotonically with drive.
            let y: Vec<f32> = x.iter().map(|v| v.tanh()).collect();
            let h = analyze(&y, s.freq_hz, SR, 8, 0);
            assert!(
                h.thd_db > last,
                "THD must rise with level: {level} dB gave {:.1} after {last:.1}",
                h.thd_db
            );
            last = h.thd_db;
        }
    }

    #[test]
    fn harmonics_above_nyquist_are_not_invented() {
        // A 9 kHz fundamental at 48 kHz: only the 2nd harmonic (18 kHz) fits
        // under Nyquist. Everything above must read as the floor, not as
        // whatever aliased into its bin.
        let s = spec(9000.0, 0.0);
        let x = tone(&s, SR);
        let y: Vec<f32> = x.iter().map(|v| v.tanh()).collect();
        let h = analyze(&y, s.freq_hz, SR, 8, 0);
        // harmonics_db[0] is the 2nd, [1] the 3rd (27 kHz — above Nyquist).
        for (i, v) in h.harmonics_db.iter().enumerate().skip(1) {
            assert!(
                (*v - h.noise_floor_db).abs() < 1e-9,
                "harmonic {} at {} Hz should be the floor, got {v}",
                i + 2,
                (i + 2) as f64 * 9000.0
            );
        }
    }

    #[test]
    fn a_buffer_shorter_than_the_window_is_not_a_panic() {
        let h = analyze(&[0.0; 128], 1000.0, SR, 8, 0);
        assert!(h.fundamental_db.is_infinite());
        assert_eq!(h.harmonics_db.len(), 7);
    }

    #[test]
    fn skip_discards_the_attack_transient() {
        // First half distorted, second half clean: skipping the first half
        // must report the clean half.
        let s = spec(1000.0, 0.0);
        let x = tone(&s, SR);
        let half = x.len() / 2;
        let mut y = x.clone();
        for v in y.iter_mut().take(half) {
            *v = v.tanh() * 0.5;
        }
        let dirty = analyze(&y, s.freq_hz, SR, 8, 0);
        let clean = analyze(&y, s.freq_hz, SR, 8, half);
        assert!(clean.thd_percent < dirty.thd_percent, "{clean:?} vs {dirty:?}");
        assert!(clean.thd_percent < 0.01);
    }
}
