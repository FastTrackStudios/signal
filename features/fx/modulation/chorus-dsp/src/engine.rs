//! Chorus engines — five distinct voice implementations.
//!
//! Each engine produces a single modulated delay voice. The chain
//! creates multiple voices per channel for the full chorus effect.

use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::denormal::flush;
use std::f64::consts::PI;

/// Chorus effect type — controls delay time ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectType {
    Chorus,
    Flanger,
    Vibrato,
}

impl EffectType {
    pub fn base_delay_ms(&self) -> f64 {
        match self {
            EffectType::Chorus => 10.0,
            EffectType::Flanger => 2.0,
            EffectType::Vibrato => 5.0,
        }
    }

    pub fn max_depth_ms(&self) -> f64 {
        match self {
            EffectType::Chorus => 12.0,
            EffectType::Flanger => 4.0,
            EffectType::Vibrato => 8.0,
        }
    }
}

/// Chorus engine type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineType {
    Cubic,
    Bbd,
    Tape,
    Orbit,
    /// Juno-style chorus with triangle LFO + allpass interpolation.
    /// Based on TAL-NoiseMaker / YKChorus (SpotlightKid) algorithm.
    Juno,
}

/// Common voice trait.
pub trait ChorusEngine: Send {
    fn update(&mut self, sample_rate: f64);
    fn tick(
        &mut self,
        input: f64,
        rate_hz: f64,
        depth: f64,
        feedback: f64,
        color: f64,
        effect_type: EffectType,
    ) -> f64;
    fn reset(&mut self);
}

// ─── Cubic Engine ───────────────────────────────────────────────────

pub struct CubicVoice {
    delay: DelayLine,
    lfo_phase: f64,
    phase_offset: f64,
    sample_rate: f64,
}

impl CubicVoice {
    const MAX_DELAY: usize = 192000 / 20 + 64;
    pub fn new(phase_offset: f64) -> Self {
        Self {
            delay: DelayLine::new(Self::MAX_DELAY),
            lfo_phase: 0.0,
            phase_offset,
            sample_rate: 48000.0,
        }
    }
}

impl ChorusEngine for CubicVoice {
    fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let needed = (sample_rate * 0.05) as usize + 64;
        if self.delay.len() < needed {
            self.delay = DelayLine::new(needed);
        }
    }

    fn tick(
        &mut self,
        input: f64,
        rate_hz: f64,
        depth: f64,
        feedback: f64,
        _color: f64,
        effect_type: EffectType,
    ) -> f64 {
        self.lfo_phase = (self.lfo_phase + rate_hz / self.sample_rate).fract();
        let lfo = ((self.lfo_phase + self.phase_offset) * 2.0 * PI).sin();
        let base_ms = effect_type.base_delay_ms();
        let depth_ms = effect_type.max_depth_ms() * depth;
        let delay_samples = ((base_ms + depth_ms * lfo) * 0.001 * self.sample_rate)
            .clamp(1.0, self.delay.len() as f64 - 4.0);
        let delayed = self.delay.read_cubic(delay_samples);
        self.delay
            .write(input + (delayed * feedback).clamp(-1.5, 1.5));
        delayed
    }

    fn reset(&mut self) {
        self.delay.clear();
        self.lfo_phase = 0.0;
    }
}

// ─── BBD Engine ─────────────────────────────────────────────────────

pub struct BbdVoice {
    lfo_phase: f64,
    phase_offset: f64,
    buckets: Vec<f64>,
    clock_phase: f64,
    prev_output: f64,
    input_lp: f64,
    output_lp: f64,
    num_stages: usize,
    sample_rate: f64,
}

impl BbdVoice {
    const DEFAULT_STAGES: usize = 512;
    pub fn new(phase_offset: f64) -> Self {
        Self {
            lfo_phase: 0.0,
            phase_offset,
            buckets: vec![0.0; Self::DEFAULT_STAGES],
            clock_phase: 0.0,
            prev_output: 0.0,
            input_lp: 0.0,
            output_lp: 0.0,
            num_stages: Self::DEFAULT_STAGES,
            sample_rate: 48000.0,
        }
    }
}

