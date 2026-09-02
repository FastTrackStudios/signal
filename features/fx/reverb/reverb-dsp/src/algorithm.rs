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
    /// A diffuse space whose delay lines random-walk — Valhalla's
    /// "Random Space" / "Smooth Random" / "Chaotic" family.
    Random,
}

impl AlgorithmType {
    /// The wet-bus trim, in dB, that puts this algorithm at the same output
    /// level as every other one.
    ///
    /// The engines were written at different times against different
    /// references and never shared a level. Measured at a matched T60 of 2 s
    /// they spanned **47 dB** — Bloom at -23 dB against Velvet at +32 — so
    /// changing algorithm was a volume change first and a character change
    /// second, and no preset library could carry a level across engines.
    ///
    /// Each constant is the negative of that algorithm's measured wet energy
    /// (in dB relative to unity) for a unit impulse at T60 = 2 s, so a
    /// calibrated engine returns unity energy there and the whole set lands
    /// together. Unity is an arbitrary but fixed anchor; matching an outside
    /// reference is then a single global offset rather than sixteen.
    ///
    /// Re-derive with `cargo run -p signal-analyzer --example wet_level`,
    /// which prints the measurement these come from.
    /// `algorithms_share_one_output_level` in `tests/stability.rs` fails if a
    /// change to an engine invalidates its constant.
    pub fn wet_calibration_db(self) -> f64 {
        match self {
            Self::Room => -3.03,
            Self::Hall => 0.47,
            Self::Plate => 8.82,
            Self::Spring => 5.93,
            Self::Cloud => -2.25,
            Self::Bloom => 22.84,
            Self::Shimmer => -0.58,
            Self::Chorale => -0.50,
            Self::Magneto => 6.36,
            Self::NonLinear => 16.72,
            // Swell renders silence at every setting — a defect of its own,
            // which a trim cannot repair and should not paper over.
            Self::Swell => 0.0,
            Self::Reflections => 5.81,
            Self::Velvet => -24.23,
            Self::FreeVerb => 5.62,
            // Convolution's level is whatever the loaded IR carries; the
            // constant is the measurement for the built-in default.
            Self::Convolution => -4.92,
            Self::Random => -3.91,
        }
    }

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
        Self::Random,
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
            Self::Random => "Random",
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
    /// The Decay Rate EQ (`fx.reverb.decay-eq`, docs/spec/fx/embedded-eq.md):
    /// up to six Bell / Shelf / Notch curves of decay-time multipliers over
    /// frequency, generalizing the low/high crossover pair above. The FDN
    /// algorithms (the Hall family) realize it exactly in the feedback path
    /// ([`crate::primitives::fdn::Fdn::set_decay_curve`]); other engines
    /// collapse it onto `low_decay_mult` / `high_decay_mult`
    /// ([`decay_bands_collapsed`]).
    pub decay_bands: [DecayBand; DECAY_BANDS],
}

/// Number of Decay Rate EQ bands.
///
/// Eight, one per octave the analyzer measures (62.5 Hz … 8 kHz). Six forced
/// the fitter to cover the outermost two octaves with a single shelf each, so
/// an error at 62.5 Hz could only be corrected by also moving 125 Hz. Giving
/// every measured band its own curve is what lets a fit actually converge.
pub const DECAY_BANDS: usize = 8;

/// Shortest decay-time multiplier a band may ask for (a tenth of the space's
/// own decay).
pub const DECAY_RATE_MIN: f64 = 0.1;
/// Longest decay-time multiplier a band may ask for.
pub const DECAY_RATE_MAX: f64 = 4.0;

/// One Decay Rate EQ band: a curve of decay-TIME multipliers over frequency
/// (`fx.reverb.decay-eq`). `rate` 1.0 = the space's natural decay; 4.0 =
/// four times longer at this band; [`DECAY_RATE_MIN`] a tenth.
///
/// The cut range goes further than Pro-R 2's 25 %–400 %, deliberately. That
/// is a product limit, not a property of the engine, and matching a real
/// space needs more: fitting our chamber to a Valhalla reference drove both
/// shelves hard against a 0.25x floor and still could not darken the tail
/// enough. Cuts are also the safe direction — the loop-runaway guard in
/// `Fdn::set_decay_curve` exists to bound boosts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecayBand {
    /// 0 = Bell, 1 = Low Shelf, 2 = High Shelf (a Notch is a Bell with a
    /// small `rate` and high `q`).
    pub shape: u32,
    pub freq_hz: f64,
    /// Decay-time multiplier, [`DECAY_RATE_MIN`]..=[`DECAY_RATE_MAX`].
    pub rate: f64,
    pub q: f64,
}

