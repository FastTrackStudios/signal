//! Native **Filter** block — a stereo state-variable filter (Cytomic/Zavalishin
//! TPT SVF), the built-in DSP for `BlockType::Filter`.
//!
//! Covers the Nord filter menu's core shapes (LP/HP/BP; LP24 later by cascading
//! two sections). Defaults are transparent-ish (LP just under Nyquist) so a
//! placeholder-parameterized preset keeps passing audio.

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
}

impl FilterMode {
    /// Its number (the `mode` param): 0 low-pass, 1 high-pass, 2 band-pass,
    /// 3 notch.
    #[must_use]
    pub fn index(self) -> u32 {
        match self {
            Self::Lowpass => 0,
            Self::Highpass => 1,
            Self::Bandpass => 2,
            Self::Notch => 3,
        }
    }

    #[must_use]
    pub fn from_index(i: u32) -> Self {
        match i {
            1 => Self::Highpass,
            2 => Self::Bandpass,
            3 => Self::Notch,
            _ => Self::Lowpass,
        }
    }

    /// Parse a mode name (`"lp"`, `"highpass"`, `"bp"`, `"notch"`, …).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let k = s.to_ascii_lowercase();
        Some(match () {
            () if k.starts_with("lp") || k.starts_with("low") => Self::Lowpass,
            () if k.starts_with("hp") || k.starts_with("high") => Self::Highpass,
            () if k.starts_with("bp") || k.starts_with("band") => Self::Bandpass,
            () if k.starts_with("notch") => Self::Notch,
            () => return None,
        })
    }
}

/// One TPT state-variable filter section (mono).
#[derive(Clone, Copy, Debug, Default)]
pub struct Svf {
    // Coefficients.
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
    // State.
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// Set cutoff/resonance. `q` ≥ ~0.5; 0.707 = flat.
    pub fn set(&mut self, cutoff_hz: f32, q: f32, sample_rate: f32) {
        let sr = sample_rate.max(1.0);
        let fc = cutoff_hz.clamp(10.0, sr * 0.45);
        let g = (core::f32::consts::PI * fc / sr).tan();
        let k = 1.0 / q.max(0.1);
        let a1 = 1.0 / (1.0 + g * (g + k));
        self.a1 = a1;
        self.a2 = g * a1;
        self.a3 = g * self.a2;
        self.k = k;
    }

    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }

    /// Process one sample, returning `(lowpass, bandpass, highpass)`.
    #[inline]
    pub fn tick(&mut self, v0: f32) -> (f32, f32, f32) {
        let v3 = v0 - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, v0 - self.k * v1 - v2)
    }
}

/// One TPT (zero-delay) one-pole section (mono): a true 6 dB/oct stage, for
/// odd pole counts and the ladder.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnePole {
    /// `g / (1 + g)`, `g = tan(π·fc/sr)`.
    big_g: f32,
    s: f32,
}

impl OnePole {
    pub fn set(&mut self, cutoff_hz: f32, sample_rate: f32) {
        let sr = sample_rate.max(1.0);
        let g = (core::f32::consts::PI * cutoff_hz.clamp(10.0, sr * 0.45) / sr).tan();
        self.big_g = g / (1.0 + g);
    }

    pub fn reset(&mut self) {
        self.s = 0.0;
    }

    /// Process one sample, returning `(lowpass, highpass)`.
    #[inline]
    pub fn tick(&mut self, x: f32) -> (f32, f32) {
        let v = (x - self.s) * self.big_g;
        let lp = v + self.s;
        self.s = lp + v;
        (lp, x - lp)
    }
}

/// Where the ladder's loop saturator clips: oscillator-level signals (±1)
/// pass it nearly linearly; a resonating loop is held to about ±2.
const LADDER_HEADROOM: f32 = 2.0;

/// A saturating ladder: `stages` TPT one-poles (lowpass, or highpass for the
/// HPF ladders) inside one resonance feedback loop, solved without a unit
/// delay (Zavalishin's zero-delay feedback) and saturated by a tanh at the
/// loop's input — Moog-style when 4 stages.
///
/// Feedback costs passband level (a factor `1 / (1 + k)`); `comp` restores a
/// share of it (0 = none, the analog behavior; 1 = all).
///
/// The "character" engine behind the Juicy/Moogie/OB/Jupiter/FATBOY families.
#[derive(Clone, Copy, Debug)]
pub struct Ladder {
    stage: [OnePole; 8],
    stages: usize,
    highpass: bool,
    k: f32,
    comp: f32,
}

impl Default for Ladder {
    fn default() -> Self {
        Self {
            stage: [OnePole::default(); 8],
            stages: 4,
            highpass: false,
            k: 0.0,
            comp: 0.5,
        }
    }
}

