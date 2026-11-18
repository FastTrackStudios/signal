//! Kontakt group insert filters and EQs, run per voice (spec `group_fx`).
//!
//! A dynamic layer's [`InsertPlan`] is built once per patch from its
//! `group_fx` entry: the stages (filter or EQ) and the controller routes that
//! move their knobs, each route's table sampled at every controller value
//! (controllers are integers, so the 128-entry table is exact). A voice of
//! that layer carries the plan (an `Arc`, no allocation at spawn) and its own
//! [`InsertState`]: the filter memories, the routes' smoothed values and the
//! coefficients, recomputed every [`COEF_EVERY`] frames.
//!
//! The laws are Kontakt's, as measured by KONTRA (Apache-2.0,
//! `crates/format/kontakt/src/laws.rs`, `docs/re/kontakt-bindings/filter-laws.tsv`):
//!
//! - state-variable filters (types 50..): cutoff knob `u` is the MIDI note
//!   `115u + 20` (at most 160); resonance `R = 1 − (1 − r)³` damps a stage
//!   by `2(1 − R)` (the second of four poles by `2(1 − 0.666R)`), and the
//!   output falls with `R` (one-pole: unaffected; two poles `1 − ½R⁵`; four
//!   `1 − 0.77R`).
//! - legacy filters (types 2..): `21759.5 · (min((e^(6.15u) − 1) · 0.0025728, 1) + 0.002)` Hz,
//!   Butterworth sections, resonance raising the sharpest one's Q by
//!   `e^(6.4r)` with a `5.5 r` dB passband loss for two poles.
//! - EQ bands: Kontakt's classic band ([`bell`]): the cookbook bell with twice the stored dB at the centre
//!   (`A = 2^(dB/6)`) and `α = sin ω · sinh(octaves · ln 2)`; a route of
//!   intensity 1 moves the stored dB by 12, clamped to ±18.
//! - a route's lag is the time to settle within 1 %: a one-pole with
//!   `τ = lag / ln 100`.

use std::sync::Arc;

use super::voice::FlexEnv;

use crate::spec::{FxRouteSpec, FxStageSpec, GroupFxSpec, kontakt_table_value};

