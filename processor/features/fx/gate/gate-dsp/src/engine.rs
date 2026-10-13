//! The gate: key → detector → hold → shaped release, and the HF de-bleed
//! expander riding on a subtractive crossover.
//!
//! Per sample (detector time runs `lookahead` samples ahead of the audio):
//!
//! ```text
//! key_ch   = keyfilter[mode](sidechain_ch or input_ch)
//! env      = peak follower (instant attack, 10 ms release toward |input|) of max_ch |key_ch|
//! open     = env ≥ threshold · (Ghost ? −20 dB : 1)        → ramp = 1, hold = 50 ms
//! closed   = hold counts down, then ramp falls 1/Length per second
//! g        = R + (1 − R) · ramp⁵
//! lo_ch    = BW5 low-pass[mode](delayed input_ch);  hi_ch = delayed input_ch − lo_ch
//! hf_env   = peak follower (instant attack, 60 ms release toward |input|) of max_ch |g · delayed_ch|
//! g_hf     = min(1, hf_env / threshold^a(debleed))
//! out_ch   = output · g · (lo_ch + g_hf · hi_ch)
//! ```

use alloc::vec;
use alloc::vec::Vec;

use dsp_core::num;
use dsp_core::{Channel, PerChannel};

use crate::filter::Bw5;

/// Drum source the gate is voiced for — the reference's four modes.
///
/// A mode is a bundle of fixed constants (no stored knob values): the
/// look-ahead, the detector's key filter and the de-bleed crossover.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// 5 ms look-ahead, key low-pass 1 kHz, crossover 500 Hz.
    #[default]
    Kick,
    /// 1.5 ms look-ahead, key band-pass 130 Hz–1 kHz, crossover 1 kHz.
    SnareTop,
    /// 5 ms look-ahead, key band-pass 130 Hz–1 kHz, crossover 500 Hz.
    SnareBottom,
    /// 5 ms look-ahead, key band-pass 130 Hz–1 kHz, crossover 1 kHz.
    Toms,
}

impl Mode {
    /// All modes, in the reference's parameter order.
    pub const ALL: [Self; 4] = [Self::Kick, Self::SnareTop, Self::SnareBottom, Self::Toms];

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Kick => "Kick",
            Self::SnareTop => "Snare Top",
            Self::SnareBottom => "Snare Bottom",
            Self::Toms => "Toms",
        }
    }

    /// Look-ahead in milliseconds (the latency the gate reports).
    #[must_use]
    pub const fn lookahead_ms(self) -> f64 {
        match self {
            Self::SnareTop => 1.5,
            Self::Kick | Self::SnareBottom | Self::Toms => 5.0,
        }
    }

    /// De-bleed crossover corner, Hz.
    #[must_use]
    pub const fn crossover_hz(self) -> f64 {
        match self {
            Self::Kick | Self::SnareBottom => 500.0,
            Self::SnareTop | Self::Toms => 1000.0,
        }
    }

    /// Key-filter high-pass corner, Hz (`None` for the kick's low-pass-only key).
    #[must_use]
    pub const fn key_high_pass_hz(self) -> Option<f64> {
        match self {
            Self::Kick => None,
            Self::SnareTop | Self::SnareBottom | Self::Toms => Some(KEY_HIGH_PASS_HZ),
        }
    }
}

/// Key-filter low-pass corner, every mode.
pub const KEY_LOW_PASS_HZ: f64 = 1000.0;
/// Key-filter high-pass corner of the snare/tom modes.
pub const KEY_HIGH_PASS_HZ: f64 = 130.0;
/// Main detector release time constant.
pub const DETECTOR_RELEASE_MS: f64 = 10.0;
/// Hold after the detector last saw the key at or above threshold.
pub const HOLD_MS: f64 = 50.0;
/// De-bleed detector release time constant.
pub const DEBLEED_RELEASE_MS: f64 = 60.0;
/// Threshold offset when Ghost is on, dB.
pub const GHOST_OFFSET_DB: f64 = -20.0;
/// Longest look-ahead of any mode, ms (sizes the delay line).
pub const MAX_LOOKAHEAD_MS: f64 = 5.0;

