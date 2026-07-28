//! Parameter schemas for synth-instrument [`BlockType`]s.
//!
//! This module is the **wiring scaffold** for the synth side of Signal —
//! every synthesis / envelope / sequencer block introduced for Spectrasonics
//! parity gets a typed parameter struct here. Storage / serialization
//! follows the same pattern as the rest of `signal-proto`.
//!
//! Runtime DSP lives in a separate crate (`signal-synth-blocks`, TODO).
//! Each block here exposes a `SynthBlockParams::DEFAULT` reference value
//! mined from the empirical corpus analysis (see
//! `docs/content/spectrasonics-corpus-params.md`) so the import pipeline
//! has somewhere to land Spectrasonics XML attributes.
//!
//! # Status
//!
//! All blocks are declared but **none are implemented at the runtime level**.
//! Methods that would compute audio / mod signals call `todo!()`.
//! Implementing a block means:
//!
//! 1. Filling in any missing param fields (start with what the corpus uses).
//! 2. Implementing the runtime trait in `signal-synth-blocks`.
//! 3. Wiring the import pipeline (`signal-import`) for the matching
//!    Spectrasonics tag.
//! 4. Removing the `todo!()` markers in this file's `process_*` stubs.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::block::BlockType;

// ─── Common types ──────────────────────────────────────────────────

/// Parameter range for a continuous knob.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct ParamRange {
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl ParamRange {
    pub const fn new(min: f32, max: f32, default: f32) -> Self {
        Self { min, max, default }
    }
    /// Unipolar `[0, 1]` with a default.
    pub const fn unipolar(default: f32) -> Self {
        Self {
            min: 0.0,
            max: 1.0,
            default,
        }
    }
    /// Bipolar `[-1, 1]` with a default.
    pub const fn bipolar(default: f32) -> Self {
        Self {
            min: -1.0,
            max: 1.0,
            default,
        }
    }
}

/// Trait every synth-block parameter struct implements.
pub trait SynthBlockParams: Sized {
    /// Which [`BlockType`] this parameter set is for.
    const BLOCK_TYPE: BlockType;
    /// Empirically-derived default values.
    fn default_params() -> Self;
}

// ─── Oscillator ────────────────────────────────────────────────────

/// Multi-mode oscillator: VA / FM / AM / additive / hard-sync.
///
/// Maps to Spectrasonics `<OSC>` (~85 attrs in corpus). Wavetable, sample,
/// and granular modes are provided by [`WavetableParams`], [`SamplerParams`],
/// and [`GranularParams`] as separate Block types — this one is the
/// "VA / FM / AM" subset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct OscillatorParams {
    /// Waveform: 0=sine 1=tri 2=saw 3=square 4=pulse … (TODO: enum)
    pub waveform: u8,
    /// Coarse pitch in semitones, relative to incoming MIDI note.
    pub semi: i8,
    /// Fine tune in cents.
    pub tune_cents: f32,
    /// Octave offset.
    pub octave: i8,
    /// Pulse width [0..1] (square / pulse waves only).
    pub pulse_width: f32,
    /// FM modulation depth.
    pub fm_depth: f32,
    /// AM modulation depth.
    pub am_depth: f32,
    /// Hard-sync ratio (0 = off).
    pub hard_sync: f32,
    /// Unison voice count (1 = mono).
    pub unison_count: u8,
    /// Unison detune in cents.
    pub unison_detune: f32,
    /// Unison stereo width [0..1].
    pub unison_width: f32,
    /// Output level.
    pub level: f32,
    /// Pan [-1..1].
    pub pan: f32,
    // TODO: 4 sub-layer mixer (LayerNAct/Mt/Vol from corpus)
    // TODO: am/fm waveform selectors
    // TODO: phase reset on note-on
}

impl SynthBlockParams for OscillatorParams {
    const BLOCK_TYPE: BlockType = BlockType::Oscillator;
    fn default_params() -> Self {
        Self {
            waveform: 2, // saw
            semi: 0,
            tune_cents: 0.0,
            octave: 0,
            pulse_width: 0.5,
            fm_depth: 0.0,
            am_depth: 0.0,
            hard_sync: 0.0,
            unison_count: 1,
            unison_detune: 0.0,
            unison_width: 0.5,
            level: 0.75,
            pan: 0.0,
        }
    }
}

