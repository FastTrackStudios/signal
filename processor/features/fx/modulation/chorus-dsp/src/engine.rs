//! Chorus engines — five distinct voice implementations.
//!
//! Each engine produces a single modulated delay voice. The chain
//! creates multiple voices per channel for the full chorus effect.

use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::denormal::flush;
use std::f64::consts::PI;

use crate::dsp::{ModLine, SvfLp, soft_sat};

/// Chorus effect type — controls delay time ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectType {
    Chorus,
    Flanger,
    Vibrato,
}

impl EffectType {
    /// Centre delay of the original five engines' voices.
    #[must_use]
    pub const fn base_delay_ms(&self) -> f64 {
        match self {
            Self::Chorus => 10.0,
            Self::Flanger => 2.6,
            Self::Vibrato => 5.0,
        }
    }

    /// Largest swing (±ms) the original engines' depth reaches.
    ///
    /// Each is kept under its centre delay: the old 12 ms chorus / 4 ms
    /// flanger / 8 ms vibrato swings ran through zero from depth ≈ 0.8 / 0.5 /
    /// 0.6, where the read clamped at one sample — a flat spot in the sweep
    /// and a pitch kink every cycle.
    #[must_use]
    pub const fn max_depth_ms(&self) -> f64 {
        match self {
            Self::Chorus => 7.5,
            Self::Flanger => 2.3,
            Self::Vibrato => 4.5,
        }
    }

    /// The most pitch deviation (±cents) any engine's depth may reach in this
    /// mode, whatever the rate — see [`crate::dsp::tame_swing`]. A chorus
    /// at full depth and full rate is allowed to be a warble, not a siren.
    #[must_use]
    pub const fn ceiling_cents(&self) -> f64 {
        match self {
            Self::Chorus => 70.0,
            Self::Flanger => 50.0,
            Self::Vibrato => 110.0,
        }
    }
}

/// Chorus engine type.
///
/// **Order is persisted**: presets store the index, so new engines are only
/// ever appended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineType {
    Cubic,
    Bbd,
    Tape,
    Orbit,
    /// Juno-style chorus with triangle LFO + allpass interpolation.
    /// Based on TAL-NoiseMaker / `YKChorus` (`SpotlightKid`) algorithm.
    Juno,
    /// Boss CE-2: one MN3007 bucket brigade, rounded-triangle LFO, the warm
    /// filtered wet at a fixed 50/50.
    Ce2,
    /// Roland Dimension D (SDD-320): two BBDs on one slow triangle in
    /// antiphase, cross-mixed — width without an audible sweep.
    Dimension,
    /// Electro-Harmonix Small Clone: one deep, dark BBD voice — the swirl.
    Clone,
    /// Three-voice rack chorus (Dyno-My-Piano / TC 1210 style): three clean
    /// lines 120° apart, left / centre / right.
    TriChorus,
    /// TC Electronic SCF in pitch-modulation mode: depth is pitch, not
    /// delay, so it stays put as the speed changes; bright and wide.
    Scf,
    /// Walrus Julia: an analogue chorus whose LFO morphs sine → triangle →
    /// random, with the dry ↔ chorus ↔ vibrato blend on the mix.
    Julia,
}

impl EngineType {
    /// Every engine, in persisted index order.
    pub const ALL: [Self; 11] = [
        Self::Cubic,
        Self::Bbd,
        Self::Tape,
        Self::Orbit,
        Self::Juno,
        Self::Ce2,
        Self::Dimension,
        Self::Clone,
        Self::TriChorus,
        Self::Scf,
        Self::Julia,
    ];

    /// Index as persisted (the `engine` parameter).
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// From the persisted index; out of range is the default (Cubic).
    #[must_use]
    pub fn from_index(i: usize) -> Self {
        Self::ALL.get(i).copied().unwrap_or(Self::Cubic)
    }

    /// Short display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cubic => "Cubic",
            Self::Bbd => "BBD",
            Self::Tape => "Tape",
            Self::Orbit => "Orbit",
            Self::Juno => "Juno",
            Self::Ce2 => "CE-2",
            Self::Dimension => "Dimension",
            Self::Clone => "Clone",
            Self::TriChorus => "Tri-Chorus",
            Self::Scf => "SCF",
            Self::Julia => "Julia",
        }
    }
}

/// The (smoothed) controls one engine tick sees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub rate_hz: f64,
    pub depth: f64,
    pub feedback: f64,
    pub color: f64,
    pub width: f64,
    pub effect: EffectType,
    /// Voices per channel for the voice-bank engines (1..4).
    pub voices: usize,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            rate_hz: 1.0,
            depth: 0.5,
            feedback: 0.0,
            color: 0.5,
            width: 1.0,
            effect: EffectType::Chorus,
            voices: 2,
        }
    }
}

