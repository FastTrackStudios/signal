//! PitchDelay — TimeLine MX "Ice" machine: slices the delay buffer and
//! plays the pieces back re-pitched.
//!
//! The delay tap feeds a granular pitch shifter (pitch-dsp
//! `GranularShifter`, dual complementary-Hann grains, cubic reads).
//! `blend` mixes dry↔ice ON THE DELAY LINE, pre-feedback, so
//! regeneration re-shifts every pass — the classic octave ladder.
//!
//! Latency: the shifter adds `grain` samples; the tap is read `grain`
//! samples early so the first repeat still lands at the delay time
//! (exact at unity speed, ± half a grain while heads drift).

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::smoothing::ParamSmoother;
use pitch_dsp::granular::GranularShifter;

/// TimeLine MX Ice interval menu. `Free` uses `PitchDelay::speed` raw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IceInterval {
    /// Use the raw `speed` ratio field (non-MX escape hatch).
    Free,
    /// Half steps, −12 (octave down) ..= +12 (octave up).
    Semitones(i8),
    /// Micro-tunings: ±25 or ±50 cents.
    Cents(i16),
    /// +1 octave and a fifth (+19 semitones).
    OctaveAndFifth,
    /// +2 octaves (+24 semitones).
    TwoOctaves,
}

impl IceInterval {
    /// Pitch ratio for the interval; `None` for `Free`.
    pub fn ratio(self) -> Option<f64> {
        match self {
            Self::Free => None,
            Self::Semitones(s) => Some(2f64.powf(f64::from(s) / 12.0)),
            Self::Cents(c) => Some(2f64.powf(f64::from(c) / 1200.0)),
            Self::OctaveAndFifth => Some(2f64.powf(19.0 / 12.0)),
            Self::TwoOctaves => Some(4.0),
        }
    }

    /// The 30-entry MX menu order: −12..−1, −50c, −25c, +25c, +50c,
    /// +1..+11, +12, +19, +24. Out-of-range indices clamp to the ends.
    pub fn from_index(i: usize) -> Self {
        match i {
            0..=11 => Self::Semitones(i as i8 - 12),
            12 => Self::Cents(-50),
            13 => Self::Cents(-25),
            14 => Self::Cents(25),
            15 => Self::Cents(50),
            16..=26 => Self::Semitones(i as i8 - 15),
            27 => Self::Semitones(12),
            28 => Self::OctaveAndFifth,
            _ => Self::TwoOctaves,
        }
    }

    pub const MENU_LEN: usize = 30;
}

/// Slice size — scales with the delay time (per the MX manual).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IceSlice {
    /// ~1/4 of the delay time (max 200 ms) — small regenerating fragments.
    Short,
    /// ~1/2 of the delay time (max 400 ms).
    Medium,
    /// ~the delay time (max 800 ms) — whole re-pitched phrases.
    Long,
}

impl IceSlice {
    fn grain_ms(self, time_ms: f64) -> f64 {
        match self {
            Self::Short => (time_ms * 0.25).clamp(10.0, 200.0),
            Self::Medium => (time_ms * 0.5).clamp(20.0, 400.0),
            Self::Long => time_ms.clamp(40.0, 800.0),
        }
    }
}

// r[impl delay.pitch.shift]
// r[impl delay.pitch.granular-crossfade]
/// Ice-style pitch-shifted delay line.
pub struct PitchDelay {
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// Playback speed ratio (used when `interval == Free`).
    pub speed: f64,
    /// Musical interval; non-`Free` overrides `speed` on `update()`.
    pub interval: IceInterval,
    /// Slice size; `None` uses `grain_ms` directly (pre-Ice behavior).
    pub slice: Option<IceSlice>,
    /// Dry↔ice balance on the delay line, pre-feedback (1.0 = all ice).
    pub blend: f64,
    /// Crossfade grain size in milliseconds (when `slice` is `None`).
    pub grain_ms: f64,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,

    decay_eq: Biquad,
    delay: DelayLine,
    shifter: GranularShifter,
    /// Shifter grain size actually in effect (samples).
    grain_samples: f64,
    dc_blocker: DcBlocker,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
}

