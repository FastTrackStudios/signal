//! Wow and flutter modulation for tape delay.
//!
//! Wow uses a cosine LFO plus Ornstein-Uhlenbeck stochastic drift (from
//! ChowDSP/ZetaCarinaeModules). Flutter uses three harmonically related
//! oscillators with distinct amplitudes and phase offsets.

use std::f64::consts::PI;

/// Wobble/LFO waveform shapes for wow modulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WobbleShape {
    Sine,
    Triangle,
    Square,
    SampleAndHold,
    Random,
}

impl WobbleShape {
    pub const COUNT: usize = 5;

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Sine,
            1 => Self::Triangle,
            2 => Self::Square,
            3 => Self::SampleAndHold,
            4 => Self::Random,
            _ => Self::Sine,
        }
    }

    pub fn to_index(self) -> usize {
        match self {
            Self::Sine => 0,
            Self::Triangle => 1,
            Self::Square => 2,
            Self::SampleAndHold => 3,
            Self::Random => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sine => "Sine",
            Self::Triangle => "Triangle",
            Self::Square => "Square",
            Self::SampleAndHold => "S&H",
            Self::Random => "Random",
        }
    }
}

// r[impl delay.modulation.flutter]
/// Three-oscillator flutter LFO (from ChowDSP AnalogTapeModel via qdelay).
///
/// Oscillators at fundamental, 2x, and 3x frequency with distinct amplitudes
/// and phase offsets produce a complex, non-repeating modulation pattern.
pub struct Flutter {
    pub rate: f64,
    pub depth: f64,
    phase: f64,
    sample_rate: f64,
    amp1: f64,
    amp2: f64,
    amp3: f64,
}

