//! MultiTapDelay — 8 user-editable taps (TimeLine MX MultiTap).
//!
//! Per spec/timeline-mx-reference.md: 8 taps on one delay line, each with
//! step position, level, pan, per-tap repeats (feedback contribution),
//! per-tap filter (9 types) + cutoff, and per-tap mod amount. Master
//! Time/Repeats scale the whole pattern relative to per-tap settings.
//!
//! Topology: `FeedbackMode::Input` recirculates the per-tap feedback sum
//! into the ONE shared line; `FeedbackMode::Parallel` runs 8 fully
//! independent lines, each recirculating only itself (allocated in
//! `update()` when the mode is selected). [`MultiTapDelay::tick_stereo`]
//! applies per-tap equal-power pan and per-tap `mod_amount` (scales the
//! shared mod LFO's excursion on that tap's read position).
//!
//! Remaining deviations (documented for later passes):
//! - `TapGrid` quantization is metadata for editors; positions here are
//!   free fractions of the pattern length.
//! - The Classic bank carries 6 patterns, not all 16 v1 patterns yet.
//! - In Parallel mode the global hicut/locut shape the line INPUT (per
//!   line-loop filtering would need 8× filter state); decay tilt is
//!   skipped. Per-tap filters run inside each line's own loop, so
//!   filtered repeats self-darken authentically in both modes.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::smoothing::ParamSmoother;

pub const MAX_TAPS: usize = 8;

/// Per-tap filter type (TimeLine MX's 9 options).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TapFilter {
    #[default]
    Off,
    Lowpass,
    LowpassPeak,
    Highpass,
    HighpassPeak,
    Bandpass,
    BandpassPeak,
    LowShelf,
    HighShelf,
}

impl TapFilter {
    /// Configure a biquad for this tap filter. Peaking variants use a
    /// resonant Q; shelves use a fixed ±6 dB tilt.
    fn configure(self, bq: &mut Biquad, cutoff: f64, sample_rate: f64) {
        let f = cutoff.clamp(40.0, 12000.0);
        match self {
            TapFilter::Off => {}
            TapFilter::Lowpass => bq.set(FilterType::Lowpass, f, 0.707, sample_rate),
            TapFilter::LowpassPeak => bq.set(FilterType::Lowpass, f, 2.5, sample_rate),
            TapFilter::Highpass => bq.set(FilterType::Highpass, f, 0.707, sample_rate),
            TapFilter::HighpassPeak => bq.set(FilterType::Highpass, f, 2.5, sample_rate),
            TapFilter::Bandpass => bq.set(FilterType::Bandpass, f, 1.0, sample_rate),
            TapFilter::BandpassPeak => bq.set(FilterType::Bandpass, f, 3.5, sample_rate),
            TapFilter::LowShelf => {
                bq.set(FilterType::LowShelf { gain_db: 6.0 }, f, 0.707, sample_rate)
            }
            TapFilter::HighShelf => {
                bq.set(FilterType::HighShelf { gain_db: 6.0 }, f, 0.707, sample_rate)
            }
        }
    }
}

/// Step-grid mode (editor quantization metadata; DSP uses free positions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TapGrid {
    #[default]
    Sixteenth,
    Triplet,
    /// Free: 1–256 steps across the 4-beat pattern.
    Off,
}

/// How tap feedback recirculates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FeedbackMode {
    /// Taps recirculate into the common input (interacting repeats).
    #[default]
    Input,
    /// 8 independent delay lines, summed, no interaction.
    Parallel,
}

/// One tap in the pattern.
#[derive(Debug, Clone, Copy)]
pub struct Tap {
    pub enabled: bool,
    /// Position as a fraction of the base delay time (0.0–1.0].
    pub position: f64,
    /// Output level (0.0–1.0).
    pub level: f64,
    /// Stereo pan (-1.0–1.0). Stored for parity; applied in the deep pass.
    pub pan: f64,
    /// Per-tap feedback contribution (0.0–1.0), scaled by master feedback.
    pub repeats: f64,
    /// Per-tap filter on this tap's output.
    pub filter: TapFilter,
    /// Per-tap filter cutoff in Hz.
    pub cutoff: f64,
    /// Per-tap modulation amount (0.0–1.0): scales the shared mod LFO's
    /// excursion on this tap's read position.
    pub mod_amount: f64,
}