impl ChorusEngine for BbdVoice {
    fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn tick(
        &mut self,
        input: f64,
        rate_hz: f64,
        depth: f64,
        feedback: f64,
        color: f64,
        effect_type: EffectType,
    ) -> f64 {
        self.lfo_phase = (self.lfo_phase + rate_hz / self.sample_rate).fract();
        let lfo = ((self.lfo_phase + self.phase_offset) * 2.0 * PI).sin();
        let base_ms = effect_type.base_delay_ms();
        let depth_ms = effect_type.max_depth_ms() * depth;
        let target_delay_ms = base_ms + depth_ms * lfo;
        let target_delay_s = (target_delay_ms * 0.001).max(0.0005);
        let clock_freq = self.num_stages as f64 / (2.0 * target_delay_s);
        // Deliberately NOT audiocore's OnePoleLp: the cutoff tracks the BBD
        // clock per-sample and this cheap sin() coefficient approximation is
        // part of the voicing.
        let input_lp_coeff = (2.0 * PI * (clock_freq / 6.0) / self.sample_rate)
            .sin()
            .clamp(0.0, 0.99);
        self.input_lp = flush(self.input_lp + (input - self.input_lp) * input_lp_coeff);
        let clock_inc = clock_freq / self.sample_rate;
        self.clock_phase += clock_inc;
        let mut output = self.prev_output;
        if self.clock_phase >= 1.0 {
            self.clock_phase -= 1.0;
            let fb = output * feedback;
            let bucket_input = (self.input_lp + fb.clamp(-1.0, 1.0)).clamp(-1.5, 1.5);
            for i in (1..self.num_stages).rev() {
                self.buckets[i] = self.buckets[i - 1];
            }
            self.buckets[0] = bucket_input;
            output = *self.buckets.last().unwrap_or(&0.0);
        }
        let frac = self.clock_phase.clamp(0.0, 1.0);
        let held = self.prev_output * (1.0 - frac) + output * frac;
        self.prev_output = output;
        let output_cutoff = clock_freq / (6.0 - color * 4.0).max(1.5);
        let output_lp_coeff = (2.0 * PI * output_cutoff / self.sample_rate)
            .sin()
            .clamp(0.0, 0.99);
        self.output_lp = flush(self.output_lp + (held - self.output_lp) * output_lp_coeff);
        self.output_lp
    }

    fn reset(&mut self) {
        self.buckets.fill(0.0);
        self.clock_phase = 0.0;
        self.prev_output = 0.0;
        self.input_lp = 0.0;
        self.output_lp = 0.0;
        self.lfo_phase = 0.0;
    }
}

// ─── Tape Engine ────────────────────────────────────────────────────

pub struct TapeVoice {
    delay: DelayLine,
    lfo_phase: f64,
    phase_offset: f64,
    wow_phase: f64,
    flutter_phase: f64,
    tone_lp: f64,
    sample_rate: f64,
}

impl TapeVoice {
    const MAX_DELAY: usize = 192000 / 20 + 64;
    pub fn new(phase_offset: f64) -> Self {
        Self {
            delay: DelayLine::new(Self::MAX_DELAY),
            lfo_phase: 0.0,
            phase_offset,
            wow_phase: 0.0,
            flutter_phase: 0.0,
            tone_lp: 0.0,
            sample_rate: 48000.0,
        }
    }
}

