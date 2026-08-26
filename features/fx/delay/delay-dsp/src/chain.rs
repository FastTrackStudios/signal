//! Delay chain — composes delay engines, ducking, and diffusion
//! into a full stereo processor.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::envelope::EnvelopeFollower;
use audiocore_dsp::note_sync::NoteValue;
use audiocore_dsp::smoothing::ParamSmoother;
use audiocore_dsp::{AudioConfig, Processor};

use crate::engine::{DelayEngine, DelayStyle};
use crate::modulation::{Diffuser, DuckingFollower};

// r[impl delay.chain.signal-flow]
/// Stereo mode for the delay chain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StereoMode {
    /// Independent L/R delays.
    Stereo,
    /// Cross-feed: L output feeds R input and vice versa.
    PingPong,
    /// Mono delay duplicated to both channels.
    Mono,
}

/// RE-201–style tape head configuration.
///
/// Controls which of the 3 playback heads are active.
/// Head 1 is at base delay time, Head 2 at 1.94×, Head 3 at 2.85×
/// (matching RE-201 physical head spacing).
///
/// Modes match the Neural DSP Archetype: Mateus Asato Echo module.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeadMode {
    /// Head 3 only — single repetition at longest interval.
    Mode1,
    /// Heads 1 + 3 — dual repetition (short + long).
    Mode2,
    /// Heads 2 + 3 — syncopated dual repetition.
    Mode3,
    /// All three heads — three-repetition pattern.
    Mode4,
}

/// Tempo-sync tap division (TimeLine MX set, plus Free).
///
/// `GoldenRatio` and `SilverRatio` divide the quarter note by φ (≈1.618)
/// and δ (≈2.414) respectively — repeats that never land on the grid,
/// TimeLine MX's signature anti-rhythmic divisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapDivision {
    Quarter,
    DottedEighth,
    Eighth,
    Triplet,
    Sixteenth,
    GoldenRatio,
    SilverRatio,
    /// Ignore tempo; use the engine's `time_ms` directly.
    Free,
}

impl TapDivision {
    pub const COUNT: usize = 8;

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Quarter,
            1 => Self::DottedEighth,
            2 => Self::Eighth,
            3 => Self::Triplet,
            4 => Self::Sixteenth,
            5 => Self::GoldenRatio,
            6 => Self::SilverRatio,
            _ => Self::Free,
        }
    }

    pub fn to_index(self) -> usize {
        match self {
            Self::Quarter => 0,
            Self::DottedEighth => 1,
            Self::Eighth => 2,
            Self::Triplet => 3,
            Self::Sixteenth => 4,
            Self::GoldenRatio => 5,
            Self::SilverRatio => 6,
            Self::Free => 7,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Quarter => "1/4",
            Self::DottedEighth => "1/8.",
            Self::Eighth => "1/8",
            Self::Triplet => "1/8T",
            Self::Sixteenth => "1/16",
            Self::GoldenRatio => "GR",
            Self::SilverRatio => "SR",
            Self::Free => "Free",
        }
    }

    /// Delay time in ms at the given tempo; `None` for `Free`.
    pub fn to_ms(self, bpm: f64) -> Option<f64> {
        let quarter = NoteValue::Quarter.to_ms(bpm);
        match self {
            Self::Quarter => Some(quarter),
            Self::DottedEighth => Some(NoteValue::DottedEighth.to_ms(bpm)),
            Self::Eighth => Some(NoteValue::Eighth.to_ms(bpm)),
            Self::Triplet => Some(NoteValue::TripletEighth.to_ms(bpm)),
            Self::Sixteenth => Some(NoteValue::Sixteenth.to_ms(bpm)),
            Self::GoldenRatio => Some(quarter / 1.618_033_988_749_895),
            Self::SilverRatio => Some(quarter / 2.414_213_562_373_095),
            Self::Free => None,
        }
    }
}

/// Full stereo delay processing chain.
///
/// Signal flow: Input → Swell → InputLevel → Diffusion(loop) → Stereo Routing →
/// Engine → Diffusion(post) → Accent → Duck → RepeatDyn → HighPass →
/// LR Offset → Width → Pan → OutputLevel → Mix
pub struct DelayChain {
    // Delay engines
    pub delay_l: DelayEngine,
    pub delay_r: DelayEngine,

    // Global parameters
    /// Dry/wet mix (0.0 = fully dry, 1.0 = fully wet).
    pub mix: f64,
    /// Stereo mode.
    pub stereo_mode: StereoMode,
    /// Stereo width (0.0 = mono, 1.0 = normal, 2.0 = extra wide).
    pub width: f64,
    /// Tape head configuration (RE-201 style, only applies to Tape style).
    pub head_mode: HeadMode,
    /// Enable diffusion.
    pub diffusion_enabled: bool,
    /// Diffusion size (0.0–1.0).
    pub diffusion_size: f64,
    /// Diffusion smear / feedback (0.0–1.0).
    pub diffusion_smear: f64,
    /// Enable ducking.
    pub ducking_enabled: bool,
    /// Ping-pong feedback (used when stereo_mode == PingPong).
    pub pingpong_feedback: f64,

    // --- New parameters ---
    /// Accent: alternating repeat volume. Bipolar -1.0 to 1.0, 0.0 = off.
    /// Positive emphasizes odd repeats, negative emphasizes even repeats.
    pub accent: f64,

    /// Groove: shuffle/swing. Bipolar -1.0 to 1.0, 0.0 = off.
    /// Negative = shuffle, positive = swing.
    pub groove: f64,

    /// Feel: draggin'/rushin'. Bipolar -1.0 to 1.0, 0.0 = neutral.
    /// Negative = draggin' (adds pre-delay), positive = rushin' (subtracts pre-delay).
    pub feel: f64,
    /// Maximum feel offset in ms.
    pub feel_max_ms: f64,

    /// Prime numbers: anti-resonance toggle.
    /// Multiplies delay time by (1.0 + 0.0013) to prevent comb filtering.
    pub prime_numbers: bool,

    /// L/R offset in ms (0.0–25.0). Delays the R channel for stereo spread.
    pub lr_offset_ms: f64,

    /// Diffusion placement: false = post (default), true = loop (before delay).
    pub diffusion_in_loop: bool,

    /// Input level: scales signal going INTO the delay (0.0–2.0, default 1.0).
    pub input_level: f64,
    /// Output level: scales the wet signal before mixing (0.0–2.0, default 1.0).
    pub output_level: f64,