/// The de-bleed threshold exponent: the HF expander's threshold is
/// `threshold^a`, i.e. `a · threshold_dB`.
///
/// Measured piecewise-linear in the Debleed control (−100…+100) with a knee
/// at +10: `0.65 − 0.015·d` up to it, `0.5 − 0.00375·(d − 10)` above.
#[must_use]
pub fn debleed_exponent(debleed: f64) -> f64 {
    let d = debleed.clamp(-100.0, 100.0);
    if d <= 10.0 {
        0.65 - 0.015 * d
    } else {
        0.5 - 0.003_75 * (d - 10.0)
    }
}

/// The gate's controls, in the reference's units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Source voicing.
    pub mode: Mode,
    /// Open threshold, dBFS of the key's peak (−70…0).
    pub threshold_db: f64,
    /// Closed-gate gain, dB (−80…0).
    pub reduction_db: f64,
    /// Release duration from full open to the reduction floor, ms (50…2000).
    pub length_ms: f64,
    /// HF de-bleed amount (−100…+100).
    pub debleed: f64,
    /// Let quieter hits through (threshold −20 dB).
    pub ghost: bool,
    /// Output gain, dB (−48…+6).
    pub output_db: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Kick,
            threshold_db: -30.0,
            reduction_db: -80.0,
            length_ms: 300.0,
            debleed: 0.0,
            ghost: false,
            output_db: 0.0,
        }
    }
}

/// Peak follower: instant attack, one-pole release toward the rectified input.
#[derive(Clone, Copy, Debug, Default)]
struct PeakFollower {
    coeff: f64,
    env: f64,
}

impl PeakFollower {
    fn new(sample_rate: f64, release_ms: f64) -> Self {
        Self {
            coeff: libm::exp(-1.0 / (release_ms * 0.001 * sample_rate)),
            env: 0.0,
        }
    }

    #[inline]
    fn tick(&mut self, rectified: f64) -> f64 {
        self.env = if rectified > self.env {
            rectified
        } else {
            rectified + self.coeff * (self.env - rectified)
        };
        self.env
    }
}

/// Fixed-capacity delay line with a fractional, linearly interpolated
/// length (allocated once, never on the hot path).
///
/// The reference's look-ahead is a time, not a sample count: 5 ms at
/// 44.1 kHz is 220.5 samples, read as the mean of two neighbours — so its
/// open path is `cos(πf/SR)`-dull there, and this reproduces that.
#[derive(Clone, Debug, Default)]
struct Delay {
    buf: Vec<f64>,
    pos: usize,
    len: usize,
    frac: f64,
}

impl Delay {
    fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0.0; capacity.max(1)],
            pos: 0,
            len: 0,
            frac: 0.0,
        }
    }

    fn set_len(&mut self, samples: f64) {
        let whole = num::f64_to_index(samples);
        self.len = whole.min(self.buf.len().saturating_sub(2));
        self.frac = samples - num::count_to_f64(self.len);
    }

    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        let cap = self.buf.len();
        if let Some(slot) = self.buf.get_mut(self.pos) {
            *slot = x;
        }
        let at = |back: usize| {
            let i = self
                .pos
                .checked_sub(back)
                .unwrap_or_else(|| self.pos.wrapping_add(cap).wrapping_sub(back));
            self.buf.get(i).copied().unwrap_or(0.0)
        };
        let y0 = at(self.len);
        let y = if self.frac > 0.0 {
            (1.0 - self.frac) * y0 + self.frac * at(self.len.wrapping_add(1))
        } else {
            y0
        };
        let next = self.pos.wrapping_add(1);
        self.pos = if next >= cap { 0 } else { next };
        y
    }

    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
        self.pos = 0;
    }
}

/// Per-channel filter state.
#[derive(Clone, Copy, Debug, Default)]
struct ChannelState {
    key_lp: Bw5,
    key_hp: Bw5,
    xover: Bw5,
}

