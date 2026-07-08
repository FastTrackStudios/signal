//! DelayEngine — unified wrapper for all delay styles.
//!
//! Provides a common interface over TapeDelay, CleanDelay, BbdDelay, LoFiDelay,
//! ShimmerDelay, ReverseDelay, and PitchDelay. The chain uses this instead of
//! a concrete delay type, enabling runtime style switching.

use crate::bbd_delay::BbdDelay;
use crate::clean_delay::CleanDelay;
use crate::drum_delay::{DrumDelay, DrumHead, HeadPlayback, GOLDEN_HEADS};
use crate::filter_delay::{FilterDelay, FilterLfoShape, FilterLocation};
use crate::lofi_delay::LoFiDelay;
use crate::modulation::WobbleShape;
use crate::multitap_delay::{MultiTapDelay, Tap, MAX_TAPS};
use crate::oilcan_delay::{OilCanDelay, OilCanHeads};
use crate::pitch_delay::PitchDelay;
use crate::reverse_delay::ReverseDelay;
use crate::rhythm_delay::RhythmDelay;
use crate::shimmer_delay::ShimmerDelay;
use crate::spectral_delay::{DensityMode, GrainDirection, GrainShape, SpectralDelay};
use crate::bbd_delay::BbdVoice;
use crate::tape_delay::{SaturationType, TapeDelay, TapeSpeed, TapeVoice};

/// Available delay styles.
///
/// TimeLine MX machine mapping: `Tape`≈dTape, `Clean`≈Digital,
/// `Bbd`≈dBucket, `LoFi`≈Lo-Fi, `Reverse`≈Reverse, `Pitch`≈Ice,
/// `Rhythm`≈TimeLine-v1 Pattern (fixed patterns), `Drum`≈Drum,
/// `OilCan`≈Oil Can, `MultiTap`≈MultiTap (editable taps),
/// `Spectral`≈Spectral, `Filter`≈Filter (+folded-in Trem).
/// `Shimmer` has no TimeLine counterpart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelayStyle {
    Tape,
    Clean,
    Bbd,
    LoFi,
    Shimmer,
    Reverse,
    Pitch,
    Rhythm,
    Drum,
    OilCan,
    MultiTap,
    Spectral,
    Filter,
}

impl DelayStyle {
    pub const COUNT: usize = 13;

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Tape,
            1 => Self::Clean,
            2 => Self::Bbd,
            3 => Self::LoFi,
            4 => Self::Shimmer,
            5 => Self::Reverse,
            6 => Self::Pitch,
            7 => Self::Rhythm,
            8 => Self::Drum,
            9 => Self::OilCan,
            10 => Self::MultiTap,
            11 => Self::Spectral,
            12 => Self::Filter,
            _ => Self::Tape,
        }
    }

    pub fn to_index(self) -> usize {
        match self {
            Self::Tape => 0,
            Self::Clean => 1,
            Self::Bbd => 2,
            Self::LoFi => 3,
            Self::Shimmer => 4,
            Self::Reverse => 5,
            Self::Pitch => 6,
            Self::Rhythm => 7,
            Self::Drum => 8,
            Self::OilCan => 9,
            Self::MultiTap => 10,
            Self::Spectral => 11,
            Self::Filter => 12,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Tape => "Tape",
            Self::Clean => "Digital",
            Self::Bbd => "BBD",
            Self::LoFi => "Lo-Fi",
            Self::Shimmer => "Shimmer",
            Self::Reverse => "Reverse",
            Self::Pitch => "Pitch",
            Self::Rhythm => "Rhythm",
            Self::Drum => "Drum",
            Self::OilCan => "Oil Can",
            Self::MultiTap => "MultiTap",
            Self::Spectral => "Spectral",
            Self::Filter => "Filter",
        }
    }

    /// Valid delay-time range in ms (TimeLine MX per-machine ranges).
    pub fn time_range_ms(self) -> (f64, f64) {
        match self {
            Self::Bbd => (80.0, 800.0),
            Self::Drum => (200.0, 2000.0),
            Self::OilCan => (200.0, 800.0),
            Self::LoFi => (2.0, 2500.0),
            _ => (60.0, 2500.0),
        }
    }
}

