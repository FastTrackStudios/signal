//! Measuring one element of a mix — a kick, a snare, a lead vocal.
//!
//! The rest of this crate compares *our processor* against *a reference
//! plugin* on stimulus we generated. This module answers a different
//! question: given a separated stem from a finished record, what is it
//! actually doing? Level, pitch, tone, fullness, dynamics — the numbers
//! an engineer would need to aim a mix at a reference.
//!
//! # Why not reuse [`crate::loudness`] wholesale
//!
//! [`crate::loudness::loudness_lufs`] is deliberately ungated, which is
//! right for continuous stimulus and wrong here: a vocal stem is mostly
//! silence between phrases, and averaging that in reports a vocal
//! several LU quieter than it sounds. Everything in this module measures
//! **only where the element is sounding** ([`GATE_DB`]).
//!
//! [`crate::loudness::band_levels_db`] is likewise eight octave bands
//! from 62.5 Hz to 8 kHz. That is too coarse to describe an EQ curve, and
//! it misses both ends that matter most — a kick fundamental sits near
//! 50 Hz and vocal air lives above 10 kHz. [`band_profile`] uses
//! sixth-octave bands across the full audible range instead.
//!
//! # Why there is no THD here
//!
//! Total harmonic distortion needs a known input: send a sine, measure
//! what comes back. Given a finished record there is no way to separate
//! "harmonics the saturator added" from "harmonics the instrument always
//! had" — and a *separated* stem carries the model's artefacts on top of
//! both. [`Fullness`] measures how thoroughly an element occupies the
//! spectrum instead, which is the question saturation was a proxy for
//! and is honestly measurable.

use realfft::RealFftPlanner;

/// How far below the loudest frame still counts as the element sounding.
///
/// Silence between hits and between phrases has to be excluded, or every
/// measurement drifts toward whatever the gaps contain.
pub const GATE_DB: f64 = -40.0;

/// Analysis frame for the gate, in milliseconds.
pub const FRAME_MS: f64 = 50.0;

/// Bands per octave in [`band_profile`].
pub const BANDS_PER_OCTAVE: f64 = 6.0;

/// Lowest and highest band centre, in hertz.
pub const BAND_LO_HZ: f64 = 20.0;
pub const BAND_HI_HZ: f64 = 20_000.0;

/// Transform size for spectral measurements.
const NFFT: usize = 8192;

/// The centre frequency of every band in [`band_profile`], low to high.
#[must_use]
pub fn band_centres() -> Vec<f64> {
    let step = 2.0_f64.powf(1.0 / BANDS_PER_OCTAVE);
    let mut out = Vec::new();
    let mut f = BAND_LO_HZ;
    while f <= BAND_HI_HZ {
        out.push(f);
        f *= step;
    }
    out
}

/// How thoroughly an element fills the spectrum.
///
/// Together these say whether something is a narrow tone, a broadband
/// wash, or a focused hit with a tail — which is what "how saturated is
/// it" was really asking, and unlike THD it is measurable from a record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fullness {
    /// Spectral flatness, 0 to 1. Near 0 is tonal (a clean sine), near 1
    /// is noise-like (a snare, heavy saturation, cymbals).
    pub flatness: f64,
    /// Centre of mass of the spectrum, in hertz. The single number that
    /// best tracks "brightness".
    pub centroid_hz: f64,
    /// Spread about the centroid, in octaves. A narrow kick is under an
    /// octave; a full-range element is several.
    pub spread_octaves: f64,
    /// Fraction of bands within 20 dB of the loudest band. High means
    /// energy is spread across the spectrum rather than concentrated.
    pub occupancy: f64,
    /// Frequency below which 85% of the energy sits.
    pub rolloff_hz: f64,
}

/// Everything measured about one element.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementProfile {
    /// K-weighted loudness over sounding frames only, in LUFS.
    pub loudness_lufs: f64,
    /// Peak minus RMS over sounding frames, in dB — the dynamic range
    /// left after whatever compression was applied.
    pub crest_db: f64,
    /// Strongest low-frequency partial, where one is clear. `None` for
    /// elements with no stable pitch, which is the honest answer for a
    /// snare or a cymbal.
    pub fundamental_hz: Option<f64>,
    /// Sixth-octave EQ profile: `(centre_hz, dB)`, normalised so the
    /// mean across bands is 0 dB. Comparable between elements and
    /// between songs regardless of how loud either is.
    pub profile: Vec<(f64, f64)>,
    pub fullness: Fullness,
}