/// A complete stereo engine: stereo in, stereo *wet* out. The chain owns the
/// dry path, the mix and the makeup.
pub trait StereoEngine: Send {
    /// Size buffers for `sample_rate`. May allocate.
    fn update(&mut self, sample_rate: f64);
    /// Clear state. Must not allocate.
    fn reset(&mut self);
    /// One frame of wet signal. Must not allocate.
    fn tick(&mut self, l: f64, r: f64, f: &Frame) -> (f64, f64);
    /// The delay the first voice is reading at, ms (display only).
    fn delay_ms(&self) -> f64;
}

/// Common voice trait.
pub trait ChorusEngine: Send {
    fn update(&mut self, sample_rate: f64);
    fn tick(
        &mut self,
        input: f64,
        rate_hz: f64,
        depth: f64,
        feedback: f64,
        color: f64,
        effect_type: EffectType,
    ) -> f64;
    fn reset(&mut self);

    /// The delay this voice is currently reading at, in ms.
    ///
    /// Recorded by `tick`, not recomputed — an editor that drew a *second*
    /// implementation of the LFO would be drawing a picture of the engine
    /// rather than the engine, and the two would drift. See
    /// [`crate::analysis`], which is the only caller.
    fn delay_ms(&self) -> f64;
}

// ─── Cubic Engine ───────────────────────────────────────────────────
//
// Cubic, Tape and Orbit read before they write, so a delay of `d` samples
// is `ModLine::read(d - 1)` (whose 0 is the newest sample). The same
// Catmull-Rom as `DelayLine::read_cubic`, on a power-of-two ring: masks
// instead of four `%` per read.

pub struct CubicVoice {
    delay: ModLine,
    lfo_phase: f64,
    phase_offset: f64,
    sample_rate: f64,
    /// Last delay `tick` read at, in ms — display only.
    last_delay_ms: f64,
}

impl CubicVoice {
    const MAX_DELAY: usize = 48_000 / 20 + 64;
    #[must_use]
    pub fn new(phase_offset: f64) -> Self {
        Self {
            last_delay_ms: 0.0,
            delay: ModLine::new(Self::MAX_DELAY),
            lfo_phase: 0.0,
            phase_offset,
            sample_rate: 48000.0,
        }
    }
}

impl ChorusEngine for CubicVoice {
    fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let needed = (sample_rate * 0.05) as usize + 64;
        self.delay.ensure(needed);
    }

    fn tick(
        &mut self,
        input: f64,
        rate_hz: f64,
        depth: f64,
        feedback: f64,
        _color: f64,
        effect_type: EffectType,
    ) -> f64 {
        self.lfo_phase = (self.lfo_phase + rate_hz / self.sample_rate).fract();
        let lfo = ((self.lfo_phase + self.phase_offset) * 2.0 * PI).sin();
        let base_ms = effect_type.base_delay_ms();
        let depth_ms = effect_type.max_depth_ms() * depth;
        let delay_samples = (depth_ms.mul_add(lfo, base_ms) * 0.001 * self.sample_rate)
            .clamp(1.0, self.delay.max_delay());
        self.last_delay_ms = delay_samples * 1000.0 / self.sample_rate;
        let delayed = self.delay.read(delay_samples - 1.0);
        self.delay
            .write(input + (delayed * feedback).clamp(-1.5, 1.5));
        delayed
    }

    fn delay_ms(&self) -> f64 {
        self.last_delay_ms
    }

    fn reset(&mut self) {
        self.delay.clear();
        self.lfo_phase = 0.0;
    }
}

// ─── BBD Engine ─────────────────────────────────────────────────────

/// A clocked bucket-brigade line whose bandwidth follows its clock.
///
/// A BBD's delay is `stages / (2·clock)`, so sweeping the delay sweeps the
/// clock — and the anti-alias and reconstruction filters that have to sit
/// under it. Here both filters track `clock = STAGES / (2·delay)`: the wet
/// darkens as the delay lengthens and opens as it shortens, which is the
/// "one control doing two things" character this engine is for.
///
/// Modelled as a host-rate modulated delay (Hermite reads) between two
/// clock-tracked 2-pole lowpasses, with a soft saturator at the bucket
/// input — the same decomposition Raffel & Smith (DAFx-10) and Holmes & van
/// Walstijn (DAFx-15) arrive at for chorus-range clocks, where the clock is
/// far above the audio band and the images/aliasing are negligible. (The
/// first version shifted a 512-bucket array at most once per host sample,
/// which pinned the delay at ≥ 10.7 ms, doubled every delay it did reach,
/// and — with its `sin()` filter coefficients folding over past fs/4 —
/// silenced the flanger outright.)
pub struct BbdVoice {
    line: ModLine,
    lfo_phase: f64,
    phase_offset: f64,
    pre: SvfLp,
    post: SvfLp,
    last_out: f64,
    ctl: u32,
    sample_rate: f64,
    /// Last delay `tick` read at, in ms — display only.
    last_delay_ms: f64,
}

