//! TapeDelay — tape echo with wow/flutter, feedback filtering, and saturation.
//!
//! Based on qdelay (tiagolr). Signal flow per channel:
//! Input → DelayLine (cubic read) → Feedback EQ → Saturation → Hard Limit → Write back
//!
//! Supports up to 3 read heads (RE-201 Space Echo style). All heads read from
//! the same delay buffer with shared wow/flutter modulation.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::one_pole::OnePoleHp;
use audiocore_dsp::prng::XorShift32;
use audiocore_dsp::smoothing::ParamSmoother;
use audiocore_dsp::soft_clip::sin_clip;

use crate::modulation::{Flutter, WobbleShape, Wow};

/// dTape voice (TimeLine MX).
///
/// Selects how the `drive` knob hits the tape:
/// - `Mx` (EC-1 lineage): drive = record level — gain into the saturation
///   stage with output makeup, so repeats get punchier as they saturate.
/// - `Classic` (TimeLine v1): drive = tape bias — bias eats headroom, the
///   saturation ceiling drops with drive. More distortion, no input punch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TapeVoice {
    #[default]
    Mx,
    Classic,
}

/// Tape transport speed (TimeLine MX dTape).
///
/// `Fast` = higher fidelity: wider playback-head bandwidth and half the
/// effective wow/flutter/crinkle for the same knob settings. `Normal` =
/// the warmer stock character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TapeSpeed {
    #[default]
    Normal,
    Fast,
}

/// Saturation character types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaturationType {
    /// Clean — no saturation, just limiting at high drive.
    Clean,
    /// Tape — high-frequency compression + low-end warmth (sin clip).
    Tape,
    /// Warm — smooth even-harmonic distortion (tanh).
    Warm,
    /// Dirt — asymmetric odd-harmonic grit.
    Dirt,
    /// Pump — hard limiter that creates pumping compression.
    Pump,
    /// Hard Limit — brickwall limiting.
    HardLimit,
    /// Soft Limit — gentle smooth limiting.
    SoftLimit,
}

impl SaturationType {
    pub const COUNT: usize = 7;

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Clean,
            1 => Self::Tape,
            2 => Self::Warm,
            3 => Self::Dirt,
            4 => Self::Pump,
            5 => Self::HardLimit,
            6 => Self::SoftLimit,
            _ => Self::Tape,
        }
    }

    pub fn to_index(self) -> usize {
        match self {
            Self::Clean => 0,
            Self::Tape => 1,
            Self::Warm => 2,
            Self::Dirt => 3,
            Self::Pump => 4,
            Self::HardLimit => 5,
            Self::SoftLimit => 6,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Clean => "Clean",
            Self::Tape => "Tape",
            Self::Warm => "Warm",
            Self::Dirt => "Dirt",
            Self::Pump => "Pump",
            Self::HardLimit => "Hard",
            Self::SoftLimit => "Soft",
        }
    }
}

// RE-201 Space Echo head spacing ratios (relative to Head 1).
// From Cherry Audio Stardust 201 documentation.
pub const HEAD2_RATIO: f64 = 1.94;
pub const HEAD3_RATIO: f64 = 2.85;

// r[impl delay.tape.core]
/// Single-channel tape delay with modulation, feedback filtering, and saturation.
///
/// Supports up to 3 read heads (like the Roland RE-201 Space Echo).
/// Head 1 is at `time_ms`, Head 2 at 1.94×, Head 3 at 2.85×.
/// All heads share wow/flutter modulation (same tape transport).
/// Feedback is derived from the combined output of all active heads.
pub struct TapeDelay {
    // Parameters
    /// Delay time in milliseconds (base time for Head 1).
    pub time_ms: f64,
    /// Feedback amount (0.0 = no repeats, 1.0 = infinite).
    pub feedback: f64,
    /// Saturation drive (0.0 = clean, 1.0 = heavy).
    pub drive: f64,
    /// Saturation type.
    pub saturation_type: SaturationType,
    /// High-cut filter frequency in Hz (0 = disabled).
    pub hicut_freq: f64,
    /// Low-cut filter frequency in Hz (0 = disabled).
    pub locut_freq: f64,
    /// Filter Q.
    pub filter_q: f64,
    /// Wow depth (0.0–1.0).
    pub wow_depth: f64,
    /// Wow rate in Hz.
    pub wow_rate: f64,
    /// Wow drift amount (0.0–1.0).
    pub wow_drift: f64,
    /// Flutter depth (0.0–1.0).
    pub flutter_depth: f64,
    /// Flutter rate in Hz.
    pub flutter_rate: f64,

