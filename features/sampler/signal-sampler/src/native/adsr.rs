//! ADSR envelope generator — the per-voice amplitude/filter envelope.
//!
//! Linear attack, analog-style exponential decay/release (one-pole toward the
//! target). Per-sample [`tick`](Adsr::tick); no allocation. Used inside voice
//! structs today (the oscillator's amp envelope); becomes the `Envelope` control
//! block once the mod-matrix lands (roadmap §2).

/// Envelope times/levels. Times in seconds, sustain in `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdsrParams {
    pub attack_s: f32,
    pub decay_s: f32,
    pub sustain: f32,
    pub release_s: f32,
}

impl Default for AdsrParams {
    fn default() -> Self {
        // A generic keys-friendly shape: fast attack, musical tail.
        Self {
            attack_s: 0.003,
            decay_s: 0.25,
            sustain: 0.8,
            release_s: 0.15,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Envelope level below which a released voice counts as finished (~-80 dB).
const SILENCE: f32 = 1e-4;

/// A per-voice ADSR. Create once, [`note_on`](Self::note_on) to (re)trigger,
/// [`tick`](Self::tick) per sample, drop the voice when [`is_idle`](Self::is_idle).
#[derive(Clone, Copy, Debug)]
pub struct Adsr {
    params: AdsrParams,
    stage: Stage,
    level: f32,
    /// Per-sample attack increment (linear).
    attack_inc: f32,
    /// One-pole coefficients for decay/release (level → target each sample).
    decay_coeff: f32,
    release_coeff: f32,
}

impl Adsr {
    pub fn new(sample_rate: f32, params: AdsrParams) -> Self {
        let mut env = Self {
            params,
            stage: Stage::Idle,
            level: 0.0,
            attack_inc: 0.0,
            decay_coeff: 0.0,
            release_coeff: 0.0,
        };
        env.set_sample_rate(sample_rate);
        env
    }

    /// Replace the ADSR parameters live — coefficients recompute, the
    /// current stage and level survive (a held note keeps sounding and
    /// takes the new times from here on).
    pub fn set_params(&mut self, sample_rate: f32, params: AdsrParams) {
        self.params = params;
        self.set_sample_rate(sample_rate);
    }

    /// Recompute per-sample coefficients for a new rate (voices survive).
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        let sr = sample_rate.max(1.0);
        self.attack_inc = 1.0 / (self.params.attack_s.max(1e-4) * sr);
        self.decay_coeff = (-1.0 / (self.params.decay_s.max(1e-4) * sr)).exp();
        self.release_coeff = (-1.0 / (self.params.release_s.max(1e-4) * sr)).exp();
    }

    /// (Re)trigger from the current level — no click on retrigger.
    pub fn note_on(&mut self) {
        self.stage = Stage::Attack;
    }

    /// Enter the release tail from wherever the envelope is.
    pub fn note_off(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    /// Hard stop (panic / deactivate).
    pub fn reset(&mut self) {
        self.stage = Stage::Idle;
        self.level = 0.0;
    }

    pub fn is_idle(&self) -> bool {
        self.stage == Stage::Idle
    }

    /// Advance one sample; returns the envelope level in `[0, 1]`.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        match self.stage {
            Stage::Idle => 0.0,
            Stage::Attack => {
                self.level += self.attack_inc;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
                self.level
            }
            Stage::Decay => {
                let sustain = self.params.sustain;
                self.level = sustain + (self.level - sustain) * self.decay_coeff;
                if (self.level - sustain).abs() < SILENCE {
                    self.level = sustain;
                    self.stage = Stage::Sustain;
                }
                self.level
            }
            Stage::Sustain => self.level,
            Stage::Release => {
                self.level *= self.release_coeff;
                if self.level < SILENCE {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
                self.level
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_cycle_reaches_peak_sustain_and_silence() {
        let sr = 48_000.0;
        let mut env = Adsr::new(sr, AdsrParams::default());
        env.note_on();
        // Through the attack (3 ms) to peak.
        let mut peak = 0.0f32;
        for _ in 0..(sr * 0.01) as usize {
            peak = peak.max(env.tick());
        }
        assert!(peak >= 1.0, "attack reaches full level, peak={peak}");
        // Settle into sustain.
        let mut level = 0.0;
        for _ in 0..(sr * 2.0) as usize {
            level = env.tick();
        }
        assert!((level - 0.8).abs() < 0.01, "sustain ≈ 0.8, level={level}");
        // Release to idle.
        env.note_off();
        for _ in 0..(sr * 2.0) as usize {
            env.tick();
        }
        assert!(env.is_idle(), "release decays to idle");
    }

    #[test]
    fn retrigger_from_release_is_continuous() {
        let mut env = Adsr::new(48_000.0, AdsrParams::default());
        env.note_on();
        for _ in 0..4_800 {
            env.tick();
        }
        env.note_off();
        for _ in 0..1_000 {
            env.tick();
        }
        let before = env.tick();
        env.note_on();
        let after = env.tick();
        assert!(
            (after - before).abs() < 0.01,
            "retrigger continues from current level ({before} → {after})"
        );
    }
}
