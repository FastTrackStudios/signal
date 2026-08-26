//! Transient EQ — SplitEQ-class dual-stream equalization.
//!
//! `input → splitter → EQ_A(transient) + EQ_B(steady) → sum`. The
//! splitter is complementary by construction (`steady = input −
//! transient`), so with both EQs flat the engine is a null — exactly
//! SplitEQ's reconstruction guarantee, via a different (unpatented)
//! split: the zero-latency dual-window relative-energy mask
//! (ZL-Splitter's "Peak/Steady" concept, clean-room from the published
//! parameter model). The HQ spectral split (Fitzgerald median-filter
//! HPSS) already lives in `trigger-dsp` and is wired at the signal-fx
//! layer, where both crates are visible.
//!
//! Splitter concept: a short mean-square window tracks "now", a long
//! window tracks "recently"; when now ≫ recently (relative threshold —
//! level-independent, like the references) a mask envelope rises with
//! an attack one-pole and falls with a hold-scaled release. `transient
//! = x·mask`, `steady = x − transient`, sample-exact complements.

use crate::chain::EqChain;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitParams {
    /// Bias which side "wins" (−50..+50, 0 = neutral). Positive pushes
    /// more of the signal into the transient stream.
    pub balance: f64,
    /// Mask attack speed (0..100, default 50).
    pub attack: f64,
    /// Transient tail length (0..100): scales the mask release.
    pub hold: f64,
    /// Detector window scaling (0..100): larger = steadier decisions.
    pub smooth: f64,
}

impl Default for SplitParams {
    fn default() -> Self {
        Self {
            balance: 0.0,
            attack: 50.0,
            hold: 50.0,
            smooth: 50.0,
        }
    }
}

/// Zero-latency transient/steady splitter (mono detector, stereo mask).
#[derive(Debug, Clone)]
pub struct PeakSteadySplitter {
    pub params: SplitParams,
    short_ms_state: f64,
    long_ms_state: f64,
    short_rise_coeff: f64,
    short_coeff: f64,
    long_coeff: f64,
    mask: f64,
    attack_coeff: f64,
    release_coeff: f64,
    ratio_thresh: f64,
    sample_rate: f64,
}

impl PeakSteadySplitter {
    pub fn new(sample_rate: f64) -> Self {
        let mut s = Self {
            params: SplitParams::default(),
            short_ms_state: 0.0,
            long_ms_state: 0.0,
            short_rise_coeff: 0.0,
            short_coeff: 0.0,
            long_coeff: 0.0,
            mask: 0.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            ratio_thresh: 2.0,
            sample_rate,
        };
        s.update(sample_rate);
        s
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let smooth = self.params.smooth.clamp(0.0, 100.0) / 100.0;
        // Short window 0.5..5 ms (fall side — the rise side is a fixed
        // fast charge so onsets register within a few samples), long
        // window 100 ms..1 s.
        let short_ms = 0.5 + smooth * 4.5;
        let long_ms = 100.0 + smooth * 900.0;
        let c = |ms: f64| 1.0 - (-1.0 / (ms * 0.001 * sample_rate)).exp();
        self.short_rise_coeff = c(0.15);
        self.short_coeff = c(short_ms);
        self.long_coeff = c(long_ms);
        // Attack 0..100 → ~3 ms..0.03 ms (50 = 0.3 ms); hold 0..100 →
        // release 5..500 ms.
        let attack_ms = 10.0f64.powf(0.5 - self.params.attack.clamp(0.0, 100.0) / 50.0);
        let release_ms = 5.0 * 100.0f64.powf(self.params.hold.clamp(0.0, 100.0) / 100.0);
        self.attack_coeff = c(attack_ms);
        self.release_coeff = c(release_ms);
        // Balance ±50 → relative-energy threshold 10^(±1) around 2×.
        self.ratio_thresh = 2.0 * 10.0f64.powf(-self.params.balance.clamp(-50.0, 50.0) / 50.0);
    }