impl PitchDelay {
    const MAX_DELAY_S: f64 = 5.0;

    pub fn new() -> Self {
        let buf_len = 48000 * 5 + 1024;
        let mut shifter = GranularShifter::new();
        shifter.mix = 1.0;
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            speed: 1.0,
            interval: IceInterval::Free,
            slice: None,
            blend: 1.0,
            grain_ms: 30.0,
            decay_tilt: 0.0,
            decay_eq: Biquad::new(),
            delay: DelayLine::new(buf_len),
            shifter,
            grain_samples: 30.0 * 48.0,
            dc_blocker: DcBlocker::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

        if let Some(ratio) = self.interval.ratio() {
            self.speed = ratio;
        }

        // Grain: slice-derived or free, capped below the delay time so the
        // early tap that compensates shifter latency stays in range.
        let ms = match self.slice {
            Some(s) => s.grain_ms(self.time_ms),
            None => self.grain_ms,
        };
        let time_samples = self.time_ms * 0.001 * sample_rate;
        let grain = (ms * 0.001 * sample_rate)
            .min(time_samples * 0.9)
            .clamp(64.0, sample_rate * 0.9);
        // Reconfiguring the shifter reseats its heads — only do it when
        // the grain or rate actually changed (update runs at control rate).
        if (grain - self.grain_samples).abs() > 1.0 {
            self.grain_samples = grain;
            self.shifter.grain_size = grain as usize;
            self.shifter.update(sample_rate);
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

        self.smoother.set_time(0.15, sample_rate);
        self.dc_blocker.set_cutoff(10.0, sample_rate);
        let target = self.time_ms * 0.001 * sample_rate;
        if self.smoother.value() == 0.0 {
            self.smoother.set_immediate(target);
        }
    }

    // r[impl delay.pitch.tick]
    /// Process one sample. Returns the (blended) delay-line output.
    pub fn tick(&mut self, input: f64) -> f64 {
        // Smooth delay time
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        let max_read = self.delay.len() as f64 - 4.0;

        // Dry path reads at the delay time. The ice path taps early to
        // compensate the shifter's re-delay, whose MEAN is speed-dependent:
        // heads reset to one grain and drift by (1 - speed) per sample, so
        // mean offset = grain * (1 + (1 - speed)/2). Exact at unity; at
        // extreme up-shifts the compensation floors at zero (repeats land
        // slightly late — can't tap the future).
        let dry_tap = self.delay.read_cubic(smooth_delay.clamp(1.0, max_read));
        let comp = (self.grain_samples * (1.0 + (1.0 - self.speed) * 0.5)).max(0.0);
        let early = (smooth_delay - comp).clamp(1.0, max_read);
        let ice_tap = self.delay.read_cubic(early);

        self.shifter.speed = self.speed;
        let iced = self.shifter.tick(ice_tap);

        // Dry↔ice blend ON the line: feedback recirculates the blended
        // signal, so each pass re-shifts (octave ladder).
        let output = dry_tap * (1.0 - self.blend) + iced * self.blend;

        // Feedback with self-limiting
        let mut fb = output * self.feedback;
        if self.decay_tilt.abs() > 0.01 {
            fb = self.decay_eq.tick(fb, 0);
        }
        let limited_fb = if fb.abs() > 0.001 {
            fb * (3.0 - fb.abs() * 2.0).max(0.0) / 3.0
        } else {
            fb
        };
        // Grain crossfades + the nonlinear limiter can build a subsonic
        // offset over many recirculations — block it inside the loop.
        let clamped_fb = self.dc_blocker.tick(limited_fb.clamp(-1.5, 1.5));

        self.delay.write(input + clamped_fb);
        self.feedback_sample = clamped_fb;

        output
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.decay_eq.reset();
        self.shifter.reset();
        self.dc_blocker.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
    }
}

impl Default for PitchDelay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn make_pitch_delay() -> PitchDelay {
        let mut d = PitchDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.speed = 1.0;
        d.update(SR);
        d
    }

    /// Goertzel energy at `freq`.
    fn goertzel(sig: &[f64], freq: f64) -> f64 {
        let omega = 2.0 * PI * freq / SR;
        let coeff = 2.0 * omega.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &x in sig {
            let s0 = x + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - coeff * s1 * s2) / sig.len() as f64
    }