/// Frames between coefficient updates (as the voice filter envelope).
pub const COEF_EVERY: u16 = 16;
/// Most stages a layer's chain keeps (Kontakt has 8 slots; CS uses ≤ 3 live).
pub const MAX_STAGES: usize = 4;
/// Most routes a layer's chain keeps.
pub const MAX_ROUTES: usize = 8;
/// Sections one stage expands into (a 3-band EQ, a four-pole filter: ≤ 3).
const MAX_SECTIONS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Law {
    Sv,
    Legacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StageKind {
    /// One-pole lowpass (SV LP1, legacy LP1).
    OnePole(Law),
    /// State-variable lowpass with `n` two-pole sections (SV LP2 / LP4,
    /// legacy LP2).
    Svf(Law, u8),
    /// 1..=3 bell bands.
    Eq(u8),
    /// Kontakt's Saturation, Enhanced mode: a memoryless curve at the base
    /// rate between the linear sections (`super::kontakt_saturation`).
    Saturation,
    /// Kontakt's classic Limiter (kind-3 group insert,
    /// `super::kontakt_limiter`): both channels at its place in the chain.
    Limiter,
    /// Stereo width: the side signal scaled by `1 + width`, mid kept
    /// (Kontakt's kind-9 group insert). Run after the per-channel stages:
    /// those treat both channels alike, so the order does not matter.
    Width,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Param {
    Cutoff,
    Resonance,
    EqGain(u8),
    /// The voice's level at its amplifier: the route's value is a gain.
    Volume,
}

/// One stage of a plan: its kind and saved knobs.
#[derive(Debug, Clone)]
struct PlanStage {
    kind: StageKind,
    cutoff: f32,
    resonance: f32,
    /// (Hz, octaves, stored dB) per band.
    bands: [[f32; 3]; MAX_SECTIONS],
    /// `Width`: the side gain is `1 + width`.
    width: f32,
    /// `Saturation`: `[drive, makeup, wet]` at the saved shape.
    sat: [f32; 3],
    /// `Limiter`: input gain (dB), release (ms).
    limiter: [f32; 2],
    /// The stage's output gain (linear).
    out_gain: f32,
}

/// One route of a plan: the knob it moves and its value at every controller
/// value (table, invert and intensity applied).
#[derive(Debug, Clone)]
struct PlanRoute {
    stage: u8,
    param: Param,
    cc: u8,
    /// Kontakt units the knob moves: 0..1 knob for cutoff/resonance, stored
    /// dB for an EQ gain.
    lut: [f32; 128],
    /// Smoothing coefficient per update (1 = no lag).
    coef: f32,
    /// An envelope source (`FxRouteSpec::env`): its segments, a level < 0
    /// standing for `cc` ÷ 127; empty = a controller route.
    env: Vec<(f32, f32, f32)>,
    /// `intensity × span` the envelope's level moves the knob by.
    env_depth: f32,
    /// The note's velocity is the source (`FxRouteSpec::source`), set once
    /// at the note's start by [`InsertState::with_velocity`].
    velocity: bool,
    /// The note's key is the source (`FxRouteSpec::source` "key"), set once
    /// at the note's start by [`InsertState::with_key`].
    key: bool,
}

/// A dynamic layer's insert chain, shared by its voices.
#[derive(Debug, Clone)]
pub struct InsertPlan {
    stages: Vec<PlanStage>,
    routes: Vec<PlanRoute>,
}

impl InsertPlan {
    /// Build the plan of one `group_fx` entry; `None` when nothing in it is
    /// a stage this engine runs.
    #[must_use]
    pub fn new(spec: &GroupFxSpec, sample_rate: f32) -> Option<Arc<Self>> {
        let mut map = Vec::new(); // spec stage index → plan stage index
        let mut stages = Vec::new();
        for (i, s) in spec.stages.iter().enumerate() {
            match plan_stage(s) {
                Some(p) if stages.len() < MAX_STAGES => {
                    map.push((i, stages.len()));
                    stages.push(p);
                }
                _ => {}
            }
        }
        // a chain of volume routes alone runs too (a layer with no inserts
        // whose level a controller moves)
        let volume_only = |r: &FxRouteSpec| r.param == "volume";
        if stages.is_empty() && !spec.routes.iter().any(volume_only) {
            return None;
        }
        let routes = spec
            .routes
            .iter()
            .filter_map(|r| {
                if volume_only(r) {
                    return plan_route(r, 0, None, sample_rate);
                }
                let stage = map.iter().find(|(i, _)| *i == usize::from(r.stage))?.1;
                plan_route(r, stage as u8, Some(&stages[stage]), sample_rate)
            })
            .take(MAX_ROUTES)
            .collect();
        Some(Arc::new(Self { stages, routes }))
    }

    /// The controllers the routes read.
    pub fn ccs(&self) -> impl Iterator<Item = u8> + '_ {
        self.routes.iter().map(|r| r.cc)
    }
}

fn plan_stage(s: &FxStageSpec) -> Option<PlanStage> {
    let kind = match s.kind.as_str() {
        "sv_lp1" => StageKind::OnePole(Law::Sv),
        "legacy_lp1" => StageKind::OnePole(Law::Legacy),
        "sv_lp2" => StageKind::Svf(Law::Sv, 1),
        "sv_lp4" => StageKind::Svf(Law::Sv, 2),
        "legacy_lp2" => StageKind::Svf(Law::Legacy, 1),
        "eq" => {
            let n = (s.bands.len() / 3).min(MAX_SECTIONS);
            if n == 0 {
                return None;
            }
            StageKind::Eq(n as u8)
        }
        "width" => StageKind::Width,
        "saturation" => StageKind::Saturation,
        "limiter" => StageKind::Limiter,
        _ => return None,
    };
    let mut bands = [[0.0; 3]; MAX_SECTIONS];
    for (b, c) in bands.iter_mut().zip(s.bands.chunks_exact(3)) {
        *b = [c[0], c[1], c[2]];
    }
    Some(PlanStage {
        kind,
        cutoff: s.cutoff,
        resonance: s.resonance,
        bands,
        width: s.width,
        sat: super::kontakt_saturation::enhanced_at(s.shape),
        limiter: [s.input_db, s.release_ms],
        out_gain: 10f32.powf(s.output_db / 20.0),
    })
}

fn plan_route(
    r: &FxRouteSpec,
    stage: u8,
    s: Option<&PlanStage>,
    sample_rate: f32,
) -> Option<PlanRoute> {
    let param = match r.param.as_str() {
        "cutoff" => Param::Cutoff,
        "resonance" => Param::Resonance,
        "volume" => Param::Volume,
        p => {
            let n: u8 = p.strip_prefix("eq_gain")?.parse().ok()?;
            match s?.kind {
                StageKind::Eq(bands) if (1..=bands).contains(&n) => Param::EqGain(n - 1),
                _ => return None,
            }
        }
    };
    if matches!(param, Param::Cutoff | Param::Resonance)
        && s.is_none_or(|s| {
            matches!(
                s.kind,
                StageKind::Eq(_) | StageKind::Width | StageKind::Saturation | StageKind::Limiter
            )
        })
    {
        return None;
    }
    // an EQ gain route of depth 1 moves the stored dB by 12
    let span = if matches!(param, Param::EqGain(_)) {
        12.0
    } else {
        1.0
    };
    let mut lut = [0.0; 128];
    for (v, out) in lut.iter_mut().enumerate() {
        let x = v as f32 / 127.0;
        let shaped = if r.points.is_empty() {
            x
        } else {
            kontakt_table_value(&r.points, x)
        };
        *out = if param == Param::Volume {
            // Kontakt's controller volume law (KONTRA `laws::volume`, plain
            // power for a controller): a gain, 1 at the top of the source
            (1.0 - (1.0 - shaped) * r.intensity).max(0.0)
        } else {
            r.intensity * span * shaped
        };
    }
    // a law the script computes, baked per source value
    if r.table.len() == 128 {
        lut.copy_from_slice(&r.table);
    }
    let dt_ms = f32::from(COEF_EVERY) * 1000.0 / sample_rate.max(1.0);
    let coef = if r.lag_ms > 0.0 {
        let tau = r.lag_ms / 100f32.ln();
        1.0 - (-dt_ms / tau).exp()
    } else {
        1.0
    };
    Some(PlanRoute {
        stage,
        param,
        cc: r.cc,
        lut,
        coef,
        env: r
            .env
            .iter()
            .map(|e| (e.time_ms, e.level, e.curve))
            .collect(),
        env_depth: r.intensity * span,
        velocity: r.source.eq_ignore_ascii_case("velocity"),
        key: r.source.eq_ignore_ascii_case("key"),
    })
}

/// One filter section's coefficients.
#[derive(Debug, Clone, Copy)]
enum Section {
    Off,
    /// Trapezoidal one-pole lowpass, `G = g / (1 + g)`.
    OnePole(f32),
    /// Trapezoidal state-variable lowpass (`a1 a2 a3` of `g`, `k`).
    Svf(f32, f32, f32),
    /// Kontakt's classic EQ band: `[b0, b1, b2, −a1, −a2]` over `a0 = 1`,
    /// run as f32 direct form I in `Process3Band_SSE`'s order.
    Df1([f32; 5]),
}

/// A voice's run of its layer's plan.
#[derive(Debug, Clone)]
pub struct InsertState {
    plan: Arc<InsertPlan>,
    sample_rate: f32,
    /// Smoothed and target value per route.
    cur: [f32; MAX_ROUTES],
    tgt: [f32; MAX_ROUTES],
    sections: [[Section; MAX_SECTIONS]; MAX_STAGES],
    /// The envelope of each envelope route, run per voice.
    envs: [Option<FlexEnv>; MAX_ROUTES],
    /// Output gain per stage (resonance loss).
    gain: [f32; MAX_STAGES],
    /// Two memories per section per channel.
    mem: [[[[f32; 4]; MAX_SECTIONS]; MAX_STAGES]; 2],
    countdown: u16,
    /// The chain's limiter stage, if it has one: its index and state.
    limiter: Option<(usize, super::kontakt_limiter::KontaktLimiter)>,
    /// A voice's amplifier balance inside the chain (`set_pan`): per
    /// channel gain, and the stage it follows (`None` = ahead of all).
    pan: Option<([f32; 2], Option<usize>)>,
    /// The volume routes' gain (`Param::Volume`), at the amplifier: after
    /// the stage `amp_at` (`None` = ahead of all), as the pan.
    volume: f32,
    amp_at: Option<usize>,
}

impl InsertState {
    /// Start a voice's chain at the controllers' current values (no lag on
    /// the way in: the routes start where the controllers are).
    pub fn new(plan: Arc<InsertPlan>, sample_rate: f32, cc: impl Fn(u8) -> u8) -> Self {
        let mut s = Self {
            plan,
            sample_rate,
            cur: [0.0; MAX_ROUTES],
            tgt: [0.0; MAX_ROUTES],
            sections: [[Section::Off; MAX_SECTIONS]; MAX_STAGES],
            envs: Default::default(),
            gain: [1.0; MAX_STAGES],
            mem: [[[[0.0; 4]; MAX_SECTIONS]; MAX_STAGES]; 2],
            countdown: 0,
            limiter: None,
            pan: None,
            volume: 1.0,
            amp_at: None,
        };
        s.amp_at = s
            .plan
            .stages
            .iter()
            .rposition(|st| st.kind == StageKind::Saturation);
        s.limiter = s
            .plan
            .stages
            .iter()
            .position(|st| st.kind == StageKind::Limiter)
            .map(|i| {
                let [input_db, release_ms] = s.plan.stages[i].limiter;
                let settings = super::kontakt_limiter::KontaktSettings {
                    input_db,
                    release_ms,
                };
                (
                    i,
                    super::kontakt_limiter::KontaktLimiter::new(settings, sample_rate as u32),
                )
            });
        for (slot, r) in s.envs.iter_mut().zip(&s.plan.routes) {
            if r.env.is_empty() {
                continue;
            }
            let v = f32::from(cc(r.cc).min(127)) / 127.0;
            let segs: Vec<(f32, f32, f32)> = r
                .env
                .iter()
                .map(|&(t, l, c)| (t, if l < 0.0 { v } else { l }, c))
                .collect();
            *slot = FlexEnv::from_segments(&segs, 0.0, sample_rate as u32, true);
        }
        s.set_ccs(cc);
        s.advance_envs(0);
        s.cur = s.tgt;
        s
    }

    /// Step the envelope routes `frames` frames and take their levels as
    /// the routes' targets (no lag: the envelope is the motion).
    fn advance_envs(&mut self, frames: u16) {
        for (i, e) in self.envs.iter_mut().enumerate() {
            let Some(e) = e else { continue };
            let mut lv = 0.0;
            for _ in 0..frames.max(1) {
                lv = e.next(false);
            }
            let v = lv * self.plan.routes[i].env_depth;
            self.tgt[i] = v;
            self.cur[i] = v;
        }
    }

    /// The controllers' current values (read again every block).
    pub fn set_ccs(&mut self, cc: impl Fn(u8) -> u8) {
        for (t, r) in self.tgt.iter_mut().zip(&self.plan.routes) {
            if r.env.is_empty() && !r.velocity && !r.key {
                *t = r.lut[usize::from(cc(r.cc).min(127))];
            }
        }
    }

    /// The note's velocity for its velocity routes (fixed for the voice's
    /// life, as a script sets the knob once at the note's start).
    #[must_use]
    pub fn with_velocity(mut self, velocity: u8) -> Self {
        for (i, r) in self.plan.routes.iter().enumerate() {
            if r.velocity {
                let v = r.lut[usize::from(velocity.min(127))];
                self.tgt[i] = v;
                self.cur[i] = v;
            }
        }
        self.countdown = 0;
        self
    }

    /// The note's key for its key routes (a Key Position modulator: fixed
    /// for the voice's life).
    #[must_use]
    pub fn with_key(mut self, key: u8) -> Self {
        for (i, r) in self.plan.routes.iter().enumerate() {
            if r.key {
                let v = r.lut[usize::from(key.min(127))];
                self.tgt[i] = v;
                self.cur[i] = v;
            }
        }
        self.countdown = 0;
        self
    }

    fn update(&mut self) {
        let Self {
            plan,
            sample_rate,
            cur,
            tgt,
            sections,
            gain,
            volume,
            ..
        } = self;
        for ((c, t), r) in cur.iter_mut().zip(tgt.iter()).zip(&plan.routes) {
            *c += (*t - *c) * r.coef;
        }
        *volume = plan
            .routes
            .iter()
            .zip(cur.iter())
            .filter(|(r, _)| r.param == Param::Volume)
            .map(|(_, v)| *v)
            .product();
        for (i, st) in plan.stages.iter().enumerate() {
            let mut cutoff = st.cutoff;
            let mut res = st.resonance;
            let mut db = [st.bands[0][2], st.bands[1][2], st.bands[2][2]];
            for (r, v) in plan.routes.iter().zip(cur.iter()) {
                if usize::from(r.stage) != i {
                    continue;
                }
                match r.param {
                    Param::Cutoff => cutoff += v,
                    Param::Resonance => res += v,
                    Param::EqGain(b) => db[usize::from(b)] += v,
                    Param::Volume => {}
                }
            }
            (sections[i], gain[i]) = coefficients(st, cutoff, res, db, *sample_rate);
        }
    }

    /// The voice's amplifier pan (−1..1, a balance) where Kontakt puts it in
    /// the group's chain: after its drive stages (a saturation runs on the
    /// voice before the amplifier), before its stereo stages (a width runs
    /// after it: KONTRA folds those into the voice's gain matrix after the
    /// pan). The linear per-channel stages commute with it.
    pub fn set_pan(&mut self, pan: f32) {
        let pan = pan.clamp(-1.0, 1.0);
        self.pan = (pan != 0.0).then_some(([1.0 - pan.max(0.0), 1.0 + pan.min(0.0)], self.amp_at));
    }

    /// Filter one stereo frame.
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        if self.countdown == 0 {
            self.countdown = COEF_EVERY;
            self.advance_envs(COEF_EVERY);
            self.update();
        }
        self.countdown -= 1;
        let mut out = [l, r];
        let n = self.plan.stages.len();
        if let Some((g, None)) = self.pan {
            out[0] *= g[0];
            out[1] *= g[1];
        }
        if self.amp_at.is_none() {
            out[0] *= self.volume;
            out[1] *= self.volume;
        }
        // stage by stage, both channels at each (a limiter needs both; the
        // per-channel stages are indifferent to the order)
        for i in 0..n {
            if let Some((at, lim)) = self.limiter.as_mut()
                && *at == i
            {
                let mut frame = [out];
                lim.process(&mut frame);
                out = frame[0];
            }
            for (ch, x) in out.iter_mut().enumerate() {
                for (sec, m) in self.sections[i].iter().zip(self.mem[ch][i].iter_mut()) {
                    *x = run(*sec, m, *x);
                }
                let st = &self.plan.stages[i];
                if st.kind == StageKind::Saturation {
                    *x = super::kontakt_saturation::enhanced(*x, st.sat);
                }
                *x *= self.gain[i];
                if let Some((g, Some(at))) = self.pan
                    && at == i
                {
                    *x *= g[ch];
                }
                if self.amp_at == Some(i) {
                    *x *= self.volume;
                }
            }
        }
        for st in &self.plan.stages {
            if st.kind == StageKind::Width {
                let mid = 0.5 * (out[0] + out[1]);
                let side = 0.5 * (out[0] - out[1]) * (1.0 + st.width);
                out = [mid + side, mid - side];
            }
        }
        (out[0], out[1])
    }
}

