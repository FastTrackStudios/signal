//! FilterDelay — delay with a tempo-synced LFO-swept filter (+ tremolo).
//!
//! TimeLine MX "Filter" machine parity: a clean delay whose wet path
//! runs through a resonant state-variable filter swept by an LFO whose
//! rate is a ratio of the delay time (1/32–32/1). The filter sits pre or
//! post the delay line. The MX folds the old Trem machine in here too:
//! `trem_depth`/`trem_speed` gate the repeats with a synced tremolo.

use crate::tilt::DecayTilt;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::denormal::flush;
use audiocore_dsp::prng::XorShift32;
use audiocore_dsp::smoothing::ParamSmoother;

/// LFO waveform for the filter sweep. `+` shapes start at their peak,
/// `-` shapes at their trough (TimeLine's polarity convention: where the
/// sweep sits when repeats begin). `Down`/`Up` are ATTACK-TRIGGERED
/// one-shot sweeps: once per detected input attack the filter sweeps
/// down (or up) over one LFO period, then holds — not cyclical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterLfoShape {
    SinePos,
    SineNeg,
    TrianglePos,
    TriangleNeg,
    SquarePos,
    Saw,
    Ramp,
    Random,
    Down,
    Up,
}

impl FilterLfoShape {
    pub const COUNT: usize = 10;

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::SinePos,
            1 => Self::SineNeg,
            2 => Self::TrianglePos,
            3 => Self::TriangleNeg,
            4 => Self::SquarePos,
            5 => Self::Saw,
            6 => Self::Ramp,
            7 => Self::Random,
            8 => Self::Down,
            _ => Self::Up,
        }
    }

    pub fn is_one_shot(self) -> bool {
        matches!(self, Self::Down | Self::Up)
    }

    /// Cyclic waveform value in [-1, 1] at `phase` (0..1). `sh` is the
    /// current sample-and-hold value for `Random`. One-shot shapes
    /// (`Down`/`Up`) are not cyclic and fall back to `SinePos` here —
    /// callers with one-shot support (the filter LFO) handle them via
    /// their own sweep phase.
    fn cyclic_value(self, phase: f64, sh: f64) -> f64 {
        match self {
            FilterLfoShape::SinePos | FilterLfoShape::Down | FilterLfoShape::Up => {
                (std::f64::consts::TAU * phase).cos()
            }
            FilterLfoShape::SineNeg => -(std::f64::consts::TAU * phase).cos(),
            FilterLfoShape::TrianglePos => 1.0 - 4.0 * (phase - 0.5).abs(),
            FilterLfoShape::TriangleNeg => 4.0 * (phase - 0.5).abs() - 1.0,
            FilterLfoShape::SquarePos => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            FilterLfoShape::Saw => 1.0 - 2.0 * phase,
            FilterLfoShape::Ramp => 2.0 * phase - 1.0,
            FilterLfoShape::Random => sh,
        }
    }
}

/// Filter placement relative to the delay line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterLocation {
    Pre,
    Post,
}

/// Chamberlin state-variable filter (lowpass output), stable to ~fs/6,
/// which covers the swept range used here.
#[derive(Debug, Clone)]
struct Svf {
    low: f64,
    band: f64,
    f: f64,
    q_inv: f64,
}

impl Svf {
    fn new() -> Self {
        Self {
            low: 0.0,
            band: 0.0,
            f: 0.1,
            q_inv: 1.0 / 0.707,
        }
    }

    fn set(&mut self, cutoff_hz: f64, q: f64, sample_rate: f64) {
        let fc = cutoff_hz.clamp(40.0, sample_rate / 6.5);
        self.f = 2.0 * (std::f64::consts::PI * fc / sample_rate).sin();
        self.q_inv = 1.0 / q.clamp(0.5, 10.0);
    }

    #[inline]
    fn tick_lp(&mut self, input: f64) -> f64 {
        let high = input - self.low - self.q_inv * self.band;
        self.band = flush(self.band + self.f * high);
        self.low = flush(self.low + self.f * self.band);
        self.low
    }