impl ChorusEngine for TapeVoice {
    fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let needed = (sample_rate * 0.05) as usize + 64;
        if self.delay.len() < needed {
            self.delay = DelayLine::new(needed);
        }
    }

    fn tick(
        &mut self,
        input: f64,
        rate_hz: f64,
        depth: f64,
        feedback: f64,
        color: f64,
        effect_type: EffectType,
    ) -> f64 {
        self.lfo_phase = (self.lfo_phase + rate_hz / self.sample_rate).fract();
        let lfo = ((self.lfo_phase + self.phase_offset) * 2.0 * PI).sin();
        self.wow_phase = (self.wow_phase + 0.33 / self.sample_rate).fract();
        let wow = (self.wow_phase * 2.0 * PI).sin() * depth * 0.3;
        self.flutter_phase = (self.flutter_phase + 5.8 / self.sample_rate).fract();
        let flutter = ((self.flutter_phase * 2.0 * PI).sin() * 0.6
            + (self.flutter_phase * 4.0 * PI).sin() * 0.3
            + (self.flutter_phase * 6.0 * PI).sin() * 0.1)
            * depth
            * 0.15;
        let base_ms = effect_type.base_delay_ms();
        let depth_ms = effect_type.max_depth_ms() * depth;
        let delay_ms = base_ms + depth_ms * lfo + wow + flutter;
        let delay_samples =
            (delay_ms * 0.001 * self.sample_rate).clamp(1.0, self.delay.len() as f64 - 4.0);
        let delayed = self.delay.read_cubic(delay_samples);
        let drive = 1.0 + color * 2.0;
        let saturated_input = (input * drive).tanh();
        let fb = (delayed * feedback).tanh();
        self.delay.write(saturated_input + fb.clamp(-1.5, 1.5));
        let cutoff = 3000.0 + color * 11000.0;
        let lp_coeff = (2.0 * PI * cutoff / self.sample_rate)
            .sin()
            .clamp(0.0, 0.99);
        self.tone_lp = flush(self.tone_lp + (delayed - self.tone_lp) * lp_coeff);
        self.tone_lp
    }

    fn reset(&mut self) {
        self.delay.clear();
        self.lfo_phase = 0.0;
        self.wow_phase = 0.0;
        self.flutter_phase = 0.0;
        self.tone_lp = 0.0;
    }
}

// ─── Orbit Engine ───────────────────────────────────────────────────

pub struct OrbitVoice {
    delay: DelayLine,
    orbit_phase: f64,
    theta: f64,
    phase_offset: f64,
    sample_rate: f64,
}

impl OrbitVoice {
    const MAX_DELAY: usize = 192000 / 20 + 64;
    pub fn new(phase_offset: f64) -> Self {
        Self {
            delay: DelayLine::new(Self::MAX_DELAY),
            orbit_phase: 0.0,
            theta: 0.0,
            phase_offset,
            sample_rate: 48000.0,
        }
    }
}

impl ChorusEngine for OrbitVoice {
    fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let needed = (sample_rate * 0.05) as usize + 64;
        if self.delay.len() < needed {
            self.delay = DelayLine::new(needed);
        }
    }

    fn tick(
        &mut self,
        input: f64,
        rate_hz: f64,
        depth: f64,
        feedback: f64,
        color: f64,
        effect_type: EffectType,
    ) -> f64 {
        let inc = rate_hz / self.sample_rate;
        self.orbit_phase = (self.orbit_phase + inc).fract();
        self.theta += inc * 0.37;
        if self.theta > 1.0 {
            self.theta -= 1.0;
        }
        let phi = (self.orbit_phase + self.phase_offset) * 2.0 * PI;
        let theta_rad = self.theta * 2.0 * PI;
        let eccentricity = color * 0.8;
        let x = phi.sin();
        let y = (1.0 - eccentricity) * phi.cos();
        let proj = x * theta_rad.cos() + y * theta_rad.sin();
        let proj2 = x * (theta_rad + PI * 0.5).cos() + y * (theta_rad + PI * 0.5).sin();
        let base_ms = effect_type.base_delay_ms();
        let depth_ms = effect_type.max_depth_ms() * depth;
        let delay1_ms = base_ms + depth_ms * proj * 0.7;
        let delay2_ms = base_ms + depth_ms * proj2 * 0.5;
        let max_delay = self.delay.len() as f64 - 4.0;
        let d1 = (delay1_ms * 0.001 * self.sample_rate).clamp(1.0, max_delay);
        let d2 = (delay2_ms * 0.001 * self.sample_rate).clamp(1.0, max_delay);
        let tap1 = self.delay.read_cubic(d1);
        let tap2 = self.delay.read_cubic(d2);
        let blended = tap1 * 0.6 + tap2 * 0.4;
        let fb = (blended * feedback).clamp(-1.5, 1.5);
        self.delay.write(input + fb);
        blended
    }

    fn reset(&mut self) {
        self.delay.clear();
        self.orbit_phase = 0.0;
        self.theta = 0.0;
    }
}