impl BbdVoice {
    /// Buckets — the MN3008/MN3207 class (512 stages would be the 3008).
    const STAGES: f64 = 512.0;
    /// Filter coefficients are recomputed every this many samples.
    const CTL: u32 = 8;

    #[must_use]
    pub fn new(phase_offset: f64) -> Self {
        Self {
            last_delay_ms: 0.0,
            line: ModLine::new(2_400),
            lfo_phase: 0.0,
            phase_offset,
            pre: SvfLp::new(6_000.0, 0.6, 48_000.0),
            post: SvfLp::new(6_000.0, 0.7, 48_000.0),
            last_out: 0.0,
            ctl: 0,
            sample_rate: 48_000.0,
        }
    }
}

impl ChorusEngine for BbdVoice {
    fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.line.ensure((sample_rate * 0.03) as usize);
        self.ctl = 0;
    }

    fn tick(
        &mut self,
        input: f64,
        rate_hz: f64,
        depth: f64,
        feedback: f64,
        color: f64,
        effect_type: EffectType,
    ) -> f64 {
        self.lfo_phase = (self.lfo_phase + rate_hz / self.sample_rate).fract();
        let lfo = ((self.lfo_phase + self.phase_offset) * 2.0 * PI).sin();
        let base_ms = effect_type.base_delay_ms();
        let depth_ms = effect_type.max_depth_ms() * depth;
        let delay_ms = depth_ms.mul_add(lfo, base_ms).max(0.25);
        self.last_delay_ms = delay_ms;
        if self.ctl == 0 {
            // Clock → the anti-alias filter under it (fc/3) and the
            // reconstruction filter, which Colour pulls from fc/6 up to fc/2.
            let clock = Self::STAGES / (2.0 * delay_ms * 0.001);
            self.pre.set(clock / 3.0, 0.6, self.sample_rate);
            self.post.set(
                clock / color.mul_add(-4.0, 6.0).max(1.5),
                0.7,
                self.sample_rate,
            );
        }
        self.ctl = (self.ctl + 1) % Self::CTL;
        let fb = soft_sat(self.last_out * feedback, 0.3);
        let x = soft_sat(self.pre.tick(input + fb), 0.08);
        self.line.write(x);
        let y = self.line.read(delay_ms * 0.001 * self.sample_rate);
        self.last_out = self.post.tick(y);
        self.last_out
    }

    fn delay_ms(&self) -> f64 {
        self.last_delay_ms
    }

    fn reset(&mut self) {
        self.line.clear();
        self.pre.reset();
        self.post.reset();
        self.last_out = 0.0;
        self.lfo_phase = 0.0;
        self.ctl = 0;
    }
}

// ─── Tape Engine ────────────────────────────────────────────────────

pub struct TapeVoice {
    delay: ModLine,
    lfo_phase: f64,
    phase_offset: f64,
    wow_phase: f64,
    flutter_phase: f64,
    tone_lp: f64,
    sample_rate: f64,
    /// Last delay `tick` read at, in ms — display only.
    last_delay_ms: f64,
}

impl TapeVoice {
    const MAX_DELAY: usize = 48_000 / 20 + 64;
    #[must_use]
    pub fn new(phase_offset: f64) -> Self {
        Self {
            last_delay_ms: 0.0,
            delay: ModLine::new(Self::MAX_DELAY),
            lfo_phase: 0.0,
            phase_offset,
            wow_phase: 0.0,
            flutter_phase: 0.0,
            tone_lp: 0.0,
            sample_rate: 48000.0,
        }
    }
}