    fn reset(&mut self) {
        self.low = 0.0;
        self.band = 0.0;
    }
}

pub struct FilterDelay {
    /// Base delay time in ms (clamped to 60–2500).
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// LFO waveform.
    pub lfo_shape: FilterLfoShape,
    /// LFO rate as a ratio of the delay time: the LFO completes
    /// `lfo_speed` cycles per delay period (1/32–32).
    pub lfo_speed: f64,
    /// Sweep depth (0.0–1.0): 0 = static filter, 1 = ±2 octaves.
    pub depth: f64,
    /// Sweep center frequency in Hz.
    pub center_hz: f64,
    /// Filter resonance (0.5–10.0).
    pub q: f64,
    /// Filter pre or post the delay line.
    pub location: FilterLocation,
    /// Tremolo depth on the repeats (0.0–1.0). 0 = off.
    pub trem_depth: f64,
    /// Tremolo rate as a ratio of the delay time (like `lfo_speed`).
    pub trem_speed: f64,
    /// Tremolo waveform. Cyclic shapes only (the MX gives trem its own
    /// shape list without the attack-triggered one-shots); `Down`/`Up`
    /// fall back to `SinePos`.
    pub trem_shape: FilterLfoShape,
    /// Decay EQ tilt (shared engine param).
    pub decay_tilt: f64,

    delay: DelayLine,
    svf: Svf,
    decay_tilt_eq: DecayTilt,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    lfo_phase: f64,
    trem_phase: f64,
    sh_value: f64,
    /// One-shot sweep phase (Down/Up shapes): 0→1 per attack, then holds.
    one_shot_phase: f64,
    /// Attack detector for one-shot sweeps.
    attack_env: audiocore_dsp::envelope::EnvelopeFollower,
    attack_gate: bool,
    rng: XorShift32,
    // SVF coefficients refresh at control-block rate while sweeping.
    ctrl_countdown: u32,
}

impl Default for FilterDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterDelay {
    pub const MIN_TIME_MS: f64 = 60.0;
    pub const MAX_TIME_MS: f64 = 2500.0;
    const MAX_DELAY_S: f64 = 3.0;
    const CTRL_BLOCK: u32 = 16;

