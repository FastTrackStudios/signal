//! Biquad filter — replaces Airwindows' flat `double biquad[15]` arrays.

use std::f64::consts::PI;

// r[impl dsp.biquad.tdf2]
// r[impl dsp.biquad.stereo]
/// Transposed Direct Form II biquad filter.
#[derive(Debug, Clone)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
    z1: [f64; 2],
    z2: [f64; 2],
}

// r[impl dsp.biquad.types]
pub enum FilterType {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
    LowShelf { gain_db: f64 },
    HighShelf { gain_db: f64 },
    Peak { gain_db: f64 },
}

impl Biquad {
    pub fn new() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: [0.0; 2],
            z2: [0.0; 2],
        }
    }

    // r[impl dsp.biquad.coefficients]
    pub fn set(&mut self, filter_type: FilterType, freq_hz: f64, q: f64, sample_rate: f64) {
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let (b0, b1, b2, a0, a1, a2) = match filter_type {
            FilterType::Lowpass => {
                let b1 = 1.0 - cos_w0;
                let b0 = b1 / 2.0;
                (b0, b1, b0, 1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha)
            }
            FilterType::Highpass => {
                let b1 = -(1.0 + cos_w0);
                let b0 = (1.0 + cos_w0) / 2.0;
                (b0, b1, b0, 1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha)
            }
            FilterType::Bandpass => {
                let b0 = alpha;
                (b0, 0.0, -b0, 1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha)
            }
            FilterType::Notch => {
                let b0 = 1.0;
                let b1 = -2.0 * cos_w0;
                (b0, b1, b0, 1.0 + alpha, b1, 1.0 - alpha)
            }
            FilterType::LowShelf { gain_db } => {
                let a = 10.0_f64.powf(gain_db / 40.0);
                let s = 2.0 * a.sqrt() * alpha;
                let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + s);
                let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
                let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - s);
                let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + s;
                let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
                let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - s;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::HighShelf { gain_db } => {
                let a = 10.0_f64.powf(gain_db / 40.0);
                let s = 2.0 * a.sqrt() * alpha;
                let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + s);
                let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
                let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - s);
                let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + s;
                let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
                let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - s;
                (b0, b1, b2, a0, a1, a2)
            }
            FilterType::Peak { gain_db } => {
                let a = 10.0_f64.powf(gain_db / 40.0);
                let b0 = 1.0 + alpha * a;
                let b1 = -2.0 * cos_w0;
                let b2 = 1.0 - alpha * a;
                let a0 = 1.0 + alpha / a;
                let a2 = 1.0 - alpha / a;
                (b0, b1, b2, a0, -2.0 * cos_w0, a2)
            }
        };

        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    /// Process a single sample. `ch`: 0 = left, 1 = right.
    #[inline]
    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let output = self.b0 * input + self.z1[ch];
        self.z1[ch] = self.b1 * input - self.a1 * output + self.z2[ch];
        self.z2[ch] = crate::denormal::flush(self.b2 * input - self.a2 * output);
        output
    }

    // r[impl dsp.biquad.reset]
    pub fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }
}

impl Default for Biquad {
    fn default() -> Self {
        Self::new()
    }
}