impl ChorusEngine for TapeVoice {
    fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let needed = (sample_rate * 0.05) as usize + 64;
        self.delay.ensure(needed);
    }

    fn tick(
        &mut self,
        input: f64,
        rate_hz: f64,
        depth: f64,
        feedback: f64,
        color: f64,
        effect_type: EffectType,
    ) -> f64 {
        self.lfo_phase = (self.lfo_phase + rate_hz / self.sample_rate).fract();
        let lfo = ((self.lfo_phase + self.phase_offset) * 2.0 * PI).sin();
        self.wow_phase = (self.wow_phase + 0.33 / self.sample_rate).fract();
        let wow = (self.wow_phase * 2.0 * PI).sin() * depth * 0.3;
        self.flutter_phase = (self.flutter_phase + 5.8 / self.sample_rate).fract();
        let flutter = (self.flutter_phase * 6.0 * PI).sin().mul_add(
            0.1,
            (self.flutter_phase * 2.0 * PI)
                .sin()
                .mul_add(0.6, (self.flutter_phase * 4.0 * PI).sin() * 0.3),
        ) * depth
            * 0.15;
        let base_ms = effect_type.base_delay_ms();
        let depth_ms = effect_type.max_depth_ms() * depth;
        let delay_ms = depth_ms.mul_add(lfo, base_ms) + wow + flutter;
        self.last_delay_ms = delay_ms;
        let delay_samples =
            (delay_ms * 0.001 * self.sample_rate).clamp(1.0, self.delay.max_delay());
        let delayed = self.delay.read(delay_samples - 1.0);
        // Drive saturates without turning the level up: `tanh(d·x)/d` is
        // unity for small signals. (Plain `tanh(d·x)` played the wet 6 dB
        // hot at noon and 9.5 dB at full Colour on a quiet guitar — a level
        // that depended on how hard you picked.)
        let drive = color.mul_add(2.0, 1.0);
        let saturated_input = (input * drive).tanh() / drive;
        let fb = (delayed * feedback).tanh();
        self.delay.write(saturated_input + fb.clamp(-1.5, 1.5));
        let cutoff = color.mul_add(11000.0, 3000.0);
        let lp_coeff = (2.0 * PI * cutoff / self.sample_rate)
            .sin()
            .clamp(0.0, 0.99);
        self.tone_lp = flush((delayed - self.tone_lp).mul_add(lp_coeff, self.tone_lp));
        self.tone_lp
    }

    fn delay_ms(&self) -> f64 {
        self.last_delay_ms
    }

    fn reset(&mut self) {
        self.delay.clear();
        self.lfo_phase = 0.0;
        self.wow_phase = 0.0;
        self.flutter_phase = 0.0;
        self.tone_lp = 0.0;
    }
}

// ─── Orbit Engine ───────────────────────────────────────────────────

pub struct OrbitVoice {
    delay: ModLine,
    orbit_phase: f64,
    theta: f64,
    phase_offset: f64,
    sample_rate: f64,
    /// Last delay `tick` read at, in ms — display only.
    last_delay_ms: f64,
}

impl OrbitVoice {
    const MAX_DELAY: usize = 48_000 / 20 + 64;
    #[must_use]
    pub fn new(phase_offset: f64) -> Self {
        Self {
            last_delay_ms: 0.0,
            delay: ModLine::new(Self::MAX_DELAY),
            orbit_phase: 0.0,
            theta: 0.0,
            phase_offset,
            sample_rate: 48000.0,
        }
    }
}

impl ChorusEngine for OrbitVoice {
    fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let needed = (sample_rate * 0.05) as usize + 64;
        self.delay.ensure(needed);
    }

    fn tick(
        &mut self,
        input: f64,
        rate_hz: f64,
        depth: f64,
        feedback: f64,
        color: f64,
        effect_type: EffectType,
    ) -> f64 {
        let inc = rate_hz / self.sample_rate;
        self.orbit_phase = (self.orbit_phase + inc).fract();
        self.theta += inc * 0.37;
        if self.theta > 1.0 {
            self.theta -= 1.0;
        }
        let phi = (self.orbit_phase + self.phase_offset) * 2.0 * PI;
        let theta_rad = self.theta * 2.0 * PI;
        let eccentricity = color * 0.8;
        let x = phi.sin();
        let y = (1.0 - eccentricity) * phi.cos();
        let proj = x * theta_rad.cos() + y * theta_rad.sin();
        let proj2 = x * PI.mul_add(0.5, theta_rad).cos() + y * PI.mul_add(0.5, theta_rad).sin();
        let base_ms = effect_type.base_delay_ms();
        let depth_ms = effect_type.max_depth_ms() * depth;
        let delay1_ms = (depth_ms * proj).mul_add(0.7, base_ms);
        self.last_delay_ms = delay1_ms;
        let delay2_ms = (depth_ms * proj2).mul_add(0.5, base_ms);
        let max_delay = self.delay.max_delay();
        let d1 = (delay1_ms * 0.001 * self.sample_rate).clamp(1.0, max_delay);
        let d2 = (delay2_ms * 0.001 * self.sample_rate).clamp(1.0, max_delay);
        let tap1 = self.delay.read(d1 - 1.0);
        let tap2 = self.delay.read(d2 - 1.0);
        let blended = tap1 * 0.6 + tap2 * 0.4;
        let fb = (blended * feedback).clamp(-1.5, 1.5);
        self.delay.write(input + fb);
        blended
    }

    fn delay_ms(&self) -> f64 {
        self.last_delay_ms
    }

    fn reset(&mut self) {
        self.delay.clear();
        self.orbit_phase = 0.0;
        self.theta = 0.0;
    }
}

// ─── Juno Engine ───────────────────────────────────────────────────

/// Juno-style chorus: triangle LFO + allpass interpolation + DC blocking.
/// Based on TAL-NoiseMaker / `YKChorus` (`SpotlightKid`, GPL-2.0).
///
/// Keeps first-order allpass interpolation (not cubic) on purpose — the
/// allpass state `z1` is part of the original BBD-flavored character and
/// feeds the feedback path.
pub struct JunoVoice {
    delay: DelayLine,
    buf_len: usize,
    lfo_phase: f64,
    lfo_sign: f64,
    phase_offset: f64,
    z1: f64,
    lp_state: f64,
    dc: DcBlocker,
    sample_rate: f64,
    /// Last delay `tick` read at, in ms — display only.
    last_delay_ms: f64,
}