enum EngineInner {
    Tape(TapeDelay),
    Clean(CleanDelay),
    Bbd(BbdDelay),
    LoFi(LoFiDelay),
    Shimmer(ShimmerDelay),
    Reverse(ReverseDelay),
    Pitch(PitchDelay),
    Rhythm(RhythmDelay),
    Drum(DrumDelay),
    OilCan(OilCanDelay),
    MultiTap(MultiTapDelay),
    Spectral(SpectralDelay),
    Filter(FilterDelay),
}

/// Unified delay engine wrapping all delay styles.
///
/// Shared parameters are stored here and synced to the active inner engine
/// on `update()`. Style-specific parameters are set via dedicated methods.
pub struct DelayEngine {
    inner: EngineInner,
    style: DelayStyle,

    // ── Shared parameters (used by all styles) ─────────────────────
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// High-cut filter frequency in Hz (0 = disabled).
    pub hicut_freq: f64,
    /// Low-cut filter frequency in Hz (0 = disabled).
    pub locut_freq: f64,

    // ── Tape-specific parameters ───────────────────────────────────
    /// Saturation drive (0.0–1.0). Tape only.
    pub drive: f64,
    /// Wow depth (0.0–1.0). Tape only.
    pub wow_depth: f64,
    /// Wow rate in Hz. Tape only.
    pub wow_rate: f64,
    /// Wow drift amount (0.0–1.0). Tape only.
    pub wow_drift: f64,
    /// Flutter depth (0.0–1.0). Tape only.
    pub flutter_depth: f64,
    /// Flutter rate in Hz. Tape only.
    pub flutter_rate: f64,
    /// Saturation type. Tape only.
    pub saturation_type: SaturationType,

    // ── Multi-head (Tape only) ─────────────────────────────────────
    pub head1_enabled: bool,
    pub head2_enabled: bool,
    pub head3_enabled: bool,

    // ── dTape parity (Tape only) ───────────────────────────────────
    /// Tape age (0.0 = fresh, 1.0 = old dull tape). Tape only.
    pub tape_age: f64,
    /// Tape crinkle: dropout/warp artifacts (0.0–1.0). Tape only.
    pub crinkle: f64,
    /// Transport speed (Fast = hi-fi, half wow/flutter/crinkle). Tape only.
    pub tape_speed: TapeSpeed,
    /// Low-end contour: in-loop HP 0.0=full lows → 1.0≈400 Hz. Tape only.
    pub low_contour: f64,

    // ── BBD-specific ───────────────────────────────────────────────
    /// LFO modulation depth (0.0–1.0). BBD only.
    pub bbd_mod_depth: f64,
    /// LFO modulation rate in Hz. BBD only.
    pub bbd_mod_rate: f64,
    /// Tone / low-pass cutoff. BBD only.
    pub bbd_tone: f64,
    /// Clock jitter amount (0.0–1.0). BBD only.
    pub bbd_clock_jitter: f64,
    /// Bucket loss: charge-transfer degradation (0.0–1.0). BBD only.
    pub bbd_bucket_loss: f64,
    /// LFO phase offset for stereo spread (set by the chain on the R
    /// engine so modulation widens the image). BBD only.
    pub bbd_phase_offset: f64,

    // ── LoFi-specific ──────────────────────────────────────────────
    /// Bit depth for quantization (4–32). LoFi only.
    pub lofi_bit_depth: f64,
    /// Sample rate divisor (1–64). LoFi only.
    pub lofi_sr_div: f64,
    /// Noise floor injection (0.0–1.0). LoFi only.
    pub lofi_noise: f64,

    // ── Shimmer-specific ───────────────────────────────────────────
    /// Pitch ratio (0.5–4.0). Shimmer only.
    pub shimmer_pitch: f64,
    /// Shimmer mix (0.0–1.0). Shimmer only.
    pub shimmer_mix: f64,

    // ── Reverse-specific ───────────────────────────────────────────
    /// Crossfade overlap (0.0–0.5). Reverse only.
    pub reverse_crossfade: f64,

