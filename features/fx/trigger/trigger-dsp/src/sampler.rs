//! Sample playback engine with velocity layers and round-robin.
//!
//! Plays back pre-loaded audio samples in response to trigger events.
//! Supports:
//! - Velocity-layered sample selection (different samples for different hit strengths)
//! - Round-robin within each velocity layer to avoid machine-gun effect
//! - Polyphonic playback (overlapping triggers)
//! - Per-sample gain and pitch (via playback rate)

// r[impl trigger.sampler.round-robin]
// r[impl trigger.sampler.velocity-layers]
// r[impl trigger.sampler.mix]

/// Maximum simultaneous voices (polyphony limit).
const MAX_VOICES: usize = 64;

/// A single loaded audio sample (mono or stereo, stored as stereo).
#[derive(Clone)]
pub struct Sample {
    /// Stereo sample data: [left, right] pairs.
    pub data: Vec<[f64; 2]>,
    /// Original sample rate of the loaded audio.
    pub sample_rate: f64,
    /// Gain applied to this sample (linear).
    pub gain: f64,
    /// Playback rate multiplier (1.0 = original pitch).
    pub playback_rate: f64,
}

impl Sample {
    pub fn new(data: Vec<[f64; 2]>, sample_rate: f64) -> Self {
        Self {
            data,
            sample_rate,
            gain: 1.0,
            playback_rate: 1.0,
        }
    }