    // --- TimeLine MX common parameters ---
    /// Host tempo in BPM. `None` = no sync; engines use `time_ms`.
    pub tempo_bpm: Option<f64>,
    /// Tap division for the left delay line (used when tempo is set).
    pub tap_div_l: TapDivision,
    /// Tap division for the right delay line (used when tempo is set).
    pub tap_div_r: TapDivision,
    /// Swell: input-triggered fade-in time in seconds (0 = off, TimeLine
    /// range 0.10–4.0). Mix-dependent semantics (per the MX spec): at
    /// mix < full the envelope rides the WET signal behind the dry; at
    /// full wet it rides the dry signal INTO the delay (volume-pedal
    /// swell emulation).
    pub swell_time_s: f64,
    /// Freeze / infinite hold: repeats hold forever, loop input muted.
    pub freeze: bool,
    /// Repeat dynamics: the tail tapers off faster than exponential.
    /// Runs IN the regeneration loop via the engines' feedback trim, so
    /// the first repeats are untouched and only the recirculation
    /// accelerates its decay (TimeLine: "high-feedback tails cut off
    /// more abruptly").
    pub repeat_dynamics: bool,
    /// Post-delay wet high-pass in Hz (0 = off, TimeLine range 20–900).
    pub high_pass_hz: f64,
    /// Left line output pan (-1.0 hard left … 1.0 hard right).
    /// Default -1.0 preserves the classic hard-L routing.
    pub pan_l: f64,
    /// Right line output pan. Default 1.0 (hard right).
    pub pan_r: f64,
    /// Duck GATE mode (TimeLine "Ducking Feedback"): while playing, the
    /// REGENERATION is muted via the engines' feedback trim — repeats
    /// recirculated during playing die, so after you stop only the last
    /// note repeats. The normal sensitivity-controlled wet duck still
    /// applies on top (matching the hardware, where GATE adds feedback
    /// ducking to the standard duck).
    pub duck_gate: bool,

    // Internal
    diffuser_l: Diffuser,
    diffuser_r: Diffuser,
    pub ducker: DuckingFollower,
    sample_rate: f64,

    // Accent internal state
    accent_phase_l: f64,
    accent_phase_r: f64,
    accent_flip_l: bool,
    accent_flip_r: bool,

    // Groove internal state
    groove_phase_l: f64,
    groove_phase_r: f64,

    // LR offset internal state
    lr_offset_delay: DelayLine,
    lr_offset_smoother: ParamSmoother,

    // Per-sample parameter smoothing (kills zipper noise on automation)
    mix_smoother: ParamSmoother,
    width_smoother: ParamSmoother,
    input_level_smoother: ParamSmoother,
    output_level_smoother: ParamSmoother,
    pingpong_fb_smoother: ParamSmoother,

    // The ping-pong cross-feed recirculates through both engines — strip
    // DC inside that outer loop too.
    pingpong_dc_l: DcBlocker,
    pingpong_dc_r: DcBlocker,

    // --- TimeLine MX common-param internals ---
    /// Tempo-derived effective delay times (ms), computed in `update()`.
    eff_time_l: f64,
    eff_time_r: f64,
    /// Freeze engage/disengage ramp (0 = live, 1 = frozen), ~30 ms.
    freeze_smoother: ParamSmoother,
    /// Swell trigger detector on the dry input.
    swell_env: EnvelopeFollower,
    swell_gate_open: bool,
    /// Swell ramp 0..1, restarted on each detected note onset.
    swell_ramp: f64,
    /// Repeat-dynamics envelope on the wet signal.
    repeat_dyn_env: EnvelopeFollower,
    /// Post-delay wet high-pass.
    hp_l: Biquad,
    hp_r: Biquad,
    pan_l_smoother: ParamSmoother,
    pan_r_smoother: ParamSmoother,
}

impl Default for DelayChain {
    fn default() -> Self {
        Self::new()
    }
}

impl DelayChain {
    pub fn new() -> Self {
        let mut lr_smoother = ParamSmoother::new(8.0);
        lr_smoother.set_time_ms(5.0, 48000.0);
        lr_smoother.set_epsilon(0.01);

        Self {
            delay_l: DelayEngine::new(),
            delay_r: DelayEngine::new(),
            mix: 0.5,
            stereo_mode: StereoMode::Stereo,
            width: 1.0,
            head_mode: HeadMode::Mode1,
            diffusion_enabled: false,
            diffusion_size: 0.5,
            diffusion_smear: 0.5,
            ducking_enabled: false,
            pingpong_feedback: 0.5,

            accent: 0.0,
            groove: 0.0,
            feel: 0.0,
            feel_max_ms: 50.0,
            prime_numbers: false,
            lr_offset_ms: 8.0,
            diffusion_in_loop: false,
            input_level: 1.0,
            output_level: 1.0,

            tempo_bpm: None,
            tap_div_l: TapDivision::Free,
            tap_div_r: TapDivision::Free,
            swell_time_s: 0.0,
            freeze: false,
            repeat_dynamics: false,
            high_pass_hz: 0.0,
            pan_l: -1.0,
            pan_r: 1.0,
            duck_gate: false,

            diffuser_l: Diffuser::new(48000.0, false),
            diffuser_r: Diffuser::new(48000.0, true),
            ducker: DuckingFollower::new(),
            sample_rate: 48000.0,

            accent_phase_l: 0.0,
            accent_phase_r: 0.0,
            accent_flip_l: false,
            accent_flip_r: false,

            groove_phase_l: 0.0,
            groove_phase_r: 0.0,

            // 25ms at 48kHz = 1200 samples, add margin
            lr_offset_delay: DelayLine::new(2048),
            lr_offset_smoother: lr_smoother,

            mix_smoother: ParamSmoother::new(0.5),
            width_smoother: ParamSmoother::new(1.0),
            input_level_smoother: ParamSmoother::new(1.0),
            output_level_smoother: ParamSmoother::new(1.0),
            pingpong_fb_smoother: ParamSmoother::new(0.5),

            pingpong_dc_l: DcBlocker::new(),
            pingpong_dc_r: DcBlocker::new(),

            eff_time_l: 250.0,
            eff_time_r: 250.0,
            freeze_smoother: ParamSmoother::new(0.0),
            swell_env: EnvelopeFollower::new(0.0),
            swell_gate_open: false,
            swell_ramp: 1.0,
            repeat_dyn_env: EnvelopeFollower::new(0.0),
            hp_l: Biquad::new(),
            hp_r: Biquad::new(),
            pan_l_smoother: ParamSmoother::new(-1.0),
            pan_r_smoother: ParamSmoother::new(1.0),
        }
    }

    /// Switch both engines to a new delay style.
    pub fn set_style(&mut self, style: DelayStyle) {
        self.delay_l.set_style(style);
        self.delay_r.set_style(style);
    }
}

impl Processor for DelayChain {
    fn reset(&mut self) {
        self.delay_l.reset();
        self.delay_r.reset();
        self.diffuser_l.reset();
        self.diffuser_r.reset();
        self.ducker.reset();

        self.accent_phase_l = 0.0;
        self.accent_phase_r = 0.0;
        self.accent_flip_l = false;
        self.accent_flip_r = false;

        self.groove_phase_l = 0.0;
        self.groove_phase_r = 0.0;

        self.lr_offset_delay.clear();
        self.lr_offset_smoother.reset(self.lr_offset_ms);

        self.mix_smoother.reset(self.mix);
        self.width_smoother.reset(self.width);
        self.input_level_smoother.reset(self.input_level);
        self.output_level_smoother.reset(self.output_level);
        self.pingpong_fb_smoother.reset(self.pingpong_feedback);
        self.pingpong_dc_l.reset();
        self.pingpong_dc_r.reset();

        self.freeze_smoother
            .reset(if self.freeze { 1.0 } else { 0.0 });
        self.swell_env.reset(0.0);
        self.swell_gate_open = false;
        self.swell_ramp = if self.swell_time_s > 0.0 { 0.0 } else { 1.0 };
        self.repeat_dyn_env.reset(0.0);
        self.hp_l.reset();
        self.hp_r.reset();
        self.pan_l_smoother.reset(self.pan_l);
        self.pan_r_smoother.reset(self.pan_r);
    }