impl Flutter {
    pub fn new() -> Self {
        Self {
            rate: 0.3,
            depth: 0.0,
            phase: 0.0,
            sample_rate: 48000.0,
            amp1: 0.0,
            amp2: 0.0,
            amp3: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.update_amps();
    }

    fn update_amps(&mut self) {
        // Amplitude coefficients scaled by sample rate (from qdelay)
        self.amp1 = -230.0 * 1000.0 / self.sample_rate;
        self.amp2 = -80.0 * 1000.0 / self.sample_rate;
        self.amp3 = -99.0 * 1000.0 / self.sample_rate;
    }

    /// Returns modulation offset in samples.
    pub fn tick(&mut self) -> f64 {
        let d2 = self.depth * self.depth; // Squared for nonlinear response
        let phase_inc = 2.0 * PI * self.rate / self.sample_rate;
        self.phase += phase_inc;
        if self.phase > 2.0 * PI {
            self.phase -= 2.0 * PI;
        }

        let p = self.phase;
        let lfo = self.amp1 * (p).cos()
            + self.amp2 * (2.0 * p + 13.0 * PI / 4.0).cos()
            + self.amp3 * (3.0 * p - PI / 10.0).cos();

        lfo * d2
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
    }
}

// r[impl delay.modulation.wow]
// r[impl delay.modulation.wow.ou-process]
/// Wow modulation: cosine LFO + Ornstein-Uhlenbeck stochastic drift.
///
/// The OU process generates slow, continuous random drift that models real
/// tape transport irregularities — partially periodic, partially random.
pub struct Wow {
    pub rate: f64,
    pub depth: f64,
    pub drift: f64,
    /// LFO waveform shape.
    pub shape: WobbleShape,
    /// Phase offset (0.0-1.0) for L/R sync control.
    pub phase_offset: f64,
    phase: f64,
    sample_rate: f64,
    amp: f64,
    ou_state: f64,
    ou_decay: f64,
    rng_state: u64,
    sh_value: f64,
    sh_triggered: bool,
}

impl Wow {
    pub fn new() -> Self {
        Self {
            rate: 0.5,
            depth: 0.0,
            drift: 0.3,
            shape: WobbleShape::Sine,
            phase_offset: 0.0,
            phase: 0.0,
            sample_rate: 48000.0,
            amp: 0.0,
            ou_state: 0.0,
            ou_decay: 0.999,
            rng_state: 0xDEAD_BEEF_CAFE_1234,
            sh_value: 0.0,
            sh_triggered: false,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.amp = 1000.0 * 1000.0 / sr;
        self.ou_decay = (-1.0 / (2.0 * sr)).exp();
    }

    /// Compute the raw LFO waveform value (-1..1) from the phase.
    fn waveform(&mut self, phase: f64) -> f64 {
        match self.shape {
            WobbleShape::Sine => phase.cos(),
            WobbleShape::Triangle => {
                let t = phase / (2.0 * PI);
                if t < 0.25 {
                    t * 4.0
                } else if t < 0.75 {
                    2.0 - t * 4.0
                } else {
                    t * 4.0 - 4.0
                }
            }
            WobbleShape::Square => {
                if phase < PI {
                    1.0
                } else {
                    -1.0
                }
            }
            WobbleShape::SampleAndHold => {
                let in_first_half = phase < PI;
                if in_first_half && !self.sh_triggered {
                    self.sh_value = self.xorshift_uniform() * 2.0 - 1.0;
                    self.sh_triggered = true;
                } else if !in_first_half {
                    self.sh_triggered = false;
                }
                self.sh_value
            }
            WobbleShape::Random => self.ou_state,
        }
    }

    /// Returns modulation offset in samples.
    pub fn tick(&mut self) -> f64 {
        let d2 = self.depth * self.depth;

        // Advance OU process
        let noise = self.gaussian_noise();
        self.ou_state =
            self.ou_state * self.ou_decay + noise * (1.0 - self.ou_decay * self.ou_decay).sqrt();

        // Rate modulated by drift
        let freq_adjust = self.rate * (1.0 + self.ou_state.abs().powf(1.25) * self.drift);
        let phase_inc = 2.0 * PI * freq_adjust / self.sample_rate;
        self.phase += phase_inc;
        if self.phase > 2.0 * PI {
            self.phase -= 2.0 * PI;
        }

        // Apply phase offset and compute waveform
        let offset_phase = (self.phase + self.phase_offset * 2.0 * PI) % (2.0 * PI);
        let lfo = self.waveform(offset_phase);

        self.amp * lfo * d2
    }

    fn gaussian_noise(&mut self) -> f64 {
        let mut sum = 0.0;
        for _ in 0..8 {
            sum += self.xorshift_uniform();
        }
        (sum - 4.0) * 0.612
    }

    fn xorshift_uniform(&mut self) -> f64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        (self.rng_state as f64) / (u64::MAX as f64)
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.ou_state = 0.0;
        self.sh_value = 0.0;
        self.sh_triggered = false;
    }
}

// r[impl delay.modulation.ducking]
/// Envelope follower for ducking — ducks delay output when dry input is loud.
///
/// Uses adaptive release (from qdelay): larger level drops trigger faster
/// release to prevent pumping.
pub struct DuckingFollower {
    pub attack_ms: f64,
    pub release_ms: f64,
    pub threshold: f64,
    pub amount: f64,
    envelope: f64,
    attack_coeff: f64,
    release_coeff: f64,
    release_fast_coeff: f64,
    sample_rate: f64,
}

impl DuckingFollower {
    pub fn new() -> Self {
        Self {
            attack_ms: 5.0,
            release_ms: 200.0,
            threshold: 0.0,
            amount: 0.0,
            envelope: 0.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            release_fast_coeff: 0.0,
            sample_rate: 48000.0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.update_coeffs();
    }

    pub fn update_coeffs(&mut self) {
        // Target -14 dB convergence in the specified time
        let target = 0.2_f64.ln();
        self.attack_coeff = (target / (self.attack_ms * 0.001 * self.sample_rate)).exp();
        self.release_coeff = (target / (self.release_ms * 0.001 * self.sample_rate)).exp();
        // Fast release is 20% of normal release time
        self.release_fast_coeff =
            (target / (self.release_ms * 0.2 * 0.001 * self.sample_rate)).exp();
    }

    /// Feed the dry input level, returns a gain multiplier for the wet signal (0..1).
    pub fn tick(&mut self, input_abs: f64) -> f64 {
        let level = if input_abs > self.threshold {
            input_abs - self.threshold
        } else {
            0.0
        };

        if level > self.envelope {
            self.envelope = self.attack_coeff * self.envelope + (1.0 - self.attack_coeff) * level;
        } else {
            // Adaptive release: faster when the drop is large
            let ratio = if self.envelope > 1e-10 {
                let r = (self.envelope - level) / self.envelope;
                r * r
            } else {
                0.0
            };
            let coeff = self.release_coeff + ratio * (self.release_fast_coeff - self.release_coeff);
            self.envelope = coeff * self.envelope + (1.0 - coeff) * level;
        }

        // Convert envelope to duck gain
        let duck = (1.0 - self.envelope * self.amount).clamp(0.0, 1.0);
        duck
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0;
    }
}

// r[impl delay.modulation.diffusion]
/// 8-allpass diffusion network (from Tarons MiniVerb via qdelay).
///
/// Cascaded allpass filters create diffuse, reverb-like smearing of the
/// delay output. Size controls delay times, smear controls feedback.
pub struct Diffuser {
    pub size: f64,
    pub smear: f64,
    allpasses: [AllpassFilter; 8],
}

/// Single allpass filter with fractional delay.
struct AllpassFilter {
    buffer: Vec<f64>,
    write_pos: usize,
    delay: f64,
    feedback: f64,
}

impl AllpassFilter {
    fn new(max_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; max_samples + 2],
            write_pos: 0,
            delay: 1.0,
            feedback: 0.5,
        }
    }

