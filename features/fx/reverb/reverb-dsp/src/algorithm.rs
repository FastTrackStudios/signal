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

    /// BigSky MX named Size options for this engine (see
    /// [`crate::chain::ReverbChain::set_size_index`]). Hall/Room sizes
    /// map onto the variant system; everything else steps
    /// `params.size`.
    pub fn size_names(self) -> &'static [&'static str] {
        match self {
            Self::Hall => &["Concert", "Arena"],
            Self::Room => &["Studio", "Club"],
            _ => &["Small", "Medium", "Large"],
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

/// Tail shaping mode for the Impulse engine's Decay control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImpulseTail {
    /// Decreasing ramp shortens the IR per the decay setting.
    #[default]
    Envelope,
    /// Abrupt truncation at the decay point.
    Gate,
}

/// Playback direction for the Impulse engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImpulseDirection {
    #[default]
    Forward,
    /// Backward reverb decay (riser) following the input.
    Reverse,
}

/// BigSky MX "Impulse" engine live shaping parameters.
///
/// `decay`/`tail`/`attack`/`stretch`/`direction` re-derive the active
/// partitioned IR from the stored original (background re-preparation —
/// see `ir::engine::ImpulseReshaper`); `feedback` is runtime DSP
/// (wet → pre-delay recirculation). Defaults are bit-transparent.
///
/// Per the MX manual, loading a new IR resets these to defaults
/// (mix, which lives on the chain, is preserved).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImpulseParams {
    /// Fraction of the IR that plays back (0.01..1.0).
    pub decay: f64,
    /// How `decay` < 1.0 shortens the tail.
    pub tail: ImpulseTail,
    /// Onset softening (0 = full attack punch, 1 = slow fade-in).
    pub attack: f64,
    /// IR re-sample factor (0.25..4.0). 1.0 = as recorded; higher =
    /// longer decay + darker (manual: "low settings reduce the decay,
    /// higher settings for longer"), lower = shorter + brighter.
    pub stretch: f64,
    pub direction: ImpulseDirection,
    /// Wet signal recirculated into the pre-delay (0..1). Character
    /// depends on the pre-delay time (`ConvolutionModParams::predelay_ms`).
    pub feedback: f64,
}

impl Default for ImpulseParams {
    fn default() -> Self {
        Self {
            decay: 1.0,
            tail: ImpulseTail::Envelope,
            attack: 0.0,
            stretch: 1.0,
            direction: ImpulseDirection::Forward,
            feedback: 0.0,
        }
    }
}

impl ImpulseParams {
    /// The shaping subset (everything except `feedback`) — equality on
    /// this tuple decides whether a re-preparation is needed.
    pub fn shape_key(&self) -> (u64, ImpulseTail, u64, u64, ImpulseDirection) {
        (
            self.decay.clamp(0.01, 1.0).to_bits(),
            self.tail,
            self.attack.clamp(0.0, 1.0).to_bits(),
            self.stretch.clamp(0.25, 4.0).to_bits(),
            self.direction,
        )
    }

    /// True when every shaping param is at its identity value (no
    /// re-preparation needed, original IR plays untouched).
    pub fn shape_is_identity(&self) -> bool {
        self.decay >= 1.0 - 1e-9
            && self.attack <= 1e-9
            && (self.stretch - 1.0).abs() <= 1e-9
            && self.direction == ImpulseDirection::Forward
    }
}

/// Where the Shimmer pitch voices take their signal from (BigSky MX
/// Shimmer "Feedback" mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShimmerFeedbackMode {
    /// Voices are generated from the dry input only — a single shimmer
    /// layer over the reverb, no octave laddering.
    Input,
    /// Voices are generated from the reverb output and recirculated
    /// (shift inside the loop → runaway octave ladders). This is the
    /// legacy behavior.
    #[default]
    Regenerative,
    /// Both sources summed.
    InputPlusRegen,
}

impl ShimmerFeedbackMode {
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Input,
            2 => Self::InputPlusRegen,
            _ => Self::Regenerative,
        }
    }
}

/// BigSky MX Shimmer engine params: two independent pitch voices +
/// feedback-mode select. Defaults are bit-transparent against the
/// legacy single-voice mapping (`extra_a` = amount, `extra_b` = coarse
/// interval select).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ShimmerParams {
    /// Voice 1 interval in semitones (-12..+12). `None` = keep the
    /// legacy `extra_b` coarse mapping (octave up / fifth / octave dn).
    pub shift1_semitones: Option<f64>,
    /// Voice 2 interval in semitones (-12..+12). Only audible when
    /// `voice2` is on.
    pub shift2_semitones: Option<f64>,
    /// Enable the second pitch voice.
    pub voice2: bool,
    /// Level of both shift voices (0..1). `None` = legacy `extra_a`
    /// mapping.
    pub amount: Option<f64>,
    pub feedback_mode: ShimmerFeedbackMode,
}