impl Ladder {
    /// Cutoff and resonance, 0..1 → feedback 0..3.8 (self-oscillation ≈ 4
    /// for 4 stages).
    pub fn set(&mut self, cutoff_hz: f32, resonance: f32, sample_rate: f32) {
        self.set_feedback(cutoff_hz, resonance.clamp(0.0, 1.0) * 3.8, sample_rate);
    }

    /// Cutoff and raw loop feedback `k`.
    pub fn set_feedback(&mut self, cutoff_hz: f32, k: f32, sample_rate: f32) {
        for st in &mut self.stage {
            st.set(cutoff_hz, sample_rate);
        }
        self.k = k.max(0.0);
    }

    /// Stage count (1..=8), highpass stages, and passband compensation.
    pub fn configure(&mut self, stages: usize, highpass: bool, comp: f32) {
        self.stages = stages.clamp(1, 8);
        self.highpass = highpass;
        self.comp = comp;
    }

    pub fn reset(&mut self) {
        for st in &mut self.stage {
            st.reset();
        }
    }

    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        // Each stage is affine in its input: y = a·x + b (a = G for a
        // lowpass, 1 − G for a highpass; b from the stage state). Chain them
        // to solve the loop instantaneously.
        let n = self.stages;
        let (mut gain, mut offset) = (1.0f32, 0.0f32);
        for st in &self.stage[..n] {
            let g = st.big_g;
            let (a, b) = if self.highpass {
                (1.0 - g, -(1.0 - g) * st.s)
            } else {
                (g, (1.0 - g) * st.s)
            };
            gain *= a;
            offset = a * offset + b;
        }
        // The saturator clips the loop softly at ±LADDER_HEADROOM.
        let u = (x - self.k * offset) / (1.0 + self.k * gain);
        let u = (u / LADDER_HEADROOM).tanh() * LADDER_HEADROOM;
        let mut y = u;
        for st in &mut self.stage[..n] {
            let (lp, hp) = st.tick(y);
            y = if self.highpass { hp } else { lp };
        }
        y * (1.0 + self.k * self.comp)
    }
}

/// Which engine realizes the filter: the clean SVF cascade or the
/// saturating ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FilterCharacter {
    #[default]
    Clean,
    Ladder,
}

/// How the 0..1 resonance knob reaches the engine, per filter model: the
/// first section's Q for the SVF cascade (geometric, `lo·(hi/lo)^(r^curve)`)
/// or the loop feedback for the ladder (linear, `lo + (hi−lo)·r^curve`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResonanceMap {
    pub lo: f32,
    pub hi: f32,
    pub curve: f32,
}

/// The `Filter` block: a stereo multi-pole filter — an SVF cascade (12 dB
/// sections plus a one-pole for odd orders; resonance on the first section,
/// the rest flat so the cascade doesn't compound Q) or a ladder.
pub struct NativeFilter {
    sample_rate: f32,
    mode: FilterMode,
    cutoff_hz: f32,
    /// The resonance knob, 0..1, and its model mapping (`None`: the generic
    /// Q 0.5..12 / feedback 0..3.8 curves).
    resonance: f32,
    res_map: Option<ResonanceMap>,
    /// Corner multiplier at full resonance (geometric in the knob): some
    /// modelled filters' corners sink as resonance rises.
    res_shift: f32,
    /// How fast the shift arrives: `shift^(resonance^curve)`.
    res_shift_curve: f32,
    poles: u32,
    character: FilterCharacter,
    left: [Svf; 4],
    right: [Svf; 4],
    odd_l: OnePole,
    odd_r: OnePole,
    ladder_l: Ladder,
    ladder_r: Ladder,
    /// Output gain (linear): a model's passband level.
    gain: f32,
    /// An emulated synth's own cutoff knob: `(setting, Hz)` points over
    /// settings −0.5..1.0 (log-interpolated). With one, param 4 (`knob`,
    /// 0..1 over that range) sets the cutoff, so modulation moves the knob
    /// the way the original's does rather than in octaves.
    taper: Vec<(f32, f32)>,
    knob: f32,
    /// Drive into the filter, 0..1 (0 = clean), and the wet mix, 0..1
    /// (1 = only the filtered signal).
    drive: f32,
    mix: f32,
    /// An envelope filter (auto-wah, a synth's per-note sweep): how far the
    /// playing's envelope moves the cutoff, in octaves (negative sweeps
    /// down; 0 = a static filter), how hard the playing drives it (0..1),
    /// and how fast it follows, attack and release (ms).
    env_octaves: f32,
    env_sens: f32,
    env_attack_ms: f32,
    env_release_ms: f32,
    /// The follower's level, and its per-sample coefficients.
    env: f32,
    env_up: f32,
    env_down: f32,
    prepared: bool,
}

