//! Envelope-follower modulation source.
//!
//! A continuous (non-triggered) counterpart to the pattern lanes: the
//! detector tracks the input's amplitude through attack / hold /
//! release stages and emits a 0..1 modulation value. Based on tiagolr's
//! REEV-R Follower lane (clean-room: concept and control set only).
//!
//! Control set:
//! - `attack_s` / `release_s` — envelope ballistics.
//! - `hold_s` — peak hold: the envelope refuses to fall for this long
//!   after each new peak (keeps short transients from fluttering).
//! - `threshold` — detector gate: input magnitude below the threshold
//!   reads as zero (the follower ignores the noise floor).
//! - `lowcut_freq` / `highcut_freq` — detector-only pre-filters, so a
//!   bass-heavy mix can drive the follower from just the kick, or just
//!   the top end (0 = disabled).
//! - `auto_release` — scales release with the observed envelope level
//!   so loud passages let go faster (program-dependent release).
//! - `depth` / `invert` — output mapping: `invert` turns the follower
//!   into a ducker (loud input → low output).

use audiocore_dsp::biquad::{Biquad, FilterType};

pub struct EnvFollower {
    /// Attack time in seconds.
    pub attack_s: f64,
    /// Peak-hold time in seconds.
    pub hold_s: f64,
    /// Release time in seconds.
    pub release_s: f64,
    /// Detector gate threshold (linear amplitude).
    pub threshold: f64,
    /// Detector highpass (0 = off).
    pub lowcut_freq: f64,
    /// Detector lowpass (0 = off).
    pub highcut_freq: f64,
    /// Program-dependent release amount (0 = fixed release).
    pub auto_release: f64,
    /// Output depth (0..1): out = 1 ± depth·env.
    pub depth: f64,
    /// Duck instead of follow: out = 1 − depth·env.
    pub invert: bool,
    /// Input gain into the detector (drive into the 0..1 range).
    pub gain: f64,

    envelope: f64,
    hold_remaining: f64,
    attack_coeff: f64,
    release_coeff: f64,
    lowcut: Biquad,
    highcut: Biquad,
    sample_rate: f64,
}

impl EnvFollower {
    pub fn new() -> Self {
        Self {
            attack_s: 0.005,
            hold_s: 0.0,
            release_s: 0.15,
            threshold: 0.0,
            lowcut_freq: 0.0,
            highcut_freq: 0.0,
            auto_release: 0.0,
            depth: 1.0,
            invert: false,
            gain: 2.0,
            envelope: 0.0,
            hold_remaining: 0.0,
            attack_coeff: 1.0,
            release_coeff: 1.0,
            lowcut: Biquad::new(),
            highcut: Biquad::new(),
            sample_rate: 48000.0,
        }
    }

    /// Recompute coefficients. Call from setup and after changing the
    /// time parameters, never per sample.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.attack_coeff = coeff(self.attack_s, sample_rate);
        self.release_coeff = coeff(self.release_s, sample_rate);
        if self.lowcut_freq > 0.0 {
            self.lowcut
                .set(FilterType::Highpass, self.lowcut_freq, 0.707, sample_rate);
        }
        if self.highcut_freq > 0.0 {
            self.highcut
                .set(FilterType::Lowpass, self.highcut_freq, 0.707, sample_rate);
        }
    }

    /// Track one input sample; returns the raw envelope (0..1).
    #[inline]
    pub fn track(&mut self, input: f64) -> f64 {
        let mut x = input;
        if self.lowcut_freq > 0.0 {
            x = self.lowcut.tick(x, 0);
        }
        if self.highcut_freq > 0.0 {
            x = self.highcut.tick(x, 0);
        }
        let mut mag = (x * self.gain).abs();
        if mag < self.threshold {
            mag = 0.0;
        }
        mag = mag.min(1.0);

        if mag > self.envelope {
            self.envelope += (mag - self.envelope) * self.attack_coeff;
            self.hold_remaining = self.hold_s * self.sample_rate;
        } else if self.hold_remaining > 0.0 {
            self.hold_remaining -= 1.0;
        } else {
            // Program-dependent release: high envelopes let go faster.
            let scale = 1.0 + self.auto_release.clamp(0.0, 1.0) * 3.0 * self.envelope;
            self.envelope -= self.envelope * self.release_coeff * scale;
        }
        self.envelope
    }

    /// One sample in → the mapped modulation gain out.
    ///
    /// Follow mode rides UP from `1 − depth` toward 1 with the input;
    /// invert (duck) mode rides DOWN from 1 toward `1 − depth`.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let env = self.track(input);
        let d = self.depth.clamp(0.0, 1.0);
        if self.invert {
            1.0 - d * env
        } else {
            (1.0 - d) + d * env
        }
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0;
        self.hold_remaining = 0.0;
        self.lowcut.reset();
        self.highcut.reset();
    }
}