    /// One mono detector sample → mask 0..1.
    #[inline]
    pub fn tick_mask(&mut self, x: f64) -> f64 {
        let sq = x * x;
        if self.long_ms_state <= 1.0e-18 {
            self.long_ms_state = sq;
        }
        let sc = if sq > self.short_ms_state {
            self.short_rise_coeff
        } else {
            self.short_coeff
        };
        self.short_ms_state += (sq - self.short_ms_state) * sc;
        self.long_ms_state += (sq - self.long_ms_state) * self.long_coeff;
        let transient_now = self.short_ms_state > self.long_ms_state * self.ratio_thresh;
        let target = if transient_now { 1.0 } else { 0.0 };
        let coeff = if target > self.mask {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.mask += (target - self.mask) * coeff;
        self.mask
    }

    pub fn reset(&mut self) {
        self.short_ms_state = 0.0;
        self.long_ms_state = 0.0;
        self.mask = 0.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamSolo {
    None,
    Transient,
    Steady,
}

/// Dual-stream EQ: transient and steady each get a full 24-band chain
/// plus a per-stream output gain, SplitEQ-style.
pub struct TransientEq {
    pub splitter: PeakSteadySplitter,
    pub transient_eq: EqChain,
    pub steady_eq: EqChain,
    pub transient_gain_db: f64,
    pub steady_gain_db: f64,
    pub solo: StreamSolo,
}

impl TransientEq {
    pub fn new(sample_rate: f64) -> Self {
        let _ = sample_rate;
        Self {
            splitter: PeakSteadySplitter::new(sample_rate),
            transient_eq: EqChain::new(),
            steady_eq: EqChain::new(),
            transient_gain_db: 0.0,
            steady_gain_db: 0.0,
            solo: StreamSolo::None,
        }
    }

    /// Process one stereo sample.
    #[inline]
    pub fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let mask = self.splitter.tick_mask(0.5 * (left + right));
        let (tl, tr) = (left * mask, right * mask);
        let (sl, sr) = (left - tl, right - tr);

        let mut t = [tl, tr];
        {
            let (a, b) = t.split_at_mut(1);
            self.transient_eq.process(&mut a[..], &mut b[..]);
        }
        let mut s = [sl, sr];
        {
            let (a, b) = s.split_at_mut(1);
            self.steady_eq.process(&mut a[..], &mut b[..]);
        }

        let tg = 10.0f64.powf(self.transient_gain_db / 20.0);
        let sg = 10.0f64.powf(self.steady_gain_db / 20.0);
        match self.solo {
            StreamSolo::None => (t[0] * tg + s[0] * sg, t[1] * tg + s[1] * sg),
            StreamSolo::Transient => (t[0] * tg, t[1] * tg),
            StreamSolo::Steady => (s[0] * sg, s[1] * sg),
        }
    }

    pub fn reset(&mut self) {
        self.splitter.reset();
        self.transient_eq.reset();
        self.steady_eq.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    /// Drum-ish test signal: steady 220 Hz tone + periodic clicks.
    fn test_signal(i: usize) -> f64 {
        let tone = 0.2 * (core::f64::consts::TAU * 220.0 * i as f64 / SR).sin();
        let click = if i % 12000 < 48 { 0.7 } else { 0.0 };
        tone + click
    }

    #[test]
    fn flat_engine_is_a_null() {
        let mut e = TransientEq::new(SR);
        let mut max_err = 0.0f64;
        for i in 0..48_000 {
            let x = test_signal(i);
            let (l, _) = e.tick(x, x);
            max_err = max_err.max((l - x).abs());
        }
        assert!(
            max_err < 1.0e-9,
            "flat dual EQ must reconstruct exactly: {max_err:e}"
        );
    }

    #[test]
    fn split_is_complementary_and_reactive() {
        let mut sp = PeakSteadySplitter::new(SR);
        let mut click_mask = 0.0f64;
        let mut tone_mask = 1.0f64;
        for i in 0..96_000 {
            let x = test_signal(i);
            let m = sp.tick_mask(x);
            if i > 24_000 {
                if i % 12000 < 48 {
                    click_mask = click_mask.max(m);
                } else if i % 12000 > 6000 {
                    tone_mask = tone_mask.min(m);
                }
            }
        }
        assert!(
            click_mask > 0.5,
            "clicks should read transient: {click_mask}"
        );
        assert!(tone_mask < 0.2, "held tone should read steady: {tone_mask}");
    }

    #[test]
    fn transient_solo_keeps_clicks_drops_tone() {
        let mut e = TransientEq::new(SR);
        e.solo = StreamSolo::Transient;
        let mut click_peak = 0.0f64;
        let mut tone_rms = 0.0;
        let mut tone_n = 0usize;
        for i in 0..96_000 {
            let x = test_signal(i);
            let (l, _) = e.tick(x, x);
            if i > 24_000 {
                if i % 12000 < 48 {
                    click_peak = click_peak.max(l.abs());
                } else if i % 12000 > 6000 {
                    tone_rms += l * l;
                    tone_n += 1;
                }
            }
        }
        let tone_rms = (tone_rms / tone_n as f64).sqrt();
        assert!(
            click_peak > 0.3,
            "clicks must pass the transient solo: {click_peak}"
        );
        assert!(
            tone_rms < 0.05,
            "held tone must mostly vanish in transient solo: {tone_rms}"
        );
    }
}
