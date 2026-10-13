//! Validated filter designs, independent of processing history.

use crate::design::biquad::Coeffs;
use crate::math::zpk::Complex;
use crate::runtime::band::Band;
use crate::{Error, FilterType};

/// Normalized coefficients for H(z) = B(z) / (1 + a1/z + a2/z²).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiquadCoefficients {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl BiquadCoefficients {
    /// Normalize and validate a section. Numerator and denominator are in
    /// ascending powers of z^-1; all denominator poles must be stable.
    ///
    /// # Errors
    /// Returns an error if the supplied configuration or processing limits are invalid.
    pub fn new(numerator: [f64; 3], denominator: [f64; 3]) -> Result<Self, Error> {
        let [a0, a1, a2] = denominator;
        let [b0, b1, b2] = numerator;
        if a0 == 0.0 || !numerator.iter().chain(&denominator).all(|x| x.is_finite()) {
            return Err(Error::InvalidCoefficients);
        }
        let inv = a0.recip();
        let c = Self {
            b0: b0 * inv,
            b1: b1 * inv,
            b2: b2 * inv,
            a1: a1 * inv,
            a2: a2 * inv,
        };
        if !c.normalized().iter().all(|x| x.is_finite()) {
            return Err(Error::InvalidCoefficients);
        }
        // Jury criterion for a real monic second-order denominator. Also
        // covers first-order sections (a2 = 0).
        if c.a2.abs() >= 1.0 || 1.0 + c.a1 + c.a2 <= 0.0 || 1.0 - c.a1 + c.a2 <= 0.0 {
            return Err(Error::UnstableFilter);
        }
        Ok(c)
    }

    pub(crate) fn from_raw([a0, a1, a2, b0, b1, b2]: Coeffs) -> Result<Self, Error> {
        Self::new([b0, b1, b2], [a0, a1, a2])
    }

    #[must_use]
    pub const fn numerator(self) -> [f64; 3] {
        [self.b0, self.b1, self.b2]
    }

    #[must_use]
    pub const fn denominator(self) -> [f64; 3] {
        [1.0, self.a1, self.a2]
    }

    /// [b0, b1, b2, a1, a2], with a0 = 1.
    #[must_use]
    pub const fn normalized(self) -> [f64; 5] {
        [self.b0, self.b1, self.b2, self.a1, self.a2]
    }

    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "complex arithmetic operates on f64 and cannot panic"
    )]
    pub fn response(self, radians: f64) -> Complex {
        let z = Complex::from_polar(1.0, -radians);
        let z2 = z * z;
        (Complex::new(self.b0, 0.0) + z * self.b1 + z2 * self.b2)
            / (Complex::ONE + z * self.a1 + z2 * self.a2)
    }
}

/// One validated digital filter. Cheap to share off-thread, with no history.
#[derive(Debug, Clone)]
pub struct PreparedFilter {
    sample_rate: f64,
    coefficients: crate::inline::InlineVec<BiquadCoefficients>,
    // Retain unnormalized values to preserve the existing division/FMA order
    // when loading the established runtime implementations.
    raw: crate::inline::InlineVec<Coeffs>,
    direct_form_one: bool,
}