    /// Create a mono sample from a single channel.
    pub fn from_mono(data: &[f64], sample_rate: f64) -> Self {
        Self {
            data: data.iter().map(|&s| [s, s]).collect(),
            sample_rate,
            gain: 1.0,
            playback_rate: 1.0,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// A velocity layer: one or more samples assigned to a velocity range.
pub struct VelocityLayer {
    /// Minimum velocity for this layer (0.0-1.0, inclusive).
    pub min_velocity: f64,
    /// Maximum velocity for this layer (0.0-1.0, inclusive).
    pub max_velocity: f64,
    /// Samples available for round-robin in this layer.
    pub samples: Vec<Sample>,
    /// Current round-robin index.
    rr_index: usize,
}

impl VelocityLayer {
    pub fn new(min_velocity: f64, max_velocity: f64) -> Self {
        Self {
            min_velocity,
            max_velocity,
            samples: Vec::new(),
            rr_index: 0,
        }
    }

    /// Add a sample to this layer's round-robin pool.
    pub fn add_sample(&mut self, sample: Sample) {
        self.samples.push(sample);
    }

    /// Select the next sample via round-robin. Returns None if no samples.
    fn next_sample(&mut self) -> Option<&Sample> {
        if self.samples.is_empty() {
            return None;
        }
        let sample = &self.samples[self.rr_index];
        self.rr_index = (self.rr_index + 1) % self.samples.len();
        Some(sample)
    }

    /// Check if a velocity falls within this layer's range.
    fn matches(&self, velocity: f64) -> bool {
        velocity >= self.min_velocity && velocity <= self.max_velocity
    }
}

/// Mix mode for combining triggered samples with the original signal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MixMode {
    /// Replace the original signal entirely with the triggered sample.
    Replace,
    /// Layer the sample on top of the original (additive).
    Layer,
    /// Blend between original and sample using the mix amount.
    Blend,
}

/// An active voice (currently playing sample).
struct Voice {
    /// Sample data reference (cloned from the layer).
    data: Vec<[f64; 2]>,
    /// Current playback position (fractional for pitch shifting).
    position: f64,
    /// Playback rate (includes sample rate conversion + pitch).
    rate: f64,
    /// Gain for this voice (velocity-scaled * sample gain).
    gain: f64,
    /// Whether this voice is still playing.
    active: bool,
}

impl Voice {
    /// Render the next sample from this voice.
    #[inline]
    fn tick(&mut self) -> [f64; 2] {
        if !self.active {
            return [0.0; 2];
        }

        let idx = self.position as usize;
        if idx >= self.data.len() {
            self.active = false;
            return [0.0; 2];
        }

        // Linear interpolation for fractional positions
        let frac = self.position - idx as f64;
        let s0 = self.data[idx];
        let s1 = if idx + 1 < self.data.len() {
            self.data[idx + 1]
        } else {
            [0.0; 2]
        };

        let left = (s0[0] + (s1[0] - s0[0]) * frac) * self.gain;
        let right = (s0[1] + (s1[1] - s0[1]) * frac) * self.gain;

        self.position += self.rate;

        [left, right]
    }
}

/// Sample playback engine.
///
/// Manages velocity layers, round-robin selection, and polyphonic playback.
pub struct Sampler {
    /// Velocity layers, sorted by min_velocity.
    pub layers: Vec<VelocityLayer>,
    /// Active playback voices.
    voices: Vec<Voice>,
    /// Mix mode.
    pub mix_mode: MixMode,
    /// Mix amount for Blend mode (0.0 = all original, 1.0 = all sample).
    pub mix_amount: f64,
    /// Master output gain (linear).
    pub output_gain: f64,

    sample_rate: f64,
}

impl Sampler {
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            voices: Vec::with_capacity(MAX_VOICES),
            mix_mode: MixMode::Replace,
            mix_amount: 1.0,
            output_gain: 1.0,
            sample_rate: 48000.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    /// Add a velocity layer.
    pub fn add_layer(&mut self, layer: VelocityLayer) {
        self.layers.push(layer);
        // Keep sorted by min_velocity for efficient lookup
        self.layers
            .sort_by(|a, b| a.min_velocity.partial_cmp(&b.min_velocity).unwrap());
    }

    /// Add a single sample covering the full velocity range.
    pub fn set_single_sample(&mut self, sample: Sample) {
        self.layers.clear();
        let mut layer = VelocityLayer::new(0.0, 1.0);
        layer.add_sample(sample);
        self.layers.push(layer);
    }

    /// Trigger a sample at the given velocity.
    ///
    /// Finds the appropriate velocity layer, selects the next round-robin
    /// sample, and starts a new voice.
    pub fn trigger(&mut self, velocity: f64) {
        // Find matching layer
        let layer = self.layers.iter_mut().find(|l| l.matches(velocity));

        let layer = match layer {
            Some(l) => l,
            None => {
                // No matching layer — find nearest
                if self.layers.is_empty() {
                    return;
                }
                // Find layer with closest velocity range
                let idx = self
                    .layers
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        let dist_a = (a.min_velocity - velocity)
                            .abs()
                            .min((a.max_velocity - velocity).abs());
                        let dist_b = (b.min_velocity - velocity)
                            .abs()
                            .min((b.max_velocity - velocity).abs());
                        dist_a.partial_cmp(&dist_b).unwrap()
                    })
                    .map(|(i, _)| i)
                    .unwrap();
                &mut self.layers[idx]
            }
        };

        let sample = match layer.next_sample() {
            Some(s) => s,
            None => return,
        };

        // Calculate playback rate for sample rate conversion + pitch
        let rate = (sample.sample_rate / self.sample_rate) * sample.playback_rate;
        let gain = velocity * sample.gain * self.output_gain;

        // Reuse an inactive voice slot or add a new one
        let voice = Voice {
            data: sample.data.clone(),
            position: 0.0,
            rate,
            gain,
            active: true,
        };

        if let Some(slot) = self.voices.iter_mut().find(|v| !v.active) {
            *slot = voice;
        } else if self.voices.len() < MAX_VOICES {
            self.voices.push(voice);
        } else {
            // Voice stealing: replace the oldest voice
            self.voices[0] = voice;
        }
    }

    /// Process one stereo sample pair.
    ///
    /// Mixes all active voices and applies the mix mode against the
    /// original (dry) signal.
    #[inline]
    pub fn tick(&mut self, dry_left: f64, dry_right: f64) -> (f64, f64) {
        // Sum all active voices
        let mut sample_l = 0.0;
        let mut sample_r = 0.0;

        for voice in &mut self.voices {
            if voice.active {
                let [l, r] = voice.tick();
                sample_l += l;
                sample_r += r;
            }
        }

        // Apply mix mode
        match self.mix_mode {
            MixMode::Replace => (sample_l, sample_r),
            MixMode::Layer => (dry_left + sample_l, dry_right + sample_r),
            MixMode::Blend => {
                let wet = self.mix_amount;
                let dry = 1.0 - wet;
                (
                    dry_left * dry + sample_l * wet,
                    dry_right * dry + sample_r * wet,
                )
            }
        }
    }

    /// Check if any voices are currently playing.
    pub fn is_playing(&self) -> bool {
        self.voices.iter().any(|v| v.active)
    }

    /// Get the number of active voices.
    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }

    pub fn reset(&mut self) {
        self.voices.clear();
    }
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}