impl NativeFilter {
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        let mut f = Self {
            sample_rate: sample_rate.max(1) as f32,
            mode: FilterMode::Lowpass,
            cutoff_hz: 20_000.0,
            resonance: Self::norm_from_q(core::f32::consts::FRAC_1_SQRT_2),
            res_map: None,
            res_shift: 1.0,
            res_shift_curve: 1.0,
            poles: 2,
            character: FilterCharacter::Clean,
            left: [Svf::default(); 4],
            right: [Svf::default(); 4],
            odd_l: OnePole::default(),
            odd_r: OnePole::default(),
            ladder_l: Ladder::default(),
            ladder_r: Ladder::default(),
            gain: 1.0,
            taper: Vec::new(),
            knob: 0.0,
            drive: 0.0,
            mix: 1.0,
            env_octaves: 0.0,
            env_sens: 0.5,
            env_attack_ms: 4.0,
            env_release_ms: 120.0,
            env: 0.0,
            env_up: 0.0,
            env_down: 0.0,
            prepared: false,
        };
        f.update_env();
        f.update_coeffs();
        f
    }

    /// Select the saturating ladder engine (lowpass and highpass; other
    /// modes fall back to the clean cascade).
    #[must_use]
    pub fn with_character(mut self, character: FilterCharacter) -> Self {
        self.character = character;
        self.update_coeffs();
        self
    }

    /// Pole count 1..=8: 12 dB sections plus a one-pole for odd counts (the
    /// ladder: one stage per pole).
    #[must_use]
    pub fn with_poles(mut self, poles: u32) -> Self {
        self.poles = poles.clamp(1, 8);
        self.update_coeffs();
        self
    }

    #[must_use]
    pub fn with_mode(mut self, mode: FilterMode) -> Self {
        self.mode = mode;
        self.update_coeffs();
        self
    }

    #[must_use]
    pub fn with_cutoff(mut self, hz: f32) -> Self {
        self.cutoff_hz = hz;
        self.update_coeffs();
        self
    }

    /// Set the resonance as a Q (generic mapping).
    #[must_use]
    pub fn with_q(mut self, q: f32) -> Self {
        self.resonance = Self::norm_from_q(q);
        self.update_coeffs();
        self
    }

    /// Set the resonance knob (0..1), mapped by the model when one is set.
    #[must_use]
    pub fn with_resonance(mut self, v: f32) -> Self {
        self.resonance = v.clamp(0.0, 1.0);
        self.update_coeffs();
        self
    }

    /// A filter model's own resonance curve.
    #[must_use]
    pub fn with_resonance_map(mut self, map: ResonanceMap) -> Self {
        self.res_map = Some(map);
        self.update_coeffs();
        self
    }

    /// The corner multiplier at full resonance, arriving as
    /// `shift^(resonance^curve)`.
    #[must_use]
    pub fn with_res_shift(mut self, shift: f32, curve: f32) -> Self {
        self.res_shift = shift.clamp(0.25, 4.0);
        self.res_shift_curve = curve.clamp(0.05, 8.0);
        self.update_coeffs();
        self
    }

    /// Ladder passband compensation (see [`Ladder`]).
    #[must_use]
    pub fn with_ladder_comp(mut self, comp: f32) -> Self {
        self.ladder_l.comp = comp;
        self.ladder_r.comp = comp;
        self
    }

    /// The lowest knob setting param 4 spans; it spans 1.5 settings.
    pub const KNOB_MIN: f32 = -0.5;
    pub const KNOB_SPAN: f32 = 1.5;

    /// Emulate a cutoff knob (see `taper`) and set it to `setting`.
    #[must_use]
    pub fn with_taper(mut self, points: Vec<(f32, f32)>, setting: f32) -> Self {
        self.taper = points;
        self.set_knob((setting - Self::KNOB_MIN) / Self::KNOB_SPAN);
        self
    }

    fn set_knob(&mut self, u: f32) {
        self.knob = u.clamp(0.0, 1.0);
        let s = Self::KNOB_MIN + Self::KNOB_SPAN * self.knob;
        if let Some(hz) = Self::taper_hz(&self.taper, s) {
            self.cutoff_hz = hz;
            self.update_coeffs();
        }
    }

    fn taper_hz(points: &[(f32, f32)], s: f32) -> Option<f32> {
        let (first, last) = (points.first()?, points.last()?);
        if s <= first.0 {
            return Some(first.1);
        }
        for w in points.windows(2) {
            let ((a, ha), (b, hb)) = (w[0], w[1]);
            if s <= b {
                let t = (s - a) / (b - a).max(1e-6);
                return Some((ha.ln() + t * (hb.ln() - ha.ln())).exp());
            }
        }
        Some(last.1)
    }

    /// Output gain in dB (a model's passband level).
    #[must_use]
    pub fn with_gain_db(mut self, db: f32) -> Self {
        self.gain = 10f32.powf(db / 20.0);
        self
    }

    fn is_ladder(&self) -> bool {
        self.character == FilterCharacter::Ladder
            && matches!(self.mode, FilterMode::Lowpass | FilterMode::Highpass)
    }

    /// The first SVF section's Q.
    fn svf_q(&self) -> f32 {
        let r = self.resonance;
        self.res_map.map_or_else(
            || Self::q_from_norm(r),
            |m| m.lo * (m.hi / m.lo.max(1e-3)).powf(r.powf(m.curve)),
        )
    }

    /// The ladder's loop feedback.
    fn ladder_k(&self) -> f32 {
        let r = self.resonance;
        self.res_map
            .map_or(r * 3.8, |m| m.lo + (m.hi - m.lo) * r.powf(m.curve))
    }

    fn update_coeffs(&mut self) {
        self.set_corner(self.cutoff_hz);
    }

    /// The follower's attack and release as one-pole coefficients.
    fn update_env(&mut self) {
        let coef = |ms: f32, sr: f32| (-1.0 / (ms.max(0.1) * 0.001 * sr)).exp();
        self.env_up = coef(self.env_attack_ms, self.sample_rate);
        self.env_down = coef(self.env_release_ms, self.sample_rate);
    }

    /// The filter's coefficients for a cutoff of `hz` (the knob's, or where
    /// the envelope has moved it).
    fn set_corner(&mut self, hz: f32) {
        let q = self.svf_q();
        let fc = hz.clamp(20.0, self.sample_rate * 0.45)
            * self
                .res_shift
                .powf(self.resonance.powf(self.res_shift_curve));
        for i in 0..(self.poles / 2) as usize {
            // Resonance on the first section only; the cascade stays flat.
            let q = if i == 0 {
                q
            } else {
                core::f32::consts::FRAC_1_SQRT_2
            };
            self.left[i].set(fc, q, self.sample_rate);
            self.right[i].set(fc, q, self.sample_rate);
        }
        self.odd_l.set(fc, self.sample_rate);
        self.odd_r.set(fc, self.sample_rate);
        let (k, hp, stages) = (
            self.ladder_k(),
            self.mode == FilterMode::Highpass,
            self.poles as usize,
        );
        for ladder in [&mut self.ladder_l, &mut self.ladder_r] {
            let comp = ladder.comp;
            ladder.configure(stages, hp, comp);
            ladder.set_feedback(fc, k, self.sample_rate);
        }
    }

    /// Run one sample through the cascade of one channel.
    #[inline]
    fn tick_chain(
        chain: &mut [Svf],
        odd: &mut OnePole,
        poles: u32,
        mode: FilterMode,
        x: f32,
    ) -> f32 {
        let mut y = x;
        for svf in chain.iter_mut().take((poles / 2) as usize) {
            let (lp, bp, hp) = svf.tick(y);
            // The SVF's band output peaks at Q; `k·bp` (k = 1/Q) is the
            // unity-peak bandpass.
            y = Self::pick(mode, lp, svf.k * bp, hp);
        }
        if poles % 2 == 1 {
            let (lp, hp) = odd.tick(y);
            y = if mode == FilterMode::Highpass { hp } else { lp };
        }
        y
    }

    /// Normalized 0..1 → 20 Hz..20 kHz (exponential).
    #[must_use]
    pub fn cutoff_from_norm(v: f32) -> f32 {
        20.0 * 1000f32.powf(v.clamp(0.0, 1.0))
    }

    /// 20 Hz..20 kHz → normalized 0..1.
    #[must_use]
    pub fn norm_from_cutoff(hz: f32) -> f32 {
        ((hz / 20.0).max(1.0).log10() / 3.0).clamp(0.0, 1.0)
    }

    /// Normalized 0..1 → Q 0.5..12 (flat ≈ 0.018).
    #[must_use]
    pub fn q_from_norm(v: f32) -> f32 {
        0.5 + 11.5 * v.clamp(0.0, 1.0)
    }

    fn norm_from_q(q: f32) -> f32 {
        ((q - 0.5) / 11.5).clamp(0.0, 1.0)
    }

    #[inline]
    fn pick(mode: FilterMode, lp: f32, bp: f32, hp: f32) -> f32 {
        match mode {
            FilterMode::Lowpass => lp,
            FilterMode::Highpass => hp,
            FilterMode::Bandpass => bp,
            FilterMode::Notch => lp + hp,
        }
    }

    fn reset_state(&mut self) {
        for svf in self.left.iter_mut().chain(self.right.iter_mut()) {
            svf.reset();
        }
        self.odd_l.reset();
        self.odd_r.reset();
        self.ladder_l.reset();
        self.ladder_r.reset();
    }

    /// One stereo frame through the configured engine.
    #[inline]
    fn tick_frame(&mut self, xl: f32, xr: f32) -> (f32, f32) {
        if self.is_ladder() {
            (self.ladder_l.tick(xl), self.ladder_r.tick(xr))
        } else {
            let (mode, poles) = (self.mode, self.poles);
            (
                Self::tick_chain(&mut self.left, &mut self.odd_l, poles, mode, xl),
                Self::tick_chain(&mut self.right, &mut self.odd_r, poles, mode, xr),
            )
        }
    }
}

