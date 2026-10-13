//! Biquad filter with multiple filter types.
//!
//! Ported from `CloudSeedCore` Biquad.h/.cpp (MIT, Ghost Note Audio).
//! Uses earlevel.com formulas for coefficient computation.

use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    LowPass6db,
    HighPass6db,
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peak,
    LowShelf,
    HighShelf,
}

pub struct Biquad {
    // Coefficients
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    // State
    x1: f64,
    x2: f64,
    y: f64,
    y1: f64,
    y2: f64,
    // Parameters
    pub filter_type: FilterType,
    pub frequency: f64,
    fs: f64,
    fs_inv: f64,
    gain_db: f64,
    gain: f64,
    q: f64,
}

impl Biquad {
    #[must_use]
    pub fn new(filter_type: FilterType, sample_rate: f64) -> Self {
        let mut bq = Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y: 0.0,
            y1: 0.0,
            y2: 0.0,
            filter_type,
            frequency: sample_rate * 0.25,
            fs: sample_rate,
            fs_inv: 1.0 / sample_rate,
            gain_db: 0.0,
            gain: 1.0,
            q: 0.5,
        };
        bq.update();
        bq
    }

    pub fn set_sample_rate(&mut self, fs: f64) {
        self.fs = fs;
        self.fs_inv = 1.0 / fs;
        self.update();
    }

    pub fn set_gain_db(&mut self, db: f64) {
        let db = db.clamp(-60.0, 60.0);
        self.gain_db = db;
        self.gain = 10.0_f64.powf(db / 20.0);
    }

    pub fn set_gain(&mut self, value: f64) {
        let value = value.clamp(0.001, 1000.0);
        self.gain = value;
        self.gain_db = value.log10() * 20.0;
    }

    pub const fn set_q(&mut self, value: f64) {
        self.q = value.max(0.001);
    }

    pub fn update(&mut self) {
        let fc = self.frequency;
        // Linear gain of half the requested dB, the RBJ shelving/peaking
        // convention. Sign is handled by picking the boost or cut form.
        let v = 10.0_f64.powf(self.gain_db.abs() / 20.0);
        let k = (PI * fc * self.fs_inv).tan();
        let q = self.q;
        let boost = self.gain_db >= 0.0;

        let c = match self.filter_type {
            FilterType::LowPass6db => Coeffs::one_pole_low(2.0 * PI * fc * self.fs_inv),
            FilterType::HighPass6db => Coeffs::one_pole_high(2.0 * PI * fc * self.fs_inv),
            FilterType::LowPass => Coeffs::low_pass(k, q),
            FilterType::HighPass => Coeffs::high_pass(k, q),
            FilterType::BandPass => Coeffs::band_pass(k, q),
            FilterType::Notch => Coeffs::notch(k, q),
            FilterType::Peak => Coeffs::peak(k, q, v, boost),
            FilterType::LowShelf => Coeffs::low_shelf(k, v, boost),
            FilterType::HighShelf => Coeffs::high_shelf(k, v, boost),
        };

        self.b0 = c.b0;
        self.b1 = c.b1;
        self.b2 = c.b2;
        self.a1 = c.a1;
        self.a2 = c.a2;
    }

    #[inline]
    pub fn tick(&mut self, x: f64) -> f64 {
        self.y = audiocore_dsp::denormal::flush(
            self.a2.mul_add(
                -self.y2,
                self.a1.mul_add(
                    -self.y1,
                    self.b2
                        .mul_add(self.x2, self.b0.mul_add(x, self.b1 * self.x1)),
                ),
            ),
        );
        self.x2 = self.x1;
        self.y2 = self.y1;
        self.x1 = x;
        self.y1 = self.y;
        self.y
    }

    pub const fn clear(&mut self) {
        self.y = 0.0;
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// One filter's difference-equation coefficients.
///
/// `update` was a single 110-line match that wrote five `self` fields in
/// every arm; the designs now each return their own `Coeffs`, so a
/// mistake in one form cannot leave another's stale value behind.
struct Coeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Coeffs {
    /// 6 dB/oct lowpass. `a1` is negated for the difference-equation
    /// convention used by `tick`.
    fn one_pole_low(w: f64) -> Self {
        let alpha = (-w).exp();
        Self {
            b0: 1.0 + alpha,
            b1: 0.0,
            b2: 0.0,
            a1: -alpha,
            a2: 0.0,
        }
    }

    /// 6 dB/oct highpass.
    fn one_pole_high(w: f64) -> Self {
        let alpha = (-w).exp();
        Self {
            b0: alpha,
            b1: -alpha,
            b2: 0.0,
            a1: -alpha,
            a2: 0.0,
        }
    }

    /// The denominator shared by the four second-order RBJ forms.
    fn rbj_norm(k: f64, q: f64) -> f64 {
        1.0 / k.mul_add(k, 1.0 + k / q)
    }

    fn low_pass(k: f64, q: f64) -> Self {
        let norm = Self::rbj_norm(k, q);
        let b0 = k * k * norm;
        Self {
            b0,
            b1: 2.0 * b0,
            b2: b0,
            a1: 2.0 * k.mul_add(k, -1.0) * norm,
            a2: k.mul_add(k, 1.0 - k / q) * norm,
        }
    }

    fn high_pass(k: f64, q: f64) -> Self {
        let norm = Self::rbj_norm(k, q);
        Self {
            b0: norm,
            b1: -2.0 * norm,
            b2: norm,
            a1: 2.0 * k.mul_add(k, -1.0) * norm,
            a2: k.mul_add(k, 1.0 - k / q) * norm,
        }
    }

    fn band_pass(k: f64, q: f64) -> Self {
        let norm = Self::rbj_norm(k, q);
        let b0 = k / q * norm;
        Self {
            b0,
            b1: 0.0,
            b2: -b0,
            a1: 2.0 * k.mul_add(k, -1.0) * norm,
            a2: k.mul_add(k, 1.0 - k / q) * norm,
        }
    }

    fn notch(k: f64, q: f64) -> Self {
        let norm = Self::rbj_norm(k, q);
        let b0 = k.mul_add(k, 1.0) * norm;
        let b1 = 2.0 * k.mul_add(k, -1.0) * norm;
        Self {
            b0,
            b1,
            b2: b0,
            a1: b1,
            a2: k.mul_add(k, 1.0 - k / q) * norm,
        }
    }

    /// Peaking bell. A cut is the boost form with `v` moved from the
    /// numerator to the denominator.
    fn peak(k: f64, q: f64, v: f64, boost: bool) -> Self {
        let (num_gain, den_gain) = if boost {
            (v / q, 1.0 / q)
        } else {
            (1.0 / q, v / q)
        };
        let norm = 1.0 / k.mul_add(k, den_gain.mul_add(k, 1.0));
        let b1 = 2.0 * k.mul_add(k, -1.0) * norm;
        Self {
            b0: k.mul_add(k, num_gain.mul_add(k, 1.0)) * norm,
            b1,
            b2: k.mul_add(k, num_gain.mul_add(-k, 1.0)) * norm,
            a1: b1,
            a2: k.mul_add(k, den_gain.mul_add(-k, 1.0)) * norm,
        }
    }

    fn low_shelf(k: f64, v: f64, boost: bool) -> Self {
        let sqrt2 = std::f64::consts::SQRT_2;
        let root = (2.0 * v).sqrt();
        if boost {
            let norm = 1.0 / k.mul_add(k, sqrt2.mul_add(k, 1.0));
            Self {
                b0: (v * k).mul_add(k, root.mul_add(k, 1.0)) * norm,
                b1: 2.0 * (v * k).mul_add(k, -1.0) * norm,
                b2: (v * k).mul_add(k, root.mul_add(-k, 1.0)) * norm,
                a1: 2.0 * k.mul_add(k, -1.0) * norm,
                a2: k.mul_add(k, sqrt2.mul_add(-k, 1.0)) * norm,
            }
        } else {
            let norm = 1.0 / (v * k).mul_add(k, root.mul_add(k, 1.0));
            Self {
                b0: k.mul_add(k, sqrt2.mul_add(k, 1.0)) * norm,
                b1: 2.0 * k.mul_add(k, -1.0) * norm,
                b2: k.mul_add(k, sqrt2.mul_add(-k, 1.0)) * norm,
                a1: 2.0 * (v * k).mul_add(k, -1.0) * norm,
                a2: (v * k).mul_add(k, root.mul_add(-k, 1.0)) * norm,
            }
        }
    }

    fn high_shelf(k: f64, v: f64, boost: bool) -> Self {
        let sqrt2 = std::f64::consts::SQRT_2;
        let root = (2.0 * v).sqrt();
        if boost {
            let norm = 1.0 / k.mul_add(k, sqrt2.mul_add(k, 1.0));
            Self {
                b0: k.mul_add(k, root.mul_add(k, v)) * norm,
                b1: 2.0 * k.mul_add(k, -v) * norm,
                b2: k.mul_add(k, root.mul_add(-k, v)) * norm,
                a1: 2.0 * k.mul_add(k, -1.0) * norm,
                a2: k.mul_add(k, sqrt2.mul_add(-k, 1.0)) * norm,
            }
        } else {
            let norm = 1.0 / k.mul_add(k, root.mul_add(k, v));
            Self {
                b0: k.mul_add(k, sqrt2.mul_add(k, 1.0)) * norm,
                b1: 2.0 * k.mul_add(k, -1.0) * norm,
                b2: k.mul_add(k, sqrt2.mul_add(-k, 1.0)) * norm,
                a1: 2.0 * k.mul_add(k, -v) * norm,
                a2: k.mul_add(k, root.mul_add(-k, v)) * norm,
            }
        }
    }
}