impl JunoVoice {
    const DELAY_MS: f64 = 7.0;
    const LP_CUTOFF: f64 = 0.95;
    /// Original fixed DC-blocker pole (R = 0.995 at 48 kHz).
    const DC_R: f64 = 0.995;

    #[must_use]
    pub fn new(phase_offset: f64) -> Self {
        let lfo_phase = phase_offset.mul_add(2.0, -1.0);
        let mut voice = Self {
            last_delay_ms: 0.0,
            delay: DelayLine::new(2048),
            buf_len: 1024,
            lfo_phase,
            lfo_sign: if lfo_phase >= 0.0 { 1.0 } else { -1.0 },
            phase_offset,
            z1: 0.0,
            lp_state: 0.0,
            dc: DcBlocker::new(),
            sample_rate: 48000.0,
        };
        voice.retune_dc();
        voice
    }

    /// Keep the original pole R = 0.995 regardless of sample rate:
    /// R = 1 - 2*pi*fc/sr  =>  fc = (1 - R) * sr / (2*pi).
    fn retune_dc(&mut self) {
        let fc = (1.0 - Self::DC_R) * self.sample_rate / (2.0 * PI);
        self.dc.set_cutoff(fc, self.sample_rate);
    }

    #[inline]
    fn next_lfo(&mut self, rate_hz: f64) -> f64 {
        let step = 4.0 * rate_hz / self.sample_rate;
        self.lfo_phase += self.lfo_sign * step;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase = 2.0 - self.lfo_phase;
            self.lfo_sign = -1.0;
        } else if self.lfo_phase <= -1.0 {
            self.lfo_phase = -2.0 - self.lfo_phase;
            self.lfo_sign = 1.0;
        }
        self.lfo_phase
    }
}

impl ChorusEngine for JunoVoice {
    fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let delay_samples = (Self::DELAY_MS * sample_rate * 0.001).floor() as usize;
        self.buf_len = delay_samples * 2;
        if self.delay.len() < self.buf_len + 4 {
            self.delay = DelayLine::new(self.buf_len + 4);
        }
        self.retune_dc();
    }

    fn tick(
        &mut self,
        input: f64,
        rate_hz: f64,
        depth: f64,
        feedback: f64,
        color: f64,
        effect_type: EffectType,
    ) -> f64 {
        let fb_sample = self.z1 * feedback;
        self.delay.write(input + fb_sample.clamp(-1.5, 1.5));

        let lfo = self.next_lfo(rate_hz);

        // Map LFO to read offset — lfo*0.3+0.4 maps [-1,+1] to [0.1,0.7]
        let base_delay = match effect_type {
            EffectType::Chorus | EffectType::Vibrato => Self::DELAY_MS * self.sample_rate * 0.001,
            EffectType::Flanger => Self::DELAY_MS * self.sample_rate * 0.001 * 0.3,
        };

        let offset = (lfo * 0.3).mul_add(depth, 0.4) * base_delay;
        let offset = offset.clamp(1.0, (self.buf_len.saturating_sub(2)) as f64);
        self.last_delay_ms = offset * 1000.0 / self.sample_rate;
        let int_offset = offset.floor() as usize;
        let frac = offset - offset.floor();

        // The just-written sample is 1 back from the write head, so an
        // original ring offset of k maps to DelayLine::read(k + 1).
        let x0 = self.delay.read(int_offset + 1);
        let x1 = self.delay.read(int_offset + 2);

        // First-order allpass interpolation
        let coeff = 1.0 - frac;
        let delayed = flush(x1 + coeff * x0 - coeff * self.z1);
        self.z1 = delayed;

        // One-pole lowpass post-filter (color controls brightness)
        let cutoff_param = Self::LP_CUTOFF * color.mul_add(0.5, 0.5);
        let p = (cutoff_param * 0.98).powi(4);
        self.lp_state = flush((1.0 - p).mul_add(delayed, p * self.lp_state));

        self.dc.tick(self.lp_state)
    }

    fn delay_ms(&self) -> f64 {
        self.last_delay_ms
    }

    fn reset(&mut self) {
        self.delay.clear();
        self.lfo_phase = self.phase_offset.mul_add(2.0, -1.0);
        self.lfo_sign = if self.lfo_phase >= 0.0 { 1.0 } else { -1.0 };
        self.z1 = 0.0;
        self.lp_state = 0.0;
        self.dc.reset();
    }
}

// ─── Voice bank: the original five as stereo engines ───────────────

/// Voices per channel a [`VoiceBank`] holds.
pub const MAX_VOICES: usize = 4;