    fn update(&mut self, config: AudioConfig) {
        self.sample_rate = config.sample_rate;

        // Configure tape heads based on head mode (only applies to Tape style)
        let (h1, h2, h3) = match self.head_mode {
            HeadMode::Mode1 => (false, false, true), // Head 3 only
            HeadMode::Mode2 => (true, false, true),  // Head 1 + 3
            HeadMode::Mode3 => (false, true, true),  // Head 2 + 3
            HeadMode::Mode4 => (true, true, true),   // All heads
        };
        self.delay_l.head1_enabled = h1;
        self.delay_l.head2_enabled = h2;
        self.delay_l.head3_enabled = h3;
        self.delay_r.head1_enabled = h1;
        self.delay_r.head2_enabled = h2;
        self.delay_r.head3_enabled = h3;

        // Tempo sync: derive effective delay times, clamped to each
        // style's valid range (TimeLine MX per-machine time ranges).
        let tempo = self.tempo_bpm;
        let synced = |div: TapDivision, fallback: f64| -> f64 {
            match tempo {
                Some(bpm) => div.to_ms(bpm).unwrap_or(fallback),
                None => fallback,
            }
        };
        let (min_l, max_l) = self.delay_l.style().time_range_ms();
        let (min_r, max_r) = self.delay_r.style().time_range_ms();
        self.eff_time_l = synced(self.tap_div_l, self.delay_l.time_ms).clamp(min_l, max_l);
        self.eff_time_r = synced(self.tap_div_r, self.delay_r.time_ms).clamp(min_r, max_r);
        // Sync owns the time param (like tap tempo latching a knob): the
        // engines' internal smoothers seed from time_ms on first update.
        self.delay_l.time_ms = self.eff_time_l;
        self.delay_r.time_ms = self.eff_time_r;

        // Freeze: engines pin feedback and bypass loop filters.
        self.delay_l.frozen = self.freeze;
        self.delay_r.frozen = self.freeze;

        // BBD stereo spread: quadrature clock LFO on the right engine so
        // any modulation depth widens the wet image (dBucket MX behavior:
        // no mod = dual-mono wet, mod > 0 = stereo).
        if self.delay_l.style() == DelayStyle::Bbd {
            self.delay_l.bbd_phase_offset = 0.0;
            self.delay_r.bbd_phase_offset = 0.25;
        }

        self.delay_l.update(config.sample_rate);
        self.delay_r.update(config.sample_rate);

        // Post-delay wet high-pass.
        if self.high_pass_hz > 0.0 {
            let hz = self.high_pass_hz.clamp(20.0, 900.0);
            self.hp_l
                .set(FilterType::Highpass, hz, 0.707, config.sample_rate);
            self.hp_r
                .set(FilterType::Highpass, hz, 0.707, config.sample_rate);
        }

        // Freeze ramp ~30 ms (click-free engage, mirrors reverb freeze).
        self.freeze_smoother.set_time_ms(30.0, config.sample_rate);
        self.freeze_smoother
            .set_target(if self.freeze { 1.0 } else { 0.0 });

        // Swell trigger detector: fast attack, moderate release.
        self.swell_env.set_times_ms(5.0, 200.0, config.sample_rate);

        // Repeat-dynamics envelope: track the wet tail level.
        self.repeat_dyn_env
            .set_times_ms(10.0, 300.0, config.sample_rate);

        for s in [&mut self.pan_l_smoother, &mut self.pan_r_smoother] {
            s.set_time_ms(5.0, config.sample_rate);
            s.set_epsilon(1e-4);
        }

        // Diffuser::update resizes its buffers itself if the sample rate
        // grew — no per-update reallocation.
        self.diffuser_l.size = self.diffusion_size;
        self.diffuser_l.smear = self.diffusion_smear;
        self.diffuser_r.size = self.diffusion_size;
        self.diffuser_r.smear = self.diffusion_smear;
        self.diffuser_l.update(config.sample_rate, false);
        self.diffuser_r.update(config.sample_rate, true);

        self.ducker.set_sample_rate(config.sample_rate);
        self.ducker.update_coeffs();

        // Per-sample smoothed params: ~5ms
        for s in [
            &mut self.mix_smoother,
            &mut self.width_smoother,
            &mut self.input_level_smoother,
            &mut self.output_level_smoother,
            &mut self.pingpong_fb_smoother,
        ] {
            s.set_time_ms(5.0, config.sample_rate);
            s.set_epsilon(1e-4);
        }
        self.pingpong_dc_l.set_cutoff(10.0, config.sample_rate);
        self.pingpong_dc_r.set_cutoff(10.0, config.sample_rate);

        // Update LR offset smoother and delay line
        self.lr_offset_smoother.set_time_ms(5.0, config.sample_rate);
        self.lr_offset_smoother.set_target(self.lr_offset_ms);

        // Ensure delay line is large enough for max offset at this sample rate
        let max_offset_samples = (25.0 * config.sample_rate / 1000.0) as usize + 64;
        if self.lr_offset_delay.len() < max_offset_samples {
            self.lr_offset_delay = DelayLine::new(max_offset_samples);
        }
    }

    // r[impl delay.chain.process]
    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        let n = left.len().min(right.len());

        // Cache delay times in samples for accent/groove phase tracking.
        // Effective times include tempo sync + style range clamps.
        let delay_time_samples_l = self.eff_time_l * self.sample_rate / 1000.0;
        let delay_time_samples_r = self.eff_time_r * self.sample_rate / 1000.0;

        // Base delay times; per-sample modulation is passed via
        // tick_at without touching the public params.
        let base_time_l = self.eff_time_l;
        let base_time_r = self.eff_time_r;

        let swell_inc = if self.swell_time_s > 0.0 {
            1.0 / (self.swell_time_s.clamp(0.10, 4.0) * self.sample_rate)
        } else {
            0.0
        };

        let stereo_field = self.delay_l.is_stereo_field_style();