    // ── Pitch-specific ─────────────────────────────────────────────
    /// Playback speed ratio. Pitch only.
    pub pitch_speed: f64,

    // ── Rhythm-specific ──────────────────────────────────────────
    /// Tap levels for rhythm mode (8 taps at 1x–8x base time).
    pub rhythm_taps: [f64; 8],

    // ── Shared new parameters ────────────────────────────────────
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,
    /// Wobble LFO shape. Tape only.
    pub wow_shape: WobbleShape,
    /// Wobble phase offset (0.0–1.0). Tape only.
    pub wow_phase_offset: f64,

    /// Freeze / infinite hold: feedback pinned to 1.0 with in-loop
    /// filters and tilt bypassed so repeats don't decay. The chain
    /// mutes the loop input while frozen.
    pub frozen: bool,

    /// Machine voice selector (TimeLine MX: dTape MX/Classic, dBucket
    /// MX/Classic, Digital 24/96 / ADM / 12-bit / Classic). Plumbing
    /// slot only — style engines adopt it in their deep passes.
    pub voice: u8,

    // ── Drum-specific ──────────────────────────────────────────────
    /// Playback head config. Drum only.
    pub drum_heads: [DrumHead; 4],
    /// Low-frequency shaping (0.0–1.0). Drum only.
    pub drum_lo_cut: f64,
    /// Motor wobble depth (0.0–1.0). Drum only.
    pub drum_wobble: f64,

    // ── OilCan-specific ────────────────────────────────────────────
    /// Head mode. OilCan only.
    pub oilcan_heads: OilCanHeads,
    /// Wobble depth (0.0–1.0). OilCan only.
    pub oilcan_wobble: f64,
    /// Loop darkness cutoff in Hz. OilCan only.
    pub oilcan_tone: f64,
    /// Rotation-speed randomization (time-domain dirt, 0.0-1.0). OilCan only.
    pub oilcan_grit: f64,

    // ── MultiTap-specific ──────────────────────────────────────────
    /// User tap pattern. MultiTap only.
    pub multitap_taps: [Tap; MAX_TAPS],

    // ── Spectral-specific ──────────────────────────────────────────
    /// Grain density. Spectral only.
    pub spectral_density: DensityMode,
    /// Grain stretch (0.0–1.0). Spectral only.
    pub spectral_stretch: f64,
    /// Octave-up blend (0.0–1.0). Spectral only.
    pub spectral_octave: f64,
    /// Random grain placement across the delay time (0.0–1.0). Spectral only.
    pub spectral_spread: f64,
    /// Grain envelope shape. Spectral only.
    pub spectral_shape: GrainShape,
    /// Grain playback direction. Spectral only.
    pub spectral_direction: GrainDirection,

    // ── Filter-specific ────────────────────────────────────────────
    /// LFO waveform. Filter only.
    pub filter_lfo_shape: FilterLfoShape,
    /// LFO cycles per delay period (1/32–32). Filter only.
    pub filter_lfo_speed: f64,
    /// Sweep depth (0.0–1.0). Filter only.
    pub filter_depth: f64,
    /// Sweep center in Hz. Filter only.
    pub filter_center: f64,
    /// Resonance (0.5–10.0). Filter only.
    pub filter_q: f64,
    /// Pre/post placement. Filter only.
    pub filter_location: FilterLocation,
    /// Tremolo depth on repeats (0.0–1.0). Filter only.
    pub filter_trem_depth: f64,
    /// Tremolo cycles per delay period. Filter only.
    pub filter_trem_speed: f64,
}