/// BigSky MX Magneto engine params (beyond the shared set).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MagnetoParams {
    /// Taps alternate hard L/R (center clarity + width).
    pub ping_pong: bool,
    /// Number of tape heads: 1 / 2 / 3 / 4 / 6 (menu order).
    pub heads: MagnetoHeads,
    /// Even = equidistant heads (equal delay times, overtly rhythmic);
    /// Uneven = irregular spacing + feedback from the last TWO heads.
    pub spacing: MagnetoSpacing,
    /// Feedback into the tape input (0.0–1.0). This is the engine's
    /// PRE-DELAY knob remap — the chain routes `predelay_ms` here and
    /// bypasses its own pre-delay line for Magneto.
    pub feedback: f64,
}

/// Magneto head-count menu (1 / 2 / 3 / 4 / 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MagnetoHeads {
    One,
    Two,
    Three,
    #[default]
    Four,
    Six,
}

impl MagnetoHeads {
    pub const COUNT: usize = 5;

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::One,
            1 => Self::Two,
            2 => Self::Three,
            4 => Self::Six,
            _ => Self::Four,
        }
    }

    pub fn count(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Six => 6,
        }
    }
}

/// Magneto head spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MagnetoSpacing {
    #[default]
    Even,
    Uneven,
}

/// BigSky MX Spring "Dwell": drive stages of the spring-tank preamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpringDwell {
    /// The cleanest spring tones.
    #[default]
    Clean,
    /// More gain, typical of combo amps with onboard spring.
    Combo,
    /// Increased gain AND harmonic content entering the tank.
    Tube,
    /// Expanded preamp gain for maximum trashiness.
    Overdrive,
}

impl SpringDwell {
    pub const COUNT: usize = 4;

    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Combo,
            2 => Self::Tube,
            3 => Self::Overdrive,
            _ => Self::Clean,
        }
    }

    /// Preamp drive into the tank (1.0 = unity/clean).
    pub fn drive(self) -> f64 {
        match self {
            Self::Clean => 1.0,
            Self::Combo => 1.7,
            Self::Tube => 2.6,
            Self::Overdrive => 4.5,
        }
    }
}

/// BigSky MX Spring engine params.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpringParams {
    pub dwell: SpringDwell,
    /// Springs in the tank (1–3).
    pub springs: u8,
}

impl Default for SpringParams {
    fn default() -> Self {
        Self {
            dwell: SpringDwell::Clean,
            springs: 2,
        }
    }
}

/// BigSky MX NonLinear envelope shapes (manual menu order, CC 0–5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NlShape {
    /// Exponential backward swell.
    Swoosh,
    /// Linear backward ramp-up then cut.
    Reverse,
    /// Triangle: up then down.
    Ramp,
    /// Even amplitude with abrupt cut-off.
    Gate,
    /// Bell-curve profile.
    Gauss,
    /// Inverted bell.
    Bounce,
}

impl NlShape {
    pub const COUNT: usize = 6;

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Swoosh,
            1 => Self::Reverse,
            2 => Self::Ramp,
            3 => Self::Gate,
            4 => Self::Gauss,
            _ => Self::Bounce,
        }
    }
}

/// BigSky MX Chamber "Color": five fixed post-tonality profiles
/// capturing "the speakers and mics used in the chamber recording
/// process". Not a continuous control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChamberColor {
    /// Wide-range flat response — natural tone.
    #[default]
    Neutral,
    /// Reduced low end (avoids mud with bass-heavy sources).
    Clear,
    /// Reduced mid response ("smile" EQ).
    Smooth,
    /// High-passed, very bright.
    Crisp,
    /// Emphasized mids — vocal qualities.
    Deep,
}

impl ChamberColor {
    pub const COUNT: usize = 5;

    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Clear,
            2 => Self::Smooth,
            3 => Self::Crisp,
            4 => Self::Deep,
            _ => Self::Neutral,
        }
    }
}

/// BigSky MX Chamber engine params.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChamberParams {
    pub color: ChamberColor,
}

/// BigSky MX NonLinear engine params: Chop (amplitude mod on the decay),
/// explicit gate speed, and the separate Late reverb stage. Defaults
/// are transparent (chop depth 0, late level 0, gate speed 1.0 ≙ the
/// legacy 90% hold point).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NonLinearParams {
    /// Envelope shape (manual menu order). `None` = legacy `extra_a`
    /// threshold mapping.
    pub shape: Option<NlShape>,
    /// Feedback from the nonlinear generator back to the input, before
    /// the late stage (0..1). This is the engine's PRE-DELAY knob
    /// remap — the chain routes `predelay_ms` here for NonLinear.
    pub feedback: f64,
    /// Chop LFO rate in Hz (0.1..15).
    pub chop_rate_hz: f64,
    /// Chop depth (0..1). 0 = off (transparent).
    pub chop_depth: f64,
    /// Gate hold fraction control (0..1) for the Gate shape: the gate
    /// holds full level until `0.5 + 0.4 * gate_speed` of the envelope
    /// window, then releases. 1.0 reproduces the legacy 0.9 hold.
    pub gate_speed: f64,
    /// Late-stage onset speed (0..1): 0 = slow swell (~500 ms),
    /// 1 = immediate.
    pub late_speed: f64,
    /// Late-stage decay (0..1).
    pub late_decay: f64,
    /// Late-stage output level (0..1). 0 = stage off (no CPU).
    pub late_level: f64,
}