#[inline]
fn run(sec: Section, m: &mut [f32; 4], x: f32) -> f32 {
    match sec {
        Section::Off => x,
        Section::OnePole(g) => {
            let v = (x - m[0]) * g;
            let y = v + m[0];
            m[0] = y + v;
            y
        }
        Section::Svf(a1, a2, a3) => {
            let (ic1, ic2) = (m[0], m[1]);
            let v3 = x - ic2;
            let v1 = a1 * ic1 + a2 * v3;
            let v2 = ic2 + a2 * ic1 + a3 * v3;
            m[0] = 2.0 * v1 - ic1;
            m[1] = 2.0 * v2 - ic2;
            v2
        }
        Section::Df1([b0, b1, b2, na1, na2]) => {
            let [x1, x2, y1, y2] = *m;
            let y = (((b0 * x + b1 * x1) + b2 * x2) + na1 * y1) + na2 * y2;
            *m = [x, x1, y, y1];
            y
        }
    }
}

/// Cutoff knob → Hz by the stage's law.
fn cutoff_hz(law: Law, u: f32) -> f32 {
    match law {
        Law::Sv => {
            let note = (115.0 * u.max(0.0) + 20.0).min(160.0);
            440.0 * ((note - 69.0) / 12.0).exp2()
        }
        Law::Legacy => {
            let corner = ((6.15 * u.clamp(0.0, 1.0)).exp() - 1.0) * 0.002_572_831_7;
            21_759.5 * (corner.min(1.0) + 0.002)
        }
    }
}