/// Measure an element.
///
/// Returns `None` for a stem that never rises above the gate — which is
/// what an absent instrument looks like, and must not be reported as a
/// measurement of zero.
#[must_use]
pub fn profile(x: &[f32], sample_rate: f64) -> Option<ElementProfile> {
    let voiced = sounding_samples(x, sample_rate)?;
    let spectrum = power_spectrum(&voiced)?;
    let profile = bands_from_spectrum(&spectrum, sample_rate);

    Some(ElementProfile {
        loudness_lufs: crate::loudness::loudness_lufs(&voiced, sample_rate),
        crest_db: crest_db_of(&voiced)?,
        fundamental_hz: fundamental_from_spectrum(&spectrum, sample_rate),
        fullness: fullness_from(&spectrum, &profile, sample_rate),
        profile,
    })
}

/// Sixth-octave EQ profile of a signal, normalised to its own mean.
#[must_use]
pub fn band_profile(x: &[f32], sample_rate: f64) -> Option<Vec<(f64, f64)>> {
    let spectrum = power_spectrum(x)?;
    Some(bands_from_spectrum(&spectrum, sample_rate))
}

/// Concatenate the frames where the signal is actually sounding.
fn sounding_samples(x: &[f32], sample_rate: f64) -> Option<Vec<f32>> {
    let frame = ((sample_rate * FRAME_MS / 1000.0) as usize).max(1);
    if x.len() < frame {
        return None;
    }
    let frames: Vec<&[f32]> = x.chunks_exact(frame).collect();
    let rms: Vec<f64> = frames.iter().map(|f| rms_of(f)).collect();
    let loudest = rms.iter().copied().fold(0.0_f64, f64::max);
    if loudest <= 0.0 {
        return None;
    }
    let threshold = loudest * 10.0_f64.powf(GATE_DB / 20.0);
    let kept: Vec<f32> = frames
        .iter()
        .zip(&rms)
        .filter(|(_, r)| **r > threshold)
        .flat_map(|(f, _)| f.iter().copied())
        .collect();
    (!kept.is_empty()).then_some(kept)
}

fn rms_of(x: &[f32]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / x.len() as f64).sqrt()
}

fn crest_db_of(x: &[f32]) -> Option<f64> {
    let peak = x.iter().fold(0.0_f64, |m, &s| m.max((s as f64).abs()));
    let rms = rms_of(x);
    (peak > 0.0 && rms > 0.0).then(|| 20.0 * (peak / rms).log10())
}

/// Averaged power per FFT bin, Hann-windowed.
fn power_spectrum(x: &[f32]) -> Option<Vec<f64>> {
    if x.len() < NFFT {
        return None;
    }
    let window: Vec<f64> = (0..NFFT)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / NFFT as f64).cos())
        .collect();

    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(NFFT);
    let mut scratch = fft.make_output_vec();

    let mut acc = vec![0.0_f64; scratch.len()];
    let mut frames = 0usize;
    for chunk in x.chunks_exact(NFFT) {
        let mut buf: Vec<f64> = chunk
            .iter()
            .zip(&window)
            .map(|(&s, w)| s as f64 * w)
            .collect();
        if fft.process(&mut buf, &mut scratch).is_err() {
            return None;
        }
        for (a, c) in acc.iter_mut().zip(&scratch) {
            *a += c.norm_sqr();
        }
        frames += 1;
    }
    (frames > 0).then(|| acc.iter().map(|p| p / frames as f64).collect())
}

/// Fold an FFT power spectrum into sixth-octave bands, in dB, normalised
/// so the mean across finite bands is 0.
///
/// Bands are integrated as power **density** with each bin weighted by
/// how much of it falls inside the band. Rounding bin edges outward
/// instead over-counts partial bins by an amount that depends on the bin
/// grid, which makes the same signal measure differently at 44.1 and
/// 48 kHz — and this corpus mixes both.
fn bands_from_spectrum(power: &[f64], sample_rate: f64) -> Vec<(f64, f64)> {
    let bin_hz = sample_rate / NFFT as f64;
    let nyquist = sample_rate / 2.0;
    let step = 2.0_f64.powf(0.5 / BANDS_PER_OCTAVE);

    let mut out: Vec<(f64, f64)> = Vec::new();
    for centre in band_centres() {
        let lo = centre / step;
        if lo >= nyquist {
            out.push((centre, f64::NEG_INFINITY));
            continue;
        }
        let hi = (centre * step).min(nyquist);
        let first = ((lo / bin_hz - 0.5).floor().max(0.0)) as usize;
        let last = (((hi / bin_hz) + 0.5).ceil() as usize).min(power.len().saturating_sub(1));

        let mut acc = 0.0;
        for (k, p) in power.iter().enumerate().take(last + 1).skip(first) {
            let klo = k as f64 * bin_hz - bin_hz / 2.0;
            let khi = k as f64 * bin_hz + bin_hz / 2.0;
            acc += p * (hi.min(khi) - lo.max(klo)).max(0.0);
        }
        let density = acc / (hi - lo).max(1e-9);
        out.push((centre, 10.0 * (density + 1e-30).log10()));
    }

    let finite: Vec<f64> = out
        .iter()
        .map(|(_, d)| *d)
        .filter(|d| d.is_finite())
        .collect();
    if finite.is_empty() {
        return out;
    }
    let mean = finite.iter().sum::<f64>() / finite.len() as f64;
    out.iter().map(|(f, d)| (*f, d - mean)).collect()
}