    fn tick(&mut self, input: f64) -> f64 {
        let len = self.buffer.len();
        let int_delay = self.delay as usize;
        let frac = self.delay - int_delay as f64;

        // Linear interpolation read
        let pos_a = (self.write_pos + len - int_delay) % len;
        let pos_b = (self.write_pos + len - int_delay - 1) % len;
        let delayed = self.buffer[pos_a] + frac * (self.buffer[pos_b] - self.buffer[pos_a]);

        let output = -input * self.feedback + delayed;
        let write_val = input + delayed * self.feedback;

        self.buffer[self.write_pos] = write_val;
        self.write_pos = (self.write_pos + 1) % len;

        output
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
    }
}

// Base delay coefficients per channel (from qdelay/Tarons MiniVerb)
const DIFFUSE_DELAYS_L: [f64; 8] = [12.11, 10.49, 8.51, 7.13, 5.37, 4.21, 3.07, 2.11];
const DIFFUSE_DELAYS_R: [f64; 8] = [12.08, 10.47, 8.49, 7.11, 5.35, 4.19, 3.05, 2.09];

impl Diffuser {
    pub fn new(sample_rate: f64, is_right: bool) -> Self {
        let mps = sample_rate / 343.0; // Samples per meter (speed of sound)
        let base_distance = mps * 3.75;
        let delays = if is_right {
            DIFFUSE_DELAYS_R
        } else {
            DIFFUSE_DELAYS_L
        };

        let allpasses = std::array::from_fn(|i| {
            let max_delay = (base_distance * delays[i] * 2.0) as usize + 4;
            let mut ap = AllpassFilter::new(max_delay);
            ap.delay = base_distance * delays[i];
            ap
        });

        Self {
            size: 0.5,
            smear: 0.5,
            allpasses,
        }
    }

    pub fn update(&mut self, sample_rate: f64, is_right: bool) {
        let mps = sample_rate / 343.0;
        let base_distance = mps * 3.75;
        let delays = if is_right {
            DIFFUSE_DELAYS_R
        } else {
            DIFFUSE_DELAYS_L
        };
        let offset = 0.9 - 0.9 * self.size;

        for (i, ap) in self.allpasses.iter_mut().enumerate() {
            ap.delay = (base_distance * delays[i] * (1.0 - offset)).max(1.0);
            ap.feedback = self.smear.clamp(0.0, 0.9);
        }
    }

    pub fn tick(&mut self, input: f64) -> f64 {
        let mut sample = input;
        for ap in &mut self.allpasses {
            sample = ap.tick(sample);
        }
        sample
    }