    // Multi-head (RE-201 style)
    /// Enable Head 1 (reads at base time_ms).
    pub head1_enabled: bool,
    /// Enable Head 2 (reads at HEAD2_RATIO × time_ms).
    pub head2_enabled: bool,
    /// Enable Head 3 (reads at HEAD3_RATIO × time_ms).
    pub head3_enabled: bool,
    /// Head 1 output level (0.0–1.0).
    pub head1_level: f64,
    /// Head 2 output level (0.0–1.0).
    pub head2_level: f64,
    /// Head 3 output level (0.0–1.0).
    pub head3_level: f64,

    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,
    /// Wobble LFO shape (Sine, Triangle, Square, S&H, Random).
    pub wow_shape: WobbleShape,
    /// Wobble phase offset (0.0–1.0) for L/R sync control.
    pub wow_phase_offset: f64,

    // ── dTape parity (TimeLine MX) ─────────────────────────────────
    /// Voice: how `drive` hits the tape (Mx = record level, Classic = bias).
    pub voice: TapeVoice,
    /// Tape age (0.0 = fresh full bandwidth, 1.0 = old dull tape ~2 kHz).
    /// Playback-path HF loss, independent of the in-loop `hicut_freq`.
    pub tape_age: f64,
    /// Tape crinkle (0.0–1.0): sparse random dropouts + tiny time warps.
    /// Event rate/severity scale with the knob and track `tape_speed`.
    pub crinkle: f64,
    /// Transport speed: Fast = wider bandwidth, half the wow/flutter/crinkle.
    pub tape_speed: TapeSpeed,
    /// Low-end contour (0.0 = full low-end, 1.0 = aggressive in-loop
    /// high-pass ~400 Hz). Major tape-voicing factor.
    pub low_contour: f64,

    // Internal state
    decay_eq: Biquad,
    delay: DelayLine,
    wow: Wow,
    flutter: Flutter,
    hicut: Biquad,
    locut: Biquad,
    dc_blocker: DcBlocker,
    /// Playback-path tape-age lowpass.
    age_filter: Biquad,
    /// In-loop low-contour highpass.
    contour_hp: OnePoleHp,
    // Crinkle event state: countdown to next event, remaining event
    // duration, event targets, and ~2 ms smoothed dip/warp values.
    crinkle_rng: XorShift32,
    crinkle_wait: u32,
    crinkle_dur: u32,
    crinkle_dip_target: f64,
    crinkle_warp_target: f64,
    crinkle_dip: f64,
    crinkle_warp: f64,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    fb_smoother: ParamSmoother,
    drive_smoother: ParamSmoother,
    hicut_smoother: ParamSmoother,
    locut_smoother: ParamSmoother,
    /// Countdown to the next biquad coefficient refresh while a cutoff
    /// smoother is still moving (refresh every 16 samples, not every one).
    coeff_refresh: u32,
}