/// The strongest partial below 400 Hz, refined by parabolic
/// interpolation across neighbouring bins.
///
/// Restricted to the low end because this exists to answer "what note is
/// the kick / the tom", not to track a melody. Returns `None` when no
/// bin clearly dominates, which is the right answer for a snare.
fn fundamental_from_spectrum(power: &[f64], sample_rate: f64) -> Option<f64> {
    const MAX_HZ: f64 = 400.0;
    const MIN_HZ: f64 = 25.0;
    let bin_hz = sample_rate / NFFT as f64;
    let lo = (MIN_HZ / bin_hz).ceil() as usize;
    let hi = ((MAX_HZ / bin_hz).floor() as usize).min(power.len().saturating_sub(2));
    if lo + 1 >= hi {
        return None;
    }

    let (peak, &pv) = power[lo..=hi]
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, v)| (i + lo, v))?;

    // A peak has to stand clear of the low-band average, or it is just
    // the loudest bin of a noise floor.
    let mean: f64 = power[lo..=hi].iter().sum::<f64>() / (hi - lo + 1) as f64;
    if mean <= 0.0 || pv / mean < 4.0 {
        return None;
    }

    let (a, b, c) = (power[peak - 1], pv, power[peak + 1]);
    let denom = a - 2.0 * b + c;
    let shift = if denom.abs() > f64::EPSILON {
        0.5 * (a - c) / denom
    } else {
        0.0
    };
    Some((peak as f64 + shift.clamp(-0.5, 0.5)) * bin_hz)
}

fn fullness_from(power: &[f64], profile: &[(f64, f64)], sample_rate: f64) -> Fullness {
    let bin_hz = sample_rate / NFFT as f64;
    let usable: Vec<(f64, f64)> = power
        .iter()
        .enumerate()
        .skip(1)
        .map(|(k, p)| (k as f64 * bin_hz, *p))
        .filter(|(f, _)| *f >= BAND_LO_HZ && *f <= BAND_HI_HZ)
        .collect();

    let total: f64 = usable.iter().map(|(_, p)| p).sum();
    if total <= 0.0 || usable.is_empty() {
        return Fullness {
            flatness: 0.0,
            centroid_hz: 0.0,
            spread_octaves: 0.0,
            occupancy: 0.0,
            rolloff_hz: 0.0,
        };
    }

    let centroid = usable.iter().map(|(f, p)| f * p).sum::<f64>() / total;

    // Geometric over arithmetic mean — the standard flatness measure.
    let n = usable.len() as f64;
    let log_mean = usable.iter().map(|(_, p)| (p + 1e-30).ln()).sum::<f64>() / n;
    let flatness = (log_mean.exp() / (total / n)).clamp(0.0, 1.0);

    let spread = if centroid > 0.0 {
        let var = usable
            .iter()
            .map(|(f, p)| {
                let d = (f / centroid).log2();
                d * d * p
            })
            .sum::<f64>()
            / total;
        var.sqrt()
    } else {
        0.0
    };

    let mut cum = 0.0;
    let mut rolloff = usable.last().map_or(0.0, |(f, _)| *f);
    for (f, p) in &usable {
        cum += p;
        if cum >= 0.85 * total {
            rolloff = *f;
            break;
        }
    }

    let finite: Vec<f64> = profile
        .iter()
        .map(|(_, d)| *d)
        .filter(|d| d.is_finite())
        .collect();
    let occupancy = if finite.is_empty() {
        0.0
    } else {
        let peak = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        finite.iter().filter(|d| **d >= peak - 20.0).count() as f64 / finite.len() as f64
    };

    Fullness {
        flatness,
        centroid_hz: centroid,
        spread_octaves: spread,
        occupancy,
        rolloff_hz: rolloff,
    }
}