impl DelayEngine {
    pub fn new() -> Self {
        Self {
            inner: EngineInner::Tape(TapeDelay::new()),
            style: DelayStyle::Tape,
            time_ms: 250.0,
            feedback: 0.4,
            hicut_freq: 8000.0,
            locut_freq: 0.0,
            drive: 0.0,
            wow_depth: 0.0,
            wow_rate: 0.5,
            wow_drift: 0.3,
            flutter_depth: 0.0,
            flutter_rate: 6.0,
            saturation_type: SaturationType::Tape,
            head1_enabled: true,
            head2_enabled: false,
            head3_enabled: false,
            tape_age: 0.0,
            crinkle: 0.0,
            tape_speed: TapeSpeed::Normal,
            low_contour: 0.0,
            bbd_mod_depth: 0.3,
            bbd_mod_rate: 1.0,
            bbd_tone: 4000.0,
            bbd_clock_jitter: 0.3,
            bbd_bucket_loss: 0.0,
            bbd_phase_offset: 0.0,
            lofi_bit_depth: 12.0,
            lofi_sr_div: 4.0,
            lofi_noise: 0.0,
            shimmer_pitch: 2.0,
            shimmer_mix: 0.5,
            reverse_crossfade: 0.1,
            pitch_speed: 1.0,
            rhythm_taps: [1.0, 0.7, 0.5, 0.35, 0.25, 0.18, 0.12, 0.08],
            decay_tilt: 0.0,
            wow_shape: WobbleShape::Sine,
            wow_phase_offset: 0.0,
            frozen: false,
            voice: 0,
            drum_heads: GOLDEN_HEADS.map(|position| DrumHead {
                playback: HeadPlayback::Full,
                position,
                feedback: position == 1.0,
                pan: 0.0,
            }),
            drum_lo_cut: 0.2,
            drum_wobble: 0.15,
            oilcan_heads: OilCanHeads::Long,
            oilcan_wobble: 0.6,
            oilcan_tone: 2500.0,
            oilcan_grit: 0.1,
            multitap_taps: crate::multitap_delay::TapPreset::Quarters.taps(),
            spectral_density: DensityMode::Synced(1.0 / 8.0),
            spectral_stretch: 0.0,
            spectral_octave: 0.0,
            spectral_spread: 0.0,
            spectral_shape: GrainShape::Soft,
            spectral_direction: GrainDirection::Forward,
            filter_lfo_shape: FilterLfoShape::SinePos,
            filter_lfo_speed: 1.0,
            filter_depth: 0.5,
            filter_center: 1200.0,
            filter_q: 2.0,
            filter_location: FilterLocation::Post,
            filter_trem_depth: 0.0,
            filter_trem_speed: 4.0,
        }
    }

    pub fn style(&self) -> DelayStyle {
        self.style
    }

    /// Switch to a new delay style. Resets internal state.
    pub fn set_style(&mut self, style: DelayStyle) {
        if self.style == style {
            return;
        }
        self.style = style;
        self.inner = match style {
            DelayStyle::Tape => EngineInner::Tape(TapeDelay::new()),
            DelayStyle::Clean => EngineInner::Clean(CleanDelay::new()),
            DelayStyle::Bbd => EngineInner::Bbd(BbdDelay::new()),
            DelayStyle::LoFi => EngineInner::LoFi(LoFiDelay::new()),
            DelayStyle::Shimmer => EngineInner::Shimmer(ShimmerDelay::new()),
            DelayStyle::Reverse => EngineInner::Reverse(ReverseDelay::new()),
            DelayStyle::Pitch => EngineInner::Pitch(PitchDelay::new()),
            DelayStyle::Rhythm => EngineInner::Rhythm(RhythmDelay::new()),
            DelayStyle::Drum => EngineInner::Drum(DrumDelay::new()),
            DelayStyle::OilCan => EngineInner::OilCan(OilCanDelay::new()),
            DelayStyle::MultiTap => EngineInner::MultiTap(MultiTapDelay::new()),
            DelayStyle::Spectral => EngineInner::Spectral(SpectralDelay::new()),
            DelayStyle::Filter => EngineInner::Filter(FilterDelay::new()),
        };
    }