impl Default for TapeDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl TapeDelay {
    /// Maximum delay time in seconds (must accommodate Head 3 at 2.85× base).
    const MAX_DELAY_S: f64 = 5.0;

    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            drive: 0.0,
            saturation_type: SaturationType::Tape,
            hicut_freq: 8000.0,
            locut_freq: 0.0,
            filter_q: 0.707,
            wow_depth: 0.0,
            wow_rate: 0.5,
            wow_drift: 0.3,
            flutter_depth: 0.0,
            flutter_rate: 6.0,
            head1_enabled: true,
            head2_enabled: false,
            head3_enabled: false,
            head1_level: 1.0,
            head2_level: 1.0,
            head3_level: 1.0,
            decay_tilt: 0.0,
            wow_shape: WobbleShape::Sine,
            wow_phase_offset: 0.0,
            voice: TapeVoice::Mx,
            tape_age: 0.0,
            crinkle: 0.0,
            tape_speed: TapeSpeed::Normal,
            low_contour: 0.0,
            decay_eq: Biquad::new(),
            delay: DelayLine::new(48000 * 5 + 1024),
            wow: Wow::new(),
            flutter: Flutter::new(),
            hicut: Biquad::new(),
            locut: Biquad::new(),
            dc_blocker: DcBlocker::new(),
            age_filter: Biquad::new(),
            contour_hp: OnePoleHp::new(20.0, 48000.0),
            crinkle_rng: XorShift32::new(0x7A9E_C41B),
            crinkle_wait: 0,
            crinkle_dur: 0,
            crinkle_dip_target: 1.0,
            crinkle_warp_target: 0.0,
            crinkle_dip: 1.0,
            crinkle_warp: 0.0,
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            fb_smoother: ParamSmoother::new(0.4),
            drive_smoother: ParamSmoother::new(0.0),
            hicut_smoother: ParamSmoother::new(8000.0),
            locut_smoother: ParamSmoother::new(0.0),
            coeff_refresh: 0,
        }
    }

    // r[impl delay.tape.update]
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

        self.wow.set_sample_rate(sample_rate);
        self.flutter.set_sample_rate(sample_rate);

        if self.hicut_freq > 0.0 {
            self.hicut.set(
                FilterType::Lowpass,
                self.hicut_freq,
                self.filter_q,
                sample_rate,
            );
        }
        if self.locut_freq > 0.0 {
            self.locut.set(
                FilterType::Highpass,
                self.locut_freq,
                self.filter_q,
                sample_rate,
            );
        }

        // Decay EQ: tilt filter in feedback path
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

        // Tape age: playback-path HF loss, exponential 18 kHz -> ~2.2 kHz.
        // Fast transport keeps ~25% more bandwidth at the same age.
        if self.tape_age > 0.005 {
            let speed_bw = match self.tape_speed {
                TapeSpeed::Fast => 1.25,
                TapeSpeed::Normal => 1.0,
            };
            let cutoff = (18_000.0 * (2_200.0f64 / 18_000.0).powf(self.tape_age) * speed_bw)
                .min(sample_rate * 0.45);
            self.age_filter
                .set(FilterType::Lowpass, cutoff, 0.707, sample_rate);
        }

        // Low-end contour: progressive in-loop high-pass, off -> ~420 Hz.
        if self.low_contour > 0.005 {
            let hp = 20.0 + self.low_contour.powf(1.5) * 400.0;
            self.contour_hp.set_cutoff(hp, sample_rate);
        }

        // Smooth delay time changes (~150ms time constant, from qdelay)
        self.smoother.set_time(0.15, sample_rate);

        // Gain-ish params get a short 5ms smoothing to kill zipper noise
        // on automation; cutoffs get 20ms with periodic coeff refresh.
        self.fb_smoother.set_time_ms(5.0, sample_rate);
        self.fb_smoother.set_epsilon(1e-4);
        self.drive_smoother.set_time_ms(5.0, sample_rate);
        self.drive_smoother.set_epsilon(1e-4);
        self.hicut_smoother.set_time_ms(20.0, sample_rate);
        self.hicut_smoother.set_epsilon(1.0);
        self.locut_smoother.set_time_ms(20.0, sample_rate);
        self.locut_smoother.set_epsilon(1.0);

        self.dc_blocker.set_cutoff(10.0, sample_rate);

        let target = self.time_ms * 0.001 * sample_rate;
        if self.smoother.value() == 0.0 {
            self.smoother.set_immediate(target);
        }
    }

    /// Advance a cutoff smoother, treating 0 (= filter disabled) as a hard
    /// switch rather than smoothing through the audible range.
    fn smooth_cutoff(smoother: &mut ParamSmoother, target: f64) -> f64 {
        if target <= 0.0 || smoother.value() <= 0.0 {
            smoother.set_immediate(target.max(0.0));
            return smoother.value();
        }
        smoother.set_target(target);
        smoother.tick()
    }

    // r[impl delay.tape.tick]
    // r[impl delay.tape.multihead]
    /// Process one sample. Returns the combined output of all active heads.
    ///
    /// Each head reads at its ratio × base time. All heads share the same
    /// wow/flutter modulation (physically correct — same tape transport).
    /// Feedback is derived from the combined output.
    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        // Fast transport = higher fidelity: half the mechanical instability
        // (wow/flutter/crinkle) for the same knob settings.
        let speed_scale = match self.tape_speed {
            TapeSpeed::Fast => 0.5,
            TapeSpeed::Normal => 1.0,
        };

        // Update modulation parameters
        self.wow.depth = self.wow_depth * speed_scale;
        self.wow.rate = self.wow_rate;
        self.wow.drift = self.wow_drift;
        self.wow.shape = self.wow_shape;
        self.wow.phase_offset = self.wow_phase_offset;
        self.flutter.depth = self.flutter_depth * speed_scale;
        self.flutter.rate = self.flutter_rate;

        // Smooth delay time (base time for all heads)
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        // Smooth gain-ish params per sample
        self.fb_smoother.set_target(self.feedback);
        let feedback = self.fb_smoother.tick();
        self.drive_smoother.set_target(self.drive);
        let drive = self.drive_smoother.tick();

        // Smooth cutoffs; refresh biquad coefficients every 16 samples
        // while still moving (full per-sample recompute is wasteful).
        let hicut_now = Self::smooth_cutoff(&mut self.hicut_smoother, self.hicut_freq);
        let locut_now = Self::smooth_cutoff(&mut self.locut_smoother, self.locut_freq);
        if self.coeff_refresh == 0 {
            if hicut_now > 0.0 && !self.hicut_smoother.is_settled() {
                self.hicut
                    .set(FilterType::Lowpass, hicut_now, self.filter_q, self.sample_rate);
            }
            if locut_now > 0.0 && !self.locut_smoother.is_settled() {
                self.locut
                    .set(FilterType::Highpass, locut_now, self.filter_q, self.sample_rate);
            }
            self.coeff_refresh = 16;
        }
        self.coeff_refresh -= 1;

        // Crinkle: sparse tape-damage events. Each event briefly dips the
        // playback level and warps the read position; both targets are
        // smoothed with a ~2 ms one-pole so the artifact crackles rather
        // than clicks. Event rate tracks the knob and the transport speed.
        if self.crinkle > 0.001 {
            if self.crinkle_dur > 0 {
                self.crinkle_dur -= 1;
                if self.crinkle_dur == 0 {
                    self.crinkle_dip_target = 1.0;
                    self.crinkle_warp_target = 0.0;
                }
            } else if self.crinkle_wait == 0 {
                // Schedule: 0.5..~10 events/s at full crinkle, halved on Fast.
                let rate_hz = (0.5 + self.crinkle * 9.0) * speed_scale;
                let mean_interval = self.sample_rate / rate_hz;
                let u = (self.crinkle_rng.next_bipolar() + 1.0) * 0.5;
                self.crinkle_wait = (mean_interval * (0.5 + u)) as u32;
                // Event: 1–5 ms, severity scales with the knob.
                let dur_u = (self.crinkle_rng.next_bipolar() + 1.0) * 0.5;
                self.crinkle_dur = (self.sample_rate * (0.001 + dur_u * 0.004)) as u32;
                let sev_u = (self.crinkle_rng.next_bipolar() + 1.0) * 0.5;
                let severity = self.crinkle * (0.3 + 0.7 * sev_u);
                self.crinkle_dip_target = 1.0 - severity * 0.85;
                self.crinkle_warp_target =
                    severity * 25.0 * self.crinkle_rng.next_bipolar().signum() * speed_scale;
            } else {
                self.crinkle_wait -= 1;
            }
        } else {
            self.crinkle_dip_target = 1.0;
            self.crinkle_warp_target = 0.0;
            self.crinkle_dur = 0;
        }
        // ~2 ms smoothing at 48 kHz.
        let crinkle_a = 1.0 - (-1.0 / (0.002 * self.sample_rate)).exp();
        self.crinkle_dip += crinkle_a * (self.crinkle_dip_target - self.crinkle_dip);
        self.crinkle_warp += crinkle_a * (self.crinkle_warp_target - self.crinkle_warp);

        // Wow/flutter offset — shared across all heads (same tape transport)
        let wow_offset = self.wow.tick();
        let flutter_offset = self.flutter.tick();
        let mod_offset = wow_offset + flutter_offset + self.crinkle_warp;
        let max_read = self.delay.len() as f64 - 4.0;

        let mut output = 0.0;

        // Read Head 1 (at base time)
        if self.head1_enabled {
            let head1_delay = (smooth_delay + mod_offset).clamp(1.0, max_read);
            output += self.delay.read_cubic(head1_delay) * self.head1_level;
        }

        // Read Head 2 (at HEAD2_RATIO × base time)
        if self.head2_enabled {
            let head2_delay = (smooth_delay * HEAD2_RATIO + mod_offset).clamp(1.0, max_read);
            output += self.delay.read_cubic(head2_delay) * self.head2_level;
        }

        // Read Head 3 (at HEAD3_RATIO × base time)
        if self.head3_enabled {
            let head3_delay = (smooth_delay * HEAD3_RATIO + mod_offset).clamp(1.0, max_read);
            output += self.delay.read_cubic(head3_delay) * self.head3_level;
        }

        // Playback-path degradation: crinkle level dip + tape-age HF loss.
        // Both sit on the head output so they recirculate through feedback
        // like a real transport (each generation gets duller/crackier).
        if self.crinkle > 0.001 || self.crinkle_dip < 0.9999 {
            output *= self.crinkle_dip;
        }
        if self.tape_age > 0.005 {
            output = self.age_filter.tick(output, ch);
        }

        // Feedback path: combined output → filter → saturate → limit
        let mut fb = output * feedback;

        if hicut_now > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }
        if locut_now > 0.0 {
            fb = self.locut.tick(fb, ch);
        }

        if self.decay_tilt.abs() > 0.01 {
            fb = self.decay_eq.tick(fb, ch);
        }

        // Low-end contour: in-loop high-pass, progressively thins the
        // repeats' low end each generation.
        if self.low_contour > 0.005 {
            fb = self.contour_hp.tick(fb);
        }

        // Saturation in feedback path. The voice decides how `drive`
        // reaches the tape:
        //   Mx      — record level: gain INTO the shaper, makeup after.
        //             Same ceiling, hotter signal = punchy saturated repeats.
        //   Classic — tape bias: the ceiling itself drops with drive
        //             (bias eats headroom). More distortion at the same
        //             loudness, none of the record-level punch.
        if drive > 0.0 {
            let (pre, post) = match self.voice {
                TapeVoice::Mx => (1.0 + drive * 4.0, 1.0 / (1.0 + drive * 1.2)),
                TapeVoice::Classic => {
                    let headroom = 1.0 - drive * 0.65;
                    (1.0 / headroom, headroom)
                }
            };
            let x = fb * pre;
            let shaped = match self.saturation_type {
                SaturationType::Clean => {
                    // Just gain + soft limit
                    x.clamp(-1.0, 1.0)
                }
                SaturationType::Tape => {
                    // Sin clip — HF compression + warmth
                    sin_clip(x)
                }
                SaturationType::Warm => {
                    // Tanh — smooth even-harmonic warmth
                    x.tanh()
                }
                SaturationType::Dirt => {
                    // Asymmetric cubic — odd harmonic grit
                    let y = x - x * x * x / 3.0;
                    y.clamp(-1.0, 1.0)
                }
                SaturationType::Pump => {
                    // Hard compression with gain reduction
                    let level = x.abs();
                    if level > 0.5 {
                        let reduction = 0.5 + (level - 0.5) * 0.2;
                        x.signum() * reduction
                    } else {
                        x
                    }
                }
                SaturationType::HardLimit => {
                    // Brickwall
                    x.clamp(-1.0, 1.0)
                }
                SaturationType::SoftLimit => {
                    // Gentle polynomial limiting
                    let x = x.clamp(-2.0, 2.0);
                    if x.abs() > 1.0 {
                        x.signum() * (1.0 - 0.25 * (2.0 - x.abs()).powi(2))
                    } else {
                        x * (1.5 - 0.5 * x * x)
                    }
                }
            };
            fb = shaped * post;
        }

        // Hard limit feedback to prevent runaway, then strip the DC that
        // asymmetric saturation (Dirt/Pump) injects into the loop.
        fb = self.dc_blocker.tick(fb.clamp(-1.5, 1.5));

        // Write input + feedback to delay line
        self.delay.write(input + fb);
        self.feedback_sample = fb;

        output
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.wow.reset();
        self.flutter.reset();
        self.hicut.reset();
        self.locut.reset();
        self.decay_eq.reset();
        self.dc_blocker.reset();
        self.age_filter.reset();
        self.contour_hp.reset();
        self.crinkle_wait = 0;
        self.crinkle_dur = 0;
        self.crinkle_dip_target = 1.0;
        self.crinkle_warp_target = 0.0;
        self.crinkle_dip = 1.0;
        self.crinkle_warp = 0.0;
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.fb_smoother.reset(self.feedback);
        self.drive_smoother.reset(self.drive);
        self.hicut_smoother.reset(self.hicut_freq);
        self.locut_smoother.reset(self.locut_freq);
        self.coeff_refresh = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn make_delay() -> TapeDelay {
        let mut d = TapeDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.update(SR);
        d
    }

    #[test]
    fn silence_in_silence_out() {
        let mut d = make_delay();
        for _ in 0..48000 {
            let out = d.tick(0.0, 0);
            assert!(out.abs() < 1e-10);
        }
    }

    #[test]
    fn impulse_delayed() {
        let mut d = make_delay();
        let expected_delay = 4800; // 100ms at 48kHz

        let mut peak_pos = 0;
        let mut peak_val = 0.0;

        for i in 0..10000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let out = d.tick(input, 0);
            if out.abs() > peak_val {
                peak_val = out.abs();
                peak_pos = i;
            }
        }

        assert!(
            (peak_pos as i64 - expected_delay as i64).unsigned_abs() < 10,
            "Peak at {peak_pos}, expected near {expected_delay}"
        );
        assert!(peak_val > 0.5, "Peak should be significant: {peak_val}");
    }

    #[test]
    fn feedback_creates_repeats() {
        let mut d = make_delay();
        d.feedback = 0.5;
        d.update(SR);

        d.tick(1.0, 0);

        let mut peaks = Vec::new();
        for i in 1..144000 {
            let out = d.tick(0.0, 0);
            if out.abs() > 0.05 && (peaks.is_empty() || i - peaks.last().unwrap() > 2000) {
                peaks.push(i);
            }
        }

        assert!(
            peaks.len() >= 3,
            "Should have multiple repeats with feedback: got {}",
            peaks.len()
        );
    }

    #[test]
    fn no_nan_with_all_features() {
        let mut d = TapeDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.7;
        d.drive = 0.8;
        d.hicut_freq = 5000.0;
        d.locut_freq = 100.0;
        d.wow_depth = 0.5;
        d.flutter_depth = 0.5;
        d.head2_enabled = true;
        d.head3_enabled = true;
        d.update(SR);

        for i in 0..96000 {
            let input = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN/Inf at sample {i}");
        }
    }

    #[test]
    fn feedback_does_not_accumulate_dc() {
        // A DC-offset input recirculating at high feedback would build up
        // to offset/(1-fb) without a DC blocker in the loop.
        let mut d = TapeDelay::new();
        d.time_ms = 50.0;
        d.feedback = 0.8;
        d.drive = 0.6;
        d.saturation_type = SaturationType::Dirt;
        d.locut_freq = 0.0; // no highpass — the DC blocker must do the work
        d.update(SR);

        let mut sum = 0.0;
        let mut count = 0usize;
        for i in 0..96000 {
            // 0.3 DC + quiet sine — deliberately asymmetric signal
            let input = 0.3 + (2.0 * PI * 220.0 * i as f64 / SR).sin() * 0.2;
            let out = d.tick(input, 0);
            if i >= 48000 {
                sum += out;
                count += 1;
            }
        }
        let mean = sum / count as f64;
        assert!(
            mean.abs() < 0.5,
            "DC should not accumulate in feedback: mean={mean}"
        );
    }

    #[test]
    fn saturation_limits_output() {
        let mut d = TapeDelay::new();
        d.time_ms = 10.0;
        d.feedback = 0.95;
        d.drive = 1.0;
        d.update(SR);

        for i in 0..48000 {
            let input = if i < 480 { 1.0 } else { 0.0 };
            let out = d.tick(input, 0);
            assert!(
                out.abs() < 3.0,
                "Output should be limited: {out} at sample {i}"
            );
        }
    }

    #[test]
    fn hicut_darkens_repeats() {
        let mut d_clean = make_delay();
        d_clean.feedback = 0.6;
        d_clean.update(SR);

        let mut d_dark = make_delay();
        d_dark.feedback = 0.6;
        d_dark.hicut_freq = 2000.0;
        d_dark.update(SR);

        let input: Vec<f64> = (0..200)
            .map(|i| (2.0 * PI * 10000.0 * i as f64 / SR).sin())
            .collect();

        for &s in &input {
            d_clean.tick(s, 0);
            d_dark.tick(s, 0);
        }

        let mut energy_clean = 0.0;
        let mut energy_dark = 0.0;
        for i in 0..20000 {
            let c = d_clean.tick(0.0, 0);
            let d = d_dark.tick(0.0, 0);
            if i > 13800 && i < 15200 {
                energy_clean += c * c;
                energy_dark += d * d;
            }
        }

        assert!(
            energy_dark < energy_clean * 0.99,
            "High-cut should reduce energy by 3rd repeat: clean={energy_clean:.6}, dark={energy_dark:.6}"
        );
    }

    #[test]
    fn smooth_time_change() {
        let mut d = make_delay();
        d.feedback = 0.0;
        d.update(SR);

        for _ in 0..4800 {
            d.tick(0.0, 0);
        }

        d.time_ms = 200.0;

        let mut prev: f64 = 0.0;
        let mut max_jump: f64 = 0.0;
        for i in 0..4800 {
            let input = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            let jump = (out - prev).abs();
            max_jump = max_jump.max(jump);
            prev = out;
        }
        assert!(
            max_jump < 1.0,
            "Time change should be smooth: max_jump={max_jump}"
        );
    }

    #[test]
    fn multihead_mode1_head3_only() {
        // Mode 1: Head 3 only — single tap at 2.85× base time
        let mut d = TapeDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.head1_enabled = false;
        d.head2_enabled = false;
        d.head3_enabled = true;
        d.update(SR);

        let mut outputs = Vec::with_capacity(20000);
        for i in 0..20000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            outputs.push(d.tick(input, 0));
        }

        // Head 3 at 100ms * 2.85 = 285ms = 13680 samples
        let head1_region = outputs[4700..4900]
            .iter()
            .map(|x| x.abs())
            .fold(0.0f64, f64::max);
        let head3_peak = outputs[13500..13900]
            .iter()
            .map(|x| x.abs())
            .fold(0.0f64, f64::max);

        assert!(
            head1_region < 0.01,
            "Head 1 should be silent in Mode 1: got {head1_region}"
        );
        assert!(
            head3_peak > 0.5,
            "Head 3 should produce peak near 13680: got {head3_peak}"
        );
    }

    #[test]
    fn multihead_mode2_head1_and_3() {
        // Mode 2: Heads 1 + 3
        let mut d = TapeDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.head1_enabled = true;
        d.head2_enabled = false;
        d.head3_enabled = true;
        d.update(SR);

        let mut outputs = Vec::with_capacity(20000);
        for i in 0..20000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            outputs.push(d.tick(input, 0));
        }

        let head1_peak = outputs[4700..4900]
            .iter()
            .map(|x| x.abs())
            .fold(0.0f64, f64::max);
        let head2_region = outputs[9200..9500]
            .iter()
            .map(|x| x.abs())
            .fold(0.0f64, f64::max);
        let head3_peak = outputs[13500..13900]
            .iter()
            .map(|x| x.abs())
            .fold(0.0f64, f64::max);

        assert!(
            head1_peak > 0.5,
            "Head 1 should be active: got {head1_peak}"
        );
        assert!(
            head2_region < 0.01,
            "Head 2 should be silent: got {head2_region}"
        );
        assert!(
            head3_peak > 0.5,
            "Head 3 should be active: got {head3_peak}"
        );
    }

    #[test]
    fn multihead_mode4_all_three() {
        // Mode 4: All three heads
        let mut d = TapeDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.head1_enabled = true;
        d.head2_enabled = true;
        d.head3_enabled = true;
        d.update(SR);

        let mut outputs = Vec::with_capacity(20000);
        for i in 0..20000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            outputs.push(d.tick(input, 0));
        }

        let head1_peak = outputs[4700..4900]
            .iter()
            .map(|x| x.abs())
            .fold(0.0f64, f64::max);
        let head2_peak = outputs[9200..9500]
            .iter()
            .map(|x| x.abs())
            .fold(0.0f64, f64::max);
        let head3_peak = outputs[13500..13900]
            .iter()
            .map(|x| x.abs())
            .fold(0.0f64, f64::max);

        assert!(head1_peak > 0.5, "Head 1: {head1_peak}");
        assert!(head2_peak > 0.5, "Head 2: {head2_peak}");
        assert!(head3_peak > 0.5, "Head 3: {head3_peak}");
    }

    #[test]
    fn multihead_no_nan() {
        let mut d = TapeDelay::new();
        d.time_ms = 500.0;
        d.feedback = 0.6;
        d.drive = 0.5;
        d.head2_enabled = true;
        d.head3_enabled = true;
        d.hicut_freq = 4000.0;
        d.wow_depth = 0.3;
        d.flutter_depth = 0.2;
        d.update(SR);

        for i in 0..96000 {
            let input = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.3;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN/Inf at sample {i}");
            assert!(out.abs() < 10.0, "Runaway at sample {i}: {out}");
        }
    }
}

