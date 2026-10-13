//! Empirical Pultec transfer residual at the captured default panel and 1 kHz.
//!
//! Linear response/phase are removed from the fixture. Coefficients are fitted
//! at -12/0 dBFS and checked at -24/-6 dBFS; this is not a circuit simulation.
use crate::{Error, model::Coloration};

/// A normalized amplifier transfer with sample-rate-correct drive automation.
///
/// Beyond the measured ±1 domain, a C1 continuous saturating extension keeps
/// output bounded. That extension is a modeling choice, not captured evidence.
#[derive(Debug, Clone)]
pub struct PultecColoration {
    dc: f64,
    dc_coefficient: f64,
    drive: f64,
    target: f64,
    coefficient: f64,
}
impl PultecColoration {
    /// # Errors
    /// Drive must be finite in 0..=100 percent, and the sample rate positive.
    pub fn new(drive_percent: f64, sample_rate: f64) -> Result<Self, Error> {
        if !sample_rate.is_finite() || sample_rate <= 0.0 {
            return Err(Error::InvalidSampleRate);
        }
        if !drive_percent.is_finite() || !(0.0..=100.0).contains(&drive_percent) {
            return Err(Error::InvalidGain);
        }
        let drive = drive_percent.mul_add(0.04, 1.0);
        Ok(Self {
            dc: 0.0,
            dc_coefficient: 1.0 - (-core::f64::consts::TAU * 5.0 / sample_rate).exp(),
            drive,
            target: drive,
            coefficient: 1.0 - (-1.0 / (sample_rate * 0.005)).exp(),
        })
    }
    fn polynomial(x: f64) -> f64 {
        let c2: f64 = -0.000_223_780_440_807_836_4;
        let c3: f64 = -0.001_174_951_658_290_389;
        let c4: f64 = 0.000_002_342_397_084_105_988;
        let c5: f64 = 0.000_010_737_972_205_851_14;
        (x * x).mul_add(c5.mul_add(x, c4).mul_add(x, c3).mul_add(x, c2), x)
    }
    fn derivative(x: f64) -> f64 {
        let c2: f64 = -0.000_223_780_440_807_836_4;
        let c3: f64 = -0.001_174_951_658_290_389;
        let c4: f64 = 0.000_002_342_397_084_105_988;
        let c5: f64 = 0.000_010_737_972_205_851_14;
        x.mul_add(
            (5.0 * c5)
                .mul_add(x, 4.0 * c4)
                .mul_add(x, 3.0 * c3)
                .mul_add(x, 2.0 * c2),
            1.0,
        )
    }
    fn transfer(x: f64) -> f64 {
        if x.abs() <= 1.0 {
            Self::polynomial(x)
        } else {
            let boundary = x.signum();
            Self::derivative(boundary).mul_add(
                boundary * (1.0 - (-(x.abs() - 1.0)).exp()),
                Self::polynomial(boundary),
            )
        }
    }
}
impl Coloration for PultecColoration {
    fn process(&mut self, sample: f64) -> f64 {
        self.drive = (self.target - self.drive).mul_add(self.coefficient, self.drive);
        let output = Self::transfer(sample * self.drive) / self.drive;
        self.dc = (output - self.dc).mul_add(self.dc_coefficient, self.dc);
        output - self.dc
    }
    fn reset(&mut self) {
        self.drive = self.target;
        self.dc = 0.0;
    }
    fn update(&mut self, prepared: &Self) {
        self.target = prepared.target;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measured_transfer_including_held_out_levels() {
        let mut count = 0;
        for row in include_str!("../tests/fixtures/pultec_coloration.csv")
            .lines()
            .filter(|row| !row.starts_with('#'))
        {
            let values: Vec<f64> = row.split(',').map(|x| x.parse().unwrap()).collect();
            let amplitude = 10.0_f64.powf(values[0] / 20.0);
            let dc = -0.000_223_780_440_807_836_4 * amplitude.powi(2) / 2.0
                + 0.000_002_342_397_084_105_988 * 3.0 * amplitude.powi(4) / 8.0;
            let predicted = PultecColoration::transfer(values[1]) - dc;
            assert!((predicted - values[2]).abs() < 7e-6, "{row}: {predicted}");
            count += 1;
        }
        assert_eq!(count, 100);
    }
    #[test]
    fn extension_is_bounded_continuous_and_monotonic() {
        let mut previous = -3.0;
        for step in -10000..=10000 {
            let x = f64::from(step) / 100.0;
            let y = PultecColoration::transfer(x);
            assert!(y.is_finite() && y.abs() < 2.0 && y >= previous);
            previous = y;
        }
        for boundary in [-1.0, 1.0] {
            let epsilon = 1e-6;
            let a = PultecColoration::transfer(boundary);
            let left = (a - PultecColoration::transfer(boundary - epsilon)) / epsilon;
            let right = (PultecColoration::transfer(boundary + epsilon) - a) / epsilon;
            assert!((left - right).abs() < 2e-6);
        }
    }
    #[test]
    fn dc_settles_and_reset_reproduces_fresh_state() {
        for rate in [44100.0, 48000.0, 96000.0] {
            let mut stage = PultecColoration::new(100.0, rate).unwrap();
            for _ in 0..96000 {
                stage.process(0.5);
            }
            assert!(stage.process(0.5).abs() < 1e-10);
            stage.reset();
            let mut fresh = PultecColoration::new(100.0, rate).unwrap();
            for _ in 0..128 {
                assert_eq!(stage.process(0.5), fresh.process(0.5));
            }
        }
    }
}