// ─── Juno Engine ───────────────────────────────────────────────────

/// Juno-style chorus: triangle LFO + allpass interpolation + DC blocking.
/// Based on TAL-NoiseMaker / YKChorus (SpotlightKid, GPL-2.0).
///
/// Keeps first-order allpass interpolation (not cubic) on purpose — the
/// allpass state `z1` is part of the original BBD-flavored character and
/// feeds the feedback path.
pub struct JunoVoice {
    delay: DelayLine,
    buf_len: usize,
    lfo_phase: f64,
    lfo_sign: f64,
    phase_offset: f64,
    z1: f64,
    lp_state: f64,
    dc: DcBlocker,
    sample_rate: f64,
}

impl JunoVoice {
    const DELAY_MS: f64 = 7.0;
    const LP_CUTOFF: f64 = 0.95;
    /// Original fixed DC-blocker pole (R = 0.995 at 48 kHz).
    const DC_R: f64 = 0.995;

    pub fn new(phase_offset: f64) -> Self {
        let lfo_phase = phase_offset * 2.0 - 1.0;
        let mut voice = Self {
            delay: DelayLine::new(2048),
            buf_len: 1024,
            lfo_phase,
            lfo_sign: if lfo_phase >= 0.0 { 1.0 } else { -1.0 },
            phase_offset,
            z1: 0.0,
            lp_state: 0.0,
            dc: DcBlocker::new(),
            sample_rate: 48000.0,
        };
        voice.retune_dc();
        voice
    }

    /// Keep the original pole R = 0.995 regardless of sample rate:
    /// R = 1 - 2*pi*fc/sr  =>  fc = (1 - R) * sr / (2*pi).
    fn retune_dc(&mut self) {
        let fc = (1.0 - Self::DC_R) * self.sample_rate / (2.0 * PI);
        self.dc.set_cutoff(fc, self.sample_rate);
    }

    #[inline]
    fn next_lfo(&mut self, rate_hz: f64) -> f64 {
        let step = 4.0 * rate_hz / self.sample_rate;
        self.lfo_phase += self.lfo_sign * step;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase = 2.0 - self.lfo_phase;
            self.lfo_sign = -1.0;
        } else if self.lfo_phase <= -1.0 {
            self.lfo_phase = -2.0 - self.lfo_phase;
            self.lfo_sign = 1.0;
        }
        self.lfo_phase
    }
}

