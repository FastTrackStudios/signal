//! CleanDelay — the TimeLine MX "Digital" machine, four voices.
//!
//! One delay line, cubic interpolation, optional feedback EQ — and a
//! per-voice conversion codec in the write path so every regeneration
//! re-encodes (the hardware puts the converter inside the loop):
//!
//! - **24/96** — pristine, no coloration.
//! - **ADM** — early-'80s 1-bit adaptive delta modulation, modelled as
//!   a CVSD codec (run-of-4 coincidence rule, 5 ms syllabic filter,
//!   leaky integrator) at 8× per host sample: fast transients
//!   slope-overload then snap — the signature percussive repeat attack.
//! - **12-Bit** — NE570-style 2:1 compander around a linear 12-bit
//!   quantizer + noise floor (compress at the write head, expand at the
//!   read head — the expander's envelope-tracking noise breathing is
//!   the era sound) plus a gentle in-loop lowpass.
//! - **Classic** — the original TimeLine digital voice: rounder/fatter
//!   (soft one-pole rolloff + mild saturation), with the voice's
//!   morphing FILTER response (full bandwidth → analog → tape) driven
//!   by [`CleanDelay::filter_morph`].

use crate::modulation::CompanderEnv;
use crate::tilt::DecayTilt;
use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::smoothing::ParamSmoother;

/// Digital machine voicing (TimeLine MX `Voice`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DigitalVoice {
    /// Modern full-bandwidth conversion; clean and pure.
    #[default]
    TwentyFour96,
    /// 1-bit adaptive delta modulation — percussive attack emphasis.
    Adm,
    /// 12-bit companded conversion — darker, warmer.
    TwelveBit,
    /// Original TimeLine digital — rounder/fatter, morphing filter.
    Classic,
}

impl DigitalVoice {
    pub const COUNT: usize = 4;

    pub fn from_index(i: usize) -> Self {
        match i {
            1 => DigitalVoice::Adm,
            2 => DigitalVoice::TwelveBit,
            3 => DigitalVoice::Classic,
            _ => DigitalVoice::TwentyFour96,
        }
    }

    pub fn to_index(self) -> usize {
        match self {
            DigitalVoice::TwentyFour96 => 0,
            DigitalVoice::Adm => 1,
            DigitalVoice::TwelveBit => 2,
            DigitalVoice::Classic => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DigitalVoice::TwentyFour96 => "24/96",
            DigitalVoice::Adm => "ADM",
            DigitalVoice::TwelveBit => "12-Bit",
            DigitalVoice::Classic => "Classic",
        }
    }
}

/// Adaptive delta modulator — CVSD-calibrated (the early-'80s 1-bit
/// codec: MC3417/HC-55516 family). 1-bit encode/decode at `OVERSAMPLE`
/// iterations per host sample:
///
/// - **run-of-N coincidence rule**: the step only grows while the last
///   `HISTORY_BITS` output bits are all equal (slope overload);
///   otherwise the **syllabic filter** decays it toward the minimum
///   (τ ≈ 5 ms, the mil/Bluetooth spec value).
/// - **leaky ("principal") integrator**, τ ≈ 1 ms, reconstructs the
///   level.
///
/// Slew is program-dependent: fast attacks overload then snap — the
/// ADM "percussive" signature; quiet sustain rides the granular
/// min-step floor.
struct AdmCodec {
    level: f64,
    step: f64,
    history: u32,
    /// Per-clock-tick syllabic decay toward MIN_STEP / charge toward
    /// MAX_STEP (exp(-1/(τ_syllabic · f_clock))).
    syllabic: f64,
    /// Per-clock-tick principal-integrator leak.
    leak: f64,
}

impl AdmCodec {
    const OVERSAMPLE: usize = 8;
    const HISTORY_BITS: u32 = 4;
    const MIN_STEP: f64 = 2.0e-4;
    const MAX_STEP: f64 = 0.12;
    /// Syllabic time constant (spec: 5 ms ± 1) and principal
    /// integrator time constant (~1 ms).
    const SYLLABIC_S: f64 = 0.005;
    const PRINCIPAL_S: f64 = 0.001;