/// How far `a` sits above `b` in each band, in dB.
///
/// Positive means `a` dominates that band. Both profiles are already
/// normalised to their own mean, so this compares *shape* — where each
/// element puts its energy — rather than which is louder overall.
#[must_use]
pub fn band_margin(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    a.iter()
        .zip(b)
        .map(|((f, x), (_, y))| (*f, x - y))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn sine(freq: f64, secs: f64, amp: f32) -> Vec<f32> {
        let n = (SR * secs) as usize;
        (0..n)
            .map(|i| amp * (2.0 * std::f64::consts::PI * freq * i as f64 / SR).sin() as f32)
            .collect()
    }

    fn noise(secs: f64, amp: f32) -> Vec<f32> {
        let n = (SR * secs) as usize;
        let mut state = 0x1234_5678_u32;
        (0..n)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((state >> 8) as f32 / 8_388_608.0 - 1.0) * amp
            })
            .collect()
    }

    #[test]
    fn a_kick_like_tone_reports_its_fundamental() {
        // 55 Hz is squarely in kick territory.
        let p = profile(&sine(55.0, 3.0, 0.5), SR).unwrap();
        let f = p.fundamental_hz.expect("a pure tone has a fundamental");
        assert!((f - 55.0).abs() < 2.0, "got {f} Hz");
    }

    #[test]
    fn noise_has_no_fundamental() {
        // A snare must not be handed a confident pitch.
        let p = profile(&noise(3.0, 0.3), SR).unwrap();
        assert!(p.fundamental_hz.is_none(), "got {:?}", p.fundamental_hz);
    }

    #[test]
    fn flatness_separates_a_tone_from_noise() {
        let tone = profile(&sine(1000.0, 3.0, 0.5), SR).unwrap();
        let hiss = profile(&noise(3.0, 0.3), SR).unwrap();
        assert!(
            tone.fullness.flatness < 0.05,
            "tone {:?}",
            tone.fullness.flatness
        );
        assert!(
            hiss.fullness.flatness > 0.3,
            "noise {:?}",
            hiss.fullness.flatness
        );
        assert!(hiss.fullness.flatness > tone.fullness.flatness);
    }

    #[test]
    fn occupancy_separates_narrow_from_broadband() {
        // The "how full is it" question, which replaced THD.
        let tone = profile(&sine(1000.0, 3.0, 0.5), SR).unwrap();
        let hiss = profile(&noise(3.0, 0.3), SR).unwrap();
        assert!(
            hiss.fullness.occupancy > tone.fullness.occupancy * 3.0,
            "noise {:.2} vs tone {:.2}",
            hiss.fullness.occupancy,
            tone.fullness.occupancy
        );
    }

    #[test]
    fn centroid_tracks_brightness() {
        let low = profile(&sine(100.0, 3.0, 0.5), SR).unwrap();
        let high = profile(&sine(6000.0, 3.0, 0.5), SR).unwrap();
        assert!(
            low.fullness.centroid_hz < 400.0,
            "{}",
            low.fullness.centroid_hz
        );
        assert!(
            high.fullness.centroid_hz > 3000.0,
            "{}",
            high.fullness.centroid_hz
        );
    }

    #[test]
    fn a_sine_has_the_textbook_crest_factor() {
        let p = profile(&sine(1000.0, 3.0, 0.5), SR).unwrap();
        assert!((p.crest_db - 3.01).abs() < 0.1, "got {}", p.crest_db);
    }

    /// The reason this module does not reuse `loudness::loudness_lufs`:
    /// silence between hits would drag every measurement down.
    #[test]
    fn silence_between_hits_is_gated_out() {
        let tone = sine(1000.0, 2.0, 0.5);
        let mut padded = tone.clone();
        padded.extend(std::iter::repeat(0.0).take(tone.len() * 2));

        let a = profile(&tone, SR).unwrap();
        let b = profile(&padded, SR).unwrap();
        assert!(
            (a.loudness_lufs - b.loudness_lufs).abs() < 0.5,
            "gate failed: {} vs {} LUFS",
            a.loudness_lufs,
            b.loudness_lufs
        );
        assert!((a.crest_db - b.crest_db).abs() < 0.2);
    }

    #[test]
    fn a_silent_stem_reports_nothing_rather_than_zero() {
        // An absent instrument is not an instrument measuring 0.
        assert!(profile(&vec![0.0; 48_000], SR).is_none());
        assert!(profile(&[], SR).is_none());
    }

    #[test]
    fn the_profile_covers_sub_and_air() {
        // The gap in the existing octave bands: a kick fundamental near
        // 50 Hz and vocal air above 10 kHz both fall outside 62.5 Hz-8 kHz.
        let c = band_centres();
        assert!(c[0] <= 20.0, "lowest band {}", c[0]);
        assert!(
            *c.last().unwrap() > 15_000.0,
            "highest band {}",
            c.last().unwrap()
        );
        assert!(c.len() > 50, "expected ~60 bands, got {}", c.len());
        assert!(
            (c[6] / c[0] - 2.0).abs() < 1e-6,
            "six bands should be an octave"
        );
    }

    #[test]
    fn margin_is_positive_where_the_first_element_leads() {
        let a = vec![(100.0, 6.0), (200.0, 0.0), (400.0, -6.0)];
        let b = vec![(100.0, 0.0), (200.0, 0.0), (400.0, 0.0)];
        let m = band_margin(&a, &b);
        assert_eq!(m[0].1, 6.0);
        assert_eq!(m[1].1, 0.0);
        assert_eq!(m[2].1, -6.0);
    }
}
