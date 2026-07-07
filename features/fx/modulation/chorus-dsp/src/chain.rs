//! ChorusChain — multi-engine stereo chorus processor.
//!
//! Supports 4 engine types (Cubic, BBD, Tape, Orbit) with
//! 1–4 voices per channel. Implements the Processor trait.

use audiocore_dsp::{AudioConfig, Processor};

use crate::engine::{create_voices, ChorusEngine, EffectType, EngineType};

/// Maximum number of voices per channel.
const MAX_VOICES: usize = 4;

/// Complete stereo chorus/flanger/vibrato processor.
pub struct ChorusChain {
    voices_l: Vec<Box<dyn ChorusEngine>>,
    voices_r: Vec<Box<dyn ChorusEngine>>,

    /// Number of active voices per channel (1–4).
    pub num_voices: usize,
    /// LFO rate in Hz.
    pub rate_hz: f64,
    /// Modulation depth (0..1).
    pub depth: f64,
    /// Feedback amount (0..1). Mainly for flanger.
    pub feedback: f64,
    /// Color/tone parameter (0..1). Engine-specific meaning.
    pub color: f64,
    /// Engine type.
    pub engine: EngineType,
    /// Effect type (chorus/flanger/vibrato).
    pub effect_type: EffectType,
    /// Dry/wet mix (0..1). For vibrato, set to 1.0 (wet only).
    pub mix: f64,
    /// Stereo width (0..1). 0 = mono, 1 = full stereo spread.
    pub width: f64,
}

impl ChorusChain {
    pub fn new() -> Self {
        let engine = EngineType::Cubic;
        Self {
            voices_l: create_voices(engine, MAX_VOICES),
            voices_r: create_voices_stereo(engine, MAX_VOICES),
            num_voices: 2,
            rate_hz: 1.0,
            depth: 0.5,
            feedback: 0.0,
            color: 0.5,
            engine,
            effect_type: EffectType::Chorus,
            mix: 0.5,
            width: 1.0,
        }
    }

    /// Switch the chorus engine. Recreates all voices.
    pub fn set_engine(&mut self, engine: EngineType) {
        if self.engine != engine {
            self.engine = engine;
            self.voices_l = create_voices(engine, MAX_VOICES);
            self.voices_r = create_voices_stereo(engine, MAX_VOICES);
        }
    }
}