    pub fn reset(&mut self) {
        for ap in &mut self.allpasses {
            ap.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn flutter_no_nan() {
        let mut f = Flutter::new();
        f.set_sample_rate(SR);
        f.depth = 0.5;
        f.rate = 5.0;
        for _ in 0..48000 {
            let v = f.tick();
            assert!(v.is_finite(), "Flutter produced NaN/Inf");
        }
    }

    #[test]
    fn flutter_zero_depth_is_zero() {
        let mut f = Flutter::new();
        f.set_sample_rate(SR);
        f.depth = 0.0;
        for _ in 0..1000 {
            assert_eq!(f.tick(), 0.0);
        }
    }

    #[test]
    fn wow_no_nan() {
        let mut w = Wow::new();
        w.set_sample_rate(SR);
        w.depth = 0.8;
        w.drift = 1.0;
        for _ in 0..96000 {
            let v = w.tick();
            assert!(v.is_finite(), "Wow produced NaN/Inf");
        }
    }

    #[test]
    fn wow_zero_depth_is_zero() {
        let mut w = Wow::new();
        w.set_sample_rate(SR);
        w.depth = 0.0;
        for _ in 0..1000 {
            assert_eq!(w.tick(), 0.0);
        }
    }

    #[test]
    fn wow_has_variation() {
        let mut w = Wow::new();
        w.set_sample_rate(SR);
        w.depth = 0.5;
        w.drift = 0.5;
        let mut min = f64::MAX;
        let mut max = f64::MIN;
        for _ in 0..48000 {
            let v = w.tick();
            min = min.min(v);
            max = max.max(v);
        }
        assert!(
            max - min > 0.1,
            "Wow should produce variation: range={}",
            max - min
        );
    }

    #[test]
    fn ducking_follower_ducks_on_loud() {
        let mut d = DuckingFollower::new();
        d.set_sample_rate(SR);
        d.amount = 1.0;
        d.threshold = 0.1;
        d.attack_ms = 1.0;
        d.update_coeffs();

        // Feed silence — gain should be ~1
        for _ in 0..1000 {
            let g = d.tick(0.0);
            assert!(g > 0.9, "Should not duck during silence: {g}");
        }

        // Feed loud signal — gain should drop
        let mut min_gain: f64 = 1.0;
        for _ in 0..4800 {
            let g = d.tick(0.8);
            min_gain = min_gain.min(g);
        }
        assert!(
            min_gain < 0.5,
            "Should duck on loud input: min_gain={min_gain}"
        );
    }

    #[test]
    fn diffuser_no_nan() {
        let mut d = Diffuser::new(SR, false);
        d.update(SR, false);
        for i in 0..48000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let v = d.tick(input);
            assert!(v.is_finite(), "Diffuser produced NaN/Inf at sample {i}");
        }
    }

    #[test]
    fn wow_shapes_no_nan() {
        for i in 0..WobbleShape::COUNT {
            let mut w = Wow::new();
            w.set_sample_rate(SR);
            w.depth = 0.5;
            w.drift = 0.5;
            w.shape = WobbleShape::from_index(i);
            for _ in 0..48000 {
                let v = w.tick();
                assert!(v.is_finite(), "{:?} shape produced NaN/Inf", w.shape);
            }
        }
    }

    #[test]
    fn wow_phase_offset_shifts_output() {
        let mut w1 = Wow::new();
        w1.set_sample_rate(SR);
        w1.depth = 0.5;
        w1.phase_offset = 0.0;

        let mut w2 = Wow::new();
        w2.set_sample_rate(SR);
        w2.depth = 0.5;
        w2.phase_offset = 0.5; // Half cycle offset

        // With same drift=0 (deterministic), outputs should differ
        w1.drift = 0.0;
        w2.drift = 0.0;

        let mut diff = 0.0;
        for _ in 0..4800 {
            diff += (w1.tick() - w2.tick()).abs();
        }
        assert!(diff > 0.1, "Phase offset should shift output: diff={diff}");
    }

    #[test]
    fn diffuser_spreads_impulse() {
        let mut d = Diffuser::new(SR, false);
        d.smear = 0.7;
        d.update(SR, false);

        let mut nonzero_count = 0;
        for i in 0..48000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let v = d.tick(input);
            if v.abs() > 1e-6 {
                nonzero_count += 1;
            }
        }
        assert!(
            nonzero_count > 100,
            "Diffuser should spread impulse over many samples: got {nonzero_count}"
        );
    }
}