/// Process one block of audio. **Not implemented.**
pub fn process_oscillator(_params: &OscillatorParams, _out: &mut [f32]) {
    todo!("Oscillator runtime — wire signal-synth-blocks crate")
}

// ─── Sampler ───────────────────────────────────────────────────────

/// Multisample player driven by a [`crate`]-level zone map.
///
/// Maps to Spectrasonics `<MULTISAMPLE>` (126K corpus instances) — the OSC
/// in "sample" mode plus the MULTISAMPLE soundsource reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct SamplerParams {
    /// Reference to a `signal-sampler` library spec.
    pub library_ref: String,
    /// Optional patch / soundsource within that library.
    pub soundsource: Option<String>,
    /// Coarse pitch shift in semitones.
    pub semi: i8,
    /// Fine tune in cents.
    pub tune_cents: f32,
    /// Output level.
    pub level: f32,
    /// Pan.
    pub pan: f32,
    // TODO: zone-mode RR cycling (already wired in signal-sampler engine —
    //       expose its config knobs here)
}

impl SynthBlockParams for SamplerParams {
    const BLOCK_TYPE: BlockType = BlockType::Sampler;
    fn default_params() -> Self {
        Self {
            library_ref: String::new(),
            soundsource: None,
            semi: 0,
            tune_cents: 0.0,
            level: 0.75,
            pan: 0.0,
        }
    }
}

pub fn process_sampler(_params: &SamplerParams, _out: &mut [f32]) {
    todo!("Sampler runtime — bridge into signal-sampler engine")
}

// ─── Wavetable ─────────────────────────────────────────────────────

/// Wavetable oscillator. Consumes `WavetableSpec`-described banks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct WavetableParams {
    /// Reference to a wavetable resource (`.stmwf` / Serum-style WAV).
    pub wavetable_ref: String,
    /// Wavetable position [0..1] (selects frame; modulatable).
    pub position: f32,
    /// Coarse pitch.
    pub semi: i8,
    /// Fine tune cents.
    pub tune_cents: f32,
    /// Output level.
    pub level: f32,
    pub pan: f32,
    // TODO: anti-aliasing mode (mip-mapped frames, oversampling, naive)
    // TODO: morph type between adjacent frames (linear / spectral / phase)
}

impl SynthBlockParams for WavetableParams {
    const BLOCK_TYPE: BlockType = BlockType::Wavetable;
    fn default_params() -> Self {
        Self {
            wavetable_ref: String::new(),
            position: 0.0,
            semi: 0,
            tune_cents: 0.0,
            level: 0.75,
            pan: 0.0,
        }
    }
}

pub fn process_wavetable(_params: &WavetableParams, _out: &mut [f32]) {
    todo!("Wavetable runtime — consume signal-sampler::WavetableSpec resources")
}

// ─── Granular ──────────────────────────────────────────────────────

/// Granular synthesizer.
///
/// Maps to Spectrasonics OSC granular fields (`gran*`, `ngrains`, …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct GranularParams {
    pub source_ref: String,
    /// Grain density [0..1].
    pub density: f32,
    /// Grain size in ms.
    pub grain_size_ms: f32,
    /// Position in source [0..1].
    pub position: f32,
    /// Position spread.
    pub position_spread: f32,
    /// Pitch spread in semitones.
    pub pitch_spread: f32,
    /// Number of grains [1..N] (Spectrasonics caps at 6).
    pub grain_count: u8,
}

impl SynthBlockParams for GranularParams {
    const BLOCK_TYPE: BlockType = BlockType::Granular;
    fn default_params() -> Self {
        Self {
            source_ref: String::new(),
            density: 0.5,
            grain_size_ms: 50.0,
            position: 0.0,
            position_spread: 0.0,
            pitch_spread: 0.0,
            grain_count: 4,
        }
    }
}

pub fn process_granular(_params: &GranularParams, _out: &mut [f32]) {
    todo!("Granular runtime — overlap-add scheduler + grain envelope")
}

