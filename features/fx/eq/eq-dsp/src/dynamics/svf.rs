//! Simper state-variable filter (bell / shelf / cuts), from Andrew
//! Simper's published "Solving the continuous SVF equations using
//! trapezoidal integration" derivations (public domain math).
//!
//! Chosen for dynamic bands because the gain factor `A` enters the
//! coefficients algebraically: per-sample gain modulation only touches
//! a handful of multiplies (no cascade redesign, no allocation), and
//! the trapezoidal integrator states stay stable under fast automation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvfShape {
    Bell,
    LowShelf,
    HighShelf,
    Lowpass,
    Highpass,
}

/// Stereo (2-channel) Simper SVF.
#[derive(Debug, Clone)]
pub struct Svf {
    shape: SvfShape,
    // Integrator states per channel.
    ic1: [f64; 2],
    ic2: [f64; 2],
    // Coefficients.
    a1: f64,
    a2: f64,
    a3: f64,
    m0: f64,
    m1: f64,
    m2: f64,
    // Cached tuning so gain-only updates skip the tan().
    g_base: f64,
    k_base: f64,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
}

impl Svf {
    pub fn new(sample_rate: f64) -> Self {
        let mut s = Self {
            shape: SvfShape::Bell,
            ic1: [0.0; 2],
            ic2: [0.0; 2],
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            m0: 1.0,
            m1: 0.0,
            m2: 0.0,
            g_base: 0.0,
            k_base: 1.0,
            freq_hz: 1000.0,
            q: 0.707,
            gain_db: 0.0,
            sample_rate,
        };
        s.set(SvfShape::Bell, 1000.0, 0.707, 0.0);
        s
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.retune();
    }

    /// Full parameter set (computes tan()).
    pub fn set(&mut self, shape: SvfShape, freq_hz: f64, q: f64, gain_db: f64) {
        self.shape = shape;
        self.freq_hz = freq_hz.clamp(10.0, self.sample_rate * 0.49);
        self.q = q.max(0.025);
        self.gain_db = gain_db;
        self.retune();
    }

    /// Gain-only update — the cheap per-sample path for dynamic bands.
    #[inline]
    pub fn set_gain_db(&mut self, gain_db: f64) {
        if (gain_db - self.gain_db).abs() < 1.0e-9 {
            return;
        }
        self.gain_db = gain_db;
        self.apply_gain();
    }

    pub fn gain_db(&self) -> f64 {
        self.gain_db
    }

    fn retune(&mut self) {
        self.g_base = (core::f64::consts::PI * self.freq_hz / self.sample_rate).tan();
        self.k_base = 1.0 / self.q;
        self.apply_gain();
    }

    /// Simper's coefficient assignments per shape; A = 10^(dB/40).
    fn apply_gain(&mut self) {
        let a = 10.0f64.powf(self.gain_db / 40.0);
        let (g, k) = match self.shape {
            SvfShape::Bell => (self.g_base, self.k_base / a),
            SvfShape::LowShelf => (self.g_base / a.sqrt(), self.k_base),
            SvfShape::HighShelf => (self.g_base * a.sqrt(), self.k_base),
            SvfShape::Lowpass | SvfShape::Highpass => (self.g_base, self.k_base),
        };
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
        match self.shape {
            SvfShape::Bell => {
                self.m0 = 1.0;
                self.m1 = k * (a * a - 1.0);
                self.m2 = 0.0;
            }
            SvfShape::LowShelf => {
                self.m0 = 1.0;
                self.m1 = k * (a - 1.0);
                self.m2 = a * a - 1.0;
            }
            SvfShape::HighShelf => {
                self.m0 = a * a;
                self.m1 = k * (1.0 - a) * a;
                self.m2 = 1.0 - a * a;
            }
            SvfShape::Lowpass => {
                self.m0 = 0.0;
                self.m1 = 0.0;
                self.m2 = 1.0;
            }
            SvfShape::Highpass => {
                self.m0 = 1.0;
                self.m1 = -k;
                self.m2 = -1.0;
            }
        }
    }

    #[inline]
    pub fn tick(&mut self, ch: usize, x: f64) -> f64 {
        let v3 = x - self.ic2[ch];
        let v1 = self.a1 * self.ic1[ch] + self.a2 * v3;
        let v2 = self.ic2[ch] + self.a2 * self.ic1[ch] + self.a3 * v3;
        self.ic1[ch] = 2.0 * v1 - self.ic1[ch];
        self.ic2[ch] = 2.0 * v2 - self.ic2[ch];
        self.m0 * x + self.m1 * v1 + self.m2 * v2
    }

    pub fn reset(&mut self) {
        self.ic1 = [0.0; 2];
        self.ic2 = [0.0; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn magnitude_at(svf: &mut Svf, freq: f64) -> f64 {
        svf.reset();
        let n = 48000;
        let mut in_e = 0.0;
        let mut out_e = 0.0;
        for i in 0..n {
            let x = (core::f64::consts::TAU * freq * i as f64 / SR).sin();
            let y = svf.tick(0, x);
            if i > n / 2 {
                in_e += x * x;
                out_e += y * y;
            }
        }
        10.0 * (out_e / in_e).log10()
    }

    #[test]
    fn bell_boosts_at_center_only() {
        let mut s = Svf::new(SR);
        s.set(SvfShape::Bell, 1000.0, 1.0, 12.0);
        let at_c = magnitude_at(&mut s, 1000.0);
        let far = magnitude_at(&mut s, 8000.0);
        assert!((at_c - 12.0).abs() < 0.5, "center gain: {at_c}");
        assert!(far.abs() < 1.0, "skirt should be flat: {far}");
    }

    #[test]
    fn shelves_shelve() {
        let mut s = Svf::new(SR);
        s.set(SvfShape::HighShelf, 2000.0, 0.707, -9.0);
        let hi = magnitude_at(&mut s, 12000.0);
        let lo = magnitude_at(&mut s, 100.0);
        assert!((hi + 9.0).abs() < 0.8, "high shelf plateau: {hi}");
        assert!(lo.abs() < 0.5, "low side flat: {lo}");
    }

    #[test]
    fn gain_only_update_matches_full_set() {
        let mut a = Svf::new(SR);
        a.set(SvfShape::Bell, 500.0, 2.0, 0.0);
        a.set_gain_db(7.5);
        let mut b = Svf::new(SR);
        b.set(SvfShape::Bell, 500.0, 2.0, 7.5);
        let ga = magnitude_at(&mut a, 500.0);
        let gb = magnitude_at(&mut b, 500.0);
        assert!((ga - gb).abs() < 1e-6, "cheap path must match: {ga} vs {gb}");
    }
}
