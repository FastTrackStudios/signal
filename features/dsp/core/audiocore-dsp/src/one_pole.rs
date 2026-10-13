//! One-pole lowpass/highpass filters for damping and tone shaping.
//!
//! Coefficient formula ported from CloudSeedCore Lp1.h/Hp1.h (MIT,
//! Ghost Note Audio): `alpha = nn - sqrt(nn^2 - 1)` where
//! `nn = 2 - cos(2*pi*fc/fs)`. Denormal-safe.

use std::f64::consts::PI;

use crate::denormal::flush;

fn alpha_for(cutoff_hz: f64, sample_rate: f64) -> f64 {
    let fc = cutoff_hz.min(sample_rate * 0.499);
    let x = 2.0 * PI * fc / sample_rate;
    let nn = 2.0 - x.cos();
    nn - (nn * nn - 1.0).sqrt()
}

/// One-pole lowpass: `y[n] = (1 - a1) * x[n] + a1 * y[n-1]`.
#[derive(Debug, Clone)]
pub struct OnePoleLp {
    output: f64,
    b0: f64,
    a1: f64,
}

impl OnePoleLp {
    pub fn new(cutoff_hz: f64, sample_rate: f64) -> Self {
        let mut lp = Self {
            output: 0.0,
            b0: 1.0,
            a1: 0.0,
        };
        lp.set_cutoff(cutoff_hz, sample_rate);
        lp
    }

    pub fn set_cutoff(&mut self, cutoff_hz: f64, sample_rate: f64) {
        self.a1 = alpha_for(cutoff_hz, sample_rate);
        self.b0 = 1.0 - self.a1;
    }

    /// Set the feedback coefficient directly (0.0 = bypass, →1.0 = max damping).
    pub fn set_coeff(&mut self, a1: f64) {
        self.a1 = a1;
        self.b0 = 1.0 - a1;
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        self.output = flush(self.b0 * input + self.a1 * self.output);
        self.output
    }

    pub fn reset(&mut self) {
        self.output = 0.0;
    }
}

/// One-pole highpass: input minus the one-pole lowpass of the input.
#[derive(Debug, Clone)]
pub struct OnePoleHp {
    lp: OnePoleLp,
}

impl OnePoleHp {
    pub fn new(cutoff_hz: f64, sample_rate: f64) -> Self {
        Self {
            lp: OnePoleLp::new(cutoff_hz, sample_rate),
        }
    }

    pub fn set_cutoff(&mut self, cutoff_hz: f64, sample_rate: f64) {
        self.lp.set_cutoff(cutoff_hz, sample_rate);
    }

    pub fn set_coeff(&mut self, a1: f64) {
        self.lp.set_coeff(a1);
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        input - self.lp.tick(input)
    }

    pub fn reset(&mut self) {
        self.lp.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn lp_passes_dc_blocks_nyquist() {
        let mut lp = OnePoleLp::new(1000.0, SR);
        let mut y = 0.0;
        for _ in 0..48000 {
            y = lp.tick(1.0);
        }
        assert!((y - 1.0).abs() < 1e-6, "DC should pass: {y}");

        lp.reset();
        let mut peak: f64 = 0.0;
        for n in 0..48000 {
            let x = if n % 2 == 0 { 1.0 } else { -1.0 }; // Nyquist
            let v = lp.tick(x);
            if n > 4800 {
                peak = peak.max(v.abs());
            }
        }
        assert!(peak < 0.1, "Nyquist should be attenuated: {peak}");
    }

    #[test]
    fn hp_blocks_dc() {
        let mut hp = OnePoleHp::new(20.0, SR);
        let mut y = 1.0;
        for _ in 0..480000 {
            y = hp.tick(1.0);
        }
        assert!(y.abs() < 1e-3, "DC should be blocked: {y}");
    }

    #[test]
    fn denormal_safe() {
        let mut lp = OnePoleLp::new(100.0, SR);
        lp.tick(1.0);
        for _ in 0..1_000_000 {
            let y = lp.tick(0.0);
            assert!(y == 0.0 || y.abs() > 1.0e-100);
        }
        assert_eq!(lp.tick(0.0), 0.0, "state should flush to hard zero");
    }
}