    /// Which of `candidates` dominates the window?
    fn dominant(sig: &[f64], candidates: &[f64]) -> f64 {
        let mut best = candidates[0];
        let mut best_e = f64::MIN;
        for &f in candidates {
            let e = goertzel(sig, f);
            if e > best_e {
                best_e = e;
                best = f;
            }
        }
        best
    }

    #[test]
    fn interval_ratios_match_theory() {
        assert!((IceInterval::Semitones(12).ratio().unwrap() - 2.0).abs() < 1e-12);
        assert!((IceInterval::Semitones(-12).ratio().unwrap() - 0.5).abs() < 1e-12);
        assert!((IceInterval::Semitones(7).ratio().unwrap() - 1.4983).abs() < 1e-4);
        assert!((IceInterval::Cents(-50).ratio().unwrap() - 0.97153).abs() < 1e-5);
        assert!((IceInterval::Cents(25).ratio().unwrap() - 1.01455).abs() < 1e-5);
        assert!((IceInterval::OctaveAndFifth.ratio().unwrap() - 2.9966).abs() < 1e-3);
        assert!((IceInterval::TwoOctaves.ratio().unwrap() - 4.0).abs() < 1e-12);
        assert_eq!(IceInterval::Free.ratio(), None);
        // Menu order endpoints
        assert_eq!(IceInterval::from_index(0), IceInterval::Semitones(-12));
        assert_eq!(IceInterval::from_index(12), IceInterval::Cents(-50));
        assert_eq!(IceInterval::from_index(16), IceInterval::Semitones(1));
        assert_eq!(IceInterval::from_index(27), IceInterval::Semitones(12));
        assert_eq!(IceInterval::from_index(29), IceInterval::TwoOctaves);
    }

    #[test]
    fn unity_pitch_delays_signal() {
        let mut d = make_pitch_delay();

        let mut peak_pos = 0;
        let mut peak_val: f64 = 0.0;

        for i in 0..10000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let out = d.tick(input);
            if out.abs() > peak_val {
                peak_val = out.abs();
                peak_pos = i;
            }
        }