    /// Sync parameters to the active engine and update coefficients.
    pub fn update(&mut self, sample_rate: f64) {
        // Freeze pins feedback at 1.0 and bypasses the in-loop filters
        // and tilt so held repeats do not decay.
        let feedback = if self.frozen { 1.0 } else { self.feedback };
        let hicut_freq = if self.frozen { 0.0 } else { self.hicut_freq };
        let locut_freq = if self.frozen { 0.0 } else { self.locut_freq };
        let decay_tilt = if self.frozen { 0.0 } else { self.decay_tilt };
        // Shadow the raw fields for the arms below.
        let (self_feedback, self_hicut, self_locut, self_tilt) =
            (feedback, hicut_freq, locut_freq, decay_tilt);

        match &mut self.inner {
            EngineInner::Tape(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.hicut_freq = self_hicut;
                d.locut_freq = self_locut;
                d.drive = self.drive;
                d.wow_depth = self.wow_depth;
                d.wow_rate = self.wow_rate;
                d.wow_drift = self.wow_drift;
                d.flutter_depth = self.flutter_depth;
                d.flutter_rate = self.flutter_rate;
                d.head1_enabled = self.head1_enabled;
                d.head2_enabled = self.head2_enabled;
                d.head3_enabled = self.head3_enabled;
                d.saturation_type = self.saturation_type;
                d.decay_tilt = self_tilt;
                d.wow_shape = self.wow_shape;
                d.wow_phase_offset = self.wow_phase_offset;
                d.voice = if self.voice == 1 {
                    TapeVoice::Classic
                } else {
                    TapeVoice::Mx
                };
                d.tape_age = self.tape_age;
                // Freeze also stops the tape damage so held repeats loop
                // cleanly instead of eroding.
                d.crinkle = if self.frozen { 0.0 } else { self.crinkle };
                d.tape_speed = self.tape_speed;
                d.low_contour = if self.frozen { 0.0 } else { self.low_contour };
                d.update(sample_rate);
            }
            EngineInner::Clean(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.hicut_freq = self_hicut;
                d.locut_freq = self_locut;
                d.decay_tilt = self_tilt;
                d.update(sample_rate);
            }
            EngineInner::Bbd(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.mod_depth = self.bbd_mod_depth;
                d.mod_rate = self.bbd_mod_rate;
                // Freeze bypasses the loop tone filter and charge loss so
                // held repeats do not decay.
                d.tone = if self.frozen { 20_000.0 } else { self.bbd_tone };
                d.clock_jitter = self.bbd_clock_jitter;
                d.bucket_loss = if self.frozen { 0.0 } else { self.bbd_bucket_loss };
                d.lfo_phase_offset = self.bbd_phase_offset;
                d.voice = if self.voice == 1 {
                    BbdVoice::Classic
                } else {
                    BbdVoice::Mx
                };
                d.decay_tilt = self_tilt;
                d.update(sample_rate);
            }
            EngineInner::LoFi(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.hicut_freq = self_hicut;
                d.locut_freq = self_locut;
                d.bit_depth = self.lofi_bit_depth;
                d.sample_rate_div = self.lofi_sr_div;
                d.noise = self.lofi_noise;
                d.decay_tilt = self_tilt;
                d.update(sample_rate);
            }
            EngineInner::Shimmer(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.hicut_freq = self_hicut;
                d.pitch_ratio = self.shimmer_pitch;
                d.shimmer_mix = self.shimmer_mix;
                d.decay_tilt = self_tilt;
                d.update(sample_rate);
            }
            EngineInner::Reverse(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.hicut_freq = self_hicut;
                d.grain_crossfade = self.reverse_crossfade;
                d.decay_tilt = self_tilt;
                d.update(sample_rate);
            }
            EngineInner::Pitch(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.speed = self.pitch_speed;
                d.decay_tilt = self_tilt;
                d.update(sample_rate);
            }
            EngineInner::Rhythm(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.hicut_freq = self_hicut;
                d.locut_freq = self_locut;
                d.tap_levels = self.rhythm_taps;
                d.decay_tilt = self_tilt;
                d.update(sample_rate);
            }
            EngineInner::Drum(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.heads = self.drum_heads;
                d.lo_cut = if self.frozen { 0.0 } else { self.drum_lo_cut };
                d.wobble = self.drum_wobble;
                d.decay_tilt = self_tilt;
                d.update(sample_rate);
            }
            EngineInner::OilCan(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.heads = self.oilcan_heads;
                d.wobble = self.oilcan_wobble;
                d.tone_hz = if self.frozen { 8000.0 } else { self.oilcan_tone };
                d.grit = self.oilcan_grit;
                d.decay_tilt = self_tilt;
                d.update(sample_rate);
            }
            EngineInner::MultiTap(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.hicut_freq = self_hicut;
                d.locut_freq = self_locut;
                d.taps = self.multitap_taps;
                d.decay_tilt = self_tilt;
                d.update(sample_rate);
            }
            EngineInner::Spectral(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.hicut_freq = self_hicut;
                d.density = self.spectral_density;
                d.stretch = self.spectral_stretch;
                d.octave = self.spectral_octave;
                d.spread = self.spectral_spread;
                d.shape = self.spectral_shape;
                d.direction = self.spectral_direction;
                d.decay_tilt = self_tilt;
                d.update(sample_rate);
            }
            EngineInner::Filter(d) => {
                d.time_ms = self.time_ms;
                d.feedback = self_feedback;
                d.lfo_shape = self.filter_lfo_shape;
                d.lfo_speed = self.filter_lfo_speed;
                d.depth = self.filter_depth;
                d.center_hz = self.filter_center;
                d.q = self.filter_q;
                d.location = self.filter_location;
                d.trem_depth = self.filter_trem_depth;
                d.trem_speed = self.filter_trem_speed;
                d.decay_tilt = self_tilt;
                d.update(sample_rate);
            }
        }
    }

