//! Reverb algorithm trait and type enum.

/// All available reverb algorithm types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlgorithmType {
    Room,
    Hall,
    Plate,
    Spring,
    Cloud,
    Bloom,
    Shimmer,
    Chorale,
    Magneto,
    NonLinear,
    Swell,
    Reflections,
    Velvet,
    FreeVerb,
    Convolution,
}

impl AlgorithmType {
    pub const ALL: &'static [AlgorithmType] = &[
        Self::Room,
        Self::Hall,
        Self::Plate,
        Self::Spring,
        Self::Cloud,
        Self::Bloom,
        Self::Shimmer,
        Self::Chorale,
        Self::Magneto,
        Self::NonLinear,
        Self::Swell,
        Self::Reflections,
        Self::Velvet,
        Self::FreeVerb,
        Self::Convolution,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Room => "Room",
            Self::Hall => "Hall",
            Self::Plate => "Plate",
            Self::Spring => "Spring",
            Self::Cloud => "Cloud",
            Self::Bloom => "Bloom",
            Self::Shimmer => "Shimmer",
            Self::Chorale => "Chorale",
            Self::Magneto => "Magneto",
            Self::NonLinear => "Non-Linear",
            Self::Swell => "Swell",
            Self::Reflections => "Reflections",
            Self::Velvet => "Velvet",
            Self::FreeVerb => "FreeVerb",
            Self::Convolution => "Convolution",
        }
    }

    /// Number of sub-type variants for this algorithm.
    pub fn variant_count(self) -> usize {
        match self {
            Self::Room => 3,   // Medium, Chamber, Studio
            Self::Hall => 3,   // Concert, Cathedral, Arena
            Self::Plate => 3,  // Dattorro, Lexicon 224, Progenitor
            Self::Spring => 2, // Classic, Vintage
            _ => 1,
        }
    }

    /// Name of a specific variant.
    pub fn variant_name(self, variant: usize) -> &'static str {
        match self {
            Self::Room => match variant {
                0 => "Medium",
                1 => "Chamber",
                2 => "Studio",
                _ => "Medium",
            },
            Self::Hall => match variant {
                0 => "Concert",
                1 => "Cathedral",
                2 => "Arena",
                _ => "Concert",
            },
            Self::Plate => match variant {
                0 => "Dattorro",
                1 => "Lexicon",
                2 => "Progenitor",
                _ => "Dattorro",
            },
            Self::Spring => match variant {
                0 => "Classic",
                1 => "Vintage",
                _ => "Classic",
            },
            _ => "Default",
        }
    }

    /// Maximum variant count across all algorithm types.
    pub fn max_variant_count() -> usize {
        Self::ALL
            .iter()
            .map(|a| a.variant_count())
            .max()
            .unwrap_or(1)
    }

    pub fn from_index(i: usize) -> Self {
        Self::ALL.get(i).copied().unwrap_or(Self::Room)
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&a| a == self).unwrap_or(0)
    }
}

/// Shared parameters that all algorithms receive.
#[derive(Debug, Clone, Copy)]
pub struct AlgorithmParams {
    /// Decay / RT60 control (0.0 = short, 1.0 = infinite).
    pub decay: f64,
    /// Room / space size (0.0 = small, 1.0 = massive).
    pub size: f64,
    /// Diffusion amount (0.0 = sparse, 1.0 = dense).
    pub diffusion: f64,
    /// High-frequency damping (0.0 = bright, 1.0 = dark).
    pub damping: f64,
    /// Modulation depth (0.0 = none, 1.0 = full).
    pub modulation: f64,
    /// Tone control (-1.0 = dark, 0.0 = neutral, 1.0 = bright).
    pub tone: f64,
    /// Extra parameter A (algorithm-specific).
    pub extra_a: f64,
    /// Extra parameter B (algorithm-specific).
    pub extra_b: f64,
    /// Low-band decay multiplier (0.0..2.0, 1.0 = neutral).
    /// Algorithms with frequency-dependent feedback split RT60 at
    /// `band_crossover_hz` and scale the low band by this. > 1.0 makes
    /// lows ring longer (warm halls). Default 1.0.
    pub low_decay_mult: f64,
    /// High-band decay multiplier (0.0..2.0, 1.0 = neutral).
    pub high_decay_mult: f64,
    /// Crossover frequency for the two decay bands.
    pub band_crossover_hz: f64,
}

impl Default for AlgorithmParams {
    fn default() -> Self {
        Self {
            decay: 0.5,
            size: 0.5,
            diffusion: 0.7,
            damping: 0.3,
            modulation: 0.2,
            tone: 0.0,
            extra_a: 0.5,
            extra_b: 0.5,
            low_decay_mult: 1.0,
            high_decay_mult: 1.0,
            band_crossover_hz: 700.0,
        }
    }
}