impl Default for DecayBand {
    fn default() -> Self {
        Self {
            shape: 0,
            freq_hz: 1000.0,
            rate: 1.0,
            q: 0.707,
        }
    }
}

impl DecayBand {
    /// Whether this band changes anything.
    pub fn is_active(&self) -> bool {
        (self.rate - 1.0).abs() > 0.005
    }

    /// The band's rate contribution at `freq`, in "rate dB"
    /// (20·log10(rate) shaped by the band's curve — the same bell/shelf
    /// magnitude the EQ display draws).
    pub fn rate_db_at(&self, freq: f64) -> f64 {
        let peak_db = 20.0 * self.rate.clamp(DECAY_RATE_MIN, DECAY_RATE_MAX).log10();
        peak_db * self.shape_weight_at(freq)
    }

    /// How much of the band's peak this frequency receives, 0..=1.
    ///
    /// Separated from [`Self::rate_db_at`] so a caller can weight some *other*
    /// gain by the band's curve — the loop-stability check needs the combined
    /// response of every band at a frequency, which is the peak gains scaled
    /// by their shapes and summed, not the peaks themselves.
    pub fn shape_weight_at(&self, freq: f64) -> f64 {
        let f0 = self.freq_hz.max(10.0);
        let q = self.q.clamp(0.1, 18.0);
        // Analog-prototype magnitudes — display-and-collapse grade.
        let w = freq / f0;
        match self.shape {
            // Low shelf: full below f0, none far above.
            1 => 1.0 / (1.0 + (w * q * 1.414).powi(2)),
            // High shelf: full above f0.
            2 => (1.0 - 1.0 / (1.0 + (w / (q * 1.414).recip()).powi(2))).clamp(0.0, 1.0),
            // Bell.
            _ => {
                let bw = w - 1.0 / w.max(1e-9);
                1.0 / (1.0 + (bw * q).powi(2))
            }
        }
    }
}

/// The whole curve's decay-rate multiplier at `freq` (bands sum in rate-dB,
/// clamped to Pro-R's 0.25..4 range).
pub fn decay_rate_at(bands: &[DecayBand; DECAY_BANDS], freq: f64) -> f64 {
    let db: f64 = bands
        .iter()
        .filter(|b| b.is_active())
        .map(|b| b.rate_db_at(freq))
        .sum();
    10.0f64
        .powf(db / 20.0)
        .clamp(DECAY_RATE_MIN, DECAY_RATE_MAX)
}