    pub fn new() -> Self {
        Self {
            time_ms: 400.0,
            feedback: 0.4,
            lfo_shape: FilterLfoShape::SinePos,
            lfo_speed: 1.0,
            depth: 0.5,
            center_hz: 1200.0,
            q: 2.0,
            location: FilterLocation::Post,
            trem_depth: 0.0,
            trem_speed: 4.0,
            trem_shape: FilterLfoShape::SinePos,
            decay_tilt: 0.0,
            delay: DelayLine::new(48000 * 3 + 1024),
            svf: Svf::new(),
            decay_tilt_eq: DecayTilt::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            lfo_phase: 0.0,
            trem_phase: 0.0,
            sh_value: 0.0,
            one_shot_phase: 1.0,
            attack_env: audiocore_dsp::envelope::EnvelopeFollower::new(0.0),
            attack_gate: false,
            rng: XorShift32::new(0x00F1_17E4),
            ctrl_countdown: 0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.time_ms = self.time_ms.clamp(Self::MIN_TIME_MS, Self::MAX_TIME_MS);
        self.lfo_speed = self.lfo_speed.clamp(1.0 / 32.0, 32.0);
        self.trem_speed = self.trem_speed.clamp(1.0 / 32.0, 32.0);

        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

        self.svf.set(self.center_hz, self.q, sample_rate);

        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        self.smoother.set_time_seeded(0.15, sample_rate, self.time_ms * 0.001 * sample_rate);

        // Attack detector for the one-shot Down/Up sweeps.
        self.attack_env.set_times_ms(3.0, 150.0, sample_rate);
    }

    /// LFO value in [-1, 1] for the current phase.
    fn lfo_value(&mut self) -> f64 {
        match self.lfo_shape {
            // One-shots use one_shot_phase, driven per-attack in tick().
            FilterLfoShape::Down => 1.0 - 2.0 * self.one_shot_phase.min(1.0),
            FilterLfoShape::Up => 2.0 * self.one_shot_phase.min(1.0) - 1.0,
            shape => shape.cyclic_value(self.lfo_phase, self.sh_value),
        }
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();
        let delay_s = smooth_delay / self.sample_rate;

        // LFO/trem phase: `speed` cycles per delay period.
        let lfo_inc = self.lfo_speed / (delay_s * self.sample_rate).max(64.0);
        self.lfo_phase += lfo_inc;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
            self.sh_value = self.rng.next_bipolar();
        }

        // One-shot Down/Up: detect input attacks and restart the sweep.
        if self.lfo_shape.is_one_shot() {
            let env = self.attack_env.tick(input.abs());
            if env > 0.02 {
                if !self.attack_gate {
                    self.attack_gate = true;
                    self.one_shot_phase = 0.0;
                }
            } else if env < 0.01 {
                self.attack_gate = false;
            }
            // Sweep completes over one LFO period, then holds.
            self.one_shot_phase = (self.one_shot_phase + lfo_inc).min(1.0);
        }
        let trem_inc = self.trem_speed / (delay_s * self.sample_rate).max(64.0);
        self.trem_phase += trem_inc;
        if self.trem_phase >= 1.0 {
            self.trem_phase -= 1.0;
        }

        // Refresh SVF cutoff at control-block rate while sweeping.
        if self.ctrl_countdown == 0 {
            self.ctrl_countdown = Self::CTRL_BLOCK;
            if self.depth > 1e-4 {
                let sweep_oct = self.lfo_value() * self.depth * 2.0;
                let cutoff = self.center_hz * sweep_oct.exp2();
                self.svf.set(cutoff, self.q, self.sample_rate);
            }
        }
        self.ctrl_countdown -= 1;

        let filtered_in = if self.location == FilterLocation::Pre {
            self.svf.tick_lp(input)
        } else {
            input
        };

        let max_read = self.delay.len() as f64 - 4.0;
        let mut output = self.delay.read_cubic(smooth_delay.clamp(1.0, max_read));

        if self.location == FilterLocation::Post {
            output = self.svf.tick_lp(output);
        }

        // Synced tremolo on the repeats (own shape list, cyclic only).
        if self.trem_depth > 1e-4 {
            let wave = self.trem_shape.cyclic_value(self.trem_phase, self.sh_value);
            let trem = 1.0 - self.trem_depth * (0.5 + 0.5 * wave);
            output *= trem;
        }

        let mut fb = output * self.feedback;
        fb = self.decay_tilt_eq.tick(fb, ch);
        fb = fb.clamp(-1.5, 1.5);

        self.delay.write(filtered_in + fb);
        self.feedback_sample = fb;

        output
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.svf.reset();
        self.decay_tilt_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.lfo_phase = 0.0;
        self.trem_phase = 0.0;
        self.sh_value = 0.0;
        self.one_shot_phase = 1.0;
        self.attack_env.reset(0.0);
        self.attack_gate = false;
        self.ctrl_countdown = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn trem_shapes_produce_distinct_patterns() {
        let run = |shape: FilterLfoShape| -> Vec<f64> {
            let mut d = FilterDelay::new();
            d.time_ms = 200.0;
            d.feedback = 0.0;
            d.depth = 0.0; // filter static; isolate the trem
            d.trem_depth = 1.0;
            d.trem_speed = 4.0;
            d.trem_shape = shape;
            d.update(SR);
            let mut out = Vec::with_capacity(48000);
            for i in 0..48000 {
                let input = (std::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.5;
                let v = d.tick(input, 0);
                assert!(v.is_finite(), "{shape:?} NaN at {i}");
                out.push(v);
            }
            out
        };

        let sine = run(FilterLfoShape::SinePos);
        let square = run(FilterLfoShape::SquarePos);
        // A square trem gates hard: its output must differ from the sine
        // trem on the same signal.
        let diff: f64 = sine
            .iter()
            .zip(square.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(diff > 10.0, "square vs sine trem should differ: {diff}");

        // One-shot shapes fall back to a cyclic wave on the trem: valid
        // output, no NaN (already asserted inside run()).
        let _ = run(FilterLfoShape::Down);
    }

    #[test]
    fn impulse_delayed() {
        let mut d = FilterDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.depth = 0.0;
        d.trem_depth = 0.0;
        d.center_hz = 6000.0;
        d.update(SR);

        let expected = (100.0 * SR / 1000.0) as i64;
        let mut peak_pos = 0i64;
        let mut peak = 0.0f64;
        for i in 0..20000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let out = d.tick(input, 0);
            if out.abs() > peak {
                peak = out.abs();
                peak_pos = i;
            }
        }
        assert!(
            (peak_pos - expected).abs() < 200,
            "peak at {peak_pos}, expected near {expected}"
        );
    }

    #[test]
    fn sweep_actually_moves_the_filter() {
        // A high-frequency tone through a deeply swept LP should show
        // amplitude variation at the sweep rate.
        let mut d = FilterDelay::new();
        d.time_ms = 250.0;
        d.feedback = 0.0;
        d.depth = 1.0;
        d.center_hz = 800.0;
        d.q = 1.0;
        d.lfo_speed = 4.0;
        d.update(SR);

        let mut min_env = f64::MAX;
        let mut max_env = 0.0f64;
        let mut env = 0.0;
        for i in 0..96000 {
            let input = (std::f64::consts::TAU * 3000.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            env = 0.999 * env + 0.001 * out.abs();
            if i > 48000 {
                min_env = min_env.min(env);
                max_env = max_env.max(env);
            }
        }
        assert!(
            max_env > min_env * 1.5,
            "sweep should modulate the tone level: min={min_env}, max={max_env}"
        );
    }

    #[test]
    fn tremolo_gates_repeats() {
        let mut d = FilterDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.depth = 0.0;
        d.center_hz = 8000.0;
        d.trem_depth = 1.0;
        d.trem_speed = 8.0;
        d.update(SR);

        let mut min_env = f64::MAX;
        let mut max_env = 0.0f64;
        let mut env = 0.0;
        for i in 0..96000 {
            let input = (std::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            env = 0.995 * env + 0.005 * out.abs();
            if i > 48000 {
                min_env = min_env.min(env);
                max_env = max_env.max(env);
            }
        }
        assert!(
            max_env > min_env * 1.3,
            "tremolo should modulate repeats: min={min_env}, max={max_env}"
        );
    }

    #[test]
    fn one_shot_down_sweeps_once_per_attack() {
        let mut d = FilterDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.0;
        d.depth = 1.0;
        d.center_hz = 1500.0;
        d.q = 1.0;
        d.lfo_shape = FilterLfoShape::Down;
        d.lfo_speed = 2.0; // sweep completes in half the delay time
        d.update(SR);

        // Silence → the one-shot phase should hold (parked), then a
        // burst restarts it: verify the sweep phase resets on attack.
        for _ in 0..9600 {
            d.tick(0.0, 0);
        }
        let parked = d.one_shot_phase;
        assert!(parked >= 1.0, "sweep should be parked: {parked}");

        // Attack: a loud burst.
        for i in 0..480 {
            let x = (std::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.8;
            d.tick(x, 0);
        }
        assert!(
            d.one_shot_phase < 1.0,
            "attack should restart the one-shot sweep: {}",
            d.one_shot_phase
        );
    }

    #[test]
    fn all_shapes_no_nan() {
        for i in 0..FilterLfoShape::COUNT {
            let mut d = FilterDelay::new();
            d.time_ms = 200.0;
            d.feedback = 0.7;
            d.depth = 1.0;
            d.q = 10.0;
            d.lfo_shape = FilterLfoShape::from_index(i);
            d.lfo_speed = 32.0;
            d.trem_depth = 0.5;
            d.update(SR);

            for s in 0..48000 {
                let input = (std::f64::consts::TAU * 440.0 * s as f64 / SR).sin() * 0.5;
                let out = d.tick(input, 0);
                assert!(out.is_finite(), "shape {i} NaN at {s}");
            }
        }
    }
}