#[cfg(test)]
mod dtape_parity_tests {
    use super::*;
    use std::f64::consts::TAU;

    const SR: f64 = 48000.0;

    /// Render the first repeat of a burst through a configured delay and
    /// return (rms, peak) of the repeat window.
    fn first_repeat(cfg: impl Fn(&mut TapeDelay)) -> (f64, f64) {
        let mut d = TapeDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.9; // hot loop so the saturator works
        d.drive = 0.7;
        d.hicut_freq = 0.0;
        cfg(&mut d);
        d.update(SR);

        // 20 ms 220 Hz burst at moderate level.
        for i in 0..960 {
            d.tick((TAU * 220.0 * i as f64 / SR).sin() * 0.5, 0);
        }
        // Collect the 3rd repeat (300 ms in) where loop shaping compounds.
        let mut rms = 0.0;
        let mut peak = 0.0f64;
        let mut n = 0.0;
        for i in 960..20000 {
            let out = d.tick(0.0, 0);
            if (14400..15400).contains(&i) {
                rms += out * out;
                peak = peak.max(out.abs());
                n += 1.0;
            }
        }
        ((rms / n).sqrt(), peak)
    }

    /// Mx (record level) pushes gain into the shaper with makeup, Classic
    /// (bias) drops the ceiling instead — at the same drive the two voices
    /// must produce measurably different repeats.
    #[test]
    fn voices_produce_different_repeats() {
        let (mx_rms, mx_peak) = first_repeat(|d| d.voice = TapeVoice::Mx);
        let (cl_rms, cl_peak) = first_repeat(|d| d.voice = TapeVoice::Classic);
        assert!(mx_rms.is_finite() && cl_rms.is_finite());
        let rms_ratio = mx_rms / cl_rms.max(1e-12);
        assert!(
            (rms_ratio - 1.0).abs() > 0.10,
            "voices should differ audibly: mx_rms={mx_rms:.4} classic_rms={cl_rms:.4} \
             (peaks {mx_peak:.4}/{cl_peak:.4})"
        );
        // Classic's ceiling drops with drive, so its repeats sit lower.
        assert!(
            cl_peak < mx_peak,
            "bias voice should not out-punch record-level voice: \
             classic={cl_peak:.4}, mx={mx_peak:.4}"
        );
    }