// ─── FM operator ───────────────────────────────────────────────────

/// One FM operator (sine + envelope + ratio + feedback).
///
/// FM stacks (Yamaha DX-style) are built by chaining several of these
/// at the Module level via the routing graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct FmOperatorParams {
    /// Frequency ratio relative to the carrier.
    pub ratio: f32,
    /// Detune in cents.
    pub tune_cents: f32,
    /// Output level [0..1].
    pub level: f32,
    /// Self-feedback amount [0..1].
    pub feedback: f32,
}

impl SynthBlockParams for FmOperatorParams {
    const BLOCK_TYPE: BlockType = BlockType::FmOperator;
    fn default_params() -> Self {
        Self {
            ratio: 1.0,
            tune_cents: 0.0,
            level: 0.75,
            feedback: 0.0,
        }
    }
}

pub fn process_fm_operator(_params: &FmOperatorParams, _out: &mut [f32]) {
    todo!("FM operator runtime")
}

// ─── Harmonic (additive) ───────────────────────────────────────────

/// Per-harmonic additive synthesizer.
///
/// Maps to Spectrasonics `<HARM>` (143K corpus instances; 16 harmonics each
/// with level / pan / shape / symmetry / sync / mod / tune / waveform).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct HarmonicParams {
    /// Per-harmonic level [0..1] (16 harmonics).
    pub levels: [f32; 16],
    /// Per-harmonic pan [-1..1].
    pub pans: [f32; 16],
    /// Per-harmonic detune cents.
    pub tunes: [f32; 16],
    /// Mix vs. underlying source [0..1].
    pub mix: f32,
    // TODO: per-harmonic shape / symmetry / sync / mod / waveform
}

impl SynthBlockParams for HarmonicParams {
    const BLOCK_TYPE: BlockType = BlockType::Harmonic;
    fn default_params() -> Self {
        Self {
            levels: [0.0; 16],
            pans: [0.0; 16],
            tunes: [0.0; 16],
            mix: 0.5,
        }
    }
}

pub fn process_harmonic(_params: &HarmonicParams, _out: &mut [f32]) {
    todo!("Harmonic / additive runtime — DFT bin generators")
}

// ─── Tonewheel (Hammond/B3) ────────────────────────────────────────

/// Tonewheel organ emulation.
///
/// Maps to Spectrasonics `<TONEWHEEL>` (43K corpus instances). Real Hammond
/// has 91 wheels; expose drawbars + leakage / click / scanner controls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct TonewheelParams {
    /// 9 drawbars 0..8 each [0..8].
    pub drawbars: [u8; 9],
    /// Tonewheel leakage / crosstalk amount [0..1].
    pub leakage: f32,
    /// Key click amount [0..1].
    pub click: f32,
    /// Vibrato / chorus scanner depth [0..1].
    pub scanner_depth: f32,
    /// Percussion enable + harmonic (2nd / 3rd) + decay.
    pub percussion: bool,
    pub percussion_harmonic: u8,
    pub percussion_decay: f32,
}

impl SynthBlockParams for TonewheelParams {
    const BLOCK_TYPE: BlockType = BlockType::Tonewheel;
    fn default_params() -> Self {
        Self {
            drawbars: [8, 8, 8, 0, 0, 0, 0, 0, 0],
            leakage: 0.05,
            click: 0.5,
            scanner_depth: 0.0,
            percussion: false,
            percussion_harmonic: 3,
            percussion_decay: 0.5,
        }
    }
}

pub fn process_tonewheel(_params: &TonewheelParams, _out: &mut [f32]) {
    todo!("Tonewheel runtime — 91-oscillator additive bank with drawbar mixing")
}

// ─── Formant / Vox ─────────────────────────────────────────────────

/// Formant resonator bank for vocal / vox synthesis.
///
/// Maps to Spectrasonics `<VOX>` / `<VOICE>` (Innerspace etc).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct FormantParams {
    /// Vowel position 0..1 (interpolates across A/E/I/O/U formant tables).
    pub vowel: f32,
    /// Per-formant frequency offset.
    pub formant_shift: f32,
    /// Mix vs source [0..1].
    pub mix: f32,
    // TODO: explicit per-formant frequency / bandwidth / amplitude tables
}