impl Tap {
    pub const fn off() -> Self {
        Self {
            enabled: false,
            position: 1.0,
            level: 0.0,
            pan: 0.0,
            repeats: 0.0,
            filter: TapFilter::Off,
            cutoff: 2000.0,
            mod_amount: 0.0,
        }
    }

    pub const fn at(position: f64, level: f64) -> Self {
        Self {
            enabled: true,
            position,
            level,
            pan: 0.0,
            repeats: if position == 1.0 { 1.0 } else { 0.0 },
            filter: TapFilter::Off,
            cutoff: 2000.0,
            mod_amount: 0.0,
        }
    }
}

/// Built-in tap-pattern presets (the start of the MX "Classic" bank).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapPreset {
    /// Classic 1: simple ping-pong (alternating half/full positions).
    Classic1PingPong,
    /// Four even quarters, decaying.
    Quarters,
    /// Dotted-eighth + quarter "U2" figure.
    DottedEighth,
    /// Golden-ratio cascade (Echorec-ish).
    Golden,
    /// Dense 8-tap early-reflection cluster.
    EarlyReflections,
    /// Accelerando: taps bunch up toward the delay time.
    Accelerando,
}

impl TapPreset {
    pub fn taps(self) -> [Tap; MAX_TAPS] {
        let mut taps = [Tap::off(); MAX_TAPS];
        match self {
            TapPreset::Classic1PingPong => {
                // Alternating L/R halves; pan takes effect in the deep pass.
                taps[0] = Tap::at(0.5, 0.9);
                taps[0].pan = -1.0;
                taps[1] = Tap::at(1.0, 0.9);
                taps[1].pan = 1.0;
            }
            TapPreset::Quarters => {
                for (i, t) in [0.25, 0.5, 0.75, 1.0].iter().enumerate() {
                    taps[i] = Tap::at(*t, 1.0 - i as f64 * 0.2);
                }
            }
            TapPreset::DottedEighth => {
                taps[0] = Tap::at(0.375, 0.8);
                taps[1] = Tap::at(0.75, 0.6);
                taps[2] = Tap::at(1.0, 1.0);
            }
            TapPreset::Golden => {
                for (i, t) in [0.146, 0.236, 0.382, 0.618, 1.0].iter().enumerate() {
                    taps[i] = Tap::at(*t, 0.5 + 0.5 * t);
                }
            }
            TapPreset::EarlyReflections => {
                let positions = [0.06, 0.11, 0.17, 0.25, 0.36, 0.5, 0.71, 1.0];
                for (i, t) in positions.iter().enumerate() {
                    taps[i] = Tap::at(*t, 0.9 - i as f64 * 0.09);
                }
            }
            TapPreset::Accelerando => {
                let positions = [0.5, 0.75, 0.875, 0.9375, 0.96875, 1.0];
                for (i, t) in positions.iter().enumerate() {
                    taps[i] = Tap::at(*t, 0.5 + i as f64 * 0.1);
                }
            }
        }
        taps
    }
}

pub struct MultiTapDelay {
    /// Base delay time in ms (clamped to 60–2500).
    pub time_ms: f64,
    /// Master feedback scaler over the per-tap `repeats` sum (0.0–1.0).
    pub feedback: f64,
    /// The tap pattern.
    pub taps: [Tap; MAX_TAPS],
    /// Step grid (editor metadata).
    pub grid: TapGrid,
    /// Feedback topology.
    pub feedback_mode: FeedbackMode,
    /// High-cut in the feedback path (0 = off).
    pub hicut_freq: f64,
    /// Low-cut in the feedback path (0 = off).
    pub locut_freq: f64,
    /// Decay EQ tilt (shared engine param).
    pub decay_tilt: f64,
    /// Shared tap-modulation LFO rate in Hz.
    pub mod_rate_hz: f64,
    /// Shared tap-modulation depth (0.0–1.0); per-tap `mod_amount`
    /// scales it per tap. Max excursion ≈ 4 ms.
    pub mod_depth: f64,

    delay: DelayLine,
    /// Independent per-tap lines for `FeedbackMode::Parallel`; empty
    /// until that mode is selected (allocated in `update()`).
    parallel_lines: Vec<DelayLine>,
    tap_filters: [Biquad; MAX_TAPS],
    hicut: Biquad,
    locut: Biquad,
    decay_eq: Biquad,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    mod_phase: f64,
}

