//! Lightweight one-pole lowpass/highpass filters for reverb damping.
//!
//! Ported from CloudSeedCore Lp1.h/Hp1.h (MIT, Ghost Note Audio).
//! Uses the exact same coefficient formula: `alpha = nn - sqrt(nn^2 - 1)`
//! where `nn = 2 - cos(2*pi*fc/fs)`.

use std::f64::consts::PI;

/// One-pole lowpass filter.
///
/// `y[n] = b0 * x[n] + a1 * y[n-1]`
/// where `a1 = alpha`, `b0 = 1 - alpha`.
pub struct Lp1 {
    output: f64,
    b0: f64,
    a1: f64,
    sample_rate: f64,
    cutoff_hz: f64,
}

impl Lp1 {
    pub fn new() -> Self {
        let mut lp = Self {
            output: 0.0,
            b0: 1.0,
            a1: 0.0,
            sample_rate: 48000.0,
            cutoff_hz: 20000.0,
        };
        lp.update();
        lp
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.update();
    }

    pub fn set_freq(&mut self, freq_hz: f64, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.cutoff_hz = freq_hz;
        self.update();
    }

    pub fn set_cutoff(&mut self, freq_hz: f64) {
        self.cutoff_hz = freq_hz;
        self.update();
    }

    /// Set coefficient directly (0.0 = no filter, approaching 1.0 = max damping).
    pub fn set_coeff(&mut self, a1: f64) {
        self.a1 = a1;
        self.b0 = 1.0 - a1;
    }

    fn update(&mut self) {
        let mut fc = self.cutoff_hz;
        // Prevent going over Nyquist
        if fc >= self.sample_rate * 0.5 {
            fc = self.sample_rate * 0.499;
        }
        let x = 2.0 * PI * fc / self.sample_rate;
        let nn = 2.0 - x.cos();
        let alpha = nn - (nn * nn - 1.0).sqrt();
        self.a1 = alpha;
        self.b0 = 1.0 - alpha;
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        // Denormal prevention
        if input == 0.0 && self.output.abs() < 1e-10 {
            self.output = 0.0;
        } else {
            self.output = self.b0 * input + self.a1 * self.output;
        }
        self.output
    }

    pub fn reset(&mut self) {
        self.output = 0.0;
    }
}

/// One-pole highpass filter (complement of Lp1).
pub struct Hp1 {
    lp_out: f64,
    output: f64,
    b0: f64,
    a1: f64,
    sample_rate: f64,
    cutoff_hz: f64,
}

impl Hp1 {
    pub fn new() -> Self {
        let mut hp = Self {
            lp_out: 0.0,
            output: 0.0,
            b0: 1.0,
            a1: 0.0,
            sample_rate: 48000.0,
            cutoff_hz: 20.0,
        };
        hp.update();
        hp
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.update();
    }

    pub fn set_freq(&mut self, freq_hz: f64, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.cutoff_hz = freq_hz;
        self.update();
    }

    pub fn set_cutoff(&mut self, freq_hz: f64) {
        self.cutoff_hz = freq_hz;
        self.update();
    }

    pub fn set_coeff(&mut self, a1: f64) {
        self.a1 = a1;
        self.b0 = 1.0 - a1;
    }

    fn update(&mut self) {
        let mut fc = self.cutoff_hz;
        if fc >= self.sample_rate * 0.5 {
            fc = self.sample_rate * 0.499;
        }
        let x = 2.0 * PI * fc / self.sample_rate;
        let nn = 2.0 - x.cos();
        let alpha = nn - (nn * nn - 1.0).sqrt();
        self.a1 = alpha;
        self.b0 = 1.0 - alpha;
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        if input == 0.0 && self.lp_out.abs() < 1e-10 {
            self.output = 0.0;
        } else {
            self.lp_out = self.b0 * input + self.a1 * self.lp_out;
            self.output = input - self.lp_out;
        }
        self.output
    }

    pub fn reset(&mut self) {
        self.lp_out = 0.0;
        self.output = 0.0;
    }
}