impl Default for EnvFollower {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn coeff(time_s: f64, sample_rate: f64) -> f64 {
    if time_s <= 0.0 {
        1.0
    } else {
        1.0 - (-1.0 / (time_s * sample_rate)).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn follows_a_burst_up_and_down() {
        let mut f = EnvFollower::new();
        f.attack_s = 0.001;
        f.release_s = 0.05;
        f.update(SR);
        let mut peak = 0.0f64;
        for _ in 0..4800 {
            peak = peak.max(f.tick(0.8));
        }
        assert!(peak > 0.9, "should ride up on a loud burst: {peak}");
        let mut out = 0.0;
        for _ in 0..48_000 {
            out = f.tick(0.0);
        }
        assert!(out < 0.05, "should release to depth floor: {out}");
    }

    #[test]
    fn invert_ducks() {
        let mut f = EnvFollower::new();
        f.invert = true;
        f.attack_s = 0.001;
        f.update(SR);
        let mut low = 1.0f64;
        for _ in 0..4800 {
            low = low.min(f.tick(0.9));
        }
        assert!(low < 0.1, "duck mode should push gain down: {low}");
    }

    #[test]
    fn hold_delays_the_release() {
        let run = |hold_s: f64| -> f64 {
            let mut f = EnvFollower::new();
            f.attack_s = 0.001;
            f.hold_s = hold_s;
            f.release_s = 0.02;
            f.update(SR);
            for _ in 0..2400 {
                f.tick(0.8);
            }
            // 30 ms of silence: with a 100 ms hold the envelope must
            // still be pinned; without it, mostly released.
            let mut out = 0.0;
            for _ in 0..1440 {
                out = f.tick(0.0);
            }
            out
        };
        assert!(run(0.1) > 0.9, "hold should pin the envelope");
        assert!(run(0.0) < 0.5, "no hold should release");
    }

    #[test]
    fn detector_filters_select_the_band() {
        // Low-frequency drive through a highpassed detector barely
        // registers; the same drive with no filter rides up.
        let run = |lowcut: f64| -> f64 {
            let mut f = EnvFollower::new();
            f.lowcut_freq = lowcut;
            f.attack_s = 0.002;
            f.update(SR);
            let mut peak = 0.0f64;
            for i in 0..9600 {
                let x = (core::f64::consts::TAU * 60.0 * i as f64 / SR).sin() * 0.7;
                peak = peak.max(f.track(x));
            }
            peak
        };
        let open = run(0.0);
        let filtered = run(2000.0);
        assert!(
            filtered < open * 0.3,
            "highpassed detector should ignore 60 Hz: open={open} filtered={filtered}"
        );
    }

    #[test]
    fn auto_release_lets_go_faster() {
        let run = |auto: f64| -> f64 {
            let mut f = EnvFollower::new();
            f.attack_s = 0.001;
            f.release_s = 0.2;
            f.auto_release = auto;
            f.update(SR);
            for _ in 0..4800 {
                f.tick(0.9);
            }
            let mut out = 0.0;
            for _ in 0..2400 {
                out = f.tick(0.0);
            }
            out
        };
        assert!(
            run(1.0) < run(0.0) * 0.8,
            "auto-release should shorten the tail of loud material"
        );
    }
}