    /// Tape age rolls off HF on the playback path.
    #[test]
    fn tape_age_darkens_playback() {
        let hf = |age: f64| -> f64 {
            let mut d = TapeDelay::new();
            d.time_ms = 100.0;
            d.feedback = 0.0;
            d.hicut_freq = 0.0;
            d.tape_age = age;
            d.update(SR);
            let mut energy = 0.0;
            for i in 0..15000 {
                let s = (TAU * 8000.0 * i as f64 / SR).sin() * 0.5;
                let out = d.tick(s, 0);
                if i > 5000 {
                    energy += out * out;
                }
            }
            energy
        };
        let fresh = hf(0.0);
        let old = hf(1.0);
        assert!(
            old < fresh * 0.3,
            "age=1 should heavily cut 8 kHz: fresh={fresh:.3}, old={old:.3}"
        );
    }

    /// Crinkle produces amplitude dropouts in an otherwise steady wet tone.
    #[test]
    fn crinkle_causes_dropouts() {
        let min_envelope = |crinkle: f64| -> f64 {
            let mut d = TapeDelay::new();
            d.time_ms = 60.0;
            d.feedback = 0.0;
            d.hicut_freq = 0.0;
            d.crinkle = crinkle;
            d.update(SR);
            // Steady tone; peak-hold envelope with a 10 ms release. A
            // dropout pulls the envelope down faster than the natural
            // inter-cycle ripple of the sine ever can.
            let release = (-1.0 / (0.010 * SR)).exp();
            let mut env = 0.0f64;
            let mut min_env = f64::MAX;
            for i in 0..96000 {
                let s = (TAU * 440.0 * i as f64 / SR).sin() * 0.5;
                let out = d.tick(s, 0);
                env = out.abs().max(env * release);
                if i > 9600 {
                    min_env = min_env.min(env);
                }
            }
            min_env
        };
        let clean = min_envelope(0.0);
        let cranky = min_envelope(1.0);
        assert!(
            cranky < clean * 0.85,
            "crinkle should dip the envelope: clean={clean:.3}, crinkle={cranky:.3}"
        );
    }