impl ChorusEngine for JunoVoice {
    fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let delay_samples = (Self::DELAY_MS * sample_rate * 0.001).floor() as usize;
        self.buf_len = delay_samples * 2;
        if self.delay.len() < self.buf_len + 4 {
            self.delay = DelayLine::new(self.buf_len + 4);
        }
        self.retune_dc();
    }

    fn tick(
        &mut self,
        input: f64,
        rate_hz: f64,
        depth: f64,
        feedback: f64,
        color: f64,
        effect_type: EffectType,
    ) -> f64 {
        let fb_sample = self.z1 * feedback;
        self.delay.write(input + fb_sample.clamp(-1.5, 1.5));

        let lfo = self.next_lfo(rate_hz);

        // Map LFO to read offset — lfo*0.3+0.4 maps [-1,+1] to [0.1,0.7]
        let base_delay = match effect_type {
            EffectType::Chorus | EffectType::Vibrato => Self::DELAY_MS * self.sample_rate * 0.001,
            EffectType::Flanger => Self::DELAY_MS * self.sample_rate * 0.001 * 0.3,
        };

        let offset = (lfo * 0.3 * depth + 0.4) * base_delay;
        let offset = offset.clamp(1.0, (self.buf_len.saturating_sub(2)) as f64);
        let int_offset = offset.floor() as usize;
        let frac = offset - offset.floor();

        // The just-written sample is 1 back from the write head, so an
        // original ring offset of k maps to DelayLine::read(k + 1).
        let x0 = self.delay.read(int_offset + 1);
        let x1 = self.delay.read(int_offset + 2);

        // First-order allpass interpolation
        let coeff = 1.0 - frac;
        let delayed = flush(x1 + coeff * x0 - coeff * self.z1);
        self.z1 = delayed;

        // One-pole lowpass post-filter (color controls brightness)
        let cutoff_param = Self::LP_CUTOFF * (0.5 + color * 0.5);
        let p = (cutoff_param * 0.98).powi(4);
        self.lp_state = flush((1.0 - p) * delayed + p * self.lp_state);

        self.dc.tick(self.lp_state)
    }

    fn reset(&mut self) {
        self.delay.clear();
        self.lfo_phase = self.phase_offset * 2.0 - 1.0;
        self.lfo_sign = if self.lfo_phase >= 0.0 { 1.0 } else { -1.0 };
        self.z1 = 0.0;
        self.lp_state = 0.0;
        self.dc.reset();
    }
}