impl MultiTapDelay {
    pub const MIN_TIME_MS: f64 = 60.0;
    pub const MAX_TIME_MS: f64 = 2500.0;
    const MAX_DELAY_S: f64 = 3.0;

    pub fn new() -> Self {
        Self {
            time_ms: 500.0,
            feedback: 0.3,
            taps: TapPreset::Quarters.taps(),
            grid: TapGrid::default(),
            feedback_mode: FeedbackMode::default(),
            hicut_freq: 0.0,
            locut_freq: 0.0,
            decay_tilt: 0.0,
            mod_rate_hz: 0.5,
            mod_depth: 0.0,
            delay: DelayLine::new(48000 * 3 + 1024),
            parallel_lines: Vec::new(),
            tap_filters: std::array::from_fn(|_| Biquad::new()),
            hicut: Biquad::new(),
            locut: Biquad::new(),
            decay_eq: Biquad::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            mod_phase: 0.0,
        }
    }

    pub fn set_preset(&mut self, preset: TapPreset) {
        self.taps = preset.taps();
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.time_ms = self.time_ms.clamp(Self::MIN_TIME_MS, Self::MAX_TIME_MS);

        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

        // Parallel mode: allocate/grow the 8 independent lines here so
        // the tick path never allocates.
        if self.feedback_mode == FeedbackMode::Parallel {
            if self.parallel_lines.len() < MAX_TAPS {
                self.parallel_lines = (0..MAX_TAPS).map(|_| DelayLine::new(max_len)).collect();
            } else if self.parallel_lines[0].len() < max_len {
                for line in &mut self.parallel_lines {
                    *line = DelayLine::new(max_len);
                }
            }
        }

        for (tap, bq) in self.taps.iter().zip(self.tap_filters.iter_mut()) {
            tap.filter.configure(bq, tap.cutoff, sample_rate);
        }

        if self.hicut_freq > 0.0 {
            self.hicut
                .set(FilterType::Lowpass, self.hicut_freq, 0.707, sample_rate);
        }
        if self.locut_freq > 0.0 {
            self.locut
                .set(FilterType::Highpass, self.locut_freq, 0.707, sample_rate);
        }
        if self.decay_tilt.abs() > 0.01 {
            if self.decay_tilt < 0.0 {
                let freq = 20000.0 * (1.0 + self.decay_tilt).max(0.05);
                self.decay_eq
                    .set(FilterType::Lowpass, freq, 0.707, sample_rate);
            } else {
                let freq = 20.0 + self.decay_tilt * 2000.0;
                self.decay_eq
                    .set(FilterType::Highpass, freq, 0.707, sample_rate);
            }
        }

        self.smoother.set_time(0.15, sample_rate);
        let target = self.time_ms * 0.001 * sample_rate;
        if self.smoother.value() == 0.0 {
            self.smoother.set_immediate(target);
        }
    }