fn legacy_voice(engine: EngineType, offset: f64) -> Box<dyn ChorusEngine> {
    match engine {
        EngineType::Bbd => Box::new(BbdVoice::new(offset)),
        EngineType::Tape => Box::new(TapeVoice::new(offset)),
        EngineType::Orbit => Box::new(OrbitVoice::new(offset)),
        EngineType::Juno => Box::new(JunoVoice::new(offset)),
        _ => Box::new(CubicVoice::new(offset)),
    }
}

/// The depth a legacy voice is given for knob `depth` at `rate_hz`.
///
/// The voices scale depth linearly to a swing in ms; the knob is shaped
/// (a 1.5 power on the chorus, so its first half is where the classic
/// 10–30 cent choruses live) and the swing is held under the mode's pitch
/// ceiling, then handed back to the voice as the depth that produces it.
#[must_use]
pub fn legacy_depth(engine: EngineType, effect: EffectType, depth: f64, rate_hz: f64) -> f64 {
    let d = depth.clamp(0.0, 1.0);
    if d <= 0.0 {
        return 0.0;
    }
    let (full_swing_ms, slope, shaped) = if engine == EngineType::Juno {
        // ±0.3·depth of a 7 ms line, on a triangle.
        let full = if effect == EffectType::Flanger {
            0.3 * JunoVoice::DELAY_MS * 0.3
        } else {
            0.3 * JunoVoice::DELAY_MS
        };
        (full, 4.0, d)
    } else {
        let shaped = match effect {
            EffectType::Chorus => d.powf(1.5),
            EffectType::Vibrato => d.powf(1.25),
            EffectType::Flanger => d,
        };
        (effect.max_depth_ms(), 2.0 * PI, shaped)
    };
    let swing = shaped * full_swing_ms * 0.001;
    let tamed = crate::dsp::tame_swing(swing, rate_hz, slope, effect.ceiling_cents());
    shaped * (tamed / swing)
}

/// Up to [`MAX_VOICES`] mono voices per channel, summed in power.
///
/// Voice count changes ramp: a voice switched in starts from a cleared line
/// and fades up, one switched out fades down, and the 1/√Σg² normalisation
/// follows the gains — no jump in level or a burst of stale audio.
pub struct VoiceBank {
    engine: EngineType,
    l: Vec<Box<dyn ChorusEngine>>,
    r: Vec<Box<dyn ChorusEngine>>,
    gain: [f64; MAX_VOICES],
    ramp: f64,
    /// The voices' depth, recomputed every [`Self::CTL`] samples and ramped.
    depth: f64,
    depth_step: f64,
    fb_comp: f64,
    ctl: usize,
    primed: bool,
    /// Where the input sits between the sides: smoothed power per side and
    /// its coefficient (~50 ms). What the wet's centre is — a guitar panned
    /// left keeps its chorus left however narrow the width, where the
    /// plain mid of the wet would pull it into both sides.
    bal: (f64, f64),
    bal_k: f64,
}

impl VoiceBank {
    #[must_use]
    pub fn new(engine: EngineType) -> Self {
        Self {
            engine,
            l: create_voices(engine, MAX_VOICES),
            r: (0..MAX_VOICES)
                // +90° on the right.
                .map(|i| legacy_voice(engine, i as f64 / MAX_VOICES as f64 + 0.25))
                .collect(),
            gain: [1.0, 1.0, 0.0, 0.0],
            ramp: 1.0 / 960.0,
            depth: 0.0,
            depth_step: 0.0,
            fb_comp: 1.0,
            ctl: 0,
            primed: false,
            bal: (0.0, 0.0),
            bal_k: 1.0 - (-1.0 / (0.05 * 48_000.0f64)).exp(),
        }
    }

    const CTL: usize = 16;

    /// The input's share per side (each 0..=1, `(0.5, 0.5)` centred — and
    /// exactly that when both sides carry the same signal).
    fn balance(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        self.bal.0 = (in_l * in_l - self.bal.0).mul_add(self.bal_k, self.bal.0);
        self.bal.1 = (in_r * in_r - self.bal.1).mul_add(self.bal_k, self.bal.1);
        let (l, r) = (self.bal.0.max(0.0).sqrt(), self.bal.1.max(0.0).sqrt());
        let sum = l + r;
        if sum <= 1.0e-12 { (0.5, 0.5) } else { (l / sum, r / sum) }
    }
}

impl StereoEngine for VoiceBank {
    fn update(&mut self, sample_rate: f64) {
        for v in self.l.iter_mut().chain(self.r.iter_mut()) {
            v.update(sample_rate);
        }
        // 20 ms voice fades.
        self.ramp = 1.0 / (0.02 * sample_rate).max(1.0);
        self.bal_k = 1.0 - (-1.0 / (0.05 * sample_rate).max(1.0)).exp();
    }

    fn reset(&mut self) {
        for v in self.l.iter_mut().chain(self.r.iter_mut()) {
            v.reset();
        }
        self.primed = false;
        self.ctl = 0;
    }