/// Create right-channel voices with stereo phase offset.
fn create_voices_stereo(engine: EngineType, count: usize) -> Vec<Box<dyn ChorusEngine>> {
    use crate::engine::*;
    (0..count)
        .map(|i| {
            let offset = i as f64 / count as f64 + 0.25; // +90° for stereo
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

impl Default for ChorusChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for ChorusChain {
    fn reset(&mut self) {
        for v in &mut self.voices_l {
            v.reset();
        }
        for v in &mut self.voices_r {
            v.reset();
        }
    }

    fn update(&mut self, config: AudioConfig) {
        for v in &mut self.voices_l {
            v.update(config.sample_rate);
        }
        for v in &mut self.voices_r {
            v.update(config.sample_rate);
        }
    }

    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        let n = self.num_voices.clamp(1, MAX_VOICES);
        let inv_n = 1.0 / n as f64;

        for i in 0..left.len().min(right.len()) {
            let dry_l = left[i];
            let dry_r = right[i];

            let mut wet_l: f64 = 0.0;
            let mut wet_r: f64 = 0.0;

            for v in 0..n {
                wet_l += self.voices_l[v].tick(
                    left[i],
                    self.rate_hz,
                    self.depth,
                    self.feedback,
                    self.color,
                    self.effect_type,
                );
                wet_r += self.voices_r[v].tick(
                    right[i],
                    self.rate_hz,
                    self.depth,
                    self.feedback,
                    self.color,
                    self.effect_type,
                );
            }

            wet_l *= inv_n;
            wet_r *= inv_n;

            // Stereo width
            let mono_wet = (wet_l + wet_r) * 0.5;
            wet_l = mono_wet + (wet_l - mono_wet) * self.width;
            wet_r = mono_wet + (wet_r - mono_wet) * self.width;

            // Vibrato: wet only
            if self.effect_type == EffectType::Vibrato {
                left[i] = wet_l;
                right[i] = wet_r;
            } else {
                left[i] = dry_l * (1.0 - self.mix) + wet_l * self.mix;
                right[i] = dry_r * (1.0 - self.mix) + wet_r * self.mix;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn config() -> AudioConfig {
        AudioConfig {
            sample_rate: SR,
            max_buffer_size: 512,
        }
    }

    #[test]
    fn silence_in_silence_out() {
        let mut c = ChorusChain::new();
        c.update(config());

        let mut l = vec![0.0; 4800];
        let mut r = vec![0.0; 4800];
        c.process(&mut l, &mut r);

        for (i, &s) in l.iter().enumerate() {
            assert!(s.abs() < 1e-10, "Non-zero at {i}: {s}");
        }
    }

    #[test]
    fn zero_mix_passes_through() {
        let mut c = ChorusChain::new();
        c.mix = 0.0;
        c.update(config());

        let input: Vec<f64> = (0..4800)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut l = input.clone();
        let mut r = input.clone();

        c.process(&mut l, &mut r);

        for (i, (&out, &inp)) in l.iter().zip(input.iter()).enumerate() {
            assert!(
                (out - inp).abs() < 1e-10,
                "Zero mix should pass through at {i}: {out} vs {inp}"
            );
        }
    }

    #[test]
    fn all_engines_no_nan() {
        for engine in &[
            EngineType::Cubic,
            EngineType::Bbd,
            EngineType::Tape,
            EngineType::Orbit,
        ] {
            let mut c = ChorusChain::new();
            c.set_engine(*engine);
            c.depth = 1.0;
            c.rate_hz = 2.0;
            c.feedback = 0.7;
            c.num_voices = 2;
            c.update(config());

            let mut l: Vec<f64> = (0..48000)
                .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
                .collect();
            let mut r = l.clone();

            c.process(&mut l, &mut r);

            for (i, &s) in l.iter().enumerate() {
                assert!(s.is_finite(), "NaN in {:?} at {i}", engine);
            }
        }
    }

    #[test]
    fn all_engines_all_effects_no_nan() {
        for engine in &[
            EngineType::Cubic,
            EngineType::Bbd,
            EngineType::Tape,
            EngineType::Orbit,
        ] {
            for effect in &[EffectType::Chorus, EffectType::Flanger, EffectType::Vibrato] {
                let mut c = ChorusChain::new();
                c.set_engine(*engine);
                c.effect_type = *effect;
                c.depth = 1.0;
                c.rate_hz = 3.0;
                c.feedback = 0.8;
                c.num_voices = 4;
                c.update(config());

                let mut l: Vec<f64> = (0..24000)
                    .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
                    .collect();
                let mut r = l.clone();

                c.process(&mut l, &mut r);

                for (i, &s) in l.iter().enumerate() {
                    assert!(s.is_finite(), "NaN in {:?}/{:?} at {i}", engine, effect);
                }
            }
        }
    }

    #[test]
    fn engine_switch_works() {
        let mut c = ChorusChain::new();
        c.update(config());

        // Process some with cubic
        let mut l = vec![0.5; 480];
        let mut r = vec![0.5; 480];
        c.process(&mut l, &mut r);

        // Switch to BBD
        c.set_engine(EngineType::Bbd);
        c.update(config());

        let mut l = vec![0.5; 480];
        let mut r = vec![0.5; 480];
        c.process(&mut l, &mut r);

        // Should not crash
    }

    #[test]
    fn different_engines_produce_different_output() {
        let input: Vec<f64> = (0..9600)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
            .collect();

        let mut outputs = Vec::new();

        for engine in &[
            EngineType::Cubic,
            EngineType::Bbd,
            EngineType::Tape,
            EngineType::Orbit,
        ] {
            let mut c = ChorusChain::new();
            c.set_engine(*engine);
            c.depth = 0.5;
            c.rate_hz = 1.0;
            c.mix = 1.0;
            c.num_voices = 1;
            c.update(config());

            let mut l = input.clone();
            let mut r = input.clone();
            c.process(&mut l, &mut r);
            outputs.push(l);
        }

        // Each pair should differ
        for i in 0..outputs.len() {
            for j in (i + 1)..outputs.len() {
                let diff: f64 = outputs[i]
                    .iter()
                    .zip(outputs[j].iter())
                    .map(|(a, b)| (a - b).abs())
                    .sum::<f64>()
                    / 9600.0;
                assert!(diff > 0.001, "Engines {i} and {j} should differ: {diff}");
            }
        }
    }
}