impl PluginInstance for NativeFilter {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.native.filter".into(),
            name: "Filter".into(),
            vendor: "Signal".into(),
            version: String::new(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        vec![
            PluginParamInfo {
                id: 0,
                name: "cutoff".into(),
                min: 0.0,
                max: 1.0,
                default: Self::norm_from_cutoff(20_000.0) as f64,
            },
            PluginParamInfo {
                id: 1,
                name: "resonance".into(),
                min: 0.0,
                max: 1.0,
                default: Self::norm_from_q(core::f32::consts::FRAC_1_SQRT_2) as f64,
            },
            // Saturation ahead of the filter (1x..8x into tanh, level
            // compensated) and the dry/filtered balance.
            PluginParamInfo {
                id: 2,
                name: "drive".into(),
                min: 0.0,
                max: 1.0,
                default: 0.0,
            },
            PluginParamInfo {
                id: 3,
                name: "mix".into(),
                min: 0.0,
                max: 1.0,
                default: 1.0,
            },
            // The emulated cutoff knob (inert without a taper).
            PluginParamInfo {
                id: 4,
                name: "knob".into(),
                min: 0.0,
                max: 1.0,
                default: 0.0,
            },
            // The envelope: its sweep in octaves (− down, 0 off), how hard
            // the playing drives it, its attack and release (ms).
            PluginParamInfo {
                id: 5,
                name: "env_octaves".into(),
                min: -4.0,
                max: 4.0,
                default: 0.0,
            },
            PluginParamInfo {
                id: 6,
                name: "env_sens".into(),
                min: 0.0,
                max: 1.0,
                default: 0.5,
            },
            PluginParamInfo {
                id: 7,
                name: "env_attack".into(),
                min: 0.5,
                max: 100.0,
                default: 4.0,
            },
            PluginParamInfo {
                id: 8,
                name: "env_release".into(),
                min: 10.0,
                max: 1000.0,
                default: 120.0,
            },
            // The response, as a number a preset can store: 0 low-pass,
            // 1 high-pass, 2 band-pass, 3 notch.
            PluginParamInfo {
                id: 9,
                name: "mode".into(),
                min: 0.0,
                max: 3.0,
                default: 0.0,
            },
        ]
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        match id {
            0 => Some(Self::norm_from_cutoff(self.cutoff_hz) as f64),
            1 => Some(self.resonance as f64),
            2 => Some(self.drive as f64),
            3 => Some(self.mix as f64),
            4 => Some(self.knob as f64),
            5 => Some(self.env_octaves as f64),
            6 => Some(self.env_sens as f64),
            7 => Some(self.env_attack_ms as f64),
            8 => Some(self.env_release_ms as f64),
            9 => Some(f64::from(self.mode.index())),
            _ => None,
        }
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }

