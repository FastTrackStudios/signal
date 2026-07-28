//! One dynamic EQ band: SVF filter + side-chain detector +
//! base/target gain crossfade.
//!
//! `gain_db(t) = base + d(t)·(target − base)` — the detector drive
//! `d ∈ [0,1]` breathes the band between its two drawn curves.
//! `target − base` is the Pro-Q-style bipolar "dynamic range".

use super::detector::Detector;
use super::svf::{Svf, SvfShape};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynShape {
    Bell,
    LowShelf,
    HighShelf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideMode {
    /// Trigger on the band's own frequency region (bandpass at the
    /// band's freq/Q) — Pro-Q "Band" / ZL "Side Link".
    BandLinked,
    /// Trigger on a custom range set by `side_lo_hz..side_hi_hz`.
    Free,
    /// Trigger on the unfiltered side signal.
    Wide,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynBandParams {
    pub shape: DynShape,
    pub freq_hz: f64,
    pub q: f64,
    /// Static (base) gain in dB.
    pub base_gain_db: f64,
    /// Dynamic range in dB: target = base + range. Negative =
    /// compress-when-loud, positive = expand-when-loud.
    pub range_db: f64,
    pub side_mode: SideMode,
    pub side_lo_hz: f64,
    pub side_hi_hz: f64,
    pub enabled: bool,
}

impl Default for DynBandParams {
    fn default() -> Self {
        Self {
            shape: DynShape::Bell,
            freq_hz: 1000.0,
            q: 1.0,
            base_gain_db: 0.0,
            range_db: 0.0,
            side_mode: SideMode::BandLinked,
            side_lo_hz: 20.0,
            side_hi_hz: 20000.0,
            enabled: true,
        }
    }
}

/// Per-sample dB smoothing time for the applied gain (declicks the
/// detector edges without adding another follower stage).
const GAIN_SMOOTH_MS: f64 = 2.0;

#[derive(Debug, Clone)]
pub struct DynBand {
    pub params: DynBandParams,
    pub detector: Detector,
    filter: Svf,
    /// Side-chain band filter (mono — detector sees the mono sum).
    side_bp: Svf,
    side_hp: Svf,
    side_lp: Svf,
    applied_gain_db: f64,
    gain_smooth_coeff: f64,
    sample_rate: f64,
}

impl DynBand {
    pub fn new(sample_rate: f64) -> Self {
        let mut b = Self {
            params: DynBandParams::default(),
            detector: Detector::new(sample_rate),
            filter: Svf::new(sample_rate),
            side_bp: Svf::new(sample_rate),
            side_hp: Svf::new(sample_rate),
            side_lp: Svf::new(sample_rate),
            applied_gain_db: 0.0,
            gain_smooth_coeff: 0.0,
            sample_rate,
        };
        b.update(sample_rate);
        b
    }

    /// Recompute filters + coefficients after parameter changes.
    /// Never call per sample.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.gain_smooth_coeff =
            1.0 - (-1.0 / (GAIN_SMOOTH_MS * 0.001 * sample_rate)).exp();
        let shape = match self.params.shape {
            DynShape::Bell => SvfShape::Bell,
            DynShape::LowShelf => SvfShape::LowShelf,
            DynShape::HighShelf => SvfShape::HighShelf,
        };
        self.filter.set_sample_rate(sample_rate);
        self.filter
            .set(shape, self.params.freq_hz, self.params.q, self.applied_gain_db);
        self.side_bp.set_sample_rate(sample_rate);
        // Band-linked trigger: bandpass-ish selectivity via a bell-Q
        // pair of cut filters is overkill — a resonant band-limited
        // path from HP+LP at the band edges tracks Pro-Q's "band
        // limited according to the band's frequency range".
        let (lo, hi) = match self.params.side_mode {
            SideMode::BandLinked => {
                let bw = self.params.freq_hz / self.params.q.max(0.1);
                (
                    (self.params.freq_hz - bw * 0.5).max(10.0),
                    (self.params.freq_hz + bw * 0.5).min(sample_rate * 0.45),
                )
            }
            SideMode::Free => (self.params.side_lo_hz, self.params.side_hi_hz),
            SideMode::Wide => (0.0, 0.0),
        };
        if lo > 0.0 {
            self.side_hp.set(SvfShape::Highpass, lo, 0.707, 0.0);
            self.side_lp.set(SvfShape::Lowpass, hi, 0.707, 0.0);
        }
        self.detector.update(sample_rate);
    }

    /// Current live gain in dB (for metering / the yellow bar).
    pub fn live_gain_db(&self) -> f64 {
        self.applied_gain_db
    }

    /// Process one stereo sample in place. `side` is the external
    /// side-chain sample (mono); pass the input's mono sum when no
    /// external side-chain is routed.
    #[inline]
    pub fn tick(&mut self, left: &mut f64, right: &mut f64, side: f64) {
        if !self.params.enabled {
            return;
        }
        // Side path: band-limit, detect.
        let filtered_side = match self.params.side_mode {
            SideMode::Wide => side,
            _ => {
                let hp = self.side_hp.tick(0, side);
                self.side_lp.tick(0, hp)
            }
        };
        let d = self.detector.tick(filtered_side, side);

        // Base → target crossfade, smoothed, cheap gain-only retune.
        let target = self.params.base_gain_db + d * self.params.range_db;
        self.applied_gain_db += (target - self.applied_gain_db) * self.gain_smooth_coeff;
        self.filter.set_gain_db(self.applied_gain_db);

        *left = self.filter.tick(0, *left);
        *right = self.filter.tick(1, *right);
    }

    pub fn reset(&mut self) {
        self.filter.reset();
        self.side_bp.reset();
        self.side_hp.reset();
        self.side_lp.reset();
        self.detector.reset();
        self.applied_gain_db = self.params.base_gain_db;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    /// RMS of a window of samples.
    fn rms(buf: &[f64]) -> f64 {
        (buf.iter().map(|x| x * x).sum::<f64>() / buf.len() as f64).sqrt()
    }

    #[test]
    fn compresses_its_band_when_loud() {
        // −12 dB dynamic range bell at 1 kHz: a loud 1 kHz sine should
        // be attenuated once the detector rides up; a quiet one passes.
        let run = |amp: f64| -> f64 {
            let mut b = DynBand::new(SR);
            b.params.range_db = -12.0;
            b.params.freq_hz = 1000.0;
            b.params.q = 1.0;
            b.detector.params.threshold_db = -20.0;
            b.detector.params.attack_ms = 2.0;
            b.detector.params.rms_mix = 1.0;
            b.update(SR);
            let n = 48_000;
            let mut out = vec![0.0; n];
            for i in 0..n {
                let mut l = amp * (core::f64::consts::TAU * 1000.0 * i as f64 / SR).sin();
                let mut r = l;
                let side = l;
                b.tick(&mut l, &mut r, side);
                out[i] = l;
            }
            20.0 * (rms(&out[n / 2..]) / (amp / 2.0f64.sqrt())).log10()
        };
        let loud = run(0.5); // ≈ −6 dB, over threshold
        let quiet = run(0.005); // ≈ −46 dB, under
        assert!(loud < -8.0, "loud sine should be pulled down: {loud}");
        assert!(quiet.abs() < 1.0, "quiet sine passes at base gain: {quiet}");
    }

    #[test]
    fn out_of_band_content_does_not_trigger() {
        // Band-linked side filter at 4 kHz: loud 200 Hz content must
        // not duck the 4 kHz band.
        let mut b = DynBand::new(SR);
        b.params.range_db = -12.0;
        b.params.freq_hz = 4000.0;
        b.params.q = 2.0;
        b.detector.params.threshold_db = -20.0;
        b.update(SR);
        let mut min_gain = 0.0f64;
        for i in 0..48_000 {
            let mut l = 0.7 * (core::f64::consts::TAU * 200.0 * i as f64 / SR).sin();
            let mut r = l;
            let side = l;
            b.tick(&mut l, &mut r, side);
            min_gain = min_gain.min(b.live_gain_db());
        }
        assert!(
            min_gain > -2.0,
            "out-of-band bass must not trigger the band: {min_gain}"
        );
    }

    #[test]
    fn expansion_rides_up() {
        let mut b = DynBand::new(SR);
        b.params.range_db = 9.0; // expand when loud
        b.detector.params.threshold_db = -20.0;
        b.detector.params.attack_ms = 2.0;
        b.update(SR);
        let mut max_gain = 0.0f64;
        for i in 0..48_000 {
            let mut l = 0.5 * (core::f64::consts::TAU * 1000.0 * i as f64 / SR).sin();
            let mut r = l;
            let side = l;
            b.tick(&mut l, &mut r, side);
            max_gain = max_gain.max(b.live_gain_db());
        }
        assert!(max_gain > 7.0, "positive range should boost when loud: {max_gain}");
    }
}