    /// Advance the time smoother + shared mod LFO phase; returns the
    /// smoothed delay in samples.
    #[inline]
    fn advance_clock(&mut self) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();
        self.mod_phase += self.mod_rate_hz.clamp(0.01, 20.0) / self.sample_rate;
        if self.mod_phase >= 1.0 {
            self.mod_phase -= 1.0;
        }
        smooth_delay
    }

    /// Read position for tap `i`, including per-tap modulation
    /// (`mod_amount` scales the shared LFO; per-tap phase offsets
    /// decorrelate the taps; excursion capped at ~4 ms).
    #[inline]
    fn tap_pos(&self, tap: &Tap, i: usize, smooth_delay: f64, max_read: f64) -> f64 {
        let mut pos = smooth_delay * tap.position;
        let amt = self.mod_depth * tap.mod_amount;
        if amt > 0.0 {
            let ph = (self.mod_phase + i as f64 * 0.125).fract();
            let lfo = (std::f64::consts::TAU * ph).sin();
            let excursion = (self.sample_rate * 0.004).min(pos * 0.02);
            pos += lfo * amt * excursion;
        }
        pos.clamp(1.0, max_read)
    }

    /// True whenever the independent-lines topology is running
    /// (Parallel selected AND lines allocated by `update()`).
    #[inline]
    fn parallel_active(&self) -> bool {
        self.feedback_mode == FeedbackMode::Parallel && !self.parallel_lines.is_empty()
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let (l, r) = self.tick_inner(input, ch, false);
        // Mono: pans ignored; tick_inner returns the plain sum in `l`.
        debug_assert_eq!(r, 0.0);
        l
    }

    /// Stereo tick: per-tap equal-power pan (unity at center).
    pub fn tick_stereo(&mut self, input: f64) -> (f64, f64) {
        self.tick_inner(input, 0, true)
    }

    fn tick_inner(&mut self, input: f64, ch: usize, stereo: bool) -> (f64, f64) {
        let smooth_delay = self.advance_clock();

        if self.parallel_active() {
            return self.tick_parallel(input, ch, stereo, smooth_delay);
        }

        let max_read = self.delay.len() as f64 - 4.0;
        let mut out_l = 0.0;
        let mut out_r = 0.0;
        let mut fb_sum = 0.0;
        for i in 0..MAX_TAPS {
            let tap = self.taps[i];
            if !tap.enabled {
                continue;
            }
            let pos = self.tap_pos(&tap, i, smooth_delay, max_read);
            let mut sample = self.delay.read_cubic(pos);
            if tap.filter != TapFilter::Off {
                sample = self.tap_filters[i].tick(sample, ch);
            }
            if stereo {
                let (gl, gr) = crate::pan_gains(tap.pan);
                out_l += sample * tap.level * gl;
                out_r += sample * tap.level * gr;
            } else {
                out_l += sample * tap.level;
            }
            // Per-tap repeats: this tap's (filtered) signal recirculates.
            fb_sum += sample * tap.repeats;
        }

        let mut fb = fb_sum * self.feedback;
        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }
        if self.locut_freq > 0.0 {
            fb = self.locut.tick(fb, ch);
        }
        if self.decay_tilt.abs() > 0.01 {
            fb = self.decay_eq.tick(fb, ch);
        }
        fb = fb.clamp(-1.5, 1.5);

        self.delay.write(input + fb);
        self.feedback_sample = fb;

        (out_l, out_r)
    }

    /// Parallel topology: each tap is an independent line recirculating
    /// only itself (per-tap filter inside its own loop, so filtered
    /// repeats self-darken). Global hicut/locut shape the line input;
    /// decay tilt is skipped in this mode (see module docs).
    fn tick_parallel(&mut self, input: f64, ch: usize, stereo: bool, smooth_delay: f64) -> (f64, f64) {
        let max_read = self.parallel_lines[0].len() as f64 - 4.0;

        let mut line_input = input;
        if self.hicut_freq > 0.0 {
            line_input = self.hicut.tick(line_input, ch);
        }
        if self.locut_freq > 0.0 {
            line_input = self.locut.tick(line_input, ch);
        }

        let mut out_l = 0.0;
        let mut out_r = 0.0;
        let mut fb_avg = 0.0;
        for i in 0..MAX_TAPS {
            let tap = self.taps[i];
            if !tap.enabled {
                // Keep disabled lines primed so enabling a tap later
                // plays history instead of silence.
                self.parallel_lines[i].write(line_input);
                continue;
            }
            let pos = self.tap_pos(&tap, i, smooth_delay, max_read);
            let mut sample = self.parallel_lines[i].read_cubic(pos);
            if tap.filter != TapFilter::Off {
                sample = self.tap_filters[i].tick(sample, ch);
            }
            if stereo {
                let (gl, gr) = crate::pan_gains(tap.pan);
                out_l += sample * tap.level * gl;
                out_r += sample * tap.level * gr;
            } else {
                out_l += sample * tap.level;
            }
            let fb = (sample * tap.repeats * self.feedback).clamp(-1.5, 1.5);
            self.parallel_lines[i].write(line_input + fb);
            fb_avg += fb;
        }
        self.feedback_sample = fb_avg / MAX_TAPS as f64;

        (out_l, out_r)
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        for line in &mut self.parallel_lines {
            line.clear();
        }
        for bq in &mut self.tap_filters {
            bq.reset();
        }
        self.hicut.reset();
        self.locut.reset();
        self.decay_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.mod_phase = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn taps_land_at_configured_positions() {
        let mut d = MultiTapDelay::new();
        d.time_ms = 800.0;
        d.feedback = 0.0;
        d.taps = [Tap::off(); MAX_TAPS];
        d.taps[0] = Tap::at(0.25, 1.0);
        d.taps[1] = Tap::at(1.0, 1.0);
        d.update(SR);

        let mut hits = Vec::new();
        for i in 0..96000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            if d.tick(input, 0).abs() > 0.3 {
                hits.push(i as i64);
            }
        }
        let t1 = (200.0 * SR / 1000.0) as i64;
        let t2 = (800.0 * SR / 1000.0) as i64;
        assert!(hits.iter().any(|&h| (h - t1).abs() < 100), "{hits:?}");
        assert!(hits.iter().any(|&h| (h - t2).abs() < 100), "{hits:?}");
    }

    #[test]
    fn disabled_taps_are_silent() {
        let mut d = MultiTapDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.0;
        d.taps = [Tap::off(); MAX_TAPS];
        d.update(SR);

        for i in 0..24000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            assert!(d.tick(input, 0).abs() < 1e-12);
        }
    }

    #[test]
    fn per_tap_repeats_regenerates_only_marked_taps() {
        // One early tap with repeats, one late tap without: the pattern
        // must recirculate at the EARLY tap's period.
        let mut d = MultiTapDelay::new();
        d.time_ms = 400.0;
        d.feedback = 0.8;
        d.taps = [Tap::off(); MAX_TAPS];
        d.taps[0] = Tap::at(0.5, 1.0); // 200 ms
        d.taps[0].repeats = 1.0;
        d.taps[1] = Tap::at(1.0, 1.0); // 400 ms
        d.taps[1].repeats = 0.0;
        d.update(SR);

        let mut out = vec![0.0f64; 48000];
        for (i, o) in out.iter_mut().enumerate() {
            let input = if i == 0 { 1.0 } else { 0.0 };
            *o = d.tick(input, 0);
        }
        // Regeneration of the 200 ms tap → repeats at 400/600 ms too.
        let at = |ms: f64| -> f64 {
            let c = (ms * SR / 1000.0) as usize;
            out[c - 200..c + 200].iter().map(|x| x * x).sum()
        };
        assert!(at(200.0) > 0.1, "first pass: {}", at(200.0));
        assert!(at(600.0) > 0.01, "recirculated early tap: {}", at(600.0));
    }

    #[test]
    fn per_tap_filter_darkens_tap() {
        let run = |filter: TapFilter| -> f64 {
            let mut d = MultiTapDelay::new();
            d.time_ms = 100.0;
            d.feedback = 0.0;
            d.taps = [Tap::off(); MAX_TAPS];
            d.taps[0] = Tap::at(1.0, 1.0);
            d.taps[0].filter = filter;
            d.taps[0].cutoff = 500.0;
            d.update(SR);

            // High-frequency energy of the tap output (first difference).
            let mut hf = 0.0;
            let mut prev = 0.0;
            for i in 0..24000 {
                let input = (std::f64::consts::TAU * 5000.0 * i as f64 / SR).sin() * 0.5;
                let out = d.tick(input, 0);
                hf += (out - prev) * (out - prev);
                prev = out;
            }
            hf
        };
        let open = run(TapFilter::Off);
        let dark = run(TapFilter::Lowpass);
        assert!(
            dark < open * 0.2,
            "500 Hz per-tap LP should kill a 5 kHz tap: {dark} vs {open}"
        );
    }

    #[test]
    fn parallel_mode_isolates_tap_lines() {
        // Tap A: 120 ms, full self-repeats, SILENT (level 0).
        // Tap B: 400 ms, no repeats, audible.
        // Input mode: A's recirculation reaches the shared line, so B
        // re-emits it at 120+400 = 520 ms. Parallel: B's line never sees
        // A's feedback — output is the single 400 ms event only.
        let run = |mode: FeedbackMode| -> (f64, f64) {
            let mut d = MultiTapDelay::new();
            d.time_ms = 400.0;
            d.feedback = 1.0;
            d.feedback_mode = mode;
            d.taps = [Tap::off(); MAX_TAPS];
            d.taps[0] = Tap::at(0.3, 0.0); // silent recirculator
            d.taps[0].repeats = 1.0;
            d.taps[0].enabled = true;
            d.taps[1] = Tap::at(1.0, 1.0); // audible, no repeats
            d.taps[1].repeats = 0.0;
            d.update(SR);

            let mut out = vec![0.0f64; 48000];
            for (i, o) in out.iter_mut().enumerate() {
                let input = if i == 0 { 1.0 } else { 0.0 };
                *o = d.tick(input, 0);
            }
            let window = |ms: f64| -> f64 {
                let c = (ms * SR / 1000.0) as usize;
                out[c - 480..c + 480].iter().map(|x| x * x).sum()
            };
            (window(400.0), window(520.0))
        };

        let (input_400, input_520) = run(FeedbackMode::Input);
        let (par_400, par_520) = run(FeedbackMode::Parallel);

        assert!(input_400 > 0.1 && par_400 > 0.1, "both modes emit the 400 ms tap");
        assert!(
            input_520 > 0.01,
            "Input mode: A's recirculation must reach B: {input_520}"
        );
        assert!(
            par_520 < input_520 * 1e-4,
            "Parallel mode: no cross-pollination: {par_520} vs {input_520}"
        );
    }

    #[test]
    fn parallel_taps_repeat_at_their_own_period() {
        let mut d = MultiTapDelay::new();
        d.time_ms = 400.0;
        d.feedback = 0.9;
        d.feedback_mode = FeedbackMode::Parallel;
        d.taps = [Tap::off(); MAX_TAPS];
        d.taps[0] = Tap::at(0.5, 1.0); // 200 ms line, self-repeating
        d.taps[0].repeats = 1.0;
        d.update(SR);

        let mut out = vec![0.0f64; 48000];
        for (i, o) in out.iter_mut().enumerate() {
            let input = if i == 0 { 1.0 } else { 0.0 };
            *o = d.tick(input, 0);
        }
        let window = |ms: f64| -> f64 {
            let c = (ms * SR / 1000.0) as usize;
            out[c - 240..c + 240].iter().map(|x| x * x).sum()
        };
        // Self-recirculation at its own 200 ms period.
        assert!(window(200.0) > 0.1);
        assert!(window(400.0) > 0.05);
        assert!(window(600.0) > 0.01);
    }

    #[test]
    fn per_tap_mod_wobbles_only_marked_taps() {
        let run = |mod_amount: f64| -> Vec<f64> {
            let mut d = MultiTapDelay::new();
            d.time_ms = 300.0;
            d.feedback = 0.0;
            d.mod_depth = 1.0;
            d.mod_rate_hz = 3.0;
            d.taps = [Tap::off(); MAX_TAPS];
            d.taps[0] = Tap::at(1.0, 1.0);
            d.taps[0].mod_amount = mod_amount;
            d.update(SR);

            let mut out = vec![0.0f64; 48000];
            for (i, o) in out.iter_mut().enumerate() {
                let input = (std::f64::consts::TAU * 880.0 * i as f64 / SR).sin() * 0.5;
                *o = d.tick(input, 0);
                assert!(o.is_finite());
            }
            out
        };

        let still = run(0.0);
        let still2 = run(0.0);
        let wobbled = run(1.0);

        // Deterministic with mod off…
        let diff_off: f64 = still
            .iter()
            .zip(still2.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(diff_off < 1e-12, "mod_amount 0 must be deterministic");
        // …and audibly different with per-tap mod engaged.
        let diff_on: f64 = still
            .iter()
            .zip(wobbled.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(diff_on > 1.0, "per-tap mod should move the tap: {diff_on}");
    }

    #[test]
    fn presets_produce_output_and_no_nan() {
        for preset in [
            TapPreset::Classic1PingPong,
            TapPreset::Quarters,
            TapPreset::DottedEighth,
            TapPreset::Golden,
            TapPreset::EarlyReflections,
            TapPreset::Accelerando,
        ] {
            let mut d = MultiTapDelay::new();
            d.time_ms = 400.0;
            d.feedback = 0.5;
            d.set_preset(preset);
            d.update(SR);

            let mut energy = 0.0;
            for i in 0..48000 {
                let input = if i < 50 { 0.8 } else { 0.0 };
                let out = d.tick(input, 0);
                assert!(out.is_finite(), "{preset:?} NaN at {i}");
                energy += out * out;
            }
            assert!(energy > 0.001, "{preset:?} should produce output");
        }
    }
}