    fn tick(&mut self, in_l: f64, in_r: f64, frame: &Frame) -> (f64, f64) {
        let f = frame;
        // A vibrato is one voice: two voices at different phases, with no
        // dry, are a chorus — and so are a left and a right at different
        // phases once they meet in a mono rig or a room. One voice, on the
        // mono input, to both sides.
        let vibrato = f.effect == EffectType::Vibrato;
        let (bl, br) = self.balance(in_l, in_r);
        let n = if vibrato {
            1
        } else {
            f.voices.clamp(1, MAX_VOICES)
        };
        let (in_l, in_r) = if vibrato {
            let mono = 0.5 * (in_l + in_r);
            (mono, mono)
        } else {
            (in_l, in_r)
        };
        if self.ctl == 0 {
            self.fb_comp = (1.0 - f.feedback.clamp(0.0, 0.98)).powf(0.6);
            let target = legacy_depth(self.engine, f.effect, f.depth, f.rate_hz);
            if self.primed {
                self.depth_step = (target - self.depth) / Self::CTL as f64;
            } else {
                self.depth = target;
                self.depth_step = 0.0;
                self.primed = true;
            }
        }
        self.ctl = (self.ctl + 1) % Self::CTL;
        self.depth += self.depth_step;
        let depth = self.depth;
        let (mut wl, mut wr, mut power) = (0.0, 0.0, 0.0);
        for v in 0..MAX_VOICES {
            let target = if v < n { 1.0 } else { 0.0 };
            let g = self.gain[v];
            if g <= 0.0 && target <= 0.0 {
                continue;
            }
            if g <= 0.0 {
                // Coming back in: start from silence, not from whatever
                // this voice's line held when it was switched out.
                self.l[v].reset();
                self.r[v].reset();
            }
            let g = if target > g {
                (g + self.ramp).min(1.0)
            } else if target < g {
                (g - self.ramp).max(0.0)
            } else {
                g
            };
            self.gain[v] = g;
            wl += g * self.l[v].tick(in_l, f.rate_hz, depth, f.feedback, f.color, f.effect);
            if !vibrato {
                wr += g * self.r[v].tick(in_r, f.rate_hz, depth, f.feedback, f.color, f.effect);
            }
            power += g * g;
        }
        if vibrato {
            // One voice on the sum, placed where the input is.
            let w = wl;
            wl = w * 2.0 * bl;
            wr = w * 2.0 * br;
        }
        // The voices are modulated apart, so they add in power. Feedback's
        // gain is taken back out: a comb fed back at g lifts the lows it
        // reinforces by up to 1/(1−g) — +8 dB on pink noise at g = 0.7 —
        // and (1−g)^0.6 holds it within ~1.5 dB of the dry level on both
        // pink noise and a guitar.
        let norm = self.fb_comp / power.max(1.0e-6).sqrt();
        let (wl, wr) = (wl * norm, wr * norm);
        // Width: the wet's spread about the input's own place, scaled. The
        // wet together, shared out as the input is shared between the sides
        // (half each for a centred input: the plain mid), is what zero width
        // narrows to — so it narrows the chorus, never the pan.
        let sum = wl + wr;
        let (cl, cr) = (sum * bl, sum * br);
        let w = f.width.clamp(0.0, 1.0);
        ((wl - cl).mul_add(w, cl), (wr - cr).mul_add(w, cr))
    }

    fn delay_ms(&self) -> f64 {
        self.l.first().map_or(0.0, |v| v.delay_ms())
    }
}

