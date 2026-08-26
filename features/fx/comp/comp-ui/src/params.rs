//! nice_plug parameter definitions and shared UI state.
//!
//! Lives in `comp-ui` (not `comp-plugin`) so the [`crate::control_view`]
//! component can render against the param tree without forcing a circular
//! dep — same split as `eq-ui::params`.

use atomic_float::AtomicF32;
use audiocore_core::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Samples kept in each waveform history ring (one per processed block —
/// ~2.7 s at 512-sample blocks / 48 kHz).
pub const WAVE_HISTORY_LEN: usize = 256;

/// Lock-free single-writer history ring for the graph's rolling traces.
///
/// The audio thread [`push`](WaveRing::push)es one value per block (no
/// allocation, relaxed atomics); the UI thread
/// [`snapshot`](WaveRing::snapshot)s the whole window oldest → newest. A
/// torn read across the head is at worst one stale sample — invisible in a
/// scrolling waveform — so no synchronization beyond the atomics is needed.
pub struct WaveRing {
    buf: [AtomicF32; WAVE_HISTORY_LEN],
    /// Next write slot (monotonically increasing, wrapped on use).
    head: AtomicUsize,
}

impl Default for WaveRing {
    fn default() -> Self {
        Self::new()
    }
}

impl WaveRing {
    pub fn new() -> Self {
        Self {
            buf: std::array::from_fn(|_| AtomicF32::new(0.0)),
            head: AtomicUsize::new(0),
        }
    }

    /// Audio thread: append one value. Lock-free, allocation-free.
    pub fn push(&self, v: f32) {
        let i = self.head.load(Ordering::Relaxed);
        self.buf[i % WAVE_HISTORY_LEN].store(v, Ordering::Relaxed);
        self.head.store(i.wrapping_add(1), Ordering::Relaxed);
    }

    /// UI thread: copy the window out, oldest → newest.
    pub fn snapshot(&self) -> Vec<f32> {
        let head = self.head.load(Ordering::Relaxed);
        (0..WAVE_HISTORY_LEN)
            .map(|k| self.buf[(head.wrapping_add(k)) % WAVE_HISTORY_LEN].load(Ordering::Relaxed))
            .collect()
    }
}

/// Audio-thread → UI metering data.
pub struct CompUiState {
    pub params: Arc<CompParams>,
    /// The stage the editor is focused on (`fx.stack.focus`) — written by the
    /// editor, read by the audio thread so the meters and traces below
    /// describe the stage the user is looking at.
    pub focused_stage: AtomicUsize,
    /// Current gain reduction in dB (positive = reducing).
    pub gain_reduction_db: AtomicF32,
    pub input_peak_db: AtomicF32,
    pub output_peak_db: AtomicF32,
    pub sample_rate: AtomicF32,
    /// Per-block input peaks (linear 0..1) for the graph's waveform fill.
    pub input_wave: WaveRing,
    /// Per-block gain reduction (dB, positive) for the graph's GR overlay.
    pub gr_wave: WaveRing,
}

impl CompUiState {
    pub fn new(params: Arc<CompParams>) -> Self {
        Self {
            params,
            focused_stage: AtomicUsize::new(0),
            gain_reduction_db: AtomicF32::new(0.0),
            input_peak_db: AtomicF32::new(-100.0),
            output_peak_db: AtomicF32::new(-100.0),
            sample_rate: AtomicF32::new(48_000.0),
            input_wave: WaveRing::new(),
            gr_wave: WaveRing::new(),
        }
    }
}

/// Compression style — the detector/ballistics model. Mirrors
/// `comp_dsp::CompressionStyle` ids 0..=3 (`Reserved` is not exposed).
pub const STYLE_LABELS: &[&str] = &["Clean", "FET", "VCA", "Opto"];

/// Character (drive) waveshaper, mirroring `ProC3Compressor::drive_transfer`'s
/// `character_mode` dispatch: 0 tanh, 1 atan, 2 x/(1+|x|), 3 HF-only,
/// 4 cubic, 5 hard clip, 6 asymmetric tanh.
pub const CHARACTER_LABELS: &[&str] = &["Tape", "Tube", "Trans", "Bright", "Cubic", "Clip", "Asym"];

/// Profile names in `comp_profiles::all_profiles()` order — the order the
/// `profile` parameter's values are in, so **append only**.
pub const PROFILE_LABELS: &[&str] = &[
    "Control",
    "1176",
    "LA-2A",
    "CL 1B",
    "Fairchild 670",
    "Manley Vari-Mu",
    "SSL Bus",
    "dbx 160",
    "Distressor",
    "1176 Silver",
    "1176 LN",
];

/// How many stages the compressor stack can hold (`fx.stack.limits`).
pub const MAX_STAGES: usize = 8;

/// One stage's complete parameter set (`fx.stack.model`): a full compressor
/// — profile, classic surface, extended surface — plus its place in the
/// stack. [`CompParams`] holds [`MAX_STAGES`] of these; stage 1 is declared
/// bare (`#[nested]`) so its ids are exactly the pre-stack plugin's and a
/// saved single-stage session loads into it bit-for-bit
/// (`fx.stack.params`); stages 2+ get `s{n}_` id prefixes.
///
/// The first eight ids (`threshold`…`link`) are the original classic set and
/// **must keep their order and ids** — hosts persist VST3 state by index.
/// Everything after them is the engine's extended surface, appended: style /
/// character + drive, input gain + auto makeup, detector shaping (RMS blend,
/// feedback, hold, lookahead, inertia), sidechain EQ, range, the expander and
/// upward-compression stages, and the soft ceiling. The multiband stage is
/// still not exposed (it wants its own crossover UI). The stack-placement
/// params (`use`/`on`/`lane`) are appended after those.
#[derive(Params)]
pub struct CompStageParams {
    /// Level above which compression starts.
    #[id = "threshold"]
    pub threshold_db: FloatParam,
    /// Compression ratio (1:1 = off).
    #[id = "ratio"]
    pub ratio: FloatParam,
    /// Attack time.
    #[id = "attack"]
    pub attack_ms: FloatParam,
    /// Release time.
    #[id = "release"]
    pub release_ms: FloatParam,
    /// Soft-knee width around the threshold (0 = hard knee).
    #[id = "knee"]
    pub knee_db: FloatParam,
    /// Makeup (output) gain applied after compression.
    #[id = "makeup"]
    pub makeup_db: FloatParam,
    /// Parallel (dry/wet) mix — the engine's `fold` parameter.
    #[id = "mix"]
    pub mix: FloatParam,
    /// Stereo detector link (1 = fully linked max of both channels).
    #[id = "link"]
    pub stereo_link: FloatParam,

    // ── Extended surface (appended — never reorder the eight above) ──────
    /// Detector/ballistics model — see [`STYLE_LABELS`].
    #[id = "style"]
    pub style: IntParam,
    /// Waveshaper used by the drive stage — see [`CHARACTER_LABELS`].
    #[id = "charmode"]
    pub character_mode: IntParam,
    /// Saturation amount feeding the character waveshaper (0 = bypassed).
    #[id = "drive"]
    pub drive: FloatParam,
    /// Trim applied before detection and compression.
    #[id = "ingain"]
    pub input_gain_db: FloatParam,
    /// Compensate the makeup gain automatically from threshold + ratio.
    #[id = "automake"]
    pub auto_makeup: BoolParam,
    /// Detector blend: 0 = pure peak, 1 = pure RMS.
    #[id = "rmsmix"]
    pub detector_rms_mix: FloatParam,
    /// Feedback detection blend (0 = feedforward, 1 = feedback/vintage).
    #[id = "feedback"]
    pub feedback: FloatParam,
    /// Freeze release for this long after gain reduction deepens.
    #[id = "hold"]
    pub hold_ms: FloatParam,
    /// Lookahead delay — buys the detector time at the cost of latency.
    #[id = "lookahead"]
    pub lookahead_ms: FloatParam,
    /// Program-dependent ballistics amount (0 = manual attack/release only).
    #[id = "inertia"]
    pub inertia: FloatParam,
    /// How slowly the inertia estimator decays back toward the manual times.
    #[id = "inertiadecay"]
    pub inertia_decay: FloatParam,
    /// Sidechain high-pass; at or below 20 Hz the filter is bypassed.
    #[id = "schp"]
    pub sidechain_freq: FloatParam,
    /// Sidechain low-pass; at or below 20 Hz the filter is bypassed.
    #[id = "sclp"]
    pub sidechain_lowpass_freq: FloatParam,
    /// Maximum gain reduction the curve may apply.
    #[id = "range"]
    pub range_db: FloatParam,
    /// Downward expander threshold (below this, the expander opens up).
    #[id = "expthresh"]
    pub expander_threshold_db: FloatParam,
    /// Downward expander ratio (1 = off).
    #[id = "expratio"]
    pub expander_ratio: FloatParam,
    /// Upward compression threshold (below this, quiet material is lifted).
    #[id = "upthresh"]
    pub upward_threshold_db: FloatParam,
    /// Upward compression ratio (1 = off).
    #[id = "upratio"]
    pub upward_ratio: FloatParam,
    /// Soft output ceiling (tanh saturation); 0 = off.
    #[id = "ceiling"]
    pub ceiling: FloatParam,
    /// Hardware profile selection — see [`PROFILE_LABELS`]. Purely a UI
    /// concern (which face is drawn); the DSP reads the params above.
    ///
    /// This is an *index*, because a host parameter has to be a number, and it
    /// is the automatable face switch. What it is **not** is how the choice is
    /// persisted — see [`CompParams::profile_id`].
    #[id = "profile"]
    pub profile: IntParam,

    /// The selected profile's id, persisted alongside the index.
    ///
    /// A parameter's stored value is normalized, so an index-valued parameter
    /// is only stable while the list length is: add a tenth profile and a
    /// session that saved the ninth reloads pointing at something else — the
    /// normalized value round-trips through a different denominator. Names do
    /// not have that problem, so the id is what a session actually restores
    /// from, and the index is reconciled to it on load.
    ///
    /// Empty means "written before ids existed"; then the index stands, which
    /// is the best that can be done for those sessions.
    #[persist = "profile_id"]
    pub profile_id: parking_lot::RwLock<String>,

    /// Position of the active profile's first compound ("macro") control.
    ///
    /// A hardware macro — LA-2A PEAK REDUCTION, 1176 INPUT — writes several
    /// engine params at once through `ParamMapping::Compound`, so its own
    /// position cannot be recovered from any single one of them. It is stored
    /// here instead, which also makes the knob the user actually turns the
    /// thing the host automates and the session reloads.
    ///
    /// Slots are assigned by the order compound controls appear in the active
    /// profile, so the same slot means PEAK REDUCTION under LA-2A and INPUT
    /// under the 1176 — the faces are mutually exclusive, and switching profile
    /// is already a change of instrument.
    #[id = "macro1"]
    pub macro1: FloatParam,
    /// Second macro slot — see [`CompParams::macro1`]. Unused by the current
    /// four profiles (each has exactly one compound control); present so a
    /// profile can add one without a state-breaking param insert.
    #[id = "macro2"]
    pub macro2: FloatParam,

    /// The stage's sidecar: a 6-band EQ on the DETECTOR key
    /// (`fx.embed-eq.one-surface`) — what this compressor listens to, not
    /// what it outputs. Ids `scshape_1`…`scq_6` (stage-prefixed for stages
    /// 2+).
    #[nested(array, group = "Sidechain EQ")]
    pub sc_eq: [ScBandParams; SC_EQ_BANDS],

    // ── Stack placement (appended — `fx.stack.params`) ────────────────────
    /// Whether this stage is part of the stack at all. Stage 1 defaults on
    /// (the plain plugin); the rest off until shift-click adds them
    /// (`fx.stack.add`).
    #[id = "use"]
    pub in_use: BoolParam,
    /// Stage bypass — present in the topology, crossfaded to identity.
    #[id = "on"]
    pub stage_on: BoolParam,
    /// Which parallel lane this stage feeds (`fx.stack.topology`). Within a
    /// lane, stages run serially in stage-number order.
    #[id = "lane"]
    pub lane: IntParam,
}

impl CompStageParams {
    /// The profile index a loaded session should be showing.
    ///
    /// The persisted id wins when it names a profile we still have: it survives
    /// the list growing, being reordered, or a profile being removed. Falls
    /// back to the index for sessions saved before ids, and for an id this
    /// build does not know (a project from a newer version).
    pub fn resolved_profile_index(&self) -> usize {
        let id = self.profile_id.read();
        comp_profiles::profile_index(&id).unwrap_or_else(|| self.profile.value().max(0) as usize)
    }

    /// Record the id for `index` — call this wherever the profile changes, so
    /// what gets saved is the name and not just the number.
    pub fn store_profile_id(&self, index: usize) {
        let id = comp_profiles::all_profiles()
            .get(index)
            .map(|p| p.id())
            .unwrap_or("control");
        *self.profile_id.write() = id.to_string();
    }
}

/// The macro slots, in assignment order — index N backs the Nth compound
/// control of the active profile.
impl CompStageParams {
    pub fn macro_slot(&self, index: usize) -> Option<&FloatParam> {
        match index {
            0 => Some(&self.macro1),
            1 => Some(&self.macro2),
            _ => None,
        }
    }
}

/// Number of sidechain EQ bands — must match `comp_dsp::chain::SC_EQ_BANDS`
/// (pinned by a test in comp-plugin, which links both).
pub const SC_EQ_BANDS: usize = 6;

/// One sidechain EQ band (`fx.embed-eq.band-params`).
#[derive(Params)]
pub struct ScBandParams {
    /// 0 Bell, 1 Low Shelf, 2 High Shelf, 3 Low Cut, 4 High Cut.
    #[id = "scshape"]
    pub shape: IntParam,
    #[id = "scfreq"]
    pub freq_hz: FloatParam,
    #[id = "scgain"]
    pub gain_db: FloatParam,
    #[id = "scq"]
    pub q: FloatParam,
}

pub const SC_SHAPE_LABELS: &[&str] = &["Bell", "Low Shelf", "High Shelf", "Low Cut", "High Cut"];

/// Default sidechain band frequencies — a useful spread, idle at 0 dB.
pub const SC_DEFAULT_FREQS: [f32; SC_EQ_BANDS] = [80.0, 250.0, 700.0, 1_800.0, 4_500.0, 10_000.0];

impl ScBandParams {
    fn new(default_freq: f32) -> Self {
        Self {
            shape: IntParam::new("SC Shape", 0, IntRange::Linear { min: 0, max: 4 })
                .with_value_to_string(label_formatter(SC_SHAPE_LABELS)),
            freq_hz: FloatParam::new(
                "SC Freq",
                default_freq,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            gain_db: FloatParam::new(
                "SC Gain",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            q: FloatParam::new(
                "SC Q",
                0.707,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 18.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
        }
    }
}

/// One parallel lane's mix controls (`fx.stack.sum`). Nested as an array,
/// so the ids come out `lgain_1..lgain_8` etc.
#[derive(Params)]
pub struct LaneParams {
    #[id = "lgain"]
    pub gain_db: FloatParam,
    #[id = "lmute"]
    pub mute: BoolParam,
    #[id = "lsolo"]
    pub solo: BoolParam,
}

impl Default for LaneParams {
    fn default() -> Self {
        Self {
            gain_db: FloatParam::new(
                "Lane Gain",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            mute: BoolParam::new("Lane Mute", false),
            solo: BoolParam::new("Lane Solo", false),
        }
    }
}

/// The full parameter tree: the stage pool, the lanes, and the stack
/// globals (`fx.stack.params`). Stage 1 is nested bare so its ids are the
/// pre-stack plugin's, unchanged; stages 2–8 are prefixed `s2_`…`s8_`.
#[derive(Params)]
pub struct CompParams {
    #[nested]
    pub stage1: CompStageParams,
    #[nested(id_prefix = "s2", group = "Stage 2")]
    pub stage2: CompStageParams,
    #[nested(id_prefix = "s3", group = "Stage 3")]
    pub stage3: CompStageParams,
    #[nested(id_prefix = "s4", group = "Stage 4")]
    pub stage4: CompStageParams,
    #[nested(id_prefix = "s5", group = "Stage 5")]
    pub stage5: CompStageParams,
    #[nested(id_prefix = "s6", group = "Stage 6")]
    pub stage6: CompStageParams,
    #[nested(id_prefix = "s7", group = "Stage 7")]
    pub stage7: CompStageParams,
    #[nested(id_prefix = "s8", group = "Stage 8")]
    pub stage8: CompStageParams,

    /// Per-lane gain / mute / solo, ids `lgain_1`…`lsolo_8`.
    #[nested(array, group = "Lane")]
    pub lanes: [LaneParams; MAX_STAGES],

    /// Parallel sum law: 0 = coherent (1/N), 1 = power (1/√N), 2 = raw
    /// (`fx.stack.sum`).
    #[id = "summode"]
    pub sum_mode: IntParam,
    /// Stack output trim.
    #[id = "stktrim"]
    pub output_trim_db: FloatParam,

    /// The editor's form factor — Responsive, a rack size, a 500-series
    /// module. Persisted by id for the same reason the profile is.
    #[persist = "editor_form"]
    pub editor_form: parking_lot::RwLock<String>,

    /// Which stage the editor is focused on (`fx.stack.focus`) — UI state,
    /// persisted so the editor reopens where it was left. 1-based, as a
    /// string for persist-plumbing simplicity.
    #[persist = "focused_stage"]
    pub focused_stage: parking_lot::RwLock<String>,
}

impl CompParams {
    /// Stage `i` (0-based). Panics past [`MAX_STAGES`].
    pub fn stage(&self, i: usize) -> &CompStageParams {
        match i {
            0 => &self.stage1,
            1 => &self.stage2,
            2 => &self.stage3,
            3 => &self.stage4,
            4 => &self.stage5,
            5 => &self.stage6,
            6 => &self.stage7,
            7 => &self.stage8,
            _ => panic!("stage {i} out of range"),
        }
    }

    /// All stages, in stack order.
    pub fn stages(&self) -> impl Iterator<Item = &CompStageParams> {
        (0..MAX_STAGES).map(|i| self.stage(i))
    }

    /// Indices of the stages currently in the stack.
    pub fn stages_in_use(&self) -> Vec<usize> {
        (0..MAX_STAGES)
            .filter(|&i| self.stage(i).in_use.value())
            .collect()
    }

    /// The editor form a loaded session should open at. An unknown or missing
    /// id means Responsive, which is the size the face asks for anyway.
    pub fn resolved_editor_form(&self) -> fts_audio_ui::EditorForm {
        fts_audio_ui::EditorForm::from_id(&self.editor_form.read()).unwrap_or_default()
    }

    pub fn store_editor_form(&self, form: fts_audio_ui::EditorForm) {
        *self.editor_form.write() = form.id().to_string();
    }

    /// The focused stage (0-based), clamped to a stage that is in use.
    pub fn resolved_focused_stage(&self) -> usize {
        let raw: usize = self.focused_stage.read().parse().unwrap_or(0);
        let idx = raw.min(MAX_STAGES - 1);
        if self.stage(idx).in_use.value() {
            idx
        } else {
            self.stages_in_use().first().copied().unwrap_or(0)
        }
    }

    pub fn store_focused_stage(&self, idx: usize) {
        *self.focused_stage.write() = idx.min(MAX_STAGES - 1).to_string();
    }

    /// First pool slot not in the stack, if any (`fx.stack.add`).
    pub fn first_free_stage(&self) -> Option<usize> {
        (0..MAX_STAGES).find(|&i| !self.stage(i).in_use.value())
    }

    /// First lane index no in-use stage feeds (`fx.stack.add`, the
    /// Ctrl+Shift-click target).
    pub fn first_free_lane(&self) -> Option<usize> {
        (0..MAX_STAGES).find(|&l| {
            !self
                .stages()
                .any(|s| s.in_use.value() && s.lane.value().max(0) as usize == l)
        })
    }
}

impl Default for CompParams {
    fn default() -> Self {
        Self {
            stage1: CompStageParams::new(true),
            stage2: CompStageParams::new(false),
            stage3: CompStageParams::new(false),
            stage4: CompStageParams::new(false),
            stage5: CompStageParams::new(false),
            stage6: CompStageParams::new(false),
            stage7: CompStageParams::new(false),
            stage8: CompStageParams::new(false),
            lanes: std::array::from_fn(|_| LaneParams::default()),
            sum_mode: IntParam::new("Sum Mode", 0, IntRange::Linear { min: 0, max: 2 })
                .with_value_to_string(label_formatter(SUM_MODE_LABELS)),
            output_trim_db: FloatParam::new(
                "Stack Trim",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            editor_form: parking_lot::RwLock::new(String::new()),
            focused_stage: parking_lot::RwLock::new(String::new()),
        }
    }
}

/// Sum-law labels, in `sum_mode` value order.
pub const SUM_MODE_LABELS: &[&str] = &["1/N", "Power", "Raw"];

impl Default for CompStageParams {
    fn default() -> Self {
        Self::new(true)
    }
}

impl CompStageParams {
    /// A stage's params. `first` = stage 1, which starts in the stack (the
    /// plain plugin); the rest start unused until added.
    pub fn new(first: bool) -> Self {
        Self {
            sc_eq: std::array::from_fn(|i| ScBandParams::new(SC_DEFAULT_FREQS[i])),
            in_use: BoolParam::new("In Stack", first),
            stage_on: BoolParam::new("Stage On", true),
            lane: IntParam::new(
                "Lane",
                0,
                IntRange::Linear {
                    min: 0,
                    max: MAX_STAGES as i32 - 1,
                },
            ),
            threshold_db: FloatParam::new(
                "Threshold",
                -20.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            ratio: FloatParam::new(
                "Ratio",
                4.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(":1")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            attack_ms: FloatParam::new(
                "Attack",
                3.0,
                FloatRange::Skewed {
                    min: 0.005,
                    max: 300.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            release_ms: FloatParam::new(
                "Release",
                100.0,
                FloatRange::Skewed {
                    min: 10.0,
                    max: 3000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            knee_db: FloatParam::new(
                "Knee",
                6.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            makeup_db: FloatParam::new(
                "Makeup",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            stereo_link: FloatParam::new(
                "Stereo Link",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),

            // ── Extended surface ────────────────────────────────────────
            style: IntParam::new("Style", 0, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(label_formatter(STYLE_LABELS)),
            character_mode: IntParam::new("Character", 0, IntRange::Linear { min: 0, max: 6 })
                .with_value_to_string(label_formatter(CHARACTER_LABELS)),
            drive: FloatParam::new("Drive", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            input_gain_db: FloatParam::new(
                "Input",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 24.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            auto_makeup: BoolParam::new("Auto Makeup", false),
            detector_rms_mix: FloatParam::new(
                "Peak / RMS",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),
            feedback: FloatParam::new("Feedback", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            hold_ms: FloatParam::new(
                "Hold",
                0.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 500.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            lookahead_ms: FloatParam::new(
                "Lookahead",
                0.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            inertia: FloatParam::new("Inertia", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            inertia_decay: FloatParam::new(
                "Inertia Decay",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 0.999,
                },
            )
            .with_unit("%")
            .with_value_to_string(formatters::v2s_f32_percentage(0)),
            // 20 Hz is the engine's bypass floor for both sidechain filters,
            // so the defaults sit exactly on "off".
            sidechain_freq: FloatParam::new(
                "SC High-Pass",
                20.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 2000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            sidechain_lowpass_freq: FloatParam::new(
                "SC Low-Pass",
                20.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            range_db: FloatParam::new(
                "Range",
                60.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 60.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            expander_threshold_db: FloatParam::new(
                "Exp Threshold",
                -80.0,
                FloatRange::Linear {
                    min: -80.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            expander_ratio: FloatParam::new(
                "Exp Ratio",
                1.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 8.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(":1")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            upward_threshold_db: FloatParam::new(
                "Up Threshold",
                -60.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            upward_ratio: FloatParam::new(
                "Up Ratio",
                1.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 4.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(":1")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            ceiling: FloatParam::new("Ceiling", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            profile: IntParam::new(
                "Profile",
                0,
                IntRange::Linear {
                    min: 0,
                    max: PROFILE_LABELS.len() as i32 - 1,
                },
            )
            .with_value_to_string(label_formatter(PROFILE_LABELS)),
            profile_id: parking_lot::RwLock::new(String::new()),
            macro1: macro_slot_param("Macro 1"),
            macro2: macro_slot_param("Macro 2"),
        }
    }
}

/// A macro slot: a plain 0..1 position with a percentage readout. The meaning
/// of the number is the active profile's business — see
/// [`CompParams::macro1`].
fn macro_slot_param(name: &str) -> FloatParam {
    FloatParam::new(name, 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_unit("%")
        .with_value_to_string(formatters::v2s_f32_percentage(0))
}

/// `with_value_to_string` helper for the discrete params: render the label at
/// the parameter's integer value instead of the bare number.
fn label_formatter(labels: &'static [&'static str]) -> Arc<dyn Fn(i32) -> String + Send + Sync> {
    Arc::new(move |v| {
        labels
            .get(v.max(0) as usize)
            .map(|s| s.to_string())
            .unwrap_or_else(|| v.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_restores_from_the_profile_id_not_the_index() {
        let params = CompParams::default();
        params
            .stage1
            .store_profile_id(comp_profiles::profile_index("dbx160").unwrap());
        // Whatever the index parameter happens to hold — a stale number from a
        // shorter list, a value the host round-tripped through a different
        // denominator — the id decides.
        assert_eq!(
            params.stage1.resolved_profile_index(),
            comp_profiles::profile_index("dbx160").unwrap()
        );
    }

    #[test]
    fn an_unknown_id_falls_back_to_the_index_rather_than_guessing() {
        let params = CompParams::default();
        // A project saved by a newer build naming a profile we do not have.
        *params.stage1.profile_id.write() = "some_future_comp".to_string();
        assert_eq!(params.stage1.resolved_profile_index(), 0);
    }

    #[test]
    fn a_session_saved_before_ids_still_resolves() {
        let params = CompParams::default();
        assert!(params.stage1.profile_id.read().is_empty());
        assert_eq!(params.stage1.resolved_profile_index(), 0);
    }

    // r[verify fx.stack.params]
    #[test]
    fn stage_one_keeps_the_pre_stack_ids_and_the_rest_are_prefixed() {
        let params = CompParams::default();
        let map = params.param_map();
        let ids: Vec<&str> = map.iter().map(|(id, _, _)| id.as_str()).collect();
        // Stage 1 = the pre-stack plugin, bit-for-bit: classic eight first,
        // in order, unprefixed.
        assert_eq!(
            &ids[..8],
            &[
                "threshold",
                "ratio",
                "attack",
                "release",
                "knee",
                "makeup",
                "mix",
                "link"
            ],
        );
        // Stage 2+ prefixed.
        assert!(ids.contains(&"s2_threshold"));
        assert!(ids.contains(&"s8_lane"));
        // Lanes as a suffixed array, stack globals appended.
        assert!(ids.contains(&"lgain_1"));
        assert!(ids.contains(&"lsolo_8"));
        assert!(ids.contains(&"summode"));
        assert!(ids.contains(&"stktrim"));
    }

    // r[verify fx.stack.model]
    #[test]
    fn a_default_stack_is_one_stage_on_lane_zero() {
        let params = CompParams::default();
        assert_eq!(params.stages_in_use(), vec![0]);
        assert_eq!(params.resolved_focused_stage(), 0);
        assert_eq!(params.first_free_stage(), Some(1));
        assert_eq!(params.first_free_lane(), Some(1));
    }

    /// The bug this whole mechanism exists for: an index-valued parameter is
    /// stored normalized, so growing the list moves what a saved value means.
    #[test]
    fn an_index_parameter_alone_would_not_have_survived_a_longer_list() {
        let count_then = 8usize;
        let count_now = PROFILE_LABELS.len();
        assert!(count_now > count_then, "this test assumes the list grew");
        let mut drifted = Vec::new();
        for saved in 0..count_then {
            let normalized = saved as f32 / (count_then - 1) as f32;
            let reloaded = (normalized * (count_now - 1) as f32).round() as usize;
            if reloaded != saved {
                drifted.push((saved, reloaded));
            }
        }
        assert!(
            !drifted.is_empty(),
            "expected some indices to move; the id is what makes that harmless"
        );
    }
}