    /// Fast transport halves the wow excursion: with identical wow settings
    /// the Fast output deviates less from an unmodulated reference.
    #[test]
    fn fast_speed_reduces_wobble() {
        let deviation = |speed: TapeSpeed| -> f64 {
            let render = |wow: f64, speed: TapeSpeed| -> Vec<f64> {
                let mut d = TapeDelay::new();
                d.time_ms = 100.0;
                d.feedback = 0.0;
                d.hicut_freq = 0.0;
                d.wow_depth = wow;
                d.wow_rate = 2.0;
                d.wow_drift = 0.0;
                d.tape_speed = speed;
                d.update(SR);
                (0..48000)
                    .map(|i| d.tick((TAU * 440.0 * i as f64 / SR).sin() * 0.5, 0))
                    .collect()
            };
            let dry = render(0.0, speed);
            let wet = render(0.6, speed);
            dry.iter().zip(&wet).map(|(a, b)| (a - b).abs()).sum()
        };
        let normal = deviation(TapeSpeed::Normal);
        let fast = deviation(TapeSpeed::Fast);
        assert!(
            fast < normal * 0.75,
            "Fast should wobble less: normal={normal:.1}, fast={fast:.1}"
        );
    }

    /// Low contour thins the repeats' low end progressively per generation.
    /// Uses an isolated burst (no steady-state overlap, whose phase-shift
    /// interference would confound an energy measure) and inspects the
    /// 3rd repeat, where the in-loop HP has been applied twice.
    #[test]
    fn low_contour_thins_lows() {
        let repeat3_energy = |contour: f64| -> f64 {
            let mut d = TapeDelay::new();
            d.time_ms = 100.0;
            d.feedback = 0.8;
            d.hicut_freq = 0.0;
            d.low_contour = contour;
            d.update(SR);
            // 30 ms 80 Hz burst.
            let mut energy = 0.0;
            for i in 0..24000 {
                let s = if i < 1440 {
                    (TAU * 80.0 * i as f64 / SR).sin() * 0.5
                } else {
                    0.0
                };
                let out = d.tick(s, 0);
                // 3rd repeat: 300 ms = 14400, window covers the burst.
                if (14400..16000).contains(&i) {
                    energy += out * out;
                }
            }
            energy
        };
        let full = repeat3_energy(0.0);
        let thin = repeat3_energy(1.0);
        assert!(
            thin < full * 0.5,
            "contour should thin 80 Hz repeats: full={full:.2}, thin={thin:.2}"
        );
    }

    /// Self-limiting repeats: max feedback + max drive for 10 s stays
    /// bounded and finite — saturation absorbs the runaway.
    #[test]
    fn max_repeats_self_limit() {
        let mut d = TapeDelay::new();
        d.time_ms = 120.0;
        d.feedback = 1.0;
        d.drive = 1.0;
        d.saturation_type = SaturationType::Tape;
        d.update(SR);

        let mut peak = 0.0f64;
        for i in 0..(SR as usize * 10) {
            let input = if i < 4800 {
                (TAU * 330.0 * i as f64 / SR).sin() * 0.8
            } else {
                0.0
            };
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at {i}");
            peak = peak.max(out.abs());
        }
        assert!(
            peak < 4.0,
            "10 s at unity feedback must stay bounded: peak={peak:.2}"
        );
    }
}
