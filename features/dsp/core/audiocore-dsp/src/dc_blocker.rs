//! First-order DC blocker for feedback loops.
//!
//! `y[n] = x[n] - x[n-1] + R * y[n-1]` with R close to 1. Any effect that
//! recirculates audio through saturation or pitch shifting accumulates a
//! DC / subsonic offset; place one of these inside the loop.

use std::f64::consts::PI;

use crate::denormal::flush;

#[derive(Debug, Clone)]
pub struct DcBlocker {
    x1: f64,
    y1: f64,
    r: f64,
}

impl DcBlocker {
    /// DC blocker with a fixed pole (R = 0.9995, ~4 Hz at 48 kHz).
    pub fn new() -> Self {
        Self {
            x1: 0.0,
            y1: 0.0,
            r: 0.9995,
        }
    }

    /// DC blocker with the -3 dB point at `cutoff_hz` for the given rate.
    pub fn with_cutoff(cutoff_hz: f64, sample_rate: f64) -> Self {
        let mut dc = Self::new();
        dc.set_cutoff(cutoff_hz, sample_rate);
        dc
    }

    /// Re-tune the pole. Keeps state, so safe to call on sample-rate change.
    pub fn set_cutoff(&mut self, cutoff_hz: f64, sample_rate: f64) {
        self.r = (1.0 - 2.0 * PI * cutoff_hz / sample_rate).clamp(0.9, 0.99999);
    }

    #[inline]
    pub fn tick(&mut self, x: f64) -> f64 {
        self.y1 = flush(x - self.x1 + self.r * self.y1);
        self.x1 = x;
        self.y1
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }
}

impl Default for DcBlocker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_dc_offset() {
        let mut dc = DcBlocker::new();
        let mut out = 0.0;
        for _ in 0..96000 {
            out = dc.tick(1.0);
        }
        assert!(out.abs() < 1e-3, "DC should be blocked: {out}");
    }

    #[test]
    fn passes_audio_band() {
        // 1 kHz sine at 48 kHz should pass nearly unattenuated.
        let mut dc = DcBlocker::new();
        let sr = 48000.0;
        let mut peak: f64 = 0.0;
        for n in 0..48000 {
            let x = (2.0 * PI * 1000.0 * n as f64 / sr).sin();
            let y = dc.tick(x);
            if n > 4800 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak > 0.99, "1 kHz should pass: peak {peak}");
    }

    #[test]
    fn no_nan() {
        let mut dc = DcBlocker::with_cutoff(20.0, 48000.0);
        for &x in &[0.0, 1.0, -1.0, 1e-300, f64::MIN_POSITIVE] {
            assert!(dc.tick(x).is_finite());
        }
    }
}