        // Latency-compensated: peak lands at the delay time (~4800).
        assert!(
            peak_pos > 4000 && peak_pos < 6000,
            "Peak at {peak_pos}, expected near 4800"
        );
        assert!(peak_val > 0.3, "Peak should be significant: {peak_val}");
    }

    #[test]
    fn octave_up_doubles_repeat_frequency() {
        let mut d = PitchDelay::new();
        d.time_ms = 400.0;
        d.feedback = 0.0;
        d.interval = IceInterval::Semitones(12);
        d.blend = 1.0;
        d.slice = Some(IceSlice::Medium);
        d.update(SR);

        // 200 ms 300 Hz burst; measure the repeat's frequency.
        let burst = (SR * 0.2) as usize;
        let n = (SR * 1.0) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let x = if i < burst {
                (2.0 * PI * 300.0 * i as f64 / SR).sin() * 0.5
            } else {
                0.0
            };
            out.push(d.tick(x));
        }
        // Repeat window: starts at 400 ms; sample its middle.
        let w0 = (SR * 0.45) as usize;
        let w1 = (SR * 0.55) as usize;
        let f = dominant(&out[w0..w1], &[150.0, 300.0, 600.0, 1200.0]);
        assert!(
            (f - 600.0).abs() < 1.0,
            "octave-up repeat should be dominated by 600 Hz, got {f}"
        );
    }

    #[test]
    fn octave_ladder_climbs_each_pass() {
        let mut d = PitchDelay::new();
        d.time_ms = 300.0;
        d.feedback = 0.7;
        d.interval = IceInterval::Semitones(12);
        d.blend = 1.0;
        d.slice = Some(IceSlice::Medium);
        d.update(SR);

        let burst = (SR * 0.15) as usize;
        let n = (SR * 1.2) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let x = if i < burst {
                (2.0 * PI * 220.0 * i as f64 / SR).sin() * 0.5
            } else {
                0.0
            };
            out.push(d.tick(x));
        }
        // Repeat 1 at 300 ms (≈440), repeat 2 at 600 ms (≈880).
        let cands = [220.0, 440.0, 880.0, 1760.0];
        let f1 = dominant(&out[(SR * 0.33) as usize..(SR * 0.42) as usize], &cands);
        let f2 = dominant(&out[(SR * 0.63) as usize..(SR * 0.72) as usize], &cands);
        assert!(
            (f1 - 440.0).abs() < 1.0 && (f2 - 880.0).abs() < 1.0,
            "successive repeats should climb an octave: f1={f1} f2={f2}"
        );
    }

    #[test]
    fn blend_zero_is_plain_delay() {
        let mut d = PitchDelay::new();
        d.time_ms = 150.0;
        d.feedback = 0.0;
        d.interval = IceInterval::Semitones(12);
        d.blend = 0.0;
        d.update(SR);

        let mut reference = DelayLine::new(48000);
        let delay_samples = 150.0 * 0.001 * SR;

        let mut max_err = 0.0f64;
        for i in 0..24000 {
            let x = (2.0 * PI * 330.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(x);
            // Match PitchDelay's read-before-write order.
            let want = reference.read_cubic(delay_samples);
            reference.write(x);
            if i > 8000 {
                max_err = max_err.max((out - want).abs());
            }
        }
        assert!(max_err < 1e-6, "blend=0 should be a plain delay: {max_err}");
    }

    #[test]
    fn slice_sizes_are_distinct() {
        let time = 400.0;
        assert!(
            IceSlice::Long.grain_ms(time) > IceSlice::Medium.grain_ms(time)
                && IceSlice::Medium.grain_ms(time) > IceSlice::Short.grain_ms(time)
        );

        // And they audibly differ.
        let run = |slice: IceSlice| -> Vec<f64> {
            let mut d = PitchDelay::new();
            d.time_ms = time;
            d.feedback = 0.0;
            d.interval = IceInterval::Semitones(12);
            d.slice = Some(slice);
            d.update(SR);
            (0..48000)
                .map(|i| {
                    let x = (2.0 * PI * 220.0 * i as f64 / SR).sin() * 0.5;
                    d.tick(x)
                })
                .collect()
        };
        let short = run(IceSlice::Short);
        let long = run(IceSlice::Long);
        let diff: f64 =
            short.iter().zip(&long).map(|(a, b)| (a - b).abs()).sum::<f64>() / 48000.0;
        assert!(diff > 0.005, "slice sizes should differ audibly: {diff}");
    }

    #[test]
    fn no_nan() {
        let mut d = PitchDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.6;
        d.speed = 1.5;
        d.update(SR);

        for i in 0..96000 {
            let input = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input);
            assert!(out.is_finite(), "NaN at sample {i}");
        }
    }

    #[test]
    fn feedback_self_limits() {
        let mut d = PitchDelay::new();
        d.time_ms = 60.0;
        d.feedback = 0.99;
        d.speed = 1.0;
        d.update(SR);

        for _ in 0..480 {
            d.tick(1.0);
        }

        let mut max_out: f64 = 0.0;
        for _ in 0..96000 {
            let out = d.tick(0.0);
            max_out = max_out.max(out.abs());
        }

        assert!(max_out < 5.0, "Should self-limit: max={max_out}");
    }

    #[test]
    fn pitch_shift_changes_output() {
        // At speed != 1.0, output should differ from normal delay
        let mut d_normal = make_pitch_delay();
        let mut d_shifted = make_pitch_delay();
        d_shifted.speed = 0.5; // Octave down
        d_shifted.update(SR);

        let mut out_normal = Vec::new();
        let mut out_shifted = Vec::new();

        for i in 0..9600 {
            let s = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            out_normal.push(d_normal.tick(s));
            out_shifted.push(d_shifted.tick(s));
        }

        // Outputs should differ significantly
        let diff: f64 = out_normal
            .iter()
            .zip(out_shifted.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / 9600.0;

        assert!(
            diff > 0.001,
            "Pitch shift should change output: avg_diff={diff}"
        );
    }
}