/// IR slot selector for dual-IR algorithms (Convolution's A/B morph).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IrSlot {
    #[default]
    A,
    B,
}

/// Convolution-specific modulation options. All depths default to 0,
/// which is bit-transparent against the unmodulated convolution.
///
/// Three independent option groups:
/// 1. **Motion** — post-convolution modulated-allpass stage
///    (`motion_depth`/`motion_rate`).
/// 2. **Mod sources** — a shared sine LFO (`lfo_rate`) on wet gain /
///    predelay / damping, plus an input envelope follower ducking the
///    wet (`duck_wet_depth`).
/// 3. **Dual-IR morph** — equal-power crossfade between IR slots A and B
///    (`morph`), optionally swept by the LFO (`morph_lfo_depth`).
#[derive(Debug, Clone, Copy)]
pub struct ConvolutionModParams {
    /// Motion stage depth (0..1). 0 = hard bypass (no CPU).
    pub motion_depth: f64,
    /// Motion LFO base rate in Hz (0.1..2).
    pub motion_rate: f64,
    /// Shared modulation LFO rate in Hz (0.05..5).
    pub lfo_rate: f64,
    /// LFO → wet gain depth (-1..1 maps to ∓/±6 dB swing).
    pub mod_wet_depth: f64,
    /// LFO → predelay depth (-1..1 maps to ±20 ms swing).
    pub mod_predelay_depth: f64,
    /// LFO → damping-cutoff depth (-1..1 maps to ±2 octaves).
    pub mod_damp_depth: f64,
    /// Input envelope → wet gain reduction (0..1).
    pub duck_wet_depth: f64,
    /// Base predelay before the convolver, in ms (0..200).
    pub predelay_ms: f64,
    /// IR A/B morph position (0 = A only, 1 = B only).
    pub morph: f64,
    /// LFO sweep depth added to `morph` (0..1).
    pub morph_lfo_depth: f64,
}

impl Default for ConvolutionModParams {
    fn default() -> Self {
        Self {
            motion_depth: 0.0,
            motion_rate: 0.5,
            lfo_rate: 0.5,
            mod_wet_depth: 0.0,
            mod_predelay_depth: 0.0,
            mod_damp_depth: 0.0,
            duck_wet_depth: 0.0,
            predelay_ms: 0.0,
            morph: 0.0,
            morph_lfo_depth: 0.0,
        }
    }
}

/// Common interface for all reverb algorithms.
///
/// Each algorithm processes one stereo sample pair at a time (tick-based),
/// returning the wet signal only. The ReverbChain handles pre-delay,
/// input filtering, mix, and width.
pub trait ReverbAlgorithm: Send {
    fn reset(&mut self);
    fn set_sample_rate(&mut self, sample_rate: f64);
    fn set_params(&mut self, params: &AlgorithmParams);
    /// Process one stereo sample, return (left_wet, right_wet).
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64);

    /// Replace this algorithm's impulse response. No-op for algorithms
    /// that don't support IRs. Returns `true` if the IR was accepted.
    /// Convolution overrides this.
    fn try_load_ir(&mut self, left: &[f64], right: &[f64]) -> bool {
        let _ = (left, right);
        false
    }

    /// Audio-thread-safe IR swap: accepts a [`crate::ir::PreparedIrPair`]
    /// whose FFTs were precomputed on a background thread. No-op
    /// outside Convolution. Returns `true` if the swap was accepted.
    fn try_load_prepared_ir(&mut self, pair: crate::ir::PreparedIrPair) -> bool {
        let _ = pair;
        false
    }

    /// Whether this algorithm accepts impulse responses via [`Self::try_load_ir`].
    fn supports_ir_loading(&self) -> bool {
        false
    }

    /// Slot-addressed IR load for dual-IR algorithms. Default: slot A
    /// falls through to [`Self::try_load_ir`], slot B is rejected.
    fn try_load_ir_slot(&mut self, left: &[f64], right: &[f64], slot: IrSlot) -> bool {
        match slot {
            IrSlot::A => self.try_load_ir(left, right),
            IrSlot::B => false,
        }
    }

    /// Slot-addressed prepared-IR swap. Default: slot A falls through to
    /// [`Self::try_load_prepared_ir`], slot B is rejected.
    fn try_load_prepared_ir_slot(
        &mut self,
        pair: crate::ir::PreparedIrPair,
        slot: IrSlot,
    ) -> bool {
        match slot {
            IrSlot::A => self.try_load_prepared_ir(pair),
            IrSlot::B => false,
        }
    }

    /// Push convolution modulation options. `snap` = land instantly
    /// (preset load) instead of ramping (automation). No-op outside
    /// Convolution; returns `true` if accepted.
    fn set_conv_mod_params(&mut self, params: &ConvolutionModParams, snap: bool) -> bool {
        let _ = (params, snap);
        false
    }
}