/// The drum gate.
#[derive(Clone, Debug)]
pub struct DrumGate {
    sample_rate: f64,
    settings: Settings,
    channels: PerChannel<ChannelState>,
    delays: Vec<Delay>,
    /// Look-ahead delays for an external key: with a sidechain, the
    /// de-bleed detector reads the (delayed) key instead of the input.
    key_delays: Vec<Delay>,
    key_hp_on: bool,
    detector: PeakFollower,
    debleed_det: PeakFollower,
    // derived
    open_level: f64,
    floor: f64,
    hf_threshold: f64,
    output: f64,
    ramp_step: f64,
    hold_samples: usize,
    lookahead: usize,
    // state
    ramp: f64,
    hold_left: usize,
}

impl DrumGate {
    /// A gate for up to `channels` channels at `sample_rate`.
    #[must_use]
    pub fn new(sample_rate: f64, channels: usize, settings: Settings) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let capacity = num::f64_to_index(libm::ceil(MAX_LOOKAHEAD_MS * 0.001 * sample_rate)).saturating_add(3);
        let mut gate = Self {
            sample_rate,
            settings,
            channels: PerChannel::default(),
            delays: (0..channels.clamp(1, dsp_core::channel::MAX_CHANNELS))
                .map(|_| Delay::new(capacity))
                .collect(),
            key_delays: (0..channels.clamp(1, dsp_core::channel::MAX_CHANNELS))
                .map(|_| Delay::new(capacity))
                .collect(),
            key_hp_on: false,
            detector: PeakFollower::new(sample_rate, DETECTOR_RELEASE_MS),
            debleed_det: PeakFollower::new(sample_rate, DEBLEED_RELEASE_MS),
            open_level: 0.0,
            floor: 0.0,
            hf_threshold: 0.0,
            output: 1.0,
            ramp_step: 0.0,
            hold_samples: 0,
            lookahead: 0,
            ramp: 0.0,
            hold_left: 0,
        };
        gate.set_settings(settings);
        gate
    }

    /// Latency in samples for `mode` at `sample_rate` (the look-ahead):
    /// the time truncated to whole samples, as the reference does
    /// (5 ms at 44.1 kHz is 220, not 221).
    #[must_use]
    pub fn latency_for(mode: Mode, sample_rate: f64) -> usize {
        num::f64_to_index(mode.lookahead_ms() * 0.001 * sample_rate)
    }

    /// Current latency in samples.
    #[must_use]
    pub const fn latency(&self) -> usize {
        self.lookahead
    }

    /// Current settings.
    #[must_use]
    pub const fn settings(&self) -> Settings {
        self.settings
    }

    /// Apply new settings. A mode change rebuilds the filters (and so
    /// clears their state); everything else takes effect immediately.
    pub fn set_settings(&mut self, s: Settings) {
        let mode_changed = s.mode != self.settings.mode || self.lookahead == 0;
        self.settings = s;
        let sr = self.sample_rate;
        if mode_changed {
            let hp = s.mode.key_high_pass_hz();
            self.key_hp_on = hp.is_some();
            for ch in &mut self.channels {
                ch.key_lp = Bw5::new(sr, KEY_LOW_PASS_HZ, false);
                ch.key_hp = Bw5::new(sr, hp.unwrap_or(KEY_HIGH_PASS_HZ), true);
                ch.xover = Bw5::new(sr, s.mode.crossover_hz(), false);
            }
            self.lookahead = Self::latency_for(s.mode, sr);
            let exact = s.mode.lookahead_ms() * 0.001 * sr;
            for d in self.delays.iter_mut().chain(&mut self.key_delays) {
                d.set_len(exact);
            }
        }
        // Ghost lowers the effective threshold, and the de-bleed threshold
        // is derived from that same lowered value.
        let thr_db = s.threshold_db + if s.ghost { GHOST_OFFSET_DB } else { 0.0 };
        self.open_level = db_to_gain(thr_db);
        self.floor = db_to_gain(s.reduction_db);
        self.hf_threshold = db_to_gain(debleed_exponent(s.debleed) * thr_db);
        self.output = output_gain(s.output_db);
        self.ramp_step = 1.0 / (s.length_ms.max(1.0) * 0.001 * sr);
        self.hold_samples = num::f64_to_index(libm::round(HOLD_MS * 0.001 * sr));
    }

    /// Clear all signal state (gate closed).
    pub fn reset(&mut self) {
        for ch in &mut self.channels {
            ch.key_lp.reset();
            ch.key_hp.reset();
            ch.xover.reset();
        }
        for d in self.delays.iter_mut().chain(&mut self.key_delays) {
            d.reset();
        }
        self.detector.env = 0.0;
        self.debleed_det.env = 0.0;
        self.ramp = 0.0;
        self.hold_left = 0;
    }

    /// Main gate gain of the last processed frame (linear).
    #[must_use]
    pub fn gain(&self) -> f64 {
        self.floor + (1.0 - self.floor) * pow5(self.ramp)
    }

    /// Process one frame in place. `frame` holds one sample per channel
    /// (only the first `channels` given to [`Self::new`] are used);
    /// `key` is the sidechain frame, or `None` to key from the input.
    pub fn process_frame(&mut self, frame: &mut [f64], key: Option<&[f64]>) {
        let n = frame.len().min(self.delays.len());
        // Detector (look-ahead side): key-filter each channel, take the max.
        let mut peak = 0.0f64;
        for (i, &x) in frame.iter().take(n).enumerate() {
            let k_in = key.and_then(|k| k.get(i).copied()).unwrap_or(x);
            let ch = &mut self.channels[Channel::new(i)];
            let mut k = ch.key_lp.tick(k_in);
            if self.key_hp_on {
                k = ch.key_hp.tick(k);
            }
            peak = peak.max(k.abs());
        }
        let env = self.detector.tick(peak);
        if env >= self.open_level {
            self.ramp = 1.0;
            self.hold_left = self.hold_samples;
        } else if self.hold_left > 0 {
            self.hold_left = self.hold_left.saturating_sub(1);
        } else {
            self.ramp = (self.ramp - self.ramp_step).max(0.0);
        }
        let g = self.gain();

        // Audio side: delay, split, de-bleed. The de-bleed detector hears
        // the gated key — the input itself unless a sidechain is given.
        let mut hf_peak = 0.0f64;
        for (i, s) in frame.iter_mut().take(n).enumerate() {
            let xd = self.delays.get_mut(i).map_or(0.0, |d| d.tick(*s));
            let heard = match key {
                Some(k) => {
                    let k_in = k.get(i).copied().unwrap_or(0.0);
                    self.key_delays.get_mut(i).map_or(0.0, |d| d.tick(k_in))
                }
                None => xd,
            };
            *s = xd;
            hf_peak = hf_peak.max((g * heard).abs());
        }
        let hf_env = self.debleed_det.tick(hf_peak);
        let g_hf = (hf_env / self.hf_threshold).min(1.0);
        for (i, s) in frame.iter_mut().take(n).enumerate() {
            // The split runs on the already-gated signal (gain before the
            // crossover), so the low-pass carries the pre-opening history.
            let v = g * *s;
            let lo = self.channels[Channel::new(i)].xover.tick(v);
            *s = self.output * (lo + g_hf * (v - lo));
        }
    }
}

/// The reference's Output Gain law, dB → linear.
///
/// Measured exact at every multiple of 6 dB; between them the realised dB is
/// `6n + 1.0016366·(x − 6n)` with `n = round(x/6)` (half up) — a ±0.005 dB
/// sawtooth, as if a 6 dB node table were interpolated with a slightly off
/// exponent. Reduction and Threshold do not have it.
#[must_use]
pub fn output_gain(db: f64) -> f64 {
    let n = libm::floor(db / 6.0 + 0.5) * 6.0;
    db_to_gain(n + OUTPUT_LAW_SLOPE * (db - n))
}

/// Slope of the Output Gain law within one 6 dB segment (see [`output_gain`]).
pub const OUTPUT_LAW_SLOPE: f64 = 1.001_636_6;

/// dB to linear gain (libm: identical on every platform).
#[must_use]
pub fn db_to_gain(db: f64) -> f64 {
    libm::pow(10.0, db / 20.0)
}

#[inline]
fn pow5(x: f64) -> f64 {
    let x2 = x * x;
    x2 * x2 * x
}