impl SynthBlockParams for FormantParams {
    const BLOCK_TYPE: BlockType = BlockType::Formant;
    fn default_params() -> Self {
        Self {
            vowel: 0.0,
            formant_shift: 0.0,
            mix: 0.5,
        }
    }
}

pub fn process_formant(_params: &FormantParams, _out: &mut [f32]) {
    todo!("Formant runtime — bandpass resonator bank")
}

// ─── Dynamic FM Synthesis (DFS) ────────────────────────────────────

/// Dynamic FM Synthesis — Spectrasonics-specific module.
///
/// Maps to `<DFS>` (105K corpus instances). Format and exact algorithm
/// undocumented — RE pending. Listed here as a Block placeholder so the
/// importer has a target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct DfsParams {
    /// Modulation depth [0..1] (placeholder — replace once RE'd).
    pub depth: f32,
    /// Spectral position [0..1].
    pub position: f32,
    /// Mix vs source [0..1].
    pub mix: f32,
    // TODO: full param set after reverse-engineering the <DFS> tag
}

impl SynthBlockParams for DfsParams {
    const BLOCK_TYPE: BlockType = BlockType::Dfs;
    fn default_params() -> Self {
        Self {
            depth: 0.5,
            position: 0.5,
            mix: 0.5,
        }
    }
}

pub fn process_dfs(_params: &DfsParams, _out: &mut [f32]) {
    todo!("DFS runtime — Spectrasonics-specific dynamic FM; see <DFS> tag")
}

// ─── Noise generator ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct NoiseParams {
    /// 0=white 1=pink 2=brown 3=blue 4=violet (TODO: enum).
    pub color: u8,
    pub level: f32,
}

impl SynthBlockParams for NoiseParams {
    const BLOCK_TYPE: BlockType = BlockType::Noise;
    fn default_params() -> Self {
        Self {
            color: 0,
            level: 0.5,
        }
    }
}

pub fn process_noise(_params: &NoiseParams, _out: &mut [f32]) {
    todo!("Noise runtime — colored noise filter chain")
}

// ─── Envelope ──────────────────────────────────────────────────────

/// Standard ADSR + hold envelope.
///
/// Spectrasonics's `<AENV>` / `<FENV>` use a 5-stage attack/hold/decay/sustain/
/// release shape with optional sync, retrigger modes, velocity sensitivity,
/// and curve shaping per segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct EnvelopeParams {
    pub attack_ms: f32,
    pub hold_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
    /// Curve shape per segment, [-1..1] (negative = exponential, positive = log).
    pub attack_curve: f32,
    pub decay_curve: f32,
    pub release_curve: f32,
    pub velocity_sensitivity: f32,
    /// True = sync to host tempo.
    pub sync: bool,
    /// Retrigger mode: 0=monophonic-glide 1=re-attack 2=legato (TODO: enum).
    pub trigger_mode: u8,
}

impl SynthBlockParams for EnvelopeParams {
    const BLOCK_TYPE: BlockType = BlockType::Envelope;
    fn default_params() -> Self {
        Self {
            attack_ms: 5.0,
            hold_ms: 0.0,
            decay_ms: 100.0,
            sustain: 0.7,
            release_ms: 200.0,
            attack_curve: 0.0,
            decay_curve: 0.0,
            release_curve: 0.0,
            velocity_sensitivity: 1.0,
            sync: false,
            trigger_mode: 1,
        }
    }
}

pub fn process_envelope(_params: &EnvelopeParams, _out: &mut [f32]) {
    todo!("Envelope runtime — sample-accurate ADSR with curve shaping")
}

// ─── Multi-segment envelope ────────────────────────────────────────

/// Multi-segment envelope (5+ stages, optional loop region).
///
/// Maps to Spectrasonics `<MOD_ENV2_2>` (5 stages × 8 multipliers + curve
/// shape per segment + loopable region).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct MultisegEnvelopeParams {
    /// Each stage: (level, time_ms, curve).
    pub stages: Vec<MultisegStage>,
    /// Loop start segment index, or None.
    pub loop_start: Option<u8>,
    /// Loop end segment index, or None.
    pub loop_end: Option<u8>,
    pub sync: bool,
    pub velocity_sensitivity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct MultisegStage {
    pub level: f32,
    pub time_ms: f32,
    pub curve: f32,
}