    /// Process one sample.
    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        match &mut self.inner {
            EngineInner::Tape(d) => d.tick(input, ch),
            EngineInner::Clean(d) => d.tick(input, ch),
            EngineInner::Bbd(d) => d.tick(input, ch),
            EngineInner::LoFi(d) => d.tick(input, ch),
            EngineInner::Shimmer(d) => d.tick(input, ch),
            EngineInner::Reverse(d) => d.tick(input, ch),
            EngineInner::Pitch(d) => d.tick(input),
            EngineInner::Rhythm(d) => d.tick(input, ch),
            EngineInner::Drum(d) => d.tick(input, ch),
            EngineInner::OilCan(d) => d.tick(input, ch),
            EngineInner::MultiTap(d) => d.tick(input, ch),
            EngineInner::Spectral(d) => d.tick(input, ch),
            EngineInner::Filter(d) => d.tick(input, ch),
        }
    }

    /// Process one sample with a per-sample modulated delay time.
    ///
    /// Used by the chain for groove/feel/prime modulation. The engine's
    /// public `time_ms` parameter is not touched; each style's internal
    /// time smoother chases the value passed here.
    pub fn tick_at(&mut self, input: f64, ch: usize, time_ms: f64) -> f64 {
        match &mut self.inner {
            EngineInner::Tape(d) => {
                d.time_ms = time_ms;
                d.tick(input, ch)
            }
            EngineInner::Clean(d) => {
                d.time_ms = time_ms;
                d.tick(input, ch)
            }
            EngineInner::Bbd(d) => {
                d.time_ms = time_ms;
                d.tick(input, ch)
            }
            EngineInner::LoFi(d) => {
                d.time_ms = time_ms;
                d.tick(input, ch)
            }
            EngineInner::Shimmer(d) => {
                d.time_ms = time_ms;
                d.tick(input, ch)
            }
            EngineInner::Reverse(d) => {
                d.time_ms = time_ms;
                d.tick(input, ch)
            }
            EngineInner::Pitch(d) => {
                d.time_ms = time_ms;
                d.tick(input)
            }
            EngineInner::Rhythm(d) => {
                d.time_ms = time_ms;
                d.tick(input, ch)
            }
            EngineInner::Drum(d) => {
                d.time_ms = time_ms.clamp(DrumDelay::MIN_TIME_MS, DrumDelay::MAX_TIME_MS);
                d.tick(input, ch)
            }
            EngineInner::OilCan(d) => {
                d.time_ms = time_ms.clamp(OilCanDelay::MIN_TIME_MS, OilCanDelay::MAX_TIME_MS);
                d.tick(input, ch)
            }
            EngineInner::MultiTap(d) => {
                d.time_ms = time_ms;
                d.tick(input, ch)
            }
            EngineInner::Spectral(d) => {
                d.time_ms = time_ms;
                d.tick(input, ch)
            }
            EngineInner::Filter(d) => {
                d.time_ms = time_ms;
                d.tick(input, ch)
            }
        }
    }

    /// Get the last feedback sample for ping-pong cross-feeding.
    pub fn last_feedback(&self) -> f64 {
        match &self.inner {
            EngineInner::Tape(d) => d.last_feedback(),
            EngineInner::Clean(d) => d.last_feedback(),
            EngineInner::Bbd(d) => d.last_feedback(),
            EngineInner::LoFi(d) => d.last_feedback(),
            EngineInner::Shimmer(d) => d.last_feedback(),
            EngineInner::Reverse(d) => d.last_feedback(),
            EngineInner::Pitch(d) => d.last_feedback(),
            EngineInner::Rhythm(d) => d.last_feedback(),
            EngineInner::Drum(d) => d.last_feedback(),
            EngineInner::OilCan(d) => d.last_feedback(),
            EngineInner::MultiTap(d) => d.last_feedback(),
            EngineInner::Spectral(d) => d.last_feedback(),
            EngineInner::Filter(d) => d.last_feedback(),
        }
    }

    pub fn reset(&mut self) {
        match &mut self.inner {
            EngineInner::Tape(d) => d.reset(),
            EngineInner::Clean(d) => d.reset(),
            EngineInner::Bbd(d) => d.reset(),
            EngineInner::LoFi(d) => d.reset(),
            EngineInner::Shimmer(d) => d.reset(),
            EngineInner::Reverse(d) => d.reset(),
            EngineInner::Pitch(d) => d.reset(),
            EngineInner::Rhythm(d) => d.reset(),
            EngineInner::Drum(d) => d.reset(),
            EngineInner::OilCan(d) => d.reset(),
            EngineInner::MultiTap(d) => d.reset(),
            EngineInner::Spectral(d) => d.reset(),
            EngineInner::Filter(d) => d.reset(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn all_styles_produce_delayed_output() {
        for i in 0..DelayStyle::COUNT {
            let style = DelayStyle::from_index(i);
            let mut e = DelayEngine::new();
            e.set_style(style);
            e.time_ms = 100.0;
            e.feedback = 0.0;
            e.update(SR);

            let mut has_output = false;
            for s in 0..48000 {
                let input = if s < 100 { 0.8 } else { 0.0 };
                let out = e.tick(input, 0);
                if out.abs() > 0.01 {
                    has_output = true;
                }
            }

            assert!(has_output, "{:?} style should produce output", style);
        }
    }

    #[test]
    fn all_styles_no_nan() {
        for i in 0..DelayStyle::COUNT {
            let style = DelayStyle::from_index(i);
            let mut e = DelayEngine::new();
            e.set_style(style);
            e.time_ms = 200.0;
            e.feedback = 0.6;
            e.update(SR);

            for s in 0..96000 {
                let input = (std::f64::consts::TAU * 440.0 * s as f64 / SR).sin() * 0.5;
                let out = e.tick(input, 0);
                assert!(
                    out.is_finite(),
                    "{:?} produced NaN/Inf at sample {s}",
                    style
                );
            }
        }
    }

    #[test]
    fn style_switch_resets() {
        let mut e = DelayEngine::new();
        e.time_ms = 100.0;
        e.feedback = 0.5;
        e.update(SR);

        // Feed some signal in Tape mode
        for s in 0..4800 {
            let input = if s < 100 { 1.0 } else { 0.0 };
            e.tick(input, 0);
        }

        // Switch to Clean — should reset state
        e.set_style(DelayStyle::Clean);
        e.update(SR);

        // First sample should be near zero (no residual from tape engine)
        let out = e.tick(0.0, 0);
        assert!(out.abs() < 0.01, "Style switch should reset: got {out}");
    }

    #[test]
    fn style_roundtrip() {
        for i in 0..DelayStyle::COUNT {
            let style = DelayStyle::from_index(i);
            assert_eq!(style.to_index(), i);
        }
    }
}