/// Prewarped trapezoidal gain `tan(π f / fs)`, the cutoff kept below Nyquist.
fn warp(hz: f32, fs: f32) -> f32 {
    (std::f32::consts::PI * hz.clamp(1.0, 0.49 * fs) / fs).tan()
}

fn svf(g: f32, k: f32) -> Section {
    let a1 = 1.0 / (1.0 + g * (g + k));
    let a2 = g * a1;
    Section::Svf(a1, a2, g * a2)
}

fn coefficients(
    st: &PlanStage,
    cutoff: f32,
    res: f32,
    db: [f32; 3],
    fs: f32,
) -> ([Section; MAX_SECTIONS], f32) {
    let mut out = [Section::Off; MAX_SECTIONS];
    let r = res.clamp(0.0, 1.0);
    match st.kind {
        StageKind::Width => (out, 1.0),
        StageKind::Saturation | StageKind::Limiter => (out, st.out_gain),
        StageKind::OnePole(law) => {
            let g = warp(cutoff_hz(law, cutoff), fs);
            out[0] = Section::OnePole(g / (1.0 + g));
            (out, 1.0)
        }
        StageKind::Svf(Law::Sv, n) => {
            let g = warp(cutoff_hz(Law::Sv, cutoff), fs);
            let big = 1.0 - (1.0 - r).powi(3);
            let k = (2.0 * (1.0 - big)).max(0.01);
            out[0] = svf(g, k);
            let gain = if n >= 2 {
                out[1] = svf(g, (2.0 * (1.0 - 0.666 * big)).max(0.01));
                1.0 - 0.77 * big
            } else {
                1.0 - 0.5 * big.powi(5)
            };
            (out, gain)
        }
        StageKind::Svf(Law::Legacy, _) => {
            let g = warp(cutoff_hz(Law::Legacy, cutoff), fs);
            // one Butterworth section; resonance sharpens it
            let k = (std::f32::consts::SQRT_2 / (6.4 * r).exp()).max(0.01);
            out[0] = svf(g, k);
            (out, 10f32.powf(-5.5 * r / 20.0))
        }
        StageKind::Eq(n) => {
            for ((b, stored), o) in st
                .bands
                .iter()
                .zip(db)
                .zip(out.iter_mut())
                .take(usize::from(n))
            {
                *o = bell(b[0], b[1], stored.clamp(-18.0, 18.0), fs);
            }
            (out, 1.0)
        }
    }
}