impl SynthBlockParams for MultisegEnvelopeParams {
    const BLOCK_TYPE: BlockType = BlockType::MultisegEnvelope;
    fn default_params() -> Self {
        Self {
            stages: vec![
                MultisegStage {
                    level: 0.0,
                    time_ms: 0.0,
                    curve: 0.0,
                },
                MultisegStage {
                    level: 1.0,
                    time_ms: 5.0,
                    curve: 0.0,
                },
                MultisegStage {
                    level: 0.7,
                    time_ms: 100.0,
                    curve: 0.0,
                },
                MultisegStage {
                    level: 0.7,
                    time_ms: 0.0,
                    curve: 0.0,
                },
                MultisegStage {
                    level: 0.0,
                    time_ms: 200.0,
                    curve: 0.0,
                },
            ],
            loop_start: None,
            loop_end: None,
            sync: false,
            velocity_sensitivity: 1.0,
        }
    }
}

pub fn process_multiseg_envelope(_params: &MultisegEnvelopeParams, _out: &mut [f32]) {
    todo!("Multiseg envelope runtime — n-stage with optional loop region")
}

// ─── LFO ───────────────────────────────────────────────────────────

/// Low-frequency oscillator with sync and randomization.
///
/// Maps to Spectrasonics `<LFO>` (341K corpus instances).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct LfoParams {
    /// 0=sine 1=tri 2=saw 3=square 4=s&h 5=random (TODO: enum).
    pub waveform: u8,
    pub rate_hz: f32,
    pub depth: f32,
    pub phase: f32,
    /// True = sync to tempo (then `rate_hz` is ignored, see `clock_div`).
    pub sync: bool,
    /// Beat division when synced (e.g. 0.25 = 16th note).
    pub clock_div: f32,
    /// Swing amount (when synced).
    pub swing: f32,
    /// Bipolar (true) or unipolar (false) output.
    pub bipolar: bool,
    /// Randomization range [0..1].
    pub random_range: f32,
    /// Reset on note-on.
    pub reset_on_trigger: bool,
}

impl SynthBlockParams for LfoParams {
    const BLOCK_TYPE: BlockType = BlockType::Lfo;
    fn default_params() -> Self {
        Self {
            waveform: 0,
            rate_hz: 4.0,
            depth: 0.5,
            phase: 0.0,
            sync: false,
            clock_div: 0.25,
            swing: 0.0,
            bipolar: true,
            random_range: 0.0,
            reset_on_trigger: true,
        }
    }
}

pub fn process_lfo(_params: &LfoParams, _out: &mut [f32]) {
    todo!("LFO runtime — phase accumulator + waveform LUT + sync clock")
}

// ─── Mod matrix ────────────────────────────────────────────────────

/// Modulation routing matrix.
///
/// Each row maps a source (LFO output, envelope, velocity, CC, macro, …) to
/// a target parameter path with an amount. Maps to Spectrasonics
/// `<MOD_MATRIX>` rows.
///
/// **Note**: Signal already has [`signal_macromod::ModulationRouteSet`] at Layer
/// and Engine levels — this Block is for **per-Module** matrix routing
/// (within a Voice / SubEngine equivalent). Implementation should delegate
/// to or share infrastructure with `macromod`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ModMatrixParams {
    pub rows: Vec<ModMatrixRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ModMatrixRow {
    /// Source identifier (e.g. `"lfo:0"`, `"env:filter"`, `"cc:74"`, `"macro:1"`).
    pub source: String,
    /// Target parameter path (e.g. `"oscillator:0/pulse_width"`).
    pub target: String,
    /// Modulation amount [-1..1].
    pub amount: f32,
    /// Optional damping / smoothing in ms.
    pub damping_ms: f32,
    pub muted: bool,
}

impl SynthBlockParams for ModMatrixParams {
    const BLOCK_TYPE: BlockType = BlockType::ModMatrix;
    fn default_params() -> Self {
        Self { rows: Vec::new() }
    }
}