/// Create a vector of voices for one channel (the original five engines;
/// any other engine type gets Cubic voices).
#[must_use]
pub fn create_voices(engine: EngineType, count: usize) -> Vec<Box<dyn ChorusEngine>> {
    (0..count)
        .map(|i| {
            let offset = i as f64 / count as f64;
            legacy_voice(engine, offset)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const SR: f64 = 48000.0;

    fn test_engine_produces_output(mut voice: Box<dyn ChorusEngine>) {
        voice.update(SR);
        let mut has_output = false;
        for i in 0..9600 {
            let s = (2.0 * PI * 440.0 * f64::from(i) / SR).sin() * 0.5;
            let out = voice.tick(s, 1.0, 0.5, 0.0, 0.5, EffectType::Chorus);
            if out.abs() > 0.01 {
                has_output = true;
            }
        }
        assert!(has_output, "Engine should produce output");
    }

    fn test_engine_no_nan(mut voice: Box<dyn ChorusEngine>) {
        voice.update(SR);
        for i in 0..48000 {
            let s = (2.0 * PI * 440.0 * f64::from(i) / SR).sin() * 0.8;
            let out = voice.tick(s, 3.0, 1.0, 0.8, 0.5, EffectType::Flanger);
            assert!(out.is_finite(), "NaN at {i}");
        }
    }

    #[test]
    fn cubic_produces_output() {
        test_engine_produces_output(Box::new(CubicVoice::new(0.0)));
    }
    #[test]
    fn cubic_no_nan() {
        test_engine_no_nan(Box::new(CubicVoice::new(0.0)));
    }
    #[test]
    fn bbd_produces_output() {
        test_engine_produces_output(Box::new(BbdVoice::new(0.0)));
    }
    #[test]
    fn bbd_no_nan() {
        test_engine_no_nan(Box::new(BbdVoice::new(0.0)));
    }
    #[test]
    fn tape_produces_output() {
        test_engine_produces_output(Box::new(TapeVoice::new(0.0)));
    }
    #[test]
    fn tape_no_nan() {
        test_engine_no_nan(Box::new(TapeVoice::new(0.0)));
    }
    #[test]
    fn orbit_produces_output() {
        test_engine_produces_output(Box::new(OrbitVoice::new(0.0)));
    }
    #[test]
    fn orbit_no_nan() {
        test_engine_no_nan(Box::new(OrbitVoice::new(0.0)));
    }
    #[test]
    fn juno_produces_output() {
        test_engine_produces_output(Box::new(JunoVoice::new(0.0)));
    }
    #[test]
    fn juno_no_nan() {
        test_engine_no_nan(Box::new(JunoVoice::new(0.0)));
    }

    #[test]
    fn juno_stereo_phase_offset() {
        let mut voice_l = JunoVoice::new(1.0);
        let mut voice_r = JunoVoice::new(0.0);
        voice_l.update(SR);
        voice_r.update(SR);
        let mut diff_sum: f64 = 0.0;
        for i in 0..4800 {
            let s = (2.0 * PI * 440.0 * f64::from(i) / SR).sin() * 0.5;
            let l = voice_l.tick(s, 0.5, 0.5, 0.0, 0.5, EffectType::Chorus);
            let r = voice_r.tick(s, 0.5, 0.5, 0.0, 0.5, EffectType::Chorus);
            diff_sum += (l - r).abs();
        }
        assert!(
            diff_sum / 4800.0 > 0.001,
            "Stereo voices should differ: avg diff = {}",
            diff_sum / 4800.0
        );
    }

    #[test]
    fn engines_sound_different() {
        let mut cubic = CubicVoice::new(0.0);
        let mut bbd = BbdVoice::new(0.0);
        let mut tape = TapeVoice::new(0.0);
        let mut orbit = OrbitVoice::new(0.0);
        cubic.update(SR);
        bbd.update(SR);
        tape.update(SR);
        orbit.update(SR);
        let mut out_cubic = Vec::new();
        let mut out_bbd = Vec::new();
        let mut out_tape = Vec::new();
        let mut out_orbit = Vec::new();
        for i in 0..9600 {
            let s = (2.0 * PI * 440.0 * f64::from(i) / SR).sin() * 0.5;
            out_cubic.push(cubic.tick(s, 1.0, 0.5, 0.0, 0.5, EffectType::Chorus));
            out_bbd.push(bbd.tick(s, 1.0, 0.5, 0.0, 0.5, EffectType::Chorus));
            out_tape.push(tape.tick(s, 1.0, 0.5, 0.0, 0.5, EffectType::Chorus));
            out_orbit.push(orbit.tick(s, 1.0, 0.5, 0.0, 0.5, EffectType::Chorus));
        }
        let diff_cb: f64 = out_cubic
            .iter()
            .zip(out_bbd.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / 9600.0;
        let diff_ct: f64 = out_cubic
            .iter()
            .zip(out_tape.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / 9600.0;
        let diff_co: f64 = out_cubic
            .iter()
            .zip(out_orbit.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / 9600.0;
        assert!(diff_cb > 0.001, "Cubic vs BBD should differ: {diff_cb}");
        assert!(diff_ct > 0.001, "Cubic vs Tape should differ: {diff_ct}");
        assert!(diff_co > 0.001, "Cubic vs Orbit should differ: {diff_co}");
    }

    #[test]
    fn color_affects_bbd_tone() {
        let mut dark = BbdVoice::new(0.0);
        let mut bright = BbdVoice::new(0.0);
        dark.update(SR);
        bright.update(SR);
        let mut energy_dark: f64 = 0.0;
        let mut energy_bright: f64 = 0.0;
        for i in 0..9600 {
            let s = (2.0 * PI * 440.0 * f64::from(i) / SR).sin() * 0.5;
            let d = dark.tick(s, 1.0, 0.5, 0.0, 0.0, EffectType::Chorus);
            let b = bright.tick(s, 1.0, 0.5, 0.0, 1.0, EffectType::Chorus);
            energy_dark += d * d;
            energy_bright += b * b;
        }
        assert!(
            energy_bright > energy_dark * 0.8,
            "Bright color should pass more: dark={energy_dark:.4}, bright={energy_bright:.4}"
        );
    }
}