    fn prepare(&mut self, sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate.max(1.0) as f32;
        self.update_coeffs();
        self.update_env();
        self.reset_state();
        self.prepared = true;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.prepared
    }

    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        // Param writes (mod matrix / UI) applied at block start.
        let mut dirty = false;
        for &(id, value) in events.params {
            match id {
                0 => {
                    self.cutoff_hz = Self::cutoff_from_norm(value as f32);
                    dirty = true;
                }
                1 => {
                    self.resonance = (value as f32).clamp(0.0, 1.0);
                    dirty = true;
                }
                2 => self.drive = (value as f32).clamp(0.0, 1.0),
                3 => self.mix = (value as f32).clamp(0.0, 1.0),
                // `set_knob` recomputes the coefficients itself.
                4 => self.set_knob(value as f32),
                5 => {
                    self.env_octaves = (value as f32).clamp(-4.0, 4.0);
                    dirty = true;
                }
                6 => self.env_sens = (value as f32).clamp(0.0, 1.0),
                7 => {
                    self.env_attack_ms = (value as f32).clamp(0.5, 100.0);
                    self.update_env();
                }
                8 => {
                    self.env_release_ms = (value as f32).clamp(10.0, 1000.0);
                    self.update_env();
                }
                9 => {
                    self.mode = FilterMode::from_index(value.round() as u32);
                    dirty = true;
                }
                _ => {}
            }
        }
        if dirty {
            self.update_coeffs();
        }
        let frames = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        if self.env_octaves.abs() > 1e-3 {
            return self.process_enveloped(in_l, in_r, out_l, out_r, frames);
        }
        if self.drive > 0.0 || self.mix < 1.0 {
            // Drive and/or a dry blend: the general path.
            let g = 1.0 + 7.0 * self.drive;
            let comp = 1.0 / g.sqrt();
            let (mix, driven, gain) = (self.mix, self.drive > 0.0, self.gain);
            for f in 0..frames {
                let (xl, xr) = (in_l[f], in_r[f]);
                let (dl, dr) = if driven {
                    ((xl * g).tanh() * comp, (xr * g).tanh() * comp)
                } else {
                    (xl, xr)
                };
                let (yl, yr) = self.tick_frame(dl, dr);
                out_l[f] = xl + (yl * gain - xl) * mix;
                out_r[f] = xr + (yr * gain - xr) * mix;
            }
            return Ok(());
        }
        let gain = self.gain;
        for f in 0..frames {
            let (yl, yr) = self.tick_frame(in_l[f], in_r[f]);
            out_l[f] = yl * gain;
            out_r[f] = yr * gain;
        }
        Ok(())
    }

    fn deactivate(&mut self) {
        self.prepared = false;
        self.reset_state();
    }
}