// ─── Macro knob ────────────────────────────────────────────────────

/// One macro knob (control source).
///
/// Mirrors Spectrasonics `<Custom0..N>`. Layer-level macros already exist
/// in [`signal_macromod::MacroBank`]; this Block lets a Module expose its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct MacroParams {
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl SynthBlockParams for MacroParams {
    const BLOCK_TYPE: BlockType = BlockType::Macro;
    fn default_params() -> Self {
        Self {
            name: String::new(),
            value: 0.0,
            min: 0.0,
            max: 1.0,
            default: 0.0,
        }
    }
}

// ─── Unison ────────────────────────────────────────────────────────

/// Dedicated unison engine (separate from per-OSC unison fields).
///
/// Maps to Spectrasonics `<UNI>` (105K corpus instances).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct UnisonParams {
    /// Number of unison voices [1..16].
    pub voices: u8,
    /// Detune per voice in cents.
    pub detune_cents: f32,
    /// Stereo width [0..1].
    pub width: f32,
    /// Phase randomization [0..1].
    pub phase_random: f32,
    /// Drift / slow-LFO amount [0..1].
    pub drift: f32,
}

impl SynthBlockParams for UnisonParams {
    const BLOCK_TYPE: BlockType = BlockType::Unison;
    fn default_params() -> Self {
        Self {
            voices: 1,
            detune_cents: 0.0,
            width: 0.5,
            phase_random: 0.0,
            drift: 0.0,
        }
    }
}

pub fn process_unison(_params: &UnisonParams, _in: &[f32], _out: &mut [f32]) {
    todo!("Unison runtime — multi-voice spread / detune / drift")
}

// ─── Step sequencer ────────────────────────────────────────────────

/// Step sequencer driving notes or modulating parameters.
///
/// Maps to Spectrasonics `<SLICESEQSTEP>` (1.4M steps in corpus). 33 steps
/// per Module is the empirical max; exposing as Vec lets users go further.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct StepSeqParams {
    pub steps: Vec<StepSeqStep>,
    /// 0=notes 1=mod-source 2=both.
    pub output_mode: u8,
    /// Beat division per step (e.g. 0.25 = 16th note).
    pub clock_div: f32,
    pub swing: f32,
    pub running: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct StepSeqStep {
    /// MIDI note (when output_mode includes notes).
    pub note: u8,
    /// Velocity / mod-amount [0..1].
    pub velocity: f32,
    /// Slice index for sample-slice sequencing (-1 = none).
    pub slice_index: i32,
    /// Step start position in source [0..1].
    pub start: f32,
    /// Step end position in source [0..1].
    pub end: f32,
    /// Channel/voice routing.
    pub channel: i8,
    /// Probability the step fires [0..1].
    pub probability: f32,
}

impl SynthBlockParams for StepSeqParams {
    const BLOCK_TYPE: BlockType = BlockType::StepSeq;
    fn default_params() -> Self {
        Self {
            steps: vec![
                StepSeqStep {
                    note: 60,
                    velocity: 0.7,
                    slice_index: -1,
                    start: 0.0,
                    end: 1.0,
                    channel: -1,
                    probability: 1.0,
                };
                16
            ],
            output_mode: 0,
            clock_div: 0.25,
            swing: 0.0,
            running: false,
        }
    }
}

pub fn process_step_seq(_params: &StepSeqParams, _clock_phase: f32) {
    todo!("Step sequencer runtime — clock-driven step advance, note + slice events")
}

// ─── Arpeggiator ───────────────────────────────────────────────────

/// Arpeggiator with feel / groove patterns.
///
/// Maps to Spectrasonics `<ARP>` + `<ARPSEQ2>` + `<ARPFEELSEQ>` (42K
/// corpus instances).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ArpeggiatorParams {
    pub running: bool,
    /// 0=up 1=down 2=updown 3=downup 4=random 5=as-played (TODO: enum).
    pub mode: u8,
    /// Beat division (e.g. 0.25 = 16th note).
    pub clock_div: f32,
    pub swing: f32,
    /// Octave range [1..8].
    pub octave_range: u8,
    /// Step length [0..1].
    pub gate_length: f32,
    /// Velocity scale [0..1].
    pub velocity_mix: f32,
    /// Optional feel / groove pattern reference.
    pub groove_name: String,
    pub snap_to_grid: bool,
    pub snap_gravity: f32,
}