    fn new() -> Self {
        let mut c = Self {
            level: 0.0,
            step: Self::MIN_STEP,
            history: 0,
            syllabic: 0.999,
            leak: 0.997,
        };
        c.configure(48000.0);
        c
    }

    fn configure(&mut self, sample_rate: f64) {
        let clock = sample_rate * Self::OVERSAMPLE as f64;
        self.syllabic = (-1.0 / (Self::SYLLABIC_S * clock)).exp();
        self.leak = (-1.0 / (Self::PRINCIPAL_S * clock)).exp();
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let mask = (1u32 << Self::HISTORY_BITS) - 1;
        for _ in 0..Self::OVERSAMPLE {
            let bit = x > self.level;
            self.history = ((self.history << 1) | bit as u32) & mask;
            let run = self.history == 0 || self.history == mask;
            self.step = if run {
                // Charge toward MAX at the syllabic rate.
                Self::MAX_STEP + (self.step - Self::MAX_STEP) * self.syllabic
            } else {
                // Decay toward MIN at the syllabic rate.
                Self::MIN_STEP + (self.step - Self::MIN_STEP) * self.syllabic
            };
            self.level = self.level * self.leak + if bit { self.step } else { -self.step };
        }
        self.level
    }

    fn reset(&mut self) {
        self.level = 0.0;
        self.step = Self::MIN_STEP;
        self.history = 0;
    }
}

/// Linear 12-bit quantizer (the era's DAC was linear — the dynamics
/// came from the external NE570 compander around it).
#[inline]
fn quantize_12bit(x: f64) -> f64 {
    (x.clamp(-1.0, 1.0) * 2048.0).round() / 2048.0
}

/// Clean digital delay — the Digital machine's four voices.
pub struct CleanDelay {
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// High-cut filter frequency in Hz (0 = disabled).
    pub hicut_freq: f64,
    /// Low-cut filter frequency in Hz (0 = disabled).
    pub locut_freq: f64,
    /// Filter Q.
    pub filter_q: f64,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,
    /// Conversion voicing.
    pub voice: DigitalVoice,
    /// Classic voice's morphing FILTER position (0.0 = full bandwidth,
    /// 0.5 ≈ analog-delay response, 1.0 ≈ tape-delay response). Ignored
    /// by the other voices (they use `hicut_freq`).
    pub filter_morph: f64,
    /// Delay-line modulation LFO rate in Hz ("classic rack-style
    /// modulated delay").
    pub mod_rate_hz: f64,
    /// Delay-line modulation depth (0.0–1.0; full scale ≈ ±3 ms).
    pub mod_depth: f64,

    decay_tilt_eq: DecayTilt,
    delay: DelayLine,
    hicut: Biquad,
    locut: Biquad,
    /// Classic-morph filter stages (lowpass + low-cut contour).
    morph_lp: Biquad,
    morph_hp: Biquad,
    /// 12-Bit voice's in-loop darkening.
    twelve_lp: Biquad,
    /// 12-Bit voice: NE570-style 2:1 compander (compress at the write
    /// head, expand at the read head — envelope-modulated noise floor).
    comp_env: CompanderEnv,
    exp_env: CompanderEnv,
    noise: audiocore_dsp::prng::XorShift32,
    adm: AdmCodec,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    lfo_phase: f64,
}

impl Default for CleanDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl CleanDelay {
    const MAX_DELAY_S: f64 = 5.0;
    /// Full-depth modulation excursion in seconds (≈ ±3 ms).
    const MOD_RANGE_S: f64 = 0.003;

    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            hicut_freq: 0.0,
            locut_freq: 0.0,
            filter_q: 0.707,
            decay_tilt: 0.0,
            voice: DigitalVoice::TwentyFour96,
            filter_morph: 0.0,
            mod_rate_hz: 0.6,
            mod_depth: 0.0,
            decay_tilt_eq: DecayTilt::new(),
            delay: DelayLine::new(48000 * 5 + 1024),
            hicut: Biquad::new(),
            locut: Biquad::new(),
            morph_lp: Biquad::new(),
            morph_hp: Biquad::new(),
            twelve_lp: Biquad::new(),
            comp_env: CompanderEnv::new(),
            exp_env: CompanderEnv::new(),
            noise: audiocore_dsp::prng::XorShift32::new(0x0D16_17A1),
            adm: AdmCodec::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            lfo_phase: 0.0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

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