        for i in 0..n {
            let dry_l = left[i];
            let dry_r = right[i];

            // --- Smoothed global params ---
            self.mix_smoother.set_target(self.mix);
            let mix = self.mix_smoother.tick();
            self.width_smoother.set_target(self.width);
            let width = self.width_smoother.tick();
            self.input_level_smoother.set_target(self.input_level);
            let input_level = self.input_level_smoother.tick();
            self.output_level_smoother.set_target(self.output_level);
            let output_level = self.output_level_smoother.tick();
            self.pingpong_fb_smoother.set_target(self.pingpong_feedback);
            let pingpong_feedback = self.pingpong_fb_smoother.tick();

            // --- Freeze: mute the loop input while frozen (ramped) ---
            let freeze_amt = self.freeze_smoother.tick();
            let live_gain = 1.0 - freeze_amt;

            // --- Swell: input-triggered fade-in on the wet input ---
            let swell_gain = if swell_inc > 0.0 {
                let env = self.swell_env.tick((dry_l.abs() + dry_r.abs()) * 0.5);
                if env > 0.02 {
                    if !self.swell_gate_open {
                        // New note onset: restart the ramp.
                        self.swell_gate_open = true;
                        self.swell_ramp = 0.0;
                    }
                } else if env < 0.01 {
                    self.swell_gate_open = false;
                }
                self.swell_ramp = (self.swell_ramp + swell_inc).min(1.0);
                self.swell_ramp * self.swell_ramp
            } else {
                1.0
            };

            // Swell placement is mix-dependent (see field docs): full-wet
            // presets swell the dry feeding the delay; otherwise the
            // envelope is applied to the wet output below.
            let swell_on_input = mix >= 0.99;
            let input_swell = if swell_on_input { swell_gain } else { 1.0 };

            // --- Input level ---
            let scaled_l = dry_l * input_level * input_swell * live_gain;
            let scaled_r = dry_r * input_level * input_swell * live_gain;

            // --- Diffusion (loop mode: applied to input before delay) ---
            let (diff_in_l, diff_in_r) = if self.diffusion_in_loop && self.diffusion_enabled {
                (
                    self.diffuser_l.tick(scaled_l),
                    self.diffuser_r.tick(scaled_r),
                )
            } else {
                (scaled_l, scaled_r)
            };

            // Compute delay input based on stereo mode
            let (in_l, in_r) = match self.stereo_mode {
                StereoMode::Stereo => (diff_in_l, diff_in_r),
                StereoMode::Mono => {
                    let mono = (diff_in_l + diff_in_r) * 0.5;
                    (mono, mono)
                }
                StereoMode::PingPong => {
                    // In ping-pong, we feed mono to L and cross-feed from
                    // outputs; DC-block the recirculating cross-feed.
                    let mono = (diff_in_l + diff_in_r) * 0.5;
                    let fb_r = self.pingpong_dc_r.tick(self.delay_r.last_feedback());
                    let fb_l = self.pingpong_dc_l.tick(self.delay_l.last_feedback());
                    (mono + fb_r * pingpong_feedback, fb_l * pingpong_feedback)
                }
            };

            // --- Feel: offset delay time ---
            let feel_offset = self.feel * self.feel_max_ms;
            let mut time_l = base_time_l + feel_offset;
            let mut time_r = base_time_r + feel_offset;

            // --- Prime numbers: anti-resonance ---
            // 0.13% detune pushes repeats off exact integer-ratio spacing
            // so stacked delays don't comb-filter against each other.
            if self.prime_numbers {
                time_l *= 1.0 + 0.0013;
                time_r *= 1.0 + 0.0013;
            }

            // --- Groove: modulate delay time based on phase ---
            if self.groove.abs() > 1e-6 && delay_time_samples_l > 1.0 {
                // L channel groove
                if self.groove_phase_l > delay_time_samples_l * 0.5 {
                    time_l += self.groove * time_l * 0.333;
                }
                // R channel groove
                if self.groove_phase_r > delay_time_samples_r * 0.5 {
                    time_r += self.groove * time_r * 0.333;
                }
            }

            // Clamp to positive
            time_l = time_l.max(0.1);
            time_r = time_r.max(0.1);

            // --- In-loop feedback trim (repeat dynamics + gate duck) ---
            // Computed BEFORE the engines tick so the trim shapes this
            // sample's recirculation. The ducker runs here (it only needs
            // the dry input); its gain is reused for the wet duck below.
            let duck_gain = if self.ducking_enabled {
                let input_level = (dry_l.abs() + dry_r.abs()) * 0.5;
                self.ducker.tick(input_level)
            } else {
                1.0
            };
            let mut fb_trim = 1.0;
            if self.repeat_dynamics {
                // Envelope of the wet tail (updated after the engines
                // below; one sample of lag is inaudible). Below the knee
                // the trim drops expansively, so a decaying tail sheds
                // feedback progressively and cuts off instead of lingering.
                // The FIRST repeat is untouched: it is written from the
                // dry input, which the trim never scales.
                const KNEE: f64 = 0.1;
                let env = self.repeat_dyn_env.value();
                if env < KNEE {
                    fb_trim *= (env / KNEE).powf(0.75);
                }
            }
            if self.ducking_enabled && self.duck_gate {
                // GATE: the regeneration closes FULLY on input presence,
                // independent of `amount` (TimeLine: "the Repeats knob is
                // effectively set to minimum while you are playing").
                // The follower's envelope is presence above threshold;
                // anything past a small knee slams the loop shut, and its
                // release curve reopens it smoothly when you stop.
                const GATE_KNEE: f64 = 0.05;
                fb_trim *= 1.0 - (self.ducker.envelope() / GATE_KNEE).clamp(0.0, 1.0);
            }
            self.delay_l.set_feedback_trim(fb_trim);
            self.delay_r.set_feedback_trim(fb_trim);

            // Stereo-field styles (Drum / MultiTap / Spectral) run ONE
            // stereo engine on the mono sum — that's what makes per-head
            // / per-tap / per-grain pans meaningful ("patterns create a
            // stereo field when using both L and R outputs, sum to mono
            // when R unplugged"). StereoMode routing (incl. PingPong
            // cross-feed) is bypassed for these styles: their stereo
            // image comes from the element pans. Other styles keep the
            // classic two-mono-engine topology untouched.
            let (mut wet_l, mut wet_r) = if stereo_field {
                let mono_in = (in_l + in_r) * 0.5;
                self.delay_l.tick_at_stereo(mono_in, time_l)
            } else {
                (
                    self.delay_l.tick_at(in_l, 0, time_l),
                    self.delay_r.tick_at(in_r, 1, time_r),
                )
            };

            // --- Diffusion (post mode: applied after delay output) ---
            if !self.diffusion_in_loop && self.diffusion_enabled {
                wet_l = self.diffuser_l.tick(wet_l);
                wet_r = self.diffuser_r.tick(wet_r);
            }

            // --- Swell on the wet output (mix < full; see above) ---
            if !swell_on_input {
                wet_l *= swell_gain;
                wet_r *= swell_gain;
            }

            // --- Accent: alternating repeat volume ---
            if self.accent.abs() > 1e-6 && delay_time_samples_l > 1.0 {
                // L channel accent
                let accent_gain_l = 1.0 + self.accent * if self.accent_flip_l { 1.0 } else { -1.0 };
                wet_l *= accent_gain_l;

                // R channel accent
                let accent_gain_r = 1.0 + self.accent * if self.accent_flip_r { 1.0 } else { -1.0 };
                wet_r *= accent_gain_r;

                // Advance accent phases
                self.accent_phase_l += 1.0;
                if self.accent_phase_l >= delay_time_samples_l {
                    self.accent_phase_l -= delay_time_samples_l;
                    self.accent_flip_l = !self.accent_flip_l;
                }
                self.accent_phase_r += 1.0;
                if self.accent_phase_r >= delay_time_samples_r {
                    self.accent_phase_r -= delay_time_samples_r;
                    self.accent_flip_r = !self.accent_flip_r;
                }
            }

            // --- Groove phase tracking ---
            if delay_time_samples_l > 1.0 {
                self.groove_phase_l += 1.0;
                if self.groove_phase_l >= delay_time_samples_l {
                    self.groove_phase_l -= delay_time_samples_l;
                }
            }
            if delay_time_samples_r > 1.0 {
                self.groove_phase_r += 1.0;
                if self.groove_phase_r >= delay_time_samples_r {
                    self.groove_phase_r -= delay_time_samples_r;
                }
            }

            // Ducking on the wet output (sensitivity-controlled; the
            // GATE-mode feedback muting already happened via the trim).
            if self.ducking_enabled {
                wet_l *= duck_gain;
                wet_r *= duck_gain;
            }

            // Track the loop level for the in-loop repeat-dynamics trim
            // (applied on the NEXT sample's recirculation). The dry input
            // is included so fresh playing keeps the loop open — only a
            // decaying tail (quiet wet, no input) drops below the knee.
            if self.repeat_dynamics {
                let loop_level =
                    ((wet_l.abs() + wet_r.abs()) * 0.5).max((dry_l.abs() + dry_r.abs()) * 0.5);
                self.repeat_dyn_env.tick(loop_level);
            }

            // --- Post-delay wet high-pass ---
            if self.high_pass_hz > 0.0 {
                wet_l = self.hp_l.tick(wet_l, 0);
                wet_r = self.hp_r.tick(wet_r, 1);
            }

            // --- LR offset: delay the R channel ---
            // Skipped for stereo-field styles: their L/R relationship IS
            // the head/tap/grain panning; a fixed R shift would smear it.
            if width > 0.001 && !stereo_field {
                self.lr_offset_smoother.set_target(self.lr_offset_ms);
                let offset_ms = self.lr_offset_smoother.tick();
                let offset_samples = offset_ms * self.sample_rate / 1000.0;
                self.lr_offset_delay.write(wet_r);
                if offset_samples > 0.5 {
                    wet_r = self.lr_offset_delay.read_cubic(offset_samples);
                }
            }

            // Stereo width (mid-side)
            if (width - 1.0).abs() > 0.001 {
                (wet_l, wet_r) = audiocore_dsp::stereo::width(wet_l, wet_r, width);
            }

            // --- Per-line pan (equal power). Defaults (-1, +1) are the
            // classic hard-L/hard-R identity routing. ---
            self.pan_l_smoother.set_target(self.pan_l);
            self.pan_r_smoother.set_target(self.pan_r);
            let pl = self.pan_l_smoother.tick();
            let pr = self.pan_r_smoother.tick();
            if (pl + 1.0).abs() > 1e-4 || (pr - 1.0).abs() > 1e-4 {
                let al = (pl + 1.0) * std::f64::consts::FRAC_PI_4;
                let ar = (pr + 1.0) * std::f64::consts::FRAC_PI_4;
                let new_l = wet_l * al.cos() + wet_r * ar.cos();
                let new_r = wet_l * al.sin() + wet_r * ar.sin();
                wet_l = new_l;
                wet_r = new_r;
            }

            // --- Output level ---
            wet_l *= output_level;
            wet_r *= output_level;

            // Mix dry/wet
            left[i] = dry_l * (1.0 - mix) + wet_l * mix;
            right[i] = dry_r * (1.0 - mix) + wet_r * mix;
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

    fn make_chain() -> DelayChain {
        let mut c = DelayChain::new();
        c.delay_l.time_ms = 100.0;
        c.delay_r.time_ms = 100.0;
        c.delay_l.feedback = 0.0;
        c.delay_r.feedback = 0.0;
        c.mix = 1.0;
        c.update(config());
        c
    }

    #[test]
    fn dry_wet_mix() {
        let mut c = make_chain();
        c.mix = 0.0; // Fully dry
        c.reset(); // Snap smoothers to the new params

        let mut l = vec![0.5; 512];
        let mut r = vec![0.5; 512];
        c.process(&mut l, &mut r);

        // Should be unchanged
        assert!((l[0] - 0.5).abs() < 1e-10);
    }

    #[test]
    fn mix_change_is_smooth() {
        let mut c = make_chain();
        c.mix = 1.0;
        c.reset();

        // Steady dry signal, delay produces nothing yet (first 100ms)
        let mut l = vec![0.8; 1024];
        let mut r = vec![0.8; 1024];
        c.process(&mut l, &mut r);

        // Step mix hard from 1.0 to 0.0 mid-stream — output must ramp,
        // not jump, sample to sample.
        c.mix = 0.0;
        let mut l2 = vec![0.8; 4096];
        let mut r2 = vec![0.8; 4096];
        c.process(&mut l2, &mut r2);

        let mut prev = l[1023];
        let mut max_jump: f64 = 0.0;
        for &v in &l2 {
            max_jump = max_jump.max((v - prev).abs());
            prev = v;
        }
        // Unsoothed, the wet→dry step would jump by up to 0.8 in one
        // sample; smoothed it moves in small increments.
        assert!(
            max_jump < 0.05,
            "Mix step should be smoothed: max_jump={max_jump}"
        );
        // And it must actually settle to fully dry.
        assert!(
            (l2[4095] - 0.8).abs() < 1e-6,
            "Should settle dry: {}",
            l2[4095]
        );
    }

    #[test]
    fn stereo_mode_works() {
        let mut c = make_chain();
        c.stereo_mode = StereoMode::Mono;
        // 80 ms: inside every style's valid time range (min is 60 ms).
        c.delay_l.time_ms = 80.0;
        c.delay_r.time_ms = 80.0;
        c.update(config());

        // Send signal only to left
        let n = 19200;
        let mut l: Vec<f64> = (0..n).map(|i| if i < 100 { 1.0 } else { 0.0 }).collect();
        let mut r = vec![0.0; n];

        c.process(&mut l, &mut r);

        // In mono mode, right should also get output
        let r_energy: f64 = r.iter().map(|x| x * x).sum();
        assert!(r_energy > 0.01, "Mono mode should output to both channels");
    }

    #[test]
    fn no_nan_full_chain() {
        let mut c = DelayChain::new();
        c.delay_l.time_ms = 200.0;
        c.delay_r.time_ms = 250.0;
        c.delay_l.feedback = 0.6;
        c.delay_r.feedback = 0.6;
        c.delay_l.drive = 0.5;
        c.delay_r.drive = 0.5;
        c.delay_l.hicut_freq = 5000.0;
        c.delay_r.hicut_freq = 5000.0;
        c.delay_l.wow_depth = 0.3;
        c.delay_r.wow_depth = 0.3;
        c.delay_l.flutter_depth = 0.3;
        c.delay_r.flutter_depth = 0.3;
        c.diffusion_enabled = true;
        c.ducking_enabled = true;
        c.ducker.amount = 0.5;
        c.ducker.threshold = 0.1;
        c.mix = 0.5;
        c.update(config());

        let n = 48000;
        let mut l: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut r = l.clone();

        c.process(&mut l, &mut r);

        for (i, (&lv, &rv)) in l.iter().zip(r.iter()).enumerate() {
            assert!(lv.is_finite(), "L NaN at {i}");
            assert!(rv.is_finite(), "R NaN at {i}");
        }
    }

    #[test]
    fn ping_pong_produces_output_both_channels() {
        let mut c = DelayChain::new();
        c.delay_l.time_ms = 50.0;
        c.delay_r.time_ms = 50.0;
        c.delay_l.feedback = 0.3;
        c.delay_r.feedback = 0.3;
        c.stereo_mode = StereoMode::PingPong;
        c.pingpong_feedback = 0.6;
        c.mix = 1.0;
        c.update(config());

        // Send a burst in left channel
        let n = 19200;
        let mut l: Vec<f64> = (0..n).map(|i| if i < 100 { 0.8 } else { 0.0 }).collect();
        let mut r = vec![0.0; n];

        c.process(&mut l, &mut r);

        // Both channels should have output (ping-pong cross-feeds)
        let l_energy: f64 = l.iter().map(|x| x * x).sum();
        let r_energy: f64 = r.iter().map(|x| x * x).sum();

        assert!(l_energy > 0.01, "L should have output: {l_energy}");
        assert!(
            r_energy > 0.001,
            "R should have output from ping-pong cross-feed: {r_energy}"
        );
    }

    #[test]
    fn diffusion_smears_impulse() {
        let mut c_clean = make_chain();
        c_clean.diffusion_enabled = false;
        c_clean.update(config());

        let mut c_diff = make_chain();
        c_diff.diffusion_enabled = true;
        c_diff.diffusion_smear = 0.7;
        c_diff.diffusion_size = 0.5;
        c_diff.update(config());

        let n = 19200;
        let impulse: Vec<f64> = (0..n).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();

        let mut l_clean = impulse.clone();
        let mut r_clean = impulse.clone();
        c_clean.process(&mut l_clean, &mut r_clean);

        let mut l_diff = impulse.clone();
        let mut r_diff = impulse.clone();
        c_diff.process(&mut l_diff, &mut r_diff);

        // Count how many samples are above threshold — diffusion should spread energy
        let count_clean = l_clean.iter().filter(|x| x.abs() > 0.01).count();
        let count_diff = l_diff.iter().filter(|x| x.abs() > 0.01).count();

        assert!(
            count_diff > count_clean,
            "Diffusion should spread the impulse: clean={count_clean}, diff={count_diff}"
        );
    }

    #[test]
    fn width_control() {
        let mut c = make_chain();
        c.delay_l.time_ms = 80.0;
        c.delay_r.time_ms = 120.0; // Different times for L/R
        c.width = 2.0; // Extra wide
        c.update(config());

        let n = 19200;
        let mut l: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut r = l.clone();

        c.process(&mut l, &mut r);

        // With different delay times and width>1, L and R should differ
        let diff: f64 = l
            .iter()
            .zip(r.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / n as f64;
        assert!(
            diff > 0.001,
            "Width should create stereo difference: avg_diff={diff}"
        );
    }

    #[test]
    fn style_switching_works() {
        for i in 0..DelayStyle::COUNT {
            let style = DelayStyle::from_index(i);
            let mut c = DelayChain::new();
            c.set_style(style);
            c.delay_l.time_ms = 30.0;
            c.delay_r.time_ms = 30.0;
            c.delay_l.feedback = 0.3;
            c.delay_r.feedback = 0.3;
            c.head_mode = HeadMode::Mode4;
            c.mix = 1.0;
            c.update(config());

            let n = 48000;
            let mut l: Vec<f64> = (0..n).map(|s| if s < 100 { 0.8 } else { 0.0 }).collect();
            let mut r = l.clone();

            c.process(&mut l, &mut r);

            let energy: f64 = l.iter().map(|x| x * x).sum();
            assert!(
                energy > 0.001,
                "{:?} style should produce output in chain: energy={energy}",
                style
            );
        }
    }

    #[test]
    fn tap_division_math() {
        // 120 BPM: quarter = 500 ms.
        assert!((TapDivision::Quarter.to_ms(120.0).unwrap() - 500.0).abs() < 1e-9);
        assert!((TapDivision::DottedEighth.to_ms(120.0).unwrap() - 375.0).abs() < 1e-9);
        assert!((TapDivision::Eighth.to_ms(120.0).unwrap() - 250.0).abs() < 1e-9);
        assert!((TapDivision::Triplet.to_ms(120.0).unwrap() - 500.0 / 3.0).abs() < 1e-9);
        assert!((TapDivision::Sixteenth.to_ms(120.0).unwrap() - 125.0).abs() < 1e-9);
        // Golden/Silver: quarter divided by φ / δ.
        assert!((TapDivision::GoldenRatio.to_ms(120.0).unwrap() - 309.016_994).abs() < 1e-3);
        assert!((TapDivision::SilverRatio.to_ms(120.0).unwrap() - 207.106_781).abs() < 1e-3);
        assert!(TapDivision::Free.to_ms(120.0).is_none());

        for i in 0..TapDivision::COUNT {
            assert_eq!(TapDivision::from_index(i).to_index(), i);
        }
    }

    #[test]
    fn tempo_sync_sets_delay_time() {
        let mut c = make_chain();
        c.set_style(DelayStyle::Clean);
        c.delay_l.time_ms = 999.0; // Should be ignored when synced
        c.delay_r.time_ms = 999.0;
        c.tempo_bpm = Some(120.0);
        c.tap_div_l = TapDivision::Eighth; // 250 ms
        c.tap_div_r = TapDivision::Eighth;
        c.mix = 1.0;
        c.update(config());

        let n = 24000; // 500 ms
        let mut l: Vec<f64> = (0..n).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);

        let expected = (250.0 * SR / 1000.0) as usize;
        let peak = l
            .iter()
            .enumerate()
            .skip(1000)
            .max_by(|(_, a), (_, b)| a.abs().partial_cmp(&b.abs()).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        assert!(
            (peak as i64 - expected as i64).unsigned_abs() < 480,
            "synced repeat at {peak}, expected near {expected}"
        );
    }

    #[test]
    fn freeze_holds_energy_and_disengages() {
        let mut c = make_chain();
        c.set_style(DelayStyle::Clean);
        c.delay_l.time_ms = 100.0;
        c.delay_r.time_ms = 100.0;
        c.delay_l.feedback = 0.5;
        c.delay_r.feedback = 0.5;
        c.mix = 1.0;
        c.update(config());
        c.reset();

        // Feed a burst, then freeze.
        let n = 4800;
        let mut l: Vec<f64> = (0..n).map(|i| if i < 200 { 0.8 } else { 0.0 }).collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);

        c.freeze = true;
        c.update(config());

        // 5 seconds frozen: energy per delay period should stay flat.
        let mut energy_first = 0.0;
        let mut energy_last = 0.0;
        let period = 4800; // 100 ms
        for block in 0..50 {
            let mut fl = vec![0.0; period];
            let mut fr = vec![0.0; period];
            c.process(&mut fl, &mut fr);
            let e: f64 = fl.iter().map(|x| x * x).sum();
            if block == 2 {
                energy_first = e;
            }
            if block == 49 {
                energy_last = e;
            }
            for &v in &fl {
                assert!(v.is_finite());
            }
        }
        assert!(energy_first > 1e-6, "frozen loop should hold audio");
        let db = 10.0 * (energy_last / energy_first).log10();
        assert!(
            db.abs() < 0.5,
            "freeze should hold level: drift {db:.2} dB over ~5 s"
        );

        // Disengage: tail must decay again.
        c.freeze = false;
        c.update(config());
        let mut tail_energy = 0.0;
        for _ in 0..50 {
            let mut fl = vec![0.0; period];
            let mut fr = vec![0.0; period];
            c.process(&mut fl, &mut fr);
            tail_energy = fl.iter().map(|x| x * x).sum();
        }
        assert!(
            tail_energy < energy_last * 0.01,
            "tail should decay after unfreeze: {tail_energy} vs {energy_last}"
        );
    }

    #[test]
    fn swell_ramps_wet_in() {
        let run = |swell: f64| -> f64 {
            let mut c = make_chain();
            c.set_style(DelayStyle::Clean);
            c.delay_l.time_ms = 100.0;
            c.delay_r.time_ms = 100.0;
            c.mix = 1.0;
            c.swell_time_s = swell;
            c.update(config());
            c.reset();

            // Steady tone; measure early wet energy (first repeat).
            let n = 12000; // 250 ms
            let mut l: Vec<f64> = (0..n)
                .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
                .collect();
            let mut r = l.clone();
            c.process(&mut l, &mut r);
            // Window just after the delay time.
            l[4800..7200].iter().map(|x| x * x).sum()
        };

        let no_swell = run(0.0);
        let with_swell = run(2.0);
        assert!(
            with_swell < no_swell * 0.25,
            "2 s swell should suppress the early wet: {with_swell} vs {no_swell}"
        );
    }

    #[test]
    fn repeat_dynamics_shortens_tail() {
        let run = |rd: bool| -> f64 {
            let mut c = make_chain();
            c.set_style(DelayStyle::Clean);
            c.delay_l.time_ms = 100.0;
            c.delay_r.time_ms = 100.0;
            c.delay_l.feedback = 0.7;
            c.delay_r.feedback = 0.7;
            c.mix = 1.0;
            c.repeat_dynamics = rd;
            c.update(config());

            let n = 96000; // 2 s
            let mut l: Vec<f64> = (0..n).map(|i| if i < 200 { 0.8 } else { 0.0 }).collect();
            let mut r = l.clone();
            c.process(&mut l, &mut r);
            // Late-tail energy.
            l[48000..].iter().map(|x| x * x).sum()
        };

        let normal = run(false);
        let tapered = run(true);
        assert!(
            tapered < normal * 0.5,
            "repeat dynamics should taper the tail: {tapered} vs {normal}"
        );
    }

    /// Goertzel magnitude of `freq` over a window of `buf`.
    fn goertzel(buf: &[f64], freq: f64) -> f64 {
        let w = 2.0 * PI * freq / SR;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &x in buf {
            let s0 = x + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt()
    }

    #[test]
    fn repeat_dynamics_first_repeat_invariant() {
        // In-loop dynamics must not touch the first repeat — that is
        // what distinguishes it from the old wet-path taper.
        let run = |rd: bool| -> (f64, f64) {
            let mut c = make_chain();
            c.set_style(DelayStyle::Clean);
            c.delay_l.time_ms = 100.0;
            c.delay_r.time_ms = 100.0;
            c.delay_l.feedback = 0.7;
            c.delay_r.feedback = 0.7;
            c.mix = 1.0;
            c.repeat_dynamics = rd;
            c.update(config());
            c.reset();

            // Pre-roll: let the engine's internal time smoother settle
            // so the burst hits a stationary delay (a freshly reset
            // engine glides its read position for the first ~150 ms).
            let mut pl = vec![0.0; 24000];
            let mut pr = vec![0.0; 24000];
            c.process(&mut pl, &mut pr);

            let n = 96000; // 2 s
            let mut l: Vec<f64> = (0..n).map(|i| if i < 200 { 0.8 } else { 0.0 }).collect();
            let mut r = l.clone();
            c.process(&mut l, &mut r);

            // First repeat: 100 ms window. Late tail: after 1 s.
            let first: f64 = l[4800..7200].iter().map(|x| x * x).sum();
            let late: f64 = l[48000..].iter().map(|x| x * x).sum();
            (first, late)
        };

        let (first_off, late_off) = run(false);
        let (first_on, late_on) = run(true);
        assert!(
            (first_on - first_off).abs() < first_off * 0.01,
            "first repeat must be unchanged: on={first_on} off={first_off}"
        );
        assert!(
            late_on < late_off * 0.5,
            "in-loop dynamics should taper the late tail: {late_on} vs {late_off}"
        );
    }

    #[test]
    fn duck_gate_kills_recirculation_only_last_note_repeats() {
        // Note A (440 Hz, 20 ms) then note B (2093 Hz, 100–600 ms)
        // covering A's first echo at 300 ms. GATE mutes the regeneration
        // while B plays, so A's echo never recirculates — A's chain is
        // DEAD at 900 ms even though playing has stopped. Without the
        // gate, A recirculated silently (the wet duck only muted the
        // output) and its history comes back at 900 ms. B, the last
        // note, must keep regenerating after the stop in both cases.
        // Frequency-tagged notes + Goertzel keep the events separable.
        let run = |gate: bool| -> (f64, f64) {
            let mut c = make_chain();
            c.set_style(DelayStyle::Clean);
            c.delay_l.time_ms = 300.0;
            c.delay_r.time_ms = 300.0;
            c.delay_l.feedback = 0.9;
            c.delay_r.feedback = 0.9;
            c.mix = 1.0;
            c.ducking_enabled = true;
            c.duck_gate = gate;
            c.ducker.amount = 1.0;
            c.ducker.threshold = 0.05;
            c.ducker.release_ms = 100.0;
            c.update(config());
            c.reset();

            // Pre-roll for the time smoother.
            let mut pl = vec![0.0; 24000];
            let mut pr = vec![0.0; 24000];
            c.process(&mut pl, &mut pr);

            let n = 96000; // 2 s
            let mut l: Vec<f64> = (0..n)
                .map(|i| {
                    let t_ms = i as f64 * 1000.0 / SR;
                    let t = i as f64 / SR;
                    if t_ms < 20.0 {
                        (2.0 * PI * 440.0 * t).sin() * 0.8
                    } else if (100.0..600.0).contains(&t_ms) {
                        (2.0 * PI * 2093.0 * t).sin() * 0.5
                    } else {
                        0.0
                    }
                })
                .collect();
            let mut r = l.clone();
            c.process(&mut l, &mut r);

            let window = |center_ms: f64| -> &[f64] {
                let c0 = (center_ms * SR / 1000.0) as usize;
                let h = (40.0 * SR / 1000.0) as usize;
                &l[c0 - h..c0 + h]
            };
            // A's 3rd echo slot (900 ms): needs recirculation at 300
            // and 600 ms — both while B was playing.
            let a_history = goertzel(window(900.0), 440.0);
            // B's post-stop regeneration (last-written content at
            // 600 ms → echoes at 900/1200; 1200 needs feedback at
            // 900 ms, well after the gate reopened).
            let b_survivor = goertzel(window(1200.0), 2093.0);
            (a_history, b_survivor)
        };

        let (a_off, b_off) = run(false);
        let (a_on, b_on) = run(true);

        assert!(
            a_on < a_off * 0.1,
            "gate must kill repeats recirculated while playing: on={a_on} off={a_off}"
        );
        assert!(
            b_on > b_off * 0.2,
            "last note's post-stop regeneration must survive: on={b_on} off={b_off}"
        );
        assert!(b_on > 1.0, "last note must audibly repeat: {b_on}");
    }

    #[test]
    fn new_styles_work_in_chain() {
        for style in [
            DelayStyle::Drum,
            DelayStyle::OilCan,
            DelayStyle::MultiTap,
            DelayStyle::Spectral,
            DelayStyle::Filter,
        ] {
            let mut c = DelayChain::new();
            c.set_style(style);
            c.delay_l.time_ms = 300.0;
            c.delay_r.time_ms = 300.0;
            c.delay_l.feedback = 0.4;
            c.delay_r.feedback = 0.4;
            c.mix = 1.0;
            c.update(config());

            let n = 96000;
            let mut l: Vec<f64> = (0..n).map(|s| if s < 200 { 0.8 } else { 0.0 }).collect();
            let mut r = l.clone();
            c.process(&mut l, &mut r);

            let energy: f64 = l.iter().map(|x| x * x).sum();
            assert!(energy > 0.001, "{style:?} should produce output in chain");
            for (i, &v) in l.iter().enumerate() {
                assert!(v.is_finite(), "{style:?} NaN at {i}");
            }
        }
    }

    #[test]
    fn drum_pan_rotation_emerges_from_feedback_topology() {
        use crate::drum_delay::HeadPlayback;

        // Even spacing, hard-alternating pans L/R/L/R.
        let run = |fb_heads: [bool; 4]| -> (f64, f64) {
            let mut c = DelayChain::new();
            c.set_style(DelayStyle::Drum);
            c.delay_l.time_ms = 400.0;
            c.delay_r.time_ms = 400.0;
            c.delay_l.feedback = 0.8;
            c.mix = 1.0;
            c.width = 1.0;
            for (i, (pos, pan)) in [(0.25, -1.0), (0.5, 1.0), (0.75, -1.0), (1.0, 1.0)]
                .iter()
                .enumerate()
            {
                c.delay_l.drum_heads[i].position = *pos;
                c.delay_l.drum_heads[i].pan = *pan;
                c.delay_l.drum_heads[i].playback = HeadPlayback::Full;
                c.delay_l.drum_heads[i].feedback = fb_heads[i];
            }
            c.delay_l.drum_wobble = 0.0;
            c.delay_l.drum_lo_cut = 0.0;
            c.update(config());

            let n = 48000;
            let mut l: Vec<f64> = (0..n).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
            let mut r = vec![0.0; n];
            // Feed L only; drum is mono-summed inside.
            c.process(&mut l, &mut r);

            // Second-pass window of head 1 (pan hard L): 400+100 = 500 ms.
            let cnt = (500.0 * SR / 1000.0) as usize;
            let h = (30.0 * SR / 1000.0) as usize;
            let le: f64 = l[cnt - h..cnt + h].iter().map(|x| x * x).sum();
            let re: f64 = r[cnt - h..cnt + h].iter().map(|x| x * x).sum();
            (le, re)
        };

        // Feedback from ONLY the last head: the recirculated pattern
        // re-excites the heads one at a time — head 1's second-pass
        // window stays hard-left lateralized.
        let (le, re) = run([false, false, false, true]);
        let lateral_last = (le - re).abs() / (le + re).max(1e-12);
        assert!(
            lateral_last > 0.8,
            "last-head fb should preserve the pan rotation: L={le} R={re}"
        );

        // Feedback from ALL heads: by the second pass every head has
        // signal simultaneously — the image collapses toward center.
        let (le2, re2) = run([true, true, true, true]);
        let lateral_all = (le2 - re2).abs() / (le2 + re2).max(1e-12);
        assert!(
            lateral_all < lateral_last * 0.6,
            "all-head fb should collapse the image: last={lateral_last:.3} all={lateral_all:.3}"
        );
    }

    #[test]
    fn stereo_field_styles_center_pan_sums_like_mono() {
        // Pan-neutral Drum config: L and R outputs must be identical
        // (each head contributes unity to both sides at pan = 0).
        let mut c = DelayChain::new();
        c.set_style(DelayStyle::Drum);
        c.delay_l.time_ms = 300.0;
        c.delay_r.time_ms = 300.0;
        c.delay_l.feedback = 0.5;
        c.delay_l.drum_wobble = 0.0;
        c.mix = 1.0;
        c.update(config());

        let n = 48000;
        let mut l: Vec<f64> = (0..n).map(|i| if i < 50 { 0.8 } else { 0.0 }).collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);

        let energy: f64 = l.iter().map(|x| x * x).sum();
        assert!(energy > 0.001, "should produce output");
        for (i, (a, b)) in l.iter().zip(r.iter()).enumerate() {
            assert!(
                (a - b).abs() < 1e-9,
                "pan-neutral stereo field must be dual-mono at {i}: {a} vs {b}"
            );
        }
    }

    #[test]
    fn multitap_pans_land_on_their_sides() {
        use crate::multitap_delay::Tap;

        let mut c = DelayChain::new();
        c.set_style(DelayStyle::MultiTap);
        c.delay_l.time_ms = 400.0;
        c.delay_r.time_ms = 400.0;
        c.delay_l.feedback = 0.0;
        let mut taps = [Tap::off(); crate::multitap_delay::MAX_TAPS];
        taps[0] = Tap::at(0.5, 1.0); // 200 ms
        taps[0].pan = -1.0;
        taps[1] = Tap::at(1.0, 1.0); // 400 ms
        taps[1].pan = 1.0;
        c.delay_l.multitap_taps = taps;
        c.mix = 1.0;
        c.update(config());

        let n = 24000;
        let mut l: Vec<f64> = (0..n).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
        let mut r = vec![0.0; n];
        c.process(&mut l, &mut r);

        let window = |buf: &[f64], ms: f64| -> f64 {
            let cidx = (ms * SR / 1000.0) as usize;
            let h = 480;
            buf[cidx - h..cidx + h].iter().map(|x| x * x).sum()
        };
        let l_tap1 = window(&l, 200.0);
        let r_tap1 = window(&r, 200.0);
        let l_tap2 = window(&l, 400.0);
        let r_tap2 = window(&r, 400.0);
        assert!(l_tap1 > 0.1, "hard-L tap on L: {l_tap1}");
        assert!(r_tap2 > 0.1, "hard-R tap on R: {r_tap2}");
        assert!(
            r_tap1 < l_tap1 * 1e-6,
            "hard-L tap must not bleed: {r_tap1}"
        );
        assert!(
            l_tap2 < r_tap2 * 1e-6,
            "hard-R tap must not bleed: {l_tap2}"
        );
    }

    #[test]
    fn lr_offset_creates_stereo_difference() {
        let mut c = make_chain();
        c.delay_l.time_ms = 50.0;
        c.delay_r.time_ms = 50.0;
        c.lr_offset_ms = 10.0; // 10ms offset on R channel
        c.width = 1.0;
        c.mix = 1.0;
        c.update(config());

        let n = 19200;
        let mut l: Vec<f64> = (0..n).map(|i| if i < 100 { 0.8 } else { 0.0 }).collect();
        let mut r = l.clone();

        c.process(&mut l, &mut r);

        // Find the sample index of peak output in each channel (after the delay)
        let l_peak_idx = l
            .iter()
            .enumerate()
            .skip(2000) // skip past initial transient
            .max_by(|(_, a), (_, b)| a.abs().partial_cmp(&b.abs()).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        let r_peak_idx = r
            .iter()
            .enumerate()
            .skip(2000)
            .max_by(|(_, a), (_, b)| a.abs().partial_cmp(&b.abs()).unwrap())
            .map(|(i, _)| i)
            .unwrap();

        // The R channel peak should be offset from L channel peak
        // 10ms at 48kHz = 480 samples
        let idx_diff = (r_peak_idx as i64 - l_peak_idx as i64).unsigned_abs();

        // Also verify L and R differ in content
        let sample_diff: f64 = l
            .iter()
            .zip(r.iter())
            .skip(2000)
            .take(4800)
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>();

        assert!(
            sample_diff > 0.01 || idx_diff > 100,
            "LR offset should make channels differ: sample_diff={sample_diff}, idx_diff={idx_diff}"
        );
    }
}