impl SynthBlockParams for ArpeggiatorParams {
    const BLOCK_TYPE: BlockType = BlockType::Arpeggiator;
    fn default_params() -> Self {
        Self {
            running: false,
            mode: 0,
            clock_div: 0.25,
            swing: 0.0,
            octave_range: 1,
            gate_length: 0.75,
            velocity_mix: 0.5,
            groove_name: String::new(),
            snap_to_grid: false,
            snap_gravity: 1.0,
        }
    }
}

pub fn process_arpeggiator(_params: &ArpeggiatorParams) {
    todo!("Arpeggiator runtime — held-note tracker + groove application")
}

// ─── Waveshaper ────────────────────────────────────────────────────

/// Waveshaper / wavefolder.
///
/// Maps to Spectrasonics `<WAVESHAPER>` (143K corpus instances).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct WaveshaperParams {
    /// 0=tanh 1=hard-clip 2=fold 3=cubic 4=sine (TODO: enum).
    pub shape_type: u8,
    pub drive: f32,
    pub depth: f32,
    pub bias: f32,
    pub gain: f32,
    pub tone: f32,
    pub mix: f32,
}

impl SynthBlockParams for WaveshaperParams {
    const BLOCK_TYPE: BlockType = BlockType::Waveshaper;
    fn default_params() -> Self {
        Self {
            shape_type: 0,
            drive: 0.0,
            depth: 1.0,
            bias: 0.0,
            gain: 1.0,
            tone: 0.5,
            mix: 1.0,
        }
    }
}

pub fn process_waveshaper(_params: &WaveshaperParams, _in: &[f32], _out: &mut [f32]) {
    todo!("Waveshaper runtime — selectable nonlinearity + tone EQ + mix")
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Every synth Block param struct's [`SynthBlockParams::BLOCK_TYPE`]
    /// must round-trip through the canonical [`BlockType`] table.
    #[test]
    fn synth_block_types_align_with_block_type_enum() {
        macro_rules! check {
            ($( $t:ty ),* $(,)?) => {
                $(
                    let bt = <$t>::BLOCK_TYPE;
                    let key = bt.as_str();
                    let parsed = BlockType::from_str(key);
                    assert_eq!(parsed, Some(bt), "round-trip failed for {bt:?}");
                )*
            };
        }
        check!(
            OscillatorParams,
            SamplerParams,
            WavetableParams,
            GranularParams,
            FmOperatorParams,
            HarmonicParams,
            TonewheelParams,
            FormantParams,
            DfsParams,
            NoiseParams,
            EnvelopeParams,
            MultisegEnvelopeParams,
            LfoParams,
            ModMatrixParams,
            MacroParams,
            UnisonParams,
            StepSeqParams,
            ArpeggiatorParams,
            WaveshaperParams,
        );
    }

    /// Every param struct must produce a default value (sanity check the
    /// `default_params()` impls don't panic).
    #[test]
    fn synth_block_defaults_construct() {
        let _ = OscillatorParams::default_params();
        let _ = SamplerParams::default_params();
        let _ = WavetableParams::default_params();
        let _ = GranularParams::default_params();
        let _ = FmOperatorParams::default_params();
        let _ = HarmonicParams::default_params();
        let _ = TonewheelParams::default_params();
        let _ = FormantParams::default_params();
        let _ = DfsParams::default_params();
        let _ = NoiseParams::default_params();
        let _ = EnvelopeParams::default_params();
        let _ = MultisegEnvelopeParams::default_params();
        let _ = LfoParams::default_params();
        let _ = ModMatrixParams::default_params();
        let _ = MacroParams::default_params();
        let _ = UnisonParams::default_params();
        let _ = StepSeqParams::default_params();
        let _ = ArpeggiatorParams::default_params();
        let _ = WaveshaperParams::default_params();
    }
}