        // Classic morph: full bandwidth (20 kHz) → analog bucket-brigade
        // rolloff (~2.8 kHz at noon) → tape response (steeper, darker,
        // plus a low contour that thins the deep lows).
        let morph = self.filter_morph.clamp(0.0, 1.0);
        let lp_freq = 20000.0 * (2800.0f64 / 20000.0).powf(morph);
        self.morph_lp
            .set(FilterType::Lowpass, lp_freq.max(900.0), 0.707, sample_rate);
        let hp_freq = 20.0 + morph * 100.0;
        self.morph_hp
            .set(FilterType::Highpass, hp_freq, 0.707, sample_rate);

        self.twelve_lp
            .set(FilterType::Lowpass, 9500.0, 0.707, sample_rate);
        self.adm.configure(sample_rate);
        self.comp_env.configure(sample_rate);
        self.exp_env.configure(sample_rate);

        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        self.smoother
            .set_time_seeded(0.15, sample_rate, self.time_ms * 0.001 * sample_rate);
    }

    /// Per-voice conversion applied to everything entering the delay
    /// line (input + regeneration), so the character compounds with
    /// each repeat exactly like a codec inside the hardware loop.
    #[inline]
    fn encode(&mut self, x: f64, ch: usize) -> f64 {
        match self.voice {
            DigitalVoice::TwentyFour96 => x,
            DigitalVoice::Adm => self.adm.process(x),
            DigitalVoice::TwelveBit => {
                // Compress (feedback 2:1) → 12-bit linear quantize with
                // a ~1.5-LSB noise floor → store. The expander at the
                // read head turns that fixed floor into the era's
                // signal-tracking breathing.
                let compressed = self.comp_env.compress(x);
                let noisy = compressed + self.noise.next_bipolar() * (1.5 / 2048.0);
                self.twelve_lp.tick(quantize_12bit(noisy), ch)
            }
            DigitalVoice::Classic => {
                // Rounder/fatter: gentle saturation + the morph filter.
                let softened = (x * 1.1).tanh() / 1.1f64.tanh();
                let lp = self.morph_lp.tick(softened, ch);
                if self.filter_morph > 0.55 {
                    // Tape territory: engage the low contour.
                    self.morph_hp.tick(lp, ch)
                } else {
                    lp
                }
            }
        }
    }

    /// Read-head decode: the 12-Bit voice expands (2:1) what the write
    /// head compressed; other voices pass through.
    #[inline]
    fn decode(&mut self, x: f64) -> f64 {
        match self.voice {
            DigitalVoice::TwelveBit => self.exp_env.expand(x),
            _ => x,
        }
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        // Rack-style delay-line modulation.
        let mod_off = if self.mod_depth > 0.0 {
            self.lfo_phase += self.mod_rate_hz / self.sample_rate;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
            }
            (self.lfo_phase * core::f64::consts::TAU).sin()
                * self.mod_depth
                * Self::MOD_RANGE_S
                * self.sample_rate
        } else {
            0.0
        };

        let max_read = self.delay.len() as f64 - 4.0;
        let read_pos = (smooth_delay + mod_off).clamp(1.0, max_read);
        let output = self.decode(self.delay.read_cubic(read_pos));

        // Feedback path: output → filter → limit
        let mut fb = output * self.feedback;

        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }
        if self.locut_freq > 0.0 {
            fb = self.locut.tick(fb, ch);
        }

        fb = self.decay_tilt_eq.tick(fb, ch);

        fb = fb.clamp(-1.5, 1.5);

        let encoded = self.encode(input + fb, ch);
        self.delay.write(encoded);
        self.feedback_sample = fb;

        output
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.hicut.reset();
        self.locut.reset();
        self.morph_lp.reset();
        self.morph_hp.reset();
        self.twelve_lp.reset();
        self.comp_env.reset();
        self.exp_env.reset();
        self.adm.reset();
        self.decay_tilt_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.lfo_phase = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn silence_in_silence_out() {
        let mut d = CleanDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.update(SR);

        for _ in 0..48000 {
            let out = d.tick(0.0, 0);
            assert!(out.abs() < 1e-10);
        }
    }

    #[test]
    fn impulse_delayed() {
        let mut d = CleanDelay::new();
        d.time_ms = 100.0; // 4800 samples
        d.feedback = 0.0;
        d.update(SR);

        let mut peak_idx = 0;
        let mut peak_val = 0.0f64;
        for i in 0..9600 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let out = d.tick(input, 0);
            if out.abs() > peak_val {
                peak_val = out.abs();
                peak_idx = i;
            }
        }

        assert!(
            (peak_idx as i64 - 4800).abs() < 10,
            "Peak at {peak_idx}, expected ~4800"
        );
        assert!(peak_val > 0.9, "Peak {peak_val} should be near unity");
    }

    /// Run a repeat through a given voice and return (output, index range).
    fn run_voice(voice: DigitalVoice, feedback: f64, n: usize) -> Vec<f64> {
        let mut d = CleanDelay::new();
        d.time_ms = 50.0;
        d.feedback = feedback;
        d.voice = voice;
        d.update(SR);
        (0..n)
            .map(|i| {
                let input = if i < 480 {
                    (core::f64::consts::TAU * 330.0 * i as f64 / SR).sin() * 0.7
                } else {
                    0.0
                };
                d.tick(input, 0)
            })
            .collect()
    }

    #[test]
    fn voices_are_distinct() {
        let n = 24000;
        let clean = run_voice(DigitalVoice::TwentyFour96, 0.5, n);
        let ref_energy: f64 = clean.iter().map(|x| x * x).sum();
        for voice in [
            DigitalVoice::Adm,
            DigitalVoice::TwelveBit,
            DigitalVoice::Classic,
        ] {
            let colored = run_voice(voice, 0.5, n);
            let diff: f64 = clean
                .iter()
                .zip(&colored)
                .map(|(a, b)| (a - b) * (a - b))
                .sum();
            assert!(
                diff > ref_energy * 1e-4,
                "{voice:?} should color the repeats: diff {diff} vs ref {ref_energy}"
            );
        }
    }

    #[test]
    fn adm_error_grows_with_frequency() {
        // ADM's delta coding tracks slow signals well and slews on fast
        // ones — the error (vs the clean voice) must grow with input
        // frequency. This is the mechanism behind the attack emphasis.
        let err_at = |freq: f64| -> f64 {
            let run = |voice: DigitalVoice| -> Vec<f64> {
                let mut d = CleanDelay::new();
                d.time_ms = 20.0;
                d.feedback = 0.0;
                d.voice = voice;
                d.update(SR);
                (0..9600)
                    .map(|i| {
                        let input = (core::f64::consts::TAU * freq * i as f64 / SR).sin() * 0.7;
                        d.tick(input, 0)
                    })
                    .collect()
            };
            let clean = run(DigitalVoice::TwentyFour96);
            let adm = run(DigitalVoice::Adm);
            clean
                .iter()
                .zip(&adm)
                .skip(2000)
                .map(|(a, b)| (a - b) * (a - b))
                .sum()
        };
        let low = err_at(110.0);
        let high = err_at(3520.0);
        assert!(
            high > low * 2.0,
            "ADM error should rise with frequency: low {low} high {high}"
        );
    }

    #[test]
    fn twelve_bit_is_darker() {
        // Broadband content through the 12-Bit loop loses more HF than
        // the clean voice.
        let hf_energy = |voice: DigitalVoice| -> f64 {
            let out = run_voice_noise(voice);
            // crude HF meter: first-difference energy
            out.windows(2).map(|w| (w[1] - w[0]) * (w[1] - w[0])).sum()
        };
        fn run_voice_noise(voice: DigitalVoice) -> Vec<f64> {
            let mut d = CleanDelay::new();
            d.time_ms = 40.0;
            d.feedback = 0.6;
            d.voice = voice;
            d.update(SR);
            let mut seed = 0x2545f491u32;
            (0..48000)
                .map(|i| {
                    seed = seed.wrapping_mul(747796405).wrapping_add(2891336453);
                    let noise = (seed >> 9) as f64 / (1u32 << 23) as f64 - 1.0;
                    let input = if i < 4800 { noise * 0.5 } else { 0.0 };
                    d.tick(input, 0)
                })
                .skip(6000)
                .collect()
        }
        let clean_hf = hf_energy(DigitalVoice::TwentyFour96);
        let twelve_hf = hf_energy(DigitalVoice::TwelveBit);
        assert!(
            twelve_hf < clean_hf,
            "12-Bit repeats should be darker: {twelve_hf} vs {clean_hf}"
        );
    }

    #[test]
    fn classic_morph_darkens_progressively() {
        let hf = |morph: f64| -> f64 {
            let mut d = CleanDelay::new();
            d.time_ms = 40.0;
            d.feedback = 0.6;
            d.voice = DigitalVoice::Classic;
            d.filter_morph = morph;
            d.update(SR);
            let mut seed = 0x1234567u32;
            let out: Vec<f64> = (0..48000)
                .map(|i| {
                    seed = seed.wrapping_mul(747796405).wrapping_add(2891336453);
                    let noise = (seed >> 9) as f64 / (1u32 << 23) as f64 - 1.0;
                    let input = if i < 4800 { noise * 0.5 } else { 0.0 };
                    d.tick(input, 0)
                })
                .skip(6000)
                .collect();
            out.windows(2).map(|w| (w[1] - w[0]) * (w[1] - w[0])).sum()
        };
        let open = hf(0.0);
        let analog = hf(0.5);
        let tape = hf(1.0);
        assert!(
            open > analog && analog > tape,
            "Classic FILTER should darken across the morph: {open} > {analog} > {tape}"
        );
    }

    #[test]
    fn modulation_moves_the_read() {
        let run = |depth: f64| -> Vec<f64> {
            let mut d = CleanDelay::new();
            d.time_ms = 120.0;
            d.feedback = 0.0;
            d.mod_depth = depth;
            d.mod_rate_hz = 2.0;
            d.update(SR);
            (0..48000)
                .map(|i| {
                    let input = (core::f64::consts::TAU * 220.0 * i as f64 / SR).sin() * 0.5;
                    d.tick(input, 0)
                })
                .collect()
        };
        let still = run(0.0);
        let wobbled = run(1.0);
        let ref_energy: f64 = still.iter().map(|x| x * x).sum();
        let diff: f64 = still
            .iter()
            .zip(&wobbled)
            .map(|(a, b)| (a - b) * (a - b))
            .sum();
        assert!(
            diff > ref_energy * 0.05,
            "modulation should move the read head: {diff} vs {ref_energy}"
        );
    }

    #[test]
    fn no_nan_all_voices() {
        for vi in 0..DigitalVoice::COUNT {
            let mut d = CleanDelay::new();
            d.time_ms = 200.0;
            d.feedback = 0.7;
            d.hicut_freq = 5000.0;
            d.locut_freq = 100.0;
            d.decay_tilt = -0.5;
            d.voice = DigitalVoice::from_index(vi);
            d.filter_morph = 0.8;
            d.mod_depth = 0.5;
            d.update(SR);

            for i in 0..96000 {
                let input = (core::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
                let out = d.tick(input, 0);
                assert!(out.is_finite(), "NaN at sample {i} voice {vi}");
                assert!(out.abs() < 10.0, "Runaway at {i} voice {vi}: {out}");
            }
        }
    }
}