/// Collapse the curve to the legacy low/high multiplier pair, for engines
/// without a per-frequency feedback path: the rate sampled in the bass
/// (100 Hz) and the top (6 kHz).
pub fn decay_bands_collapsed(bands: &[DecayBand; DECAY_BANDS]) -> (f64, f64) {
    if !bands.iter().any(|b| b.is_active()) {
        return (1.0, 1.0);
    }
    (decay_rate_at(bands, 100.0), decay_rate_at(bands, 6000.0))
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
            decay_bands: [DecayBand::default(); DECAY_BANDS],
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
    /// Decay EQ: low-band end gain in dB (−24..+12) at ~250 Hz —
    /// negative shortens the low decay, positive stretches it.
    pub decay_lo_db: f64,
    /// Decay EQ: high-band end gain in dB (−24..+12) at ~4 kHz.
    pub decay_hi_db: f64,
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
            decay_lo_db: 0.0,
            decay_hi_db: 0.0,
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
    #[allow(clippy::type_complexity)]
    pub fn shape_key(&self) -> (u64, ImpulseTail, u64, u64, ImpulseDirection, u64, u64) {
        (
            self.decay.clamp(0.01, 1.0).to_bits(),
            self.tail,
            self.attack.clamp(0.0, 1.0).to_bits(),
            self.stretch.clamp(0.25, 4.0).to_bits(),
            self.direction,
            self.decay_lo_db.clamp(-24.0, 12.0).to_bits(),
            self.decay_hi_db.clamp(-24.0, 12.0).to_bits(),
        )
    }

    /// True when every shaping param is at its identity value (no
    /// re-preparation needed, original IR plays untouched).
    pub fn shape_is_identity(&self) -> bool {
        self.decay >= 1.0 - 1e-9
            && self.attack <= 1e-9
            && (self.stretch - 1.0).abs() <= 1e-9
            && self.direction == ImpulseDirection::Forward
            && self.decay_lo_db.abs() <= 0.05
            && self.decay_hi_db.abs() <= 0.05
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

    /// True-stereo (4-leg) IR load: LL/RR drive the direct convolvers,
    /// LR/RL the cross-feed pair. Slot A only. No-op outside
    /// Convolution.
    fn try_load_ir_true_stereo(&mut self, ll: &[f64], lr: &[f64], rl: &[f64], rr: &[f64]) -> bool {
        let _ = (ll, lr, rl, rr);
        false
    }

    /// Cross-leg reshape originals (LR, RL) for a true-stereo slot A
    /// impulse, if loaded.
    #[allow(clippy::type_complexity)]
    fn impulse_reshape_cross_source(
        &self,
    ) -> Option<(std::sync::Arc<Vec<f64>>, std::sync::Arc<Vec<f64>>)> {
        None
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
    fn set_ir_trash_sender(&mut self, _tx: crossbeam_channel::Sender<crate::ir::IrTrash>) -> bool {
        false
    }

    fn try_load_prepared_ir_slot(&mut self, pair: crate::ir::PreparedIrPair, slot: IrSlot) -> bool {
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

// ═══════════════════════════════════════════════════════════════════
// Decay-time mapping
// ═══════════════════════════════════════════════════════════════════

/// The `decay` value at or above which an engine holds indefinitely.
pub const FREEZE_DECAY: f64 = 0.999;

/// The T60 used to mean "hold forever".
pub const INFINITE_T60: f64 = 1.0e6;

/// Map the normalized `decay` control (0–1) onto a reverberation time in
/// seconds, logarithmically between `min_s` and `max_s`.
///
/// Every engine should route its decay through this rather than inventing a
/// feedback gain. A raw gain is not a time: it makes `decay` mean something
/// different in each algorithm, it cannot express a target RT60, and it caps
/// the reachable tail at whatever the gain range happens to allow. Measuring
/// the engines against reference reverbs showed exactly that — Hall (which
/// already used an exact-T60 model) landed within a factor of two of its
/// reference, while Room, which set a gain directly, could not produce a tail
/// longer than 1.7 s and its Chamber variant was stuck near 0.18 s.
///
/// Logarithmic because reverberation time is perceived that way: the step
/// from 0.2 s to 0.4 s is as large a change as 2 s to 4 s.
///
/// `decay >= FREEZE_DECAY` returns [`INFINITE_T60`].
pub fn decay_to_t60(decay: f64, min_s: f64, max_s: f64) -> f64 {
    if decay >= FREEZE_DECAY {
        return INFINITE_T60;
    }
    let min_s = min_s.max(0.01);
    let max_s = max_s.max(min_s * 1.001);
    min_s * (max_s / min_s).powf(decay.clamp(0.0, 1.0))
}

/// Split a midband T60 into the DC and Nyquist targets `Fdn::set_t60` wants.
///
/// The low/high decay multipliers scale their end of the shelf directly, and
/// damping shortens the top. Note that this means a tilt also moves the
/// *overall* decay — taming the lows really does shorten the tail, which is
/// the intended behaviour. Callers that need a specific midband time despite
/// a tilt should pre-compensate; [`tilt_midband_factor`] is that factor, and
/// `ReverbChain::set_decay_seconds` applies it.
///
/// Shared so every engine derives its shelf the same way.
pub fn t60_shelf_targets(
    t60: f64,
    low_decay_mult: f64,
    high_decay_mult: f64,
    damping: f64,
) -> (f64, f64) {
    let t60_dc = (t60 * low_decay_mult.max(0.05)).max(0.05);
    let hf_ratio = ((0.15 + 0.85 * (1.0 - damping)) * high_decay_mult.max(0.05)).clamp(0.02, 1.5);
    let t60_ny = (t60 * hf_ratio).max(0.02);
    (t60_dc, t60_ny)
}

/// How much a decay tilt moves the midband, as a multiple of the requested
/// T60.
///
/// The shelf runs between the DC and Nyquist targets, so the midband lands at
/// their geometric mean. Asking for 2.5 s while also taming the lows to 0.5x
/// otherwise yields about 1.36 s — dividing the request by this factor puts
/// the midband back where it was asked for, without changing what a tilt
/// means for anyone setting the raw `decay` control.
///
/// Damping is deliberately excluded: absorption genuinely shortens a tail, so
/// a damped space is *meant* to decay faster and compensating for it would
/// fight the control. Only the explicit tilt multipliers are corrected.
pub fn tilt_midband_factor(low_decay_mult: f64, high_decay_mult: f64) -> f64 {
    let lo = low_decay_mult.max(0.05);
    let hi = high_decay_mult.max(0.05);
    (lo * hi).sqrt().max(1e-6)
}

/// The reverberation-time range each engine's `decay` control spans.
///
/// The floors are ordered by the size of the space, not by what a meter can
/// resolve. A studio room reaches shorter than a hall because a small room
/// *is* shorter — inverting that (as a first pass at these numbers did, by
/// reading floors off what the analyzer could still fit) makes a room ring
/// longer than a hall at the bottom of the range, which is nonsense and which
/// `dual::tests::split_isolates_channels` rightly rejects.
///
/// The spread matters beyond plausibility: callers use it to decide whether an
/// engine suits a request. A preset named for a big space is often a short one
/// — "PALACE-1982 Room Mics" wants 0.29 s, well under any hall's floor — and
/// `signal-import` reads these ranges to route such a preset to a smaller
/// engine rather than let the request clamp.
///
/// One table so the range is queryable from outside the engine — which is
/// what lets a caller ask for a *time* ([`AlgorithmType::t60_range`] +
/// [`t60_to_decay`]) instead of guessing at a normalized control. Each
/// engine reads its own entry, so these are the single source of truth
/// rather than a parallel copy.
pub const ROOM_T60: (f64, f64) = (0.12, 6.0);
/// Chambers ring longer than rooms and shorter than halls.
///
/// The ceiling has headroom on purpose. A real chamber preset asked for 5.2 s
/// in its low band; against a 6 s ceiling that sits at decay 0.96, close
/// enough to freeze that damping and the tilt compensation could not get
/// there, and the fit stalled with the low bands short.
pub const ROOM_CHAMBER_T60: (f64, f64) = (0.2, 12.0);
/// A treated studio room — deliberately short.
pub const ROOM_STUDIO_T60: (f64, f64) = (0.08, 2.0);
pub const HALL_T60: (f64, f64) = (0.4, 30.0);
pub const HALL_CATHEDRAL_T60: (f64, f64) = (0.6, 40.0);
pub const HALL_ARENA_T60: (f64, f64) = (0.8, 40.0);
/// Random spaces span room-to-large-hall lengths.
pub const RANDOM_T60: (f64, f64) = (0.15, 25.0);
/// The plate tank.
///
/// The floor is a round trip and a half, not an arbitrary minimum: the
/// Dattorro loop takes [`PLATE_LOOP_SECONDS`], so a request much below that
/// decays inside a single pass. The tank stops recirculating, the tail is
/// whatever the diffusers happen to give, and its low end collapses — a
/// "Slappy Tom Plate" asking for 0.8 s came out with its 125 Hz band decaying
/// faster than its 500 Hz one, which is not a plate at all. Shorter requests
/// are better served by another engine, and `signal-import` routes them there.
pub const PLATE_T60: (f64, f64) = (1.05, 30.0);

/// Dattorro tank round-trip time, in seconds.
///
/// The published delay lengths sum to 21589 samples at the reference 29761 Hz,
/// and every length scales with the sample rate, so the loop takes the same
/// 0.725 s whatever rate it runs at.
pub const PLATE_LOOP_SECONDS: f64 = 21589.0 / 29761.0;
/// Times the tank gain is applied per round trip (twice per tank, two tanks).
pub const PLATE_DECAY_APPLICATIONS: f64 = 4.0;

/// The Dattorro tank gain that yields `t60_s`.
///
/// A tank sets a per-loop *gain*, not a time, which is why the plate had no
/// decay-time model and `decay_time` was a silent no-op on it: a translated
/// preset could not be tuned to the right length at all. The conversion is
/// exact — losing 60 dB over `t60_s` at `applications` multiplications per
/// `loop_seconds` round trip means each multiplication is
/// `10^(-3·loop/(applications·t60))`.
pub fn dattorro_gain_for_t60(t60_s: f64, loop_seconds: f64, applications: f64) -> f64 {
    let t60 = t60_s.max(0.01);
    let exponent = -3.0 * loop_seconds / (applications.max(1.0) * t60);
    // Below 0.997 the tank always decays; the floor keeps a very short
    // request from collapsing the tank to silence.
    10.0f64.powf(exponent).clamp(0.02, 0.997)
}

impl AlgorithmType {
    /// Whether this engine realizes the Decay Rate EQ exactly, in its own
    /// feedback path (`Fdn::set_decay_curve`).
    ///
    /// Engines that do must NOT also have the curve collapsed onto the legacy
    /// `low_decay_mult` / `high_decay_mult` pair — that would apply it twice,
    /// once per-frequency and once as a broadband multiplier on the T60
    /// shelf. The broadband half wins: a 300 Hz low shelf at 0.5x measured as
    /// halving the decay at 4 kHz as well, which is the opposite of what a
    /// shelf is for.
    ///
    /// This is a property of the engine, not a hardcoded list at the call
    /// site, so wiring `set_decay_curve` into another engine means updating
    /// one place rather than silently double-applying.
    pub fn realizes_decay_curve(self) -> bool {
        matches!(self, Self::Hall | Self::Room | Self::Random)
    }

    /// The `(min, max)` reverberation time in seconds that this engine's
    /// `decay` control spans, for a given variant.
    ///
    /// `None` for engines that do not model decay as a time — the plate tank
    /// and the character engines set a feedback coefficient directly, so
    /// there is no honest time to report.
    pub fn t60_range(self, variant: usize) -> Option<(f64, f64)> {
        match (self, variant) {
            (Self::Room, 1) => Some(ROOM_CHAMBER_T60),
            (Self::Room, 2) => Some(ROOM_STUDIO_T60),
            (Self::Room, _) => Some(ROOM_T60),
            (Self::Hall, 1) => Some(HALL_CATHEDRAL_T60),
            (Self::Hall, 2) => Some(HALL_ARENA_T60),
            (Self::Hall, _) => Some(HALL_T60),
            (Self::Random, _) => Some(RANDOM_T60),
            // Only the base Dattorro tank is converted; the Lexicon and
            // Progenitor variants keep their own gain mapping.
            (Self::Plate, 0) => Some(PLATE_T60),
            _ => None,
        }
    }
}

/// Inverse of [`decay_to_t60`]: the `decay` control that yields `t60_s`.
///
/// Clamps to `0..1`, so a time outside the engine's reach saturates at its
/// nearest end rather than producing an out-of-range control value. Callers
/// that need to know they were clamped should compare against
/// [`AlgorithmType::t60_range`] first.
pub fn t60_to_decay(t60_s: f64, min_s: f64, max_s: f64) -> f64 {
    let min_s = min_s.max(0.01);
    let max_s = max_s.max(min_s * 1.001);
    let t = t60_s.clamp(min_s, max_s);
    (t / min_s).ln() / (max_s / min_s).ln()
}

#[cfg(test)]
mod decay_time_tests {
    use super::*;

    #[test]
    fn spans_the_requested_range_logarithmically() {
        assert!((decay_to_t60(0.0, 0.2, 20.0) - 0.2).abs() < 1e-9);
        // The top of the *range* is approached just below the freeze
        // threshold; decay = 1.0 itself means hold forever.
        // 0.2·100^0.9989 = 19.90 — within 1% of the top of the range.
        assert!((decay_to_t60(0.9989, 0.2, 20.0) - 20.0).abs() / 20.0 < 0.01);
        // Midpoint is the geometric mean, not the arithmetic one.
        let mid = decay_to_t60(0.5, 0.2, 20.0);
        assert!((mid - 2.0).abs() < 1e-9, "got {mid}");
    }

    #[test]
    fn equal_steps_give_equal_ratios() {
        let a = decay_to_t60(0.25, 0.1, 10.0);
        let b = decay_to_t60(0.5, 0.1, 10.0);
        let c = decay_to_t60(0.75, 0.1, 10.0);
        assert!(((b / a) - (c / b)).abs() < 1e-9);
    }

    #[test]
    fn freezes_at_the_top() {
        assert_eq!(decay_to_t60(0.999, 0.2, 8.0), INFINITE_T60);
        assert_eq!(decay_to_t60(1.5, 0.2, 8.0), INFINITE_T60);
    }

    #[test]
    fn clamps_out_of_range_and_degenerate_inputs() {
        assert!((decay_to_t60(-1.0, 0.2, 8.0) - 0.2).abs() < 1e-9);
        // A max below the min must not invert or produce NaN.
        let t = decay_to_t60(0.5, 4.0, 1.0);
        assert!(t.is_finite() && t >= 4.0, "got {t}");
        assert!(decay_to_t60(0.5, 0.0, 8.0).is_finite());
    }

    #[test]
    fn decay_and_t60_round_trip() {
        for &(lo, hi) in &[ROOM_T60, ROOM_CHAMBER_T60, HALL_T60, HALL_ARENA_T60] {
            for d in [0.0, 0.1, 0.35, 0.5, 0.75, 0.9] {
                let t = decay_to_t60(d, lo, hi);
                let back = t60_to_decay(t, lo, hi);
                assert!((back - d).abs() < 1e-9, "{d} -> {t} -> {back}");
            }
        }
    }

    #[test]
    fn a_time_outside_the_range_saturates_rather_than_escaping_0_1() {
        let (lo, hi) = ROOM_STUDIO_T60;
        assert_eq!(t60_to_decay(0.001, lo, hi), 0.0);
        assert_eq!(t60_to_decay(600.0, lo, hi), 1.0);
    }

    #[test]
    fn the_tank_gain_inverts_its_own_decay_formula() {
        // Losing 60 dB over t60 at four multiplications per round trip.
        for t60 in [0.5, 2.0, 8.0, 25.0] {
            let g = dattorro_gain_for_t60(t60, PLATE_LOOP_SECONDS, PLATE_DECAY_APPLICATIONS);
            // Recover the time the gain implies and check it round-trips.
            let db_per_loop = 20.0 * PLATE_DECAY_APPLICATIONS * g.log10();
            let implied = PLATE_LOOP_SECONDS * -60.0 / db_per_loop;
            assert!(
                (implied - t60).abs() / t60 < 0.01,
                "{t60} s -> gain {g} -> {implied} s"
            );
        }
    }

    #[test]
    fn the_tank_gain_stays_stable_for_extreme_requests() {
        // However long the request, the tank must still decay.
        let g = dattorro_gain_for_t60(1.0e6, PLATE_LOOP_SECONDS, PLATE_DECAY_APPLICATIONS);
        assert!(g < 1.0, "a tank gain of 1.0 or more never decays: {g}");
        // And a very short one must not collapse it to silence.
        assert!(dattorro_gain_for_t60(0.001, PLATE_LOOP_SECONDS, PLATE_DECAY_APPLICATIONS) > 0.0);
    }

    #[test]
    fn variants_report_their_own_ranges() {
        assert_eq!(AlgorithmType::Room.t60_range(1), Some(ROOM_CHAMBER_T60));
        assert_eq!(AlgorithmType::Room.t60_range(0), Some(ROOM_T60));
        assert_eq!(AlgorithmType::Hall.t60_range(1), Some(HALL_CATHEDRAL_T60));
        // A chamber must actually reach the ~2.5 s a real one rings for —
        // the old feedback-gain model topped out near 0.18 s.
        let (lo, hi) = ROOM_CHAMBER_T60;
        assert!(lo < 2.5 && hi > 2.5);
        // The Dattorro tank is converted; its heritage variants are not.
        assert_eq!(AlgorithmType::Plate.t60_range(0), Some(PLATE_T60));
        assert_eq!(AlgorithmType::Plate.t60_range(1), None);
        // Engines with no time model say so instead of inventing one.
        assert_eq!(AlgorithmType::Velvet.t60_range(0), None);
        assert_eq!(AlgorithmType::Cloud.t60_range(0), None);
    }

    #[test]
    fn neutral_multipliers_leave_the_midband_alone() {
        let (dc, ny) = t60_shelf_targets(2.0, 1.0, 1.0, 0.0);
        assert!((dc - 2.0).abs() < 1e-9);
        assert!((ny - 2.0).abs() < 1e-9);
        assert!((tilt_midband_factor(1.0, 1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_tilt_shortens_the_tail_and_the_factor_says_by_how_much() {
        // Taming the lows genuinely shortens the tail — that behaviour is
        // relied on. What the factor provides is a way to ask for a midband
        // time *despite* a tilt.
        let (dc, ny) = t60_shelf_targets(2.5, 0.5, 1.0, 0.0);
        assert!(dc < 2.5, "tamed lows must actually decay faster");
        let midband = (dc * ny).sqrt();
        assert!(midband < 2.5);

        let factor = tilt_midband_factor(0.5, 1.0);
        assert!((midband - 2.5 * factor * (0.15f64 + 0.85).sqrt()).abs() < 1e-9);

        // Pre-compensating restores the requested midband.
        let (dc2, ny2) = t60_shelf_targets(2.5 / factor, 0.5, 1.0, 0.0);
        assert!(((dc2 * ny2).sqrt() - 2.5).abs() < 1e-9);
    }

    #[test]
    fn damping_still_shortens_the_tail() {
        // Absorption is physical: a damped space really does decay faster,
        // so damping must NOT be normalized away like the tilt controls.
        let (_, ny_open) = t60_shelf_targets(2.0, 1.0, 1.0, 0.0);
        let (_, ny_damped) = t60_shelf_targets(2.0, 1.0, 1.0, 1.0);
        assert!(ny_damped < ny_open);
    }

    #[test]
    fn shelf_targets_stay_positive_for_extreme_multipliers() {
        let (dc, ny) = t60_shelf_targets(0.05, 0.0, 0.0, 1.0);
        assert!(dc > 0.0 && ny > 0.0);
        let (dc2, ny2) = t60_shelf_targets(30.0, 2.0, 0.05, 0.0);
        assert!(dc2.is_finite() && ny2.is_finite() && ny2 > 0.0);
    }
}

#[cfg(test)]
mod decay_eq_filter_probe {
    use audiocore_dsp::biquad::{Biquad, FilterType};

    /// Magnitude of a biquad by running a sine through it.
    fn mag_at(mut bq: Biquad, f: f64, sr: f64) -> f64 {
        let n = 48_000;
        let mut peak = 0.0f64;
        for i in 0..n {
            let x = (std::f64::consts::TAU * f * i as f64 / sr).sin();
            let y = bq.tick(x, 0);
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn low_shelf_is_unity_well_above_its_corner() {
        let sr = 48_000.0;
        let mut bq = Biquad::default();
        bq.set(FilterType::LowShelf { gain_db: -6.0 }, 300.0, 0.707, sr);
        let at_dc = 20.0 * mag_at(bq, 40.0, sr).log10();
        let mut bq2 = Biquad::default();
        bq2.set(FilterType::LowShelf { gain_db: -6.0 }, 300.0, 0.707, sr);
        let at_hf = 20.0 * mag_at(bq2, 4000.0, sr).log10();
        assert!(
            (at_dc + 6.0).abs() < 1.0,
            "40 Hz should be -6 dB, got {at_dc}"
        );
        assert!(at_hf.abs() < 0.5, "4 kHz should be unity, got {at_hf}");
    }
}

#[cfg(test)]
mod decay_eq_localization {
    use crate::algorithm::{DecayBand, DECAY_BANDS};
    use crate::primitives::fdn::{Fdn, MixMatrix};

    const SR: f64 = 48_000.0;

    /// Drive a windowed sine burst into a bare FDN, then measure how fast the
    /// tail at that frequency decays, as dB per second.
    fn decay_db_per_s(bands: [DecayBand; DECAY_BANDS], probe_hz: f64) -> f64 {
        decay_db_per_s_with(bands, probe_hz, 0.0)
    }

    fn decay_db_per_s_with(
        bands: [DecayBand; DECAY_BANDS],
        probe_hz: f64,
        loop_allpass: f64,
    ) -> f64 {
        let delays = [787usize, 967, 1153, 1373, 1607, 1861, 2129, 2411];
        let mut fdn = Fdn::new(&delays, MixMatrix::Householder);
        fdn.set_damping(10_000.0, SR);
        if loop_allpass != 0.0 {
            fdn.set_loop_allpass(loop_allpass);
        }
        fdn.set_t60(2.5, 2.5, SR);
        fdn.set_decay_curve(2.5, &bands, SR);

        let drive = (SR * 0.2) as usize;
        let total = (SR * 4.0) as usize;
        let mut out = Vec::with_capacity(total);
        for i in 0..total {
            let x = if i < drive {
                let env = (std::f64::consts::PI * i as f64 / drive as f64).sin();
                (std::f64::consts::TAU * probe_hz * i as f64 / SR).sin() * env
            } else {
                0.0
            };
            out.push(fdn.tick(x));
        }

        // Energy in two windows well after the drive stops.
        let win = (SR * 0.4) as usize;
        let a0 = drive + (SR * 0.3) as usize;
        let b0 = a0 + (SR * 1.0) as usize;
        let energy = |start: usize| -> f64 {
            out[start..(start + win).min(out.len())]
                .iter()
                .map(|x| x * x)
                .sum::<f64>()
                .max(1e-300)
        };
        // dB drop over 1 second.
        10.0 * (energy(b0) / energy(a0)).log10()
    }

    fn low_shelf(freq: f64, rate: f64) -> [DecayBand; DECAY_BANDS] {
        let mut b = [DecayBand::default(); DECAY_BANDS];
        b[0] = DecayBand {
            shape: 1,
            freq_hz: freq,
            rate,
            q: 0.707,
        };
        b
    }

    fn boost_band(idx: usize, freq: f64, rate: f64) -> DecayBand {
        let _ = idx;
        DecayBand {
            shape: 0,
            freq_hz: freq,
            rate,
            q: 1.4,
        }
    }

    /// Boosting several *separated* bands must not scale them down the way
    /// boosting the same total at one frequency does.
    ///
    /// The loop-stability guard bounds the combined loop gain at each
    /// frequency. Summing every band's peak instead treats bands an octave
    /// apart as though they stacked, and scales a wide curve back to a
    /// fraction of what it asked for — which is why a bass-tilted chamber
    /// could not be matched at any length.
    #[test]
    fn separated_boosts_are_not_scaled_like_stacked_ones() {
        let mut spread = [DecayBand::default(); DECAY_BANDS];
        spread[0] = boost_band(0, 125.0, 3.0);
        spread[1] = boost_band(1, 500.0, 3.0);
        spread[2] = boost_band(2, 2000.0, 3.0);

        let mut single = [DecayBand::default(); DECAY_BANDS];
        single[0] = boost_band(0, 500.0, 3.0);

        // The 500 Hz band asks for the same lift in both curves, so it should
        // achieve close to the same decay in both — its neighbours are far
        // enough away not to compete for headroom.
        let alone = decay_db_per_s(single, 500.0);
        let with_neighbours = decay_db_per_s(spread, 500.0);
        assert!(
            (alone - with_neighbours).abs() < 4.0,
            "a boost should not be throttled by distant bands: \
alone {alone:.1} dB/s, with neighbours {with_neighbours:.1} dB/s"
        );

        // And it must actually lengthen the tail relative to flat.
        let flat = [DecayBand::default(); DECAY_BANDS];
        assert!(
            with_neighbours > decay_db_per_s(flat, 500.0) + 2.0,
            "a 3x boost should slow the decay"
        );
    }

    /// The guard still has to bite when bands genuinely overlap.
    #[test]
    fn stacked_boosts_are_still_bounded() {
        let mut stacked = [DecayBand::default(); DECAY_BANDS];
        for (i, slot) in stacked.iter_mut().take(4).enumerate() {
            *slot = boost_band(i, 500.0, 4.0);
        }
        // Four maximum boosts on the same frequency: the loop must stay
        // stable, i.e. the tail must still decay rather than run away.
        let slope = decay_db_per_s(stacked, 500.0);
        assert!(
            slope < 0.0 && slope.is_finite(),
            "overlapping maximum boosts must not destabilize the loop: {slope} dB/s"
        );
    }

    /// With an in-loop allpass engaged — the diffusion the Room engines now
    /// use — the shelf must STILL localize.
    #[test]
    fn a_low_shelf_localizes_with_an_in_loop_allpass_too() {
        let flat = [DecayBand::default(); DECAY_BANDS];
        let hf_flat = decay_db_per_s_with(flat, 4000.0, 0.7);
        let hf_cut = decay_db_per_s_with(low_shelf(300.0, 0.5), 4000.0, 0.7);
        assert!(
            (hf_cut - hf_flat).abs() < 3.0,
            "4 kHz should be untouched by a 300 Hz low shelf even with an \
in-loop allpass: {hf_flat:.1} -> {hf_cut:.1} dB/s"
        );
    }

    /// A low shelf must change the decay BELOW its corner and leave the top
    /// alone. Measured at the FDN level, with no diffusers, modulated
    /// allpasses or output stage in the way.
    #[test]
    fn a_low_shelf_does_not_change_the_decay_far_above_its_corner() {
        let flat = [DecayBand::default(); DECAY_BANDS];

        let lf_flat = decay_db_per_s(flat, 125.0);
        let lf_cut = decay_db_per_s(low_shelf(300.0, 0.5), 125.0);
        let hf_flat = decay_db_per_s(flat, 4000.0);
        let hf_cut = decay_db_per_s(low_shelf(300.0, 0.5), 4000.0);

        // Below the corner the tail must die faster (a more negative slope).
        assert!(
            lf_cut < lf_flat - 5.0,
            "125 Hz should decay faster with a 0.5x low shelf: {lf_flat:.1} -> {lf_cut:.1} dB/s"
        );
        // Nearly four octaves above it, nothing should change.
        assert!(
            (hf_cut - hf_flat).abs() < 3.0,
            "4 kHz should be untouched by a 300 Hz low shelf: {hf_flat:.1} -> {hf_cut:.1} dB/s"
        );
    }
}
