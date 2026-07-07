//! Biquad filter with multiple filter types.
//!
//! Ported from CloudSeedCore Biquad.h/.cpp (MIT, Ghost Note Audio).
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

    pub fn set_q(&mut self, value: f64) {
        self.q = value.max(0.001);
    }

    pub fn update(&mut self) {
        let fc = self.frequency;
        let v = 10.0_f64.powf(self.gain_db.abs() / 20.0);
        let k = (PI * fc * self.fs_inv).tan();
        let q = self.q;

        match self.filter_type {
            FilterType::LowPass6db => {
                self.a1 = (-2.0 * PI * fc * self.fs_inv).exp();
                self.b0 = 1.0 + self.a1;
                // Negate a1 for the difference equation convention
                self.a1 = -self.a1;
                self.b1 = 0.0;
                self.b2 = 0.0;
                self.a2 = 0.0;
            }
            FilterType::HighPass6db => {
                let alpha = (-2.0 * PI * fc * self.fs_inv).exp();
                self.a1 = -alpha;
                self.b0 = alpha;
                self.b1 = -alpha;
                self.b2 = 0.0;
                self.a2 = 0.0;
            }
            FilterType::LowPass => {
                let norm = 1.0 / (1.0 + k / q + k * k);
                self.b0 = k * k * norm;
                self.b1 = 2.0 * self.b0;
                self.b2 = self.b0;
                self.a1 = 2.0 * (k * k - 1.0) * norm;
                self.a2 = (1.0 - k / q + k * k) * norm;
            }
            FilterType::HighPass => {
                let norm = 1.0 / (1.0 + k / q + k * k);
                self.b0 = norm;
                self.b1 = -2.0 * self.b0;
                self.b2 = self.b0;
                self.a1 = 2.0 * (k * k - 1.0) * norm;
                self.a2 = (1.0 - k / q + k * k) * norm;
            }
            FilterType::BandPass => {
                let norm = 1.0 / (1.0 + k / q + k * k);
                self.b0 = k / q * norm;
                self.b1 = 0.0;
                self.b2 = -self.b0;
                self.a1 = 2.0 * (k * k - 1.0) * norm;
                self.a2 = (1.0 - k / q + k * k) * norm;
            }
            FilterType::Notch => {
                let norm = 1.0 / (1.0 + k / q + k * k);
                self.b0 = (1.0 + k * k) * norm;
                self.b1 = 2.0 * (k * k - 1.0) * norm;
                self.b2 = self.b0;
                self.a1 = self.b1;
                self.a2 = (1.0 - k / q + k * k) * norm;
            }
            FilterType::Peak => {
                if self.gain_db >= 0.0 {
                    let norm = 1.0 / (1.0 + 1.0 / q * k + k * k);
                    self.b0 = (1.0 + v / q * k + k * k) * norm;
                    self.b1 = 2.0 * (k * k - 1.0) * norm;
                    self.b2 = (1.0 - v / q * k + k * k) * norm;
                    self.a1 = self.b1;
                    self.a2 = (1.0 - 1.0 / q * k + k * k) * norm;
                } else {
                    let norm = 1.0 / (1.0 + v / q * k + k * k);
                    self.b0 = (1.0 + 1.0 / q * k + k * k) * norm;
                    self.b1 = 2.0 * (k * k - 1.0) * norm;
                    self.b2 = (1.0 - 1.0 / q * k + k * k) * norm;
                    self.a1 = self.b1;
                    self.a2 = (1.0 - v / q * k + k * k) * norm;
                }
            }
            FilterType::LowShelf => {
                let sqrt2 = std::f64::consts::SQRT_2;
                if self.gain_db >= 0.0 {
                    let norm = 1.0 / (1.0 + sqrt2 * k + k * k);
                    self.b0 = (1.0 + (2.0 * v).sqrt() * k + v * k * k) * norm;
                    self.b1 = 2.0 * (v * k * k - 1.0) * norm;
                    self.b2 = (1.0 - (2.0 * v).sqrt() * k + v * k * k) * norm;
                    self.a1 = 2.0 * (k * k - 1.0) * norm;
                    self.a2 = (1.0 - sqrt2 * k + k * k) * norm;
                } else {
                    let norm = 1.0 / (1.0 + (2.0 * v).sqrt() * k + v * k * k);
                    self.b0 = (1.0 + sqrt2 * k + k * k) * norm;
                    self.b1 = 2.0 * (k * k - 1.0) * norm;
                    self.b2 = (1.0 - sqrt2 * k + k * k) * norm;
                    self.a1 = 2.0 * (v * k * k - 1.0) * norm;
                    self.a2 = (1.0 - (2.0 * v).sqrt() * k + v * k * k) * norm;
                }
            }
            FilterType::HighShelf => {
                let sqrt2 = std::f64::consts::SQRT_2;
                if self.gain_db >= 0.0 {
                    let norm = 1.0 / (1.0 + sqrt2 * k + k * k);
                    self.b0 = (v + (2.0 * v).sqrt() * k + k * k) * norm;
                    self.b1 = 2.0 * (k * k - v) * norm;
                    self.b2 = (v - (2.0 * v).sqrt() * k + k * k) * norm;
                    self.a1 = 2.0 * (k * k - 1.0) * norm;
                    self.a2 = (1.0 - sqrt2 * k + k * k) * norm;
                } else {
                    let norm = 1.0 / (v + (2.0 * v).sqrt() * k + k * k);
                    self.b0 = (1.0 + sqrt2 * k + k * k) * norm;
                    self.b1 = 2.0 * (k * k - 1.0) * norm;
                    self.b2 = (1.0 - sqrt2 * k + k * k) * norm;
                    self.a1 = 2.0 * (k * k - v) * norm;
                    self.a2 = (v - (2.0 * v).sqrt() * k + k * k) * norm;
                }
            }
        }
    }

    #[inline]
    pub fn tick(&mut self, x: f64) -> f64 {
        self.y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.y2 = self.y1;
        self.x1 = x;
        self.y1 = self.y;
        self.y
    }

    pub fn clear(&mut self) {
        self.y = 0.0;
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}