impl Default for NonLinearParams {
    fn default() -> Self {
        Self {
            shape: None,
            feedback: 0.0,
            chop_rate_hz: 4.0,
            chop_depth: 0.0,
            gate_speed: 1.0,
            late_speed: 0.5,
            late_decay: 0.5,
            late_level: 0.0,
        }
    }
}

/// BigSky MX Cloud engine params (beyond the shared set).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloudParams {
    /// Ensemble level (0..1): a pitch-tracked synthetic string/pad
    /// layer blended into the reverb input (Cloudburst-style). 0 = off
    /// (transparent, no CPU). Coexists with Diffusion.
    pub ensemble: f64,
}

impl Default for CloudParams {
    fn default() -> Self {
        Self { ensemble: 0.0 }
    }
}

/// BigSky MX Bloom engine params (beyond the shared set).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BloomParams {
    /// Harmonics level (0..1): filter-bank overtone generator
    /// (octave-up partials, POG-style) fed into the trail. 0 = off
    /// (transparent, no CPU).
    pub harmonics: f64,
}

impl Default for BloomParams {
    fn default() -> Self {
        Self { harmonics: 0.0 }
    }
}

/// BigSky MX Chorale vowel programs (manual menu order, CC 0–6).
/// Combination entries morph slowly between their vowels; `Random`
/// wanders the whole formant space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChoraleVowel {
    Aahhoo,
    Aahh,
    Aahhoh,
    Oh,
    Ooohoh,
    Ooo,
    Random,
}

impl ChoraleVowel {
    pub const COUNT: usize = 7;

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Aahhoo,
            1 => Self::Aahh,
            2 => Self::Aahhoh,
            3 => Self::Oh,
            4 => Self::Ooohoh,
            5 => Self::Ooo,
            _ => Self::Random,
        }
    }
}

/// BigSky MX Chorale "Resonance": intensity of the vowel via the
/// vocal-filter Q.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChoraleResonance {
    /// Subtle vocal quality.
    #[default]
    Mild,
    /// Increased intensity.
    Medium,
    /// Most resonant.
    High,
}

impl ChoraleResonance {
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Medium,
            2 => Self::High,
            _ => Self::Mild,
        }
    }

    /// (Q, peak dB) for the formant filters.
    pub fn q_gain(self) -> (f64, f64) {
        match self {
            Self::Mild => (3.0, 8.0),
            Self::Medium => (4.5, 10.0),
            Self::High => (6.5, 12.0),
        }
    }
}

/// Chorale choir range (BigSky MX "Choir Voice").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChoirVoice {
    /// Mid-to-high chorale range — the legacy voicing.
    #[default]
    Tenor,
    /// Low chorale range: formant centers shifted DOWN (the pedal's
    /// second range is Baritone, not up).
    Baritone,
}

/// BigSky MX Chorale engine params (beyond the shared set).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ChoraleParams {
    /// Choir voice level (0..1). `None` = legacy `extra_a` mapping.
    pub choir_level: Option<f64>,
    pub voice: ChoirVoice,
    /// Vowel program (manual menu). `None` = legacy continuous
    /// `extra_b` morph.
    pub vowel: Option<ChoraleVowel>,
    /// Formant-resonance intensity (vocal-filter Q).
    pub resonance: ChoraleResonance,
    /// Per-voice pitch/timbre randomization (0..1): more mod = more
    /// distinct singers (decorrelated vibrato + formant drift).
    /// 0 = off (transparent).
    pub mod_amount: f64,
}

/// BigSky MX per-engine Voice select. MX = the current voicing
/// (default, bit-transparent); Classic selects the counterpart
/// heritage.
///
/// - Plate / Spring: pair-mapped onto the existing variant system
///   (Plate: Dattorro ↔ Lexicon 224; Spring: Classic ↔ Vintage) —
///   both implementations genuinely exist.
/// - Hall / Room / Shimmer: `Classic` is a re-tune of the same
///   algorithm (slappier, punchier, more resonant harmonic buildup —
///   the manual's description), NOT a port of the original BigSky.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReverbVoice {
    #[default]
    Mx,
    Classic,
}