/// Create a vector of engines for one channel.
pub fn create_voices(engine: EngineType, count: usize) -> Vec<Box<dyn ChorusEngine>> {
    (0..count)
        .map(|i| {
            let offset = i as f64 / count as f64;
            let voice: Box<dyn ChorusEngine> = match engine {
                EngineType::Cubic => Box::new(CubicVoice::new(offset)),
                EngineType::Bbd => Box::new(BbdVoice::new(offset)),
                EngineType::Tape => Box::new(TapeVoice::new(offset)),
                EngineType::Orbit => Box::new(OrbitVoice::new(offset)),
                EngineType::Juno => Box::new(JunoVoice::new(offset)),
            };
            voice
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const SR: f64 = 48000.0;

    fn test_engine_produces_output(mut voice: Box<dyn ChorusEngine>) {
        voice.update(SR);
        let mut has_output = false;
        for i in 0..9600 {
            let s = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = voice.tick(s, 1.0, 0.5, 0.0, 0.5, EffectType::Chorus);
            if out.abs() > 0.01 {
                has_output = true;
            }
        }
        assert!(has_output, "Engine should produce output");
    }

    fn test_engine_no_nan(mut voice: Box<dyn ChorusEngine>) {
        voice.update(SR);
        for i in 0..48000 {
            let s = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.8;
            let out = voice.tick(s, 3.0, 1.0, 0.8, 0.5, EffectType::Flanger);
            assert!(out.is_finite(), "NaN at {i}");
        }
    }

    #[test]
    fn cubic_produces_output() {
        test_engine_produces_output(Box::new(CubicVoice::new(0.0)));
    }
    #[test]
    fn cubic_no_nan() {
        test_engine_no_nan(Box::new(CubicVoice::new(0.0)));
    }
    #[test]
    fn bbd_produces_output() {
        test_engine_produces_output(Box::new(BbdVoice::new(0.0)));
    }
    #[test]
    fn bbd_no_nan() {
        test_engine_no_nan(Box::new(BbdVoice::new(0.0)));
    }
    #[test]
    fn tape_produces_output() {
        test_engine_produces_output(Box::new(TapeVoice::new(0.0)));
    }
    #[test]
    fn tape_no_nan() {
        test_engine_no_nan(Box::new(TapeVoice::new(0.0)));
    }
    #[test]
    fn orbit_produces_output() {
        test_engine_produces_output(Box::new(OrbitVoice::new(0.0)));
    }
    #[test]
    fn orbit_no_nan() {
        test_engine_no_nan(Box::new(OrbitVoice::new(0.0)));
    }
    #[test]
    fn juno_produces_output() {
        test_engine_produces_output(Box::new(JunoVoice::new(0.0)));
    }
    #[test]
    fn juno_no_nan() {
        test_engine_no_nan(Box::new(JunoVoice::new(0.0)));
    }

    #[test]
    fn juno_stereo_phase_offset() {
        let mut voice_l = JunoVoice::new(1.0);
        let mut voice_r = JunoVoice::new(0.0);
        voice_l.update(SR);
        voice_r.update(SR);
        let mut diff_sum: f64 = 0.0;
        for i in 0..4800 {
            let s = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            let l = voice_l.tick(s, 0.5, 0.5, 0.0, 0.5, EffectType::Chorus);
            let r = voice_r.tick(s, 0.5, 0.5, 0.0, 0.5, EffectType::Chorus);
            diff_sum += (l - r).abs();
        }
        assert!(
            diff_sum / 4800.0 > 0.001,
            "Stereo voices should differ: avg diff = {}",
            diff_sum / 4800.0
        );
    }

    #[test]
    fn engines_sound_different() {
        let mut cubic = CubicVoice::new(0.0);
        let mut bbd = BbdVoice::new(0.0);
        let mut tape = TapeVoice::new(0.0);
        let mut orbit = OrbitVoice::new(0.0);
        cubic.update(SR);
        bbd.update(SR);
        tape.update(SR);
        orbit.update(SR);
        let mut out_cubic = Vec::new();
        let mut out_bbd = Vec::new();
        let mut out_tape = Vec::new();
        let mut out_orbit = Vec::new();
        for i in 0..9600 {
            let s = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            out_cubic.push(cubic.tick(s, 1.0, 0.5, 0.0, 0.5, EffectType::Chorus));
            out_bbd.push(bbd.tick(s, 1.0, 0.5, 0.0, 0.5, EffectType::Chorus));
            out_tape.push(tape.tick(s, 1.0, 0.5, 0.0, 0.5, EffectType::Chorus));
            out_orbit.push(orbit.tick(s, 1.0, 0.5, 0.0, 0.5, EffectType::Chorus));
        }
        let diff_cb: f64 = out_cubic
            .iter()
            .zip(out_bbd.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / 9600.0;
        let diff_ct: f64 = out_cubic
            .iter()
            .zip(out_tape.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / 9600.0;
        let diff_co: f64 = out_cubic
            .iter()
            .zip(out_orbit.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / 9600.0;
        assert!(diff_cb > 0.001, "Cubic vs BBD should differ: {diff_cb}");
        assert!(diff_ct > 0.001, "Cubic vs Tape should differ: {diff_ct}");
        assert!(diff_co > 0.001, "Cubic vs Orbit should differ: {diff_co}");
    }

    #[test]
    fn color_affects_bbd_tone() {
        let mut dark = BbdVoice::new(0.0);
        let mut bright = BbdVoice::new(0.0);
        dark.update(SR);
        bright.update(SR);
        let mut energy_dark: f64 = 0.0;
        let mut energy_bright: f64 = 0.0;
        for i in 0..9600 {
            let s = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            let d = dark.tick(s, 1.0, 0.5, 0.0, 0.0, EffectType::Chorus);
            let b = bright.tick(s, 1.0, 0.5, 0.0, 1.0, EffectType::Chorus);
            energy_dark += d * d;
            energy_bright += b * b;
        }
        assert!(
            energy_bright > energy_dark * 0.8,
            "Bright color should pass more: dark={energy_dark:.4}, bright={energy_bright:.4}"
        );
    }
}