/// Kontakt 8.7.1's classic EQ band (KONTRA `laws::eq_band` and
/// `eq::kontakt_bell`, 6b09e30). `BEQBand::SetBand` (0x67e91c) stores each
/// knob as a 0..=1 value through cheap log2/exp2 approximations and
/// `SetBandInt` (0x67f8f8) turns them back, once more before the first
/// block (`BEffectBase::Reset`): the band plays at a moved frequency and a
/// gain a hair off `2^(dB/6)`. Octaves are held to 1/3..=3, dB to ±18; a band
/// at 0 dB is skipped. The cookbook peak (`A`, `α = sin ω·sinh(octaves·ln 2)`)
/// with `ω` and its sine and cosine in f32, the coefficients in f64 from
/// them, stored as f32.
fn bell(hz: f32, octaves: f32, stored_db: f32, fs: f32) -> Section {
    if !(hz > 0.0 && octaves > 0.0) {
        return Section::Off;
    }
    let mut unit = eq_hz_to_unit(hz);
    unit = eq_hz_to_unit(eq_unit_to_hz(unit));
    let hz = eq_unit_to_hz(unit);
    let bandwidth = ((octaves - 1.0 / 3.0) / (8.0 / 3.0)).clamp(0.0, 1.0);
    let octaves = bandwidth * 2.666_666_7 + 0.333_333_34;
    let g = (stored_db / 18.0).clamp(-1.0, 1.0);
    let a = fast_exp2(((g * 0.5 + 0.5) * 36.0 - 18.0) / 6.0);
    if a == 1.0 {
        return Section::Off;
    }
    let q = 1.0 / (2.0 * (octaves * std::f32::consts::LN_2).sinh());
    let w = (f64::from((hz / fs).min(0.49)) * std::f64::consts::TAU) as f32;
    let (s, c) = (
        f64::from(f64::from(w).sin() as f32),
        f64::from(f64::from(w).cos() as f32),
    );
    let a = f64::from(a);
    let alpha = s / (2.0 * f64::from(q).max(1e-3));
    let d4 = alpha / a;
    let inv = 1.0 / (d4 + 1.0);
    Section::Df1([
        (alpha.mul_add(a, 1.0) * inv) as f32,
        ((c * -2.0) * inv) as f32,
        ((-alpha).mul_add(a, 1.0) * inv) as f32,
        ((c + c) * inv) as f32,
        ((d4 - 1.0) * inv) as f32,
    ])
}