/// How the Hall Swell shapes the signal (BigSky MX "Swell Type").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwellType {
    /// Swell the reverb only — dry stays untouched.
    #[default]
    Wet,
    /// Swell the whole output (volume-pedal feel).
    WetPlusDry,
}

/// BigSky MX Hall engine params. Consumed at the CHAIN level (not the
/// algorithm): the Mid EQ rides the wet output bus so it covers every
/// hall variant, and the swell needs both wet and dry. Defaults are
/// transparent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HallParams {
    /// Mid-band cut/boost in dB around ~1 kHz on the wet (-6..+6).
    /// Negative = the mid-scooped "space for the dry" EQ.
    pub mid_db: f64,
    /// Swell rise (0..1): 0 = off (transparent), higher = slower/longer
    /// volume swell into each note (envelope logic borrowed from the
    /// Swell algorithm).
    pub swell_rise: f64,
    pub swell_type: SwellType,
}

impl Default for HallParams {
    fn default() -> Self {
        Self {
            mid_db: 0.0,
            swell_rise: 0.0,
            swell_type: SwellType::Wet,
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
    /// Attach a disposal channel for buffers displaced by audio-thread
    /// IR swaps (see [`crate::ir::IrTrash`]). Returns false when the
    /// algorithm has no swap path (everything but Convolution).
    fn set_ir_trash_sender(
        &mut self,
        _tx: crossbeam_channel::Sender<crate::ir::IrTrash>,
    ) -> bool {
        false
    }

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

    /// Push Shimmer engine params. No-op outside Shimmer; returns
    /// `true` if accepted.
    fn set_shimmer_params(&mut self, params: &ShimmerParams) -> bool {
        let _ = params;
        false
    }

    /// Push Magneto engine params. No-op outside Magneto; returns
    /// `true` if accepted.
    /// Engage the Classic-voice vintage texture (truncated common-mode
    /// chorus reads, era voicings). Returns false when the algorithm
    /// has no vintage path.
    fn set_vintage(&mut self, _on: bool) -> bool {
        false
    }

    /// Push Spring params. No-op outside the Spring engines.
    fn set_spring_params(&mut self, _params: &SpringParams) -> bool {
        false
    }

    /// Push Chamber params. No-op outside the Chamber engine.
    fn set_chamber_params(&mut self, _params: &ChamberParams) -> bool {
        false
    }

    fn set_magneto_params(&mut self, params: &MagnetoParams) -> bool {
        let _ = params;
        false
    }

    /// Push NonLinear engine params. No-op outside NonLinear; returns
    /// `true` if accepted.
    fn set_nonlinear_params(&mut self, params: &NonLinearParams) -> bool {
        let _ = params;
        false
    }

    /// Push Cloud engine params. No-op outside Cloud; returns `true`
    /// if accepted.
    fn set_cloud_params(&mut self, params: &CloudParams) -> bool {
        let _ = params;
        false
    }

    /// Push Bloom engine params. No-op outside Bloom; returns `true`
    /// if accepted.
    fn set_bloom_params(&mut self, params: &BloomParams) -> bool {
        let _ = params;
        false
    }

    /// Push Chorale engine params. No-op outside Chorale; returns
    /// `true` if accepted.
    fn set_chorale_params(&mut self, params: &ChoraleParams) -> bool {
        let _ = params;
        false
    }

    /// Push Impulse-engine shaping params. Runtime parts (feedback)
    /// apply immediately; shaping parts mark the algorithm dirty for a
    /// background re-preparation (see [`Self::impulse_reshape_source`]).
    /// No-op outside Convolution; returns `true` if accepted.
    fn set_impulse_params(&mut self, params: &ImpulseParams, snap: bool) -> bool {
        let _ = (params, snap);
        false
    }

    /// When the algorithm needs its IR re-shaped (shaping params changed
    /// since the last applied preparation), returns the ORIGINAL IR for
    /// the slot as cheap `Arc` clones (RT-safe — no allocation) and
    /// clears the dirty flag for that slot. `None` = nothing to do or
    /// original unavailable.
    #[allow(clippy::type_complexity)]
    fn impulse_reshape_source(
        &mut self,
        slot: IrSlot,
    ) -> Option<(std::sync::Arc<Vec<f64>>, std::sync::Arc<Vec<f64>>)> {
        let _ = slot;
        None
    }

    /// Swap in a re-shaped prepared IR (background-FFT'd). Unlike
    /// [`Self::try_load_prepared_ir_slot`], this neither resets the
    /// impulse params nor marks a user IR as loaded — it's the return
    /// leg of the re-preparation pipeline. Returns `true` if accepted.
    fn swap_reshaped_ir(&mut self, pair: crate::ir::PreparedIrPair, slot: IrSlot) -> bool {
        let _ = (pair, slot);
        false
    }
}