impl NativeFilter {
    /// Samples between cutoff updates while the envelope moves it.
    const ENV_STRIDE: usize = 16;

    /// The enveloped path: the playing's level (the louder channel, through
    /// an attack/release follower) moves the cutoff up to `env_octaves`
    /// from the knob's, re-set every [`Self::ENV_STRIDE`] samples. Drive
    /// and mix apply as on the plain path.
    fn process_enveloped(&mut self, in_l: &[f32], in_r: &[f32], out_l: &mut [f32], out_r: &mut [f32], frames: usize) -> Result<(), PluginError> {
        // Sensitivity as the follower's input gain: 0.5 → ×8 (a bass's
        // or a guitar's pick reaching the top of the sweep).
        let sens = 1.0 + 30.0 * self.env_sens * self.env_sens;
        let g = 1.0 + 7.0 * self.drive;
        let comp = 1.0 / g.sqrt();
        let (mix, driven, gain) = (self.mix, self.drive > 0.0, self.gain);
        let mut f = 0;
        while f < frames {
            let end = (f + Self::ENV_STRIDE).min(frames);
            for i in f..end {
                let level = in_l[i].abs().max(in_r[i].abs()) * sens;
                let c = if level > self.env { self.env_up } else { self.env_down };
                self.env = level + (self.env - level) * c;
            }
            let hz = self.cutoff_hz * 2f32.powf(self.env_octaves * self.env.min(1.0));
            self.set_corner(hz);
            for i in f..end {
                let (xl, xr) = (in_l[i], in_r[i]);
                let (dl, dr) = if driven { ((xl * g).tanh() * comp, (xr * g).tanh() * comp) } else { (xl, xr) };
                let (yl, yr) = self.tick_frame(dl, dr);
                out_l[i] = xl + (yl * gain - xl) * mix;
                out_r[i] = xr + (yr * gain - xr) * mix;
            }
            f = end;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|s| s * s).sum::<f32>() / buf.len().max(1) as f32).sqrt()
    }

    /// Render a sine at `freq` through the filter, return output RMS.
    fn sine_response(filter: &mut NativeFilter, freq: f32, sr: f32) -> f32 {
        let n = 4_096;
        let input: Vec<f32> = (0..n)
            .map(|i| (core::f32::consts::TAU * freq * i as f32 / sr).sin())
            .collect();
        let (mut out_l, mut out_r) = (vec![0.0; n], vec![0.0; n]);
        let ev = PluginEvents {
            params: &[],
            midi: &[],
            note_expressions: &[],
        };
        filter
            .process_block(&input, &input, &mut out_l, &mut out_r, &ev)
            .unwrap();
        // Skip the transient at the head.
        rms(&out_l[n / 2..])
    }

    #[test]
    fn lowpass_passes_low_attenuates_high() {
        let sr = 48_000.0;
        let mut f = NativeFilter::new(48_000).with_cutoff(1_000.0);
        f.prepare(48_000.0, 4_096).unwrap();
        let low = sine_response(&mut f, 100.0, sr);
        f.prepare(48_000.0, 4_096).unwrap(); // reset state
        let high = sine_response(&mut f, 10_000.0, sr);
        assert!(low > 0.6, "passband ~unity, rms={low}");
        assert!(
            high < 0.1,
            "10 kHz through a 1 kHz LP is >20 dB down, rms={high}"
        );
    }

    /// An envelope filter opens with the playing: a loud note through a
    /// low-set band-pass sweeping up comes out brighter (more 2 kHz) than a
    /// quiet one.
    #[test]
    fn the_envelope_opens_the_filter_with_the_playing() {
        let sr = 48_000.0;
        let tone = |amp: f32| {
            let mut f = NativeFilter::new(48_000).with_mode(FilterMode::Bandpass).with_cutoff(300.0);
            f.prepare(48_000.0, 4_096).unwrap();
            let ev = PluginEvents { params: &[(5, 3.0), (6, 0.6)], midi: &[], note_expressions: &[] };
            f.process_block(&[], &[], &mut [], &mut [], &ev).unwrap();
            let n = 8_192;
            let input: Vec<f32> = (0..n).map(|i| amp * (core::f32::consts::TAU * 2_000.0 * i as f32 / sr).sin()).collect();
            let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
            let none = PluginEvents { params: &[], midi: &[], note_expressions: &[] };
            f.process_block(&input, &input, &mut l, &mut r, &none).unwrap();
            rms(&l[n / 2..]) / amp
        };
        let (quiet, loud) = (tone(0.01), tone(0.5));
        assert!(loud > quiet * 2.0, "a hard note opens the band toward 2 kHz: quiet {quiet}, loud {loud}");
    }

    #[test]
    fn highpass_mirrors() {
        let sr = 48_000.0;
        let mut f = NativeFilter::new(48_000)
            .with_mode(FilterMode::Highpass)
            .with_cutoff(1_000.0);
        f.prepare(48_000.0, 4_096).unwrap();
        let low = sine_response(&mut f, 100.0, sr);
        f.prepare(48_000.0, 4_096).unwrap();
        let high = sine_response(&mut f, 10_000.0, sr);
        assert!(high > 0.6, "highs pass, rms={high}");
        assert!(low < 0.1, "lows cut, rms={low}");
    }

    #[test]
    fn more_poles_roll_off_steeper() {
        let sr = 48_000.0;
        // 10 kHz through a 1 kHz LP: 24 dB/oct attenuates far more than 12.
        let mut f12 = NativeFilter::new(48_000).with_cutoff(1_000.0).with_poles(2);
        f12.prepare(48_000.0, 4_096).unwrap();
        let two_pole = sine_response(&mut f12, 10_000.0, sr);
        let mut f48 = NativeFilter::new(48_000).with_cutoff(1_000.0).with_poles(8);
        f48.prepare(48_000.0, 4_096).unwrap();
        let eight_pole = sine_response(&mut f48, 10_000.0, sr);
        assert!(
            eight_pole < two_pole * 0.05,
            "8-pole ≫ steeper than 2-pole: 2p={two_pole} 8p={eight_pole}"
        );
    }

    #[test]
    fn notch_cuts_the_center() {
        let sr = 48_000.0;
        let mut f = NativeFilter::new(48_000)
            .with_mode(FilterMode::Notch)
            .with_cutoff(1_000.0)
            .with_q(4.0);
        f.prepare(48_000.0, 4_096).unwrap();
        let at_center = sine_response(&mut f, 1_000.0, sr);
        f.prepare(48_000.0, 4_096).unwrap();
        let far_away = sine_response(&mut f, 100.0, sr);
        assert!(far_away > 0.6, "off-notch passes, rms={far_away}");
        assert!(
            at_center < far_away * 0.35,
            "notch cuts its center: center={at_center} off={far_away}"
        );
    }

    #[test]
    fn ladder_lowpasses_and_resonates() {
        let sr = 48_000.0;
        // Lowpass behavior: highs cut.
        let mut f = NativeFilter::new(48_000)
            .with_character(FilterCharacter::Ladder)
            .with_poles(4)
            .with_cutoff(1_000.0);
        f.prepare(48_000.0, 4_096).unwrap();
        let low = sine_response(&mut f, 100.0, sr);
        f.prepare(48_000.0, 4_096).unwrap();
        let high = sine_response(&mut f, 10_000.0, sr);
        assert!(low > 0.4, "ladder passband, rms={low}");
        assert!(high < low * 0.1, "ladder cuts highs: low={low} high={high}");

        // Resonance: a driven ladder boosts near the cutoff (full resonance
        // — the feedback peak grows sharply toward self-oscillation).
        let mut res = NativeFilter::new(48_000)
            .with_character(FilterCharacter::Ladder)
            .with_poles(4)
            .with_cutoff(1_000.0)
            .with_q(12.0);
        res.prepare(48_000.0, 4_096).unwrap();
        let at_cutoff = sine_response(&mut res, 1_000.0, sr);
        res.prepare(48_000.0, 4_096).unwrap();
        let below = sine_response(&mut res, 200.0, sr);
        assert!(
            at_cutoff > below * 1.3,
            "resonant peak at cutoff: peak={at_cutoff} passband={below}"
        );
        // Saturation keeps it bounded even when resonating.
        assert!(
            at_cutoff < 3.0,
            "tanh bounds the resonance, rms={at_cutoff}"
        );
    }

    #[test]
    fn odd_pole_counts_roll_off_at_their_own_slope() {
        // One octave apart, well above a 250 Hz corner (and clear of the
        // bilinear warp near Nyquist): n poles ≈ 6n dB/oct.
        let sr = 48_000.0;
        for poles in [1u32, 3] {
            let mut f = NativeFilter::new(48_000)
                .with_cutoff(250.0)
                .with_poles(poles);
            f.prepare(48_000.0, 4_096).unwrap();
            let a = sine_response(&mut f, 2_000.0, sr);
            f.prepare(48_000.0, 4_096).unwrap();
            let b = sine_response(&mut f, 4_000.0, sr);
            let slope = 20.0 * (b / a).log10();
            let want = -6.0 * poles as f32;
            assert!((slope - want).abs() < 1.5, "{poles} poles: {slope} dB/oct");
        }
    }

    #[test]
    fn a_ladder_highpass_cuts_lows() {
        let sr = 48_000.0;
        let mut f = NativeFilter::new(48_000)
            .with_character(FilterCharacter::Ladder)
            .with_mode(FilterMode::Highpass)
            .with_poles(4)
            .with_cutoff(1_000.0);
        f.prepare(48_000.0, 4_096).unwrap();
        let low = sine_response(&mut f, 100.0, sr);
        f.prepare(48_000.0, 4_096).unwrap();
        let high = sine_response(&mut f, 8_000.0, sr);
        assert!(high > 0.5 && low < high * 0.01, "low={low} high={high}");
    }

    #[test]
    fn a_resonance_map_sets_the_models_q() {
        // lo 0.7071 at knob 0: a Butterworth corner (−3 dB at cutoff).
        let sr = 48_000.0;
        let map = ResonanceMap {
            lo: core::f32::consts::FRAC_1_SQRT_2,
            hi: 20.0,
            curve: 1.0,
        };
        let mut f = NativeFilter::new(48_000)
            .with_cutoff(1_000.0)
            .with_resonance_map(map)
            .with_resonance(0.0);
        f.prepare(48_000.0, 4_096).unwrap();
        let at = sine_response(&mut f, 1_000.0, sr);
        assert!((20.0 * (at / core::f32::consts::FRAC_1_SQRT_2).log10() + 3.0).abs() < 0.3);
        // Full knob: a 20-Q peak (+26 dB).
        let mut f = f.with_resonance(1.0);
        f.prepare(48_000.0, 4_096).unwrap();
        let peak = sine_response(&mut f, 1_000.0, sr);
        assert!(20.0 * (peak / at).log10() > 25.0, "{peak} vs {at}");
    }

    #[test]
    fn a_taper_maps_the_knob_to_its_corner() {
        let taper = vec![(0.0, 100.0), (0.5, 1_000.0), (1.0, 10_000.0)];
        let mut f = NativeFilter::new(48_000).with_taper(taper, 0.5);
        assert!((f.cutoff_hz - 1_000.0).abs() < 1.0);
        // Knob 0..1 spans settings −0.5..1.0: setting 0.25 → √(100·1000).
        let u = f64::from((0.25 - NativeFilter::KNOB_MIN) / NativeFilter::KNOB_SPAN);
        let ev = PluginEvents {
            params: &[(4, u)],
            midi: &[],
            note_expressions: &[],
        };
        let (i, mut o1, mut o2) = (vec![0.0; 16], vec![0.0; 16], vec![0.0; 16]);
        f.prepare(48_000.0, 16).unwrap();
        f.process_block(&i, &i, &mut o1, &mut o2, &ev).unwrap();
        assert!((f.cutoff_hz - 316.2).abs() < 1.0, "{}", f.cutoff_hz);
    }

    #[test]
    fn default_filter_is_transparent_enough() {
        // A default (LP ~20 kHz) filter must not swallow a preset's audio.
        let sr = 48_000.0;
        let mut f = NativeFilter::new(48_000);
        f.prepare(48_000.0, 4_096).unwrap();
        let mid = sine_response(&mut f, 440.0, sr);
        assert!(mid > 0.6, "default filter passes midrange, rms={mid}");
    }
}