/// `SetBand`'s Hz knob → 0..=1: `log2(1 + 468.7109·(hz − 20)/19980)·ln 2 / 6.156954`.
fn eq_hz_to_unit(hz: f32) -> f32 {
    let x = ((hz - 20.0) / 19980.0).clamp(0.0, 1.0) * 468.710_9 + 1.0;
    fast_log2(x) * std::f32::consts::LN_2 / 6.156_954
}

/// `SetBandInt`'s 0..=1 → Hz, the inverse through [`fast_exp2`].
fn eq_unit_to_hz(unit: f32) -> f32 {
    let x = fast_exp2(unit * 6.156_954 * 1.442_695);
    ((x - 1.0) / 468.710_9).clamp(0.0, 1.0) * 19980.0 + 20.0
}

/// Kontakt's fast log2: the float's bits as an integer over 2^23, less 127,
/// plus `(m − m²)·0.346607` on its fraction `m`.
fn fast_log2(x: f32) -> f32 {
    let y = x.to_bits() as f32 / 8_388_608.0 - 127.0;
    let m = y - y.floor();
    y + (m - m * m) * 0.346_607
}

/// Kontakt's fast exp2: `p` plus `(m − m²)·(−0.339710)` on its fraction,
/// plus 127, to 23 fractional bits, read as a float's bits.
fn fast_exp2(p: f32) -> f32 {
    let m = p - p.floor();
    let v = p + (m - m * m) * -0.339_710 + 127.0;
    f32::from_bits((v * 8_388_608.0) as i32 as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: &str, cutoff: f32, bands: Vec<f32>, routes: Vec<FxRouteSpec>) -> GroupFxSpec {
        GroupFxSpec {
            dynamic: "ff".into(),
            stages: vec![FxStageSpec {
                kind: kind.into(),
                cutoff,
                resonance: 0.0,
                bands,
                width: 0.0,
                input_db: 0.0,
                release_ms: 0.0,
                shape: 0.0,
                output_db: 0.0,
            }],
            routes,
        }
    }

    /// Steady-state gain of a sine through the chain.
    fn gain_at(plan: Arc<InsertPlan>, hz: f32, cc: u8) -> f32 {
        let fs = 48_000.0;
        let mut st = InsertState::new(plan, fs, |_| cc);
        let n = 48_000;
        let mut peak = 0.0f32;
        for i in 0..n {
            let x = (2.0 * std::f32::consts::PI * hz * i as f32 / fs).sin();
            let (y, _) = st.tick(x, x);
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        20.0 * peak.log10()
    }

    #[test]
    fn sv_cutoff_law_matches_kontakt_display() {
        // the Kontakt GUI: 0 → 26.0 Hz, 0.2305 → 120.0 Hz
        assert!((cutoff_hz(Law::Sv, 0.0) - 25.96).abs() < 0.05);
        assert!((cutoff_hz(Law::Sv, 0.230_490_12) - 120.0).abs() < 0.2);
        // legacy LP1 at 0.25098 shows 249.9 Hz
        assert!((cutoff_hz(Law::Legacy, 0.250_976_7) - 249.9).abs() < 2.5);
    }

    #[test]
    fn one_pole_is_3db_down_at_its_cutoff() {
        let plan = InsertPlan::new(&spec("sv_lp1", 0.5, vec![], vec![]), 48_000.0).unwrap();
        let fc = cutoff_hz(Law::Sv, 0.5);
        let g = gain_at(plan, fc, 0);
        assert!((g + 3.01).abs() < 0.2, "{g}");
    }

    #[test]
    fn a_volume_route_follows_kontakts_controller_law() {
        // MOJO 2 Trumpet transitions: CC113 → volume at 0.3937, CC113 at its
        // start value 100: 1 − (27/127)·0.3937 = 0.9163 (−0.76 dB), flat
        let route = FxRouteSpec {
            stage: 0,
            param: "volume".into(),
            cc: 113,
            intensity: 0.393_701,
            lag_ms: 50.0,
            points: vec![],
            env: vec![],
            source: String::new(),
            table: vec![],
        };
        // an EQ with no gain: the volume route alone
        let plan = InsertPlan::new(
            &spec("eq", 0.0, vec![1000.0, 1.0, 0.0], vec![route]),
            48_000.0,
        )
        .unwrap();
        for hz in [200.0, 4000.0] {
            let g = gain_at(Arc::clone(&plan), hz, 100);
            assert!((g - 20.0 * 0.916_3f32.log10()).abs() < 0.02, "{hz}: {g}");
        }
        assert!(gain_at(Arc::clone(&plan), 1000.0, 127).abs() < 0.02);
        let g0 = gain_at(plan, 1000.0, 0);
        assert!(
            (g0 - 20.0 * (1.0f32 - 0.393_701).log10()).abs() < 0.02,
            "{g0}"
        );
    }

    #[test]
    fn cc_route_opens_the_filter() {
        // CSB Solo Trumpet Mix vibsus ff: closed below CC1 ≈ 76, open at 127
        let route = FxRouteSpec {
            stage: 0,
            param: "cutoff".into(),
            cc: 1,
            intensity: 1.0,
            lag_ms: 120.0,
            points: vec![0.0, 0.0, 0.600_393_7, 0.0, -0.168_918_88, 1.0, 1.0, 0.0],
            env: vec![],
            source: String::new(),
            table: vec![],
        };
        let plan = InsertPlan::new(&spec("sv_lp1", 0.0, vec![], vec![route]), 48_000.0).unwrap();
        assert!(gain_at(Arc::clone(&plan), 1000.0, 127) > -0.1);
        assert!(gain_at(plan, 1000.0, 40) < -30.0);
    }

    #[test]
    fn eq_band_peaks_at_twice_the_stored_db() {
        let plan =
            InsertPlan::new(&spec("eq", 0.0, vec![1000.0, 1.0, 6.0], vec![]), 48_000.0).unwrap();
        let g = gain_at(plan, 1000.0, 0);
        assert!((g - 12.04).abs() < 0.1, "{g}");
    }

    #[test]
    fn width_scales_the_side_only() {
        let mut g = spec("width", 0.0, vec![], vec![]);
        g.stages[0].width = -0.3;
        let plan = InsertPlan::new(&g, 48_000.0).unwrap();
        let mut st = InsertState::new(plan, 48_000.0, |_| 0);
        // L = mid + side, R = mid − side with mid 0.5, side 0.25
        let (l, r) = st.tick(0.75, 0.25);
        assert!((0.5 * (l + r) - 0.5).abs() < 1e-6, "mid {l} {r}");
        assert!((0.5 * (l - r) - 0.25 * 0.7).abs() < 1e-6, "side {l} {r}");
    }

    #[test]
    fn velocity_route_holds_its_table_value() {
        // the MOJO 2 law: cutoff = min(1, v/127 + 0.5), as a velocity table
        let table: Vec<f32> = (0..128)
            .map(|v| (v as f32 / 127.0 + 0.5).min(1.0))
            .collect();
        let route = FxRouteSpec {
            stage: 0,
            param: "cutoff".into(),
            cc: 0,
            intensity: 0.0,
            lag_ms: 0.0,
            points: vec![],
            env: vec![],
            source: "velocity".into(),
            table,
        };
        let plan = InsertPlan::new(&spec("sv_lp1", 0.0, vec![], vec![route]), 48_000.0).unwrap();
        let fs = 48_000.0;
        let run = |vel: u8| {
            // controllers move; a velocity route must not follow them
            let mut st = InsertState::new(Arc::clone(&plan), fs, |_| 127).with_velocity(vel);
            st.set_ccs(|_| 0);
            let mut peak = 0.0f32;
            for i in 0..48_000 {
                let x = (2.0 * std::f32::consts::PI * 8000.0 * i as f32 / fs).sin();
                let (y, _) = st.tick(x, x);
                if i > 24_000 {
                    peak = peak.max(y.abs());
                }
            }
            20.0 * peak.log10()
        };
        let (soft, loud) = (run(10), run(100));
        assert!(soft < loud - 3.0, "soft {soft} loud {loud}");
        assert!(loud > -1.0, "loud {loud}");
    }
}