impl PreparedFilter {
    /// Build a cascade from legacy `[a0, a1, a2, b0, b1, b2]` sections.
    ///
    /// # Errors
    /// Returns an error for an invalid sample rate, excessive section count, nonfinite coefficients, or unstable poles.
    pub fn from_sections(sample_rate: f64, sections: &[Coeffs]) -> Result<Self, Error> {
        validate_sample_rate(sample_rate)?;
        if sections.len() > crate::runtime::band::MAX_SECTIONS {
            return Err(Error::TooManySections);
        }
        let coefficients = sections
            .iter()
            .copied()
            .map(BiquadCoefficients::from_raw)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            sample_rate,
            coefficients,
            raw: sections.iter().copied().collect(),
            direct_form_one: false,
        })
    }

    /// Build a cascade from named, already validated coefficients.
    ///
    /// # Errors
    /// Returns an error for an invalid sample rate or excessive section count.
    pub fn from_coefficients(
        sample_rate: f64,
        coefficients: &[BiquadCoefficients],
    ) -> Result<Self, Error> {
        let raw: crate::inline::InlineVec<_> = coefficients
            .iter()
            .map(|c| {
                let [b0, b1, b2, a1, a2] = c.normalized();
                [1.0, a1, a2, b0, b1, b2]
            })
            .collect();
        Self::from_sections(sample_rate, &raw)
    }

    pub(crate) fn design(
        shape: FilterType,
        hz: f64,
        gain: f64,
        q: f64,
        order: usize,
        fraction: f64,
        sample_rate: f64,
    ) -> Result<Self, Error> {
        validate_sample_rate(sample_rate)?;
        validate_frequency(hz, sample_rate)?;
        if !q.is_finite() || q <= 0.0 {
            return Err(Error::InvalidQ);
        }
        if !gain.is_finite() {
            return Err(Error::InvalidGain);
        }
        let mut raw = if order == 0 {
            crate::inline::InlineVec::new()
        } else {
            crate::design::design_filter(shape, hz, q, gain, sample_rate, order)
        };
        if fraction > 0.0 {
            raw.extend(crate::design::fractional::sections(
                hz,
                fraction,
                sample_rate,
                shape == FilterType::Highpass,
            ));
        }
        let mut result = Self::from_sections(sample_rate, &raw)?;
        result.direct_form_one = shape == FilterType::Peak;
        Ok(result)
    }

    #[must_use]
    pub const fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    #[must_use]
    pub fn coefficients(&self) -> &[BiquadCoefficients] {
        &self.coefficients
    }

    pub(crate) fn raw(&self) -> &[Coeffs] {
        &self.raw
    }
    pub(crate) const fn direct_form_one(&self) -> bool {
        self.direct_form_one
    }

    /// Complex static response at a frequency in Hz.
    ///
    /// # Errors
    /// Returns `InvalidFrequency` for a nonfinite frequency or one outside DC through Nyquist.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "complex arithmetic operates on f64 and cannot panic"
    )]
    pub fn response(&self, hz: f64) -> Result<Complex, Error> {
        if !hz.is_finite() || !(0.0..=self.sample_rate * 0.5).contains(&hz) {
            return Err(Error::InvalidFrequency);
        }
        let w = core::f64::consts::TAU * hz / self.sample_rate;
        Ok(self
            .coefficients
            .iter()
            .fold(Complex::ONE, |h, c| h * c.response(w)))
    }

    ///
    /// # Errors
    /// Returns `InvalidFrequency` for a frequency outside the prepared response domain.
    pub fn magnitude_db(&self, hz: f64) -> Result<f64, Error> {
        Ok(20.0 * self.response(hz)?.mag().max(1e-30).log10())
    }

    ///
    /// # Errors
    /// Returns `InvalidFrequency` for a frequency outside the prepared response domain.
    pub fn phase_radians(&self, hz: f64) -> Result<f64, Error> {
        Ok(self.response(hz)?.arg())
    }

    ///
    /// # Errors
    /// Returns `InvalidFrequency` for a frequency outside the prepared response domain.
    pub fn group_delay_samples(&self, hz: f64) -> Result<f64, Error> {
        self.response(hz)?;
        Ok(crate::runtime::response::compute_group_delay(
            &self.raw,
            hz,
            self.sample_rate,
        ))
    }

    /// Fill caller-owned storage without allocation. Invalid input leaves it unchanged.
    ///
    /// # Errors
    /// Returns an error for unequal slice lengths or any invalid frequency. Output is unchanged on failure.
    pub fn magnitude_response(&self, hz: &[f64], output: &mut [f64]) -> Result<(), Error> {
        if hz.len() != output.len() {
            return Err(Error::ChannelLengthMismatch);
        }
        for &f in hz {
            self.response(f)?;
        }
        for (&f, value) in hz.iter().zip(output) {
            *value = self.magnitude_db(f)?;
        }
        Ok(())
    }
}

/// Single-filter processor with private history and allocation-free installation.
pub struct FilterProcessor {
    band: Band,
}

impl Default for FilterProcessor {
    fn default() -> Self {
        let mut band = Band::new();
        band.snap_bypass();
        Self { band }
    }
}

impl FilterProcessor {
    #[must_use]
    pub fn new(filter: &PreparedFilter) -> Self {
        let mut band = Band::new();
        band.install(filter);
        band.snap_bypass();
        Self { band }
    }

    /// Validate and design a bounded single-filter update without allocation.
    /// Failure leaves the current filter and history unchanged.
    ///
    /// # Errors
    /// Returns a design error for invalid parameters or unstable coefficients, leaving the processor unchanged.
    pub fn configure(&mut self, filter: crate::Filter, sample_rate: f64) -> Result<(), Error> {
        let prepared = filter.prepare(sample_rate)?;
        self.install(&prepared);
        Ok(())
    }

    /// Install coefficients with a 5 ms crossfade from the previous state.
    pub fn install(&mut self, filter: &PreparedFilter) {
        self.band.install(filter);
    }
    pub fn reset(&mut self) {
        self.band.reset();
        self.band.snap_bypass();
    }

    pub fn process_mono(&mut self, samples: &mut [f64]) {
        for x in samples {
            *x = self.band.tick(*x, 0);
        }
    }

    ///
    /// # Errors
    /// Returns an error for unequal channel lengths or a block larger than prepared capacity. Audio and history are unchanged on failure.
    pub fn process_stereo(&mut self, left: &mut [f64], right: &mut [f64]) -> Result<(), Error> {
        if left.len() != right.len() {
            return Err(Error::ChannelLengthMismatch);
        }
        for (l, r) in left.iter_mut().zip(right) {
            let [a, b] = self.process_frame([*l, *r]);
            *l = a;
            *r = b;
        }
        Ok(())
    }

    /// Advance smoothing once for a complete stereo frame.
    #[must_use]
    pub fn process_frame(&mut self, [left, right]: [f64; 2]) -> [f64; 2] {
        self.band.advance_frame();
        [
            self.band.process_channel(left, 0),
            self.band.process_channel(right, 1),
        ]
    }

    pub const fn set_enabled(&mut self, enabled: bool) {
        self.band.enabled = enabled;
    }

    #[must_use]
    pub fn process_sample(&mut self, sample: f64) -> f64 {
        self.band.tick(sample, 0)
    }
}

pub fn validate_sample_rate(rate: f64) -> Result<(), Error> {
    if rate.is_finite() && rate > 0.0 {
        Ok(())
    } else {
        Err(Error::InvalidSampleRate)
    }
}

pub fn validate_frequency(hz: f64, rate: f64) -> Result<(), Error> {
    if hz.is_finite() && hz > 0.0 && hz < rate * 0.5 {
        Ok(())
    } else {
        Err(Error::InvalidFrequency)
    }
}
