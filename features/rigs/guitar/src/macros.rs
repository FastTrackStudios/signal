//! The macro bar's engine: a row of knobs that each turn a whole corner of
//! the patch at once — Drive through the drive stages, Delay's time and
//! feedback and level together, Width from mono to wide.
//!
//! # Relative to the patch
//!
//! A macro never replaces a value. Each knob has a **rest** position (0.5
//! for most; Width rests at the patch's own spread) where it changes
//! nothing; above rest it gives *more* of what the patch already does,
//! below it *less*, scaled around each param's own value:
//!
//! ```text
//! live = f(baseline, offset)      offset = −1 (knob down) .. 0 (rest) .. 1 (up)
//! ```
//!
//! The **baseline** is the patch as dialled — the chain as built from the
//! patch and its overrides. Macros are not the only way to change a sound,
//! so the two layers are kept apart:
//!
//! - a macro move writes only the live param (the DSP and the chain the UI
//!   draws); it is **never** recorded as a patch override, so turning a
//!   macro up and back leaves the patch exactly where it was;
//! - a direct edit (a block knob, the control surface) moves the baseline:
//!   the value the player dialled is kept live, and the baseline becomes
//!   whatever sits under it at the macro's current offset
//!   ([`MacroEngine::direct_edit`]) — which is what the override records;
//! - the macro positions are kept per patch as offsets
//!   ([`crate::profiles::MacroValueDef`]), so a patch comes back with its
//!   knobs where they were, and a patch with none plays as its values say.
//!
//! Levelling renders patches from their definitions, where no macro lives,
//! so a patch's loudness is measured with every knob at rest.
//!
//! # The bank
//!
//! Built from the live chain by block type and name, so it fits every
//! profile and patch; a knob whose blocks the patch lacks is left out. The
//! hierarchy and the drive stage rules are the legacy macro bar's
//! (`MacroBank` / `MacroKnob` / `MacroBinding` from `signal-macromod`, a
//! parent setting each child to `min + (max − min) × parent`), with one
//! change: a child rests where its binding puts it at the parent's rest, so
//! a parent at rest leaves every child — and every param — as the patch has
//! it.

//! # Tuned by the presets
//!
//! How far a macro moves a param is the preset's to say. A block preset
//! (`blocks.styx`) and a module snapshot (`modules.styx`, keyed by block)
//! carry [`MacroResponseDef`]s — for a knob and param, where the param lands
//! at the knob's bottom and top and the curve between. A module snapshot's
//! entry wins over the block preset's, which wins over a seed (a musical
//! default by the preset's character, [`seed_responses`]), which wins over
//! the engine's own relative response. A composed preset's snapshot carries
//! knob *positions* ([`crate::compose::PresetSnapshotDef::macros`]): the
//! patch's own position wins, then the snapshot's, then rest.

use std::collections::HashMap;

use crate::proto::{BlockParam, LiveBlock, MacroChildView, MacroKnobView};
use signal_macromod::{MacroBank, MacroBinding, MacroKnob};
use signal_proto::block::BlockType;

use crate::compose::{Compositions, MacroResponseDef};
use crate::profiles::{MacroValueDef, PatchDef};

// ── How a knob moves a param ────────────────────────────────────────────────

/// The shape of an offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    /// Toward the end of the range, by a fraction of the way there.
    Lin,
    /// As [`Lin`](Self::Lin) in ratio — times and frequencies, where a step
    /// should sound the same size anywhere on the knob.
    Log,
    /// Plus or minus a span in the param's own units (dB), clamped.
    Add(f32),
    /// A pan's distance from the centre toward the side it is on (0 stays
    /// 0), or toward the centre.
    Spread,
}

/// The shape of a tuned response between rest and an end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Lin,
    /// By ratio: equal steps sound equal on times and frequencies.
    Log,
    /// Slow, then fast.
    Exp,
    /// Slow at both ends.
    S,
}

impl Shape {
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "log" => Self::Log,
            "exp" => Self::Exp,
            "s" | "scurve" | "s-curve" => Self::S,
            _ => Self::Lin,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lin => "lin",
            Self::Log => "log",
            Self::Exp => "exp",
            Self::S => "s",
        }
    }

    /// `from` moved `t` (0..1) of the way to `to`.
    #[must_use]
    pub fn between(self, from: f32, to: f32, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Log if from > 0.0 && to > 0.0 => from * (to / from).powf(t),
            Self::Exp => (to - from).mul_add(t * t, from),
            Self::S => (to - from).mul_add(t * t * 2.0f32.mul_add(-t, 3.0), from),
            _ => (to - from).mul_add(t, from),
        }
    }

    /// The `from` under `v` = `between(from, to, t)` — `None` at `t` = 1,
    /// where every `from` lands on `to`.
    #[must_use]
    pub fn from_of(self, v: f32, to: f32, t: f32) -> Option<f32> {
        let t = t.clamp(0.0, 1.0);
        if t >= 0.999 {
            return None;
        }
        Some(match self {
            Self::Log if v > 0.0 && to > 0.0 => (v / to.powf(t)).powf(1.0 / (1.0 - t)),
            Self::Exp => (v - to * t * t) / (1.0 - t * t),
            Self::S => {
                let s = t * t * 2.0f32.mul_add(-t, 3.0);
                (v - to * s) / (1.0 - s)
            }
            _ => (v - to * t) / (1.0 - t),
        })
    }
}

/// A preset's response on one param ([`MacroResponseDef`], resolved).
#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub shape: Shape,
    pub off: bool,
    /// A drive stage: where on the knob's upper half it comes in.
    pub enter: Option<f32>,
    /// Who says so: `module`, `block`, `seed`, `tuning`, or `stage` (the
    /// drive journey's own default).
    pub source: &'static str,
}

impl Response {
    #[must_use]
    pub fn of(def: &MacroResponseDef, source: &'static str) -> Self {
        Self {
            min: def.min,
            max: def.max,
            shape: Shape::parse(&def.curve),
            off: def.off,
            enter: def.enter,
            source,
        }
    }

    /// The live value for `base` with the knob at offset `m`. `entry` is
    /// where an off drive stage fades in from (instead of `base`).
    #[must_use]
    pub fn apply(&self, base: f32, m: f32, entry: Option<f32>) -> f32 {
        if self.off || m == 0.0 {
            return base;
        }
        if m > 0.0 {
            self.max
                .map_or(base, |to| self.shape.between(entry.unwrap_or(base), to, m))
        } else {
            self.min.map_or(base, |to| self.shape.between(base, to, -m))
        }
    }

    /// [`apply`](Self::apply)'s inverse (without an entry).
    #[must_use]
    pub fn invert(&self, live: f32, m: f32) -> Option<f32> {
        if self.off || m == 0.0 {
            return Some(live);
        }
        let end = if m > 0.0 { self.max } else { self.min };
        match end {
            None => Some(live),
            Some(to) => self.shape.from_of(live, to, m.abs()),
        }
    }

    /// As a definition, for saving.
    #[must_use]
    pub fn def(&self, block: &str, knob: &str, param: &str) -> MacroResponseDef {
        MacroResponseDef {
            block: block.to_string(),
            knob: knob.to_string(),
            param: param.to_string(),
            min: self.min,
            max: self.max,
            curve: self.shape.name().to_string(),
            off: self.off,
            enter: self.enter,
        }
    }
}

/// A tune-mode edit on one target, over what the presets say: each field
/// set is the tuning's, each left `None` inherits (module snapshot, block
/// preset, seed, the engine's own — whichever resolves).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edit {
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub shape: Option<Shape>,
    pub off: Option<bool>,
    pub enter: Option<f32>,
}

impl Edit {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.min.is_none()
            && self.max.is_none()
            && self.shape.is_none()
            && self.off.is_none()
            && self.enter.is_none()
    }
}

/// A target tuned in the panel, as it would be saved.
#[derive(Clone, Debug, PartialEq)]
pub struct Tuned {
    /// The chain block's id and name.
    pub block_id: String,
    pub block: String,
    /// The entry — keyed to the bar knob, so it applies to whichever row
    /// the block sits in.
    pub def: MacroResponseDef,
}

/// A param that says *when*, not *how much*: a delay's time, its tempo
/// divisions, a tempo, a pre-delay. No macro moves these — a macro is more
/// or less of what the patch does, never a different rhythm — and a
/// preset entry naming one is ignored.
#[must_use]
pub fn is_timing(param: &str) -> bool {
    matches!(
        param,
        "time"
            | "time_b"
            | "tap_div_l"
            | "tap_div_r"
            | "tempo_bpm"
            | "predelay"
            | "density"
            | "density_ms"
    )
}

/// A response for one block of the patch, with who set it.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// The chain block, by name.
    pub block: String,
    pub def: MacroResponseDef,
    pub source: &'static str,
}

/// One param a knob moves, and how.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub block: String,
    pub param: String,
    pub lo: f32,
    pub hi: f32,
    pub curve: Curve,
    /// `1` = more is toward `hi`; `-1` = more is toward `lo` (a
    /// compressor's threshold).
    pub dir: f32,
    /// How far all the way up goes: 1 = to the end of the range.
    pub depth: f32,
    /// A preset's own response, in place of `curve`.
    pub resp: Option<Response>,
    /// Tune mode's edit over it.
    pub edit: Option<Edit>,
}

impl Target {
    fn effect(&self, m: f32) -> f32 {
        (m * self.dir * self.depth).clamp(-1.0, 1.0)
    }

    fn log_floor(&self) -> f32 {
        if self.lo > 0.0 {
            self.lo
        } else {
            (self.hi / 1000.0).max(1e-3)
        }
    }

    fn norm(&self, v: f32) -> f32 {
        match self.curve {
            Curve::Log => {
                let f = self.log_floor();
                if self.hi <= f {
                    return 0.0;
                }
                ((v.max(f) / f).ln() / (self.hi / f).ln()).clamp(0.0, 1.0)
            }
            _ => {
                if self.hi > self.lo {
                    ((v - self.lo) / (self.hi - self.lo)).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
        }
    }

    fn denorm(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self.curve {
            Curve::Log => {
                let f = self.log_floor();
                if self.hi <= f {
                    return self.lo;
                }
                // The floor stands in for a 0 Hz "off": a knob all the way
                // down is off again, not 20 Hz.
                if t <= 0.0 {
                    self.lo
                } else {
                    f * (self.hi / f).powf(t)
                }
            }
            _ => t.mul_add(self.hi - self.lo, self.lo),
        }
    }

    /// The live value for `base` with the knob at offset `m`.
    #[must_use]
    pub fn apply(&self, base: f32, m: f32) -> f32 {
        self.apply_from(base, m, None)
    }

    /// As [`apply`](Self::apply), an off drive stage fading in from `entry`.
    #[must_use]
    pub fn apply_from(&self, base: f32, m: f32, entry: Option<f32>) -> f32 {
        if let Some(r) = self.effective(base) {
            return r
                .apply(base, m, entry)
                .clamp(self.lo.min(self.hi), self.hi.max(self.lo));
        }
        self.apply_engine(base, m)
    }

    /// The engine's own relative response, as a response for `base`: where
    /// it lands at each end, on its curve — what an edit of one side keeps
    /// for the other.
    #[must_use]
    pub fn engine_response(&self, base: f32) -> Response {
        Response {
            min: Some(self.apply_engine(base, -1.0)),
            max: Some(self.apply_engine(base, 1.0)),
            shape: if self.curve == Curve::Log {
                Shape::Log
            } else {
                Shape::Lin
            },
            off: false,
            enter: None,
            source: "",
        }
    }

    /// The response in force: the preset's (or the engine's own, when only
    /// an edit says anything) with tune mode's edit over it. `None` = the
    /// engine's own, untouched.
    #[must_use]
    pub fn effective(&self, base: f32) -> Option<Response> {
        let mut r = match (&self.resp, &self.edit) {
            (Some(r), _) => r.clone(),
            (None, Some(_)) => self.engine_response(base),
            (None, None) => return None,
        };
        if let Some(e) = &self.edit {
            if e.min.is_some() {
                r.min = e.min;
            }
            if e.max.is_some() {
                r.max = e.max;
            }
            if let Some(s) = e.shape {
                r.shape = s;
            }
            if let Some(o) = e.off {
                r.off = o;
            }
            if e.enter.is_some() {
                r.enter = e.enter;
            }
            r.source = "tuning";
        }
        Some(r)
    }

    /// The engine's own relative response (no preset, no edit).
    #[must_use]
    pub fn apply_engine(&self, base: f32, m: f32) -> f32 {
        let e = self.effect(m);
        if e == 0.0 {
            return base;
        }
        match self.curve {
            Curve::Lin | Curve::Log => {
                let t = self.norm(base);
                self.denorm(toward(t, e))
            }
            Curve::Add(span) => span.mul_add(e, base).clamp(self.lo, self.hi),
            Curve::Spread => {
                let reach = self.lo.abs().max(self.hi.abs()).max(1e-6);
                let a = (base.abs() / reach).clamp(0.0, 1.0);
                if a == 0.0 {
                    return base;
                }
                base.signum() * toward(a, e) * reach
            }
        }
    }

    /// The baseline under a live value `live` with the knob at offset `m` —
    /// `apply`'s inverse. `None` where there is none: a knob all the way to
    /// one end pins every baseline to that end, so the baseline is kept.
    #[must_use]
    pub fn invert(&self, live: f32, m: f32) -> Option<f32> {
        // An edit over the engine's own: its other end is read at `live`,
        // near enough to the baseline under it.
        if let Some(r) = self.effective(live) {
            return r
                .invert(live, m)
                .map(|v| v.clamp(self.lo.min(self.hi), self.hi.max(self.lo)));
        }
        let e = self.effect(m);
        if e == 0.0 {
            return Some(live);
        }
        match self.curve {
            Curve::Lin | Curve::Log => away(self.norm(live), e).map(|t| self.denorm(t)),
            Curve::Add(span) => Some((-span).mul_add(e, live).clamp(self.lo, self.hi)),
            Curve::Spread => {
                let reach = self.lo.abs().max(self.hi.abs()).max(1e-6);
                let a = (live.abs() / reach).clamp(0.0, 1.0);
                if a == 0.0 {
                    return Some(live);
                }
                away(a, e).map(|b| live.signum() * b * reach)
            }
        }
    }
}

/// `t` moved toward 1 (e > 0) or 0 (e < 0) by the fraction `|e|` of the way.
fn toward(t: f32, e: f32) -> f32 {
    if e >= 0.0 {
        (1.0 - t).mul_add(e, t)
    } else {
        t.mul_add(e, t)
    }
}

/// [`toward`]'s inverse.
fn away(t: f32, e: f32) -> Option<f32> {
    if e >= 0.0 {
        (e < 0.999).then(|| ((t - e) / (1.0 - e)).clamp(0.0, 1.0))
    } else {
        (e > -0.999).then(|| (t / (1.0 + e)).clamp(0.0, 1.0))
    }
}

/// How a touch remote reads a bar knob (see `MacroKnobView::scale`).
#[must_use]
pub fn knob_scale(id: &str) -> &'static str {
    match id {
        "delay" | "reverb" | "space" => "wet",
        "gain" | "tone" | "width" | "output" | "input" => "relative",
        _ => "level",
    }
}

/// A knob's offset from its position: −1 at 0, 0 at `rest`, 1 at 1.
#[must_use]
pub fn offset_of(value: f32, rest: f32) -> f32 {
    let d = value - rest;
    if d >= 0.0 {
        if rest >= 1.0 { 0.0 } else { d / (1.0 - rest) }
    } else if rest <= 0.0 {
        0.0
    } else {
        d / rest
    }
}

/// [`offset_of`]'s inverse.
#[must_use]
pub fn value_of(offset: f32, rest: f32) -> f32 {
    let m = offset.clamp(-1.0, 1.0);
    if m >= 0.0 {
        (1.0 - rest).mul_add(m, rest)
    } else {
        rest.mul_add(m, rest)
    }
}

// ── Bypass rules (the legacy bar's, generalised) ───────────────────────────

/// When a child is on, by its parent's value.
#[derive(Clone, Debug, PartialEq)]
pub struct BypassRule {
    pub child_id: String,
    /// `[lo, hi)` ranges of the parent's value where the child is on.
    pub active_ranges: Vec<(f32, f32)>,
}

/// Rules per parent knob id.
pub type BypassRules = HashMap<String, Vec<BypassRule>>;

/// For each parent with rules, switch its children on or off by the
/// parent's value — the legacy `apply_bypass_rules`, unchanged.
pub fn apply_bypass_rules(bank: &mut MacroBank, rules: &BypassRules) {
    for parent in &mut bank.knobs {
        if let Some(parent_rules) = rules.get(&parent.id) {
            let v = parent.value;
            for rule in parent_rules {
                let active = rule.active_ranges.iter().any(|&(lo, hi)| v >= lo && v < hi);
                if let Some(child) = parent.children.iter_mut().find(|c| c.id == rule.child_id) {
                    child.bypassed = !active;
                }
            }
        }
    }
}

/// Where one drive stage comes and goes on the Drive knob, as offsets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    /// On in the patch.
    pub on: bool,
    /// Where its part of the upper half starts: an off stage comes in
    /// above it, and every stage ramps from there to the top.
    pub enter: f32,
    /// An on stage: how far below rest (as a positive offset) it goes off.
    pub drop: f32,
}

/// The drive journey, relative to how the patch has its stages (`on`, in
/// chain order). At rest each stage is as the patch has it. The upper half
/// is shared out in chain order: stage 1 ramps from rest, stage 2 comes in
/// (if the patch has it off) and ramps from a quarter of the way up, and so
/// on — every stage at its top at the top of the knob. Below rest the stages
/// that are on go, last first, spread over the lower half, until at the
/// bottom none is left. The legacy three-stage rule (stage 1 from the start,
/// stage 2 in the middle, stage 3 at the top), made to fit any number of
/// stages and any starting point.
#[must_use]
pub fn journey(on: &[bool]) -> Vec<Stage> {
    let n = on.len().max(1) as f32;
    let n_on = on.iter().filter(|o| **o).count().max(1) as f32;
    let mut on_left = on.iter().filter(|o| **o).count() as f32;
    on.iter()
        .enumerate()
        .map(|(k, &is_on)| {
            let enter = k as f32 / n;
            if is_on {
                // The first on stage in the chain is the last to go.
                let drop = on_left / n_on;
                on_left -= 1.0;
                Stage {
                    on: true,
                    enter,
                    drop,
                }
            } else {
                Stage {
                    on: false,
                    enter,
                    drop: 0.0,
                }
            }
        })
        .collect()
}

/// The stages' bypass rules on the Drive knob's value (0..1, rest 0.5).
#[must_use]
pub fn drive_rules(ids: &[String], stages: &[Stage]) -> Vec<BypassRule> {
    ids.iter()
        .zip(stages)
        .map(|(id, s)| {
            let lo = if s.on {
                0.5 - s.drop / 2.0
            } else {
                0.5 + s.enter / 2.0
            } + 1e-4;
            BypassRule {
                child_id: id.clone(),
                active_ranges: vec![(lo, 1.01)],
            }
        })
        .collect()
}

/// A stage's knob position for the Drive knob at offset `m`: 0.5 (the
/// patch) at rest; going up, each stage ramps from where it comes in to its
/// top at the top of the knob — the first across the whole upper half, a
/// late one only once it is in; going down, each on stage lowers toward its
/// bottom until it drops out. How far "top" and "bottom" are is the stage's
/// response (a Drive snapshot's, or [`stage_fallback`]).
#[must_use]
pub fn drive_stage_value(m: f32, s: Stage) -> f32 {
    if m >= 0.0 {
        let u = ((m - s.enter) / (1.0 - s.enter).max(1e-6)).clamp(0.0, 1.0);
        0.5 + 0.5 * u
    } else if s.on {
        let u = (-m / s.drop.max(1e-6)).clamp(0.0, 1.0);
        0.5 - 0.5 * u
    } else {
        0.5
    }
}

/// A drive stage's response when no Drive snapshot shapes it, around its
/// dialled drive `r`: up to a sensible top (60 % of the way from `r` to
/// full, not full), down to a gentle 30 % of `r` — and a stage the patch
/// has off fades in from that gentle floor rather than jumping in at `r`.
#[must_use]
pub fn stage_fallback(r: f32) -> Response {
    Response {
        min: Some(r * 0.3),
        max: Some(0.6f32.mul_add(1.0 - r, r)),
        shape: Shape::Lin,
        off: false,
        enter: None,
        source: "stage",
    }
}

// ── Knob metadata ──────────────────────────────────────────────────────────

/// An absolute choice (Type, Interval): the knob writes the param itself.
#[derive(Clone, Debug, PartialEq)]
pub struct Select {
    pub block: String,
    pub param: String,
    /// The param value of each choice, in knob order.
    pub choices: Vec<f32>,
}

/// What a knob does beyond its `MacroKnob` state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub targets: Vec<Target>,
    pub select: Option<Select>,
    /// The position that changes nothing.
    pub rest: f32,
    /// Readout: how to print it, and the (block, param) it prints.
    pub fmt: &'static str,
    pub show: Option<(String, String)>,
    /// A second value the readout needs (a reverb's algorithm).
    pub aux: Option<(String, String)>,
    /// The row it sits in (`DLY 1`).
    pub group: String,
    /// A drive stage: the block its ON/OFF pad switches.
    pub pad_block: Option<String>,
}

/// A bar knob's panel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Panel {
    pub layout: &'static str,
    pub headers: Vec<String>,
    pub anchor: String,
    /// Drive: the stages' children, in chain order (children follow the
    /// [`journey`], not linear bindings).
    pub journey: bool,
    pub style: &'static str,
}

/// The bank a chain gets, before any patch state is put on it.
#[derive(Clone, Debug, Default)]
pub struct Built {
    pub bank: MacroBank,
    pub meta: HashMap<String, Meta>,
    pub panels: HashMap<String, Panel>,
    pub rules: BypassRules,
}

// ── Building the bank from the chain ───────────────────────────────────────

/// A block of the Pre FX module (in front of the amp). The bar's time,
/// modulation and motion knobs are the end of the chain, so they skip these.
fn is_pre_fx(b: &LiveBlock) -> bool {
    b.name.starts_with("Pre ") && !b.name.eq_ignore_ascii_case("Pre Comp")
}

fn param<'a>(b: &'a LiveBlock, name: &str) -> Option<&'a BlockParam> {
    b.params.iter().find(|p| p.name == name)
}

fn value(b: &LiveBlock, name: &str, dflt: f32) -> f32 {
    param(b, name).map_or(dflt, |p| p.value)
}

fn target(b: &LiveBlock, name: &str, curve: Curve, dir: f32, depth: f32) -> Option<Target> {
    param(b, name).map(|p| Target {
        block: b.id.clone(),
        param: p.name.clone(),
        lo: p.min,
        hi: p.max,
        curve,
        dir,
        depth,
        resp: None,
        edit: None,
    })
}

fn knob(id: &str, label: &str, color: &str) -> MacroKnob {
    let mut k = MacroKnob::new(id, label);
    k.color = Some(color.to_string());
    k
}

/// A child of a bar knob: its `MacroKnob`, its metadata, and the binding
/// range the parent drives it across (`None` = the parent leaves it be).
struct Child {
    knob: MacroKnob,
    meta: Meta,
    range: Option<(f32, f32)>,
}

impl Child {
    fn new(id: &str, label: &str, color: &str, meta: Meta, range: Option<(f32, f32)>) -> Self {
        Self {
            knob: knob(id, label, color),
            meta,
            range,
        }
    }
}

/// A relative child over one param of `b`.
#[allow(clippy::too_many_arguments)]
fn rel_child(
    b: &LiveBlock,
    id: &str,
    label: &str,
    color: &str,
    pname: &str,
    curve: Curve,
    dir: f32,
    fmt: &'static str,
    range: Option<(f32, f32)>,
) -> Option<Child> {
    let t = target(b, pname, curve, dir, 1.0)?;
    Some(Child::new(
        id,
        label,
        color,
        Meta {
            targets: vec![t],
            fmt,
            show: Some((b.id.clone(), pname.to_string())),
            group: b.name.clone(),
            ..Meta::default()
        },
        range,
    ))
}

/// An absolute-choice child over `pname` of `b`, its choices `0..=max`.
fn select_child(
    b: &LiveBlock,
    id: &str,
    label: &str,
    color: &str,
    pname: &str,
    fmt: &'static str,
) -> Option<Child> {
    let p = param(b, pname)?;
    let choices: Vec<f32> = (p.min.round() as i32..=p.max.round() as i32)
        .map(|i| i as f32)
        .collect();
    Some(Child::new(
        id,
        label,
        color,
        Meta {
            select: Some(Select {
                block: b.id.clone(),
                param: pname.to_string(),
                choices,
            }),
            fmt,
            show: Some((b.id.clone(), pname.to_string())),
            group: b.name.clone(),
            ..Meta::default()
        },
        None,
    ))
}

/// The bar, left to right: Input, Gate, Pre-Comp, Pitch, Drive, Gain, Tone,
/// Comp, Mod, Motion, Boost, Delay, Reverb, Space, Clarity, Width, Output —
/// each only when the chain has what it turns.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn build(blocks: &[LiveBlock]) -> Built {
    let mut out = Built::default();
    let find = |bt: BlockType, name: &str| {
        blocks
            .iter()
            .find(|b| b.block_type == bt && b.name.eq_ignore_ascii_case(name))
    };
    let post = |pred: &dyn Fn(&LiveBlock) -> bool| -> Vec<&LiveBlock> {
        blocks.iter().filter(|b| !is_pre_fx(b) && pred(b)).collect()
    };
    let delays = post(&|b| b.block_type == BlockType::Delay);
    let verbs = post(&|b| b.block_type == BlockType::Reverb);
    let delays: Vec<&LiveBlock> = delays.into_iter().take(2).collect();
    let verbs: Vec<&LiveBlock> = verbs.into_iter().take(2).collect();

    // A knob with its own targets and no panel.
    let add_single =
        |out: &mut Built, id: &str, label: &str, color: &str, targets: Vec<Target>, rest: f32| {
            if targets.is_empty() {
                return;
            }
            let mut k = knob(id, label, color);
            k.set_value(rest);
            out.bank.knobs.push(k);
            out.meta.insert(
                id.to_string(),
                Meta {
                    targets,
                    rest,
                    ..Meta::default()
                },
            );
        };
    // A bar knob with a panel of children.
    let add_parent = |out: &mut Built,
                      id: &str,
                      label: &str,
                      color: &str,
                      panel: Panel,
                      children: Vec<Child>| {
        if children.is_empty() {
            return;
        }
        let mut parent = knob(id, label, color);
        parent.set_value(0.5);
        for c in children {
            let mut ck = c.knob;
            let rest = match c.range {
                Some((min, max)) => {
                    let binding = MacroBinding::from_ids("self", &ck.id, min, max);
                    // Where the binding puts the child with the parent at
                    // rest: the child's own rest.
                    let rest = parent.compute_binding_value(&binding);
                    parent.bindings.push(binding);
                    rest
                }
                // A selector's rest is its current choice, set when the
                // patch is put on; anything else unbound rests mid-way.
                None => 0.5,
            };
            ck.set_value(rest);
            let mut meta = c.meta;
            meta.rest = rest;
            out.meta.insert(ck.id.clone(), meta);
            parent.children.push(ck);
        }
        out.meta.insert(
            id.to_string(),
            Meta {
                rest: 0.5,
                ..Meta::default()
            },
        );
        out.panels.insert(id.to_string(), panel);
        out.bank.knobs.push(parent);
    };

    // ── Input: an input trim, where the chain has one ──
    if let Some(b) = blocks.iter().find(|b| {
        b.block_type == BlockType::Volume
            && (b.name.eq_ignore_ascii_case("Input") || b.name.eq_ignore_ascii_case("Input Trim"))
    }) {
        let t: Vec<Target> = target(b, "gain_db", Curve::Add(12.0), 1.0, 1.0)
            .into_iter()
            .collect();
        add_single(&mut out, "input", "Input", "#6B7280", t, 0.5);
    }

    // ── Gate ──
    if let Some(b) = blocks.iter().find(|b| b.block_type == BlockType::Gate) {
        let spec: [(&str, &str, &str, &str, Curve, (f32, f32)); 5] = [
            (
                "threshold",
                "Threshold",
                "#CBD5E1",
                "db_gain",
                Curve::Add(12.0),
                (0.0, 0.8),
            ),
            (
                "range",
                "Range",
                "#E2E8F0",
                "db_gain",
                Curve::Lin,
                (0.0, 0.9),
            ),
            ("attack", "Attack", "#F1F5F9", "ms", Curve::Log, (0.0, 0.5)),
            (
                "release",
                "Release",
                "#CBD5E1",
                "ms",
                Curve::Log,
                (0.1, 0.8),
            ),
            ("hold", "Hold", "#CBD5E1", "ms", Curve::Log, (0.0, 0.6)),
        ];
        let kids = spec
            .iter()
            .filter_map(|(p, l, c, f, curve, r)| {
                rel_child(b, &format!("gate-{p}"), l, c, p, *curve, 1.0, f, Some(*r))
            })
            .collect();
        add_parent(
            &mut out,
            "gate",
            "Gate",
            "#94A3B8",
            Panel {
                layout: "row",
                ..Panel::default()
            },
            kids,
        );
    }

    // ── Pre-Comp / Comp: more = more compression ──
    let comp_kids = |b: &LiveBlock, pfx: &str, colors: [&str; 4]| -> Vec<Child> {
        let spec: [(&str, &str, &str, Curve, f32, (f32, f32)); 4] = [
            (
                "threshold",
                "Threshold",
                "db_gain",
                Curve::Add(12.0),
                -1.0,
                (0.0, 0.8),
            ),
            ("ratio", "Ratio", "ratio", Curve::Log, 1.0, (0.0, 0.7)),
            ("attack", "Attack", "ms", Curve::Log, 1.0, (0.0, 0.6)),
            ("release", "Release", "ms", Curve::Log, 1.0, (0.1, 0.8)),
        ];
        spec.iter()
            .zip(colors)
            .filter_map(|((p, l, f, curve, dir, r), c)| {
                rel_child(b, &format!("{pfx}-{p}"), l, c, p, *curve, *dir, f, Some(*r))
            })
            .collect()
    };
    if let Some(b) = find(BlockType::Compressor, "Pre Comp") {
        let kids = comp_kids(b, "pre-comp", ["#F3F4F6", "#E5E7EB", "#D1D5DB", "#F9FAFB"]);
        add_parent(
            &mut out,
            "pre-comp",
            "Pre-Comp",
            "#E5E7EB",
            Panel {
                layout: "row",
                ..Panel::default()
            },
            kids,
        );
    }

    // ── Pitch: the octaves (a POG-style blend on the Pitch block) and the
    // pitch-shifted signal wherever else the patch has one ──
    {
        let mut kids = Vec::new();
        let mut n = 0;
        let r = Some((0.1, 0.9));
        for b in blocks {
            let tag = n + 1;
            let before = kids.len();
            match b.block_type {
                // Two voices over the dry: more Pitch is more of both
                // octaves; the block comes in above rest (it is bypassed in
                // most patches) and goes at the bottom.
                BlockType::Pitch => {
                    if let Some(mut mix) = rel_child(
                        b,
                        &format!("pitch-mix{tag}"),
                        "Mix",
                        "#FDE047",
                        "mix",
                        Curve::Lin,
                        1.0,
                        "pct",
                        r,
                    ) {
                        mix.meta.pad_block = Some(b.id.clone());
                        kids.push(mix);
                    }
                    kids.extend(rel_child(
                        b,
                        &format!("pitch-down{tag}"),
                        "Oct Down",
                        "#EAB308",
                        "b_level",
                        Curve::Lin,
                        1.0,
                        "pct",
                        r,
                    ));
                    kids.extend(rel_child(
                        b,
                        &format!("pitch-up{tag}"),
                        "Oct Up",
                        "#FEF08A",
                        "a_level",
                        Curve::Lin,
                        1.0,
                        "pct",
                        r,
                    ));
                    kids.extend(rel_child(
                        b,
                        &format!("pitch-dry{tag}"),
                        "Dry",
                        "#FEFCE8",
                        "dry",
                        Curve::Lin,
                        1.0,
                        "pct",
                        None,
                    ));
                    kids.extend(select_child(
                        b,
                        &format!("pitch-a{tag}"),
                        "Interval A",
                        "#FEF9C3",
                        "semitones",
                        "semitones",
                    ));
                    kids.extend(select_child(
                        b,
                        &format!("pitch-b{tag}"),
                        "Interval B",
                        "#FEF9C3",
                        "b_semitones",
                        "semitones",
                    ));
                }
                // The Ice machine: its blend is how much of the repeats is
                // shifted.
                BlockType::Delay if !is_pre_fx(b) && value(b, "style", -1.0).round() == 6.0 => {
                    kids.extend(rel_child(
                        b,
                        &format!("pitch-mix{tag}"),
                        "Blend",
                        "#FDE047",
                        "blend",
                        Curve::Lin,
                        1.0,
                        "pct",
                        r,
                    ));
                    kids.extend(select_child(
                        b,
                        &format!("pitch-a{tag}"),
                        "Interval",
                        "#FEF9C3",
                        "interval",
                        "interval",
                    ));
                }
                // A shimmer delay has no amount of its own: its level is it.
                BlockType::Delay if !is_pre_fx(b) && value(b, "style", -1.0).round() == 4.0 => {
                    kids.extend(rel_child(
                        b,
                        &format!("pitch-mix{tag}"),
                        "Shimmer",
                        "#FDE047",
                        "level",
                        Curve::Add(12.0),
                        1.0,
                        "db",
                        r,
                    ));
                }
                BlockType::Reverb
                    if !is_pre_fx(b) && value(b, "algorithm", -1.0).round() == 6.0 =>
                {
                    kids.extend(rel_child(
                        b,
                        &format!("pitch-mix{tag}"),
                        "Shimmer",
                        "#FDE047",
                        "shim_amount",
                        Curve::Lin,
                        1.0,
                        "pct",
                        r,
                    ));
                    kids.extend(select_child(
                        b,
                        &format!("pitch-a{tag}"),
                        "Shift",
                        "#FEF9C3",
                        "shim_shift1",
                        "semitones",
                    ));
                }
                _ => {}
            }
            if kids.len() > before {
                n += 1;
            }
        }
        add_parent(
            &mut out,
            "pitch",
            "Pitch",
            "#FACC15",
            Panel {
                layout: "grouped",
                ..Panel::default()
            },
            kids,
        );
    }

    // ── Drive: a journey through the drive stages ──
    {
        // Every slot of the drive board, in chain order — the Boost pedal
        // and the drives — so the stage count follows the chain.
        let stages: Vec<&LiveBlock> = blocks
            .iter()
            .filter(|b| matches!(b.block_type, BlockType::Boost | BlockType::Drive))
            .collect();
        let colors = ["#FB923C", "#F97316", "#EF4444", "#DC2626"];
        let kids: Vec<Child> = stages
            .iter()
            .enumerate()
            .filter_map(|(k, b)| {
                let mut c = rel_child(
                    b,
                    &format!("drive-{}", k + 1),
                    &b.name,
                    colors[k % colors.len()],
                    "drive",
                    Curve::Lin,
                    1.0,
                    "pct",
                    None,
                )?;
                c.meta.rest = 0.5;
                c.meta.pad_block = Some(b.id.clone());
                Some(c)
            })
            .collect();
        add_parent(
            &mut out,
            "drive",
            "Drive",
            "#F97316",
            Panel {
                layout: "row",
                journey: true,
                ..Panel::default()
            },
            kids,
        );
    }

    // ── Gain: the amps' drive ──
    {
        let t = blocks
            .iter()
            .filter(|b| b.block_type == BlockType::Amp && !b.preset.is_empty())
            .filter_map(|b| target(b, "drive", Curve::Lin, 1.0, 1.0))
            .collect();
        add_single(&mut out, "gain", "Gain", "#D6B36A", t, 0.5);
    }

    // ── Tone: a dark ↔ bright tilt on the Amp EQ, and the wet's top ──
    {
        let mut kids = Vec::new();
        if let Some(eq) = find(BlockType::Eq, "Amp EQ") {
            let bands = tone_bands(eq);
            let spec = [
                ("low", "Low", "#4ADE80", (0.9, 0.1)),
                ("mid", "Mid", "#86EFAC", (0.35, 0.35)),
                ("high", "High", "#BBF7D0", (0.1, 0.9)),
            ];
            for ((key, label, color, r), band) in spec.iter().zip(bands) {
                if let Some(i) = band {
                    kids.extend(rel_child(
                        eq,
                        &format!("tone-{key}"),
                        label,
                        color,
                        &format!("b{i}_gain"),
                        Curve::Add(9.0),
                        1.0,
                        "db_gain",
                        Some(*r),
                    ));
                }
            }
        }
        let cuts: Vec<Target> = delays
            .iter()
            .chain(verbs.iter())
            .filter_map(|b| target(b, "high_cut", Curve::Log, 1.0, 1.0))
            .collect();
        if let Some(first) = cuts.first() {
            kids.push(Child::new(
                "tone-air",
                "Hi Cut",
                "#DCFCE7",
                Meta {
                    show: Some((first.block.clone(), first.param.clone())),
                    targets: cuts,
                    fmt: "hz",
                    group: "Wet".to_string(),
                    ..Meta::default()
                },
                Some((0.1, 0.9)),
            ));
        }
        let before = out.bank.knobs.len();
        add_parent(
            &mut out,
            "tone",
            "Tone",
            "#22C55E",
            Panel {
                layout: "row",
                ..Panel::default()
            },
            kids,
        );
        if out.bank.knobs.len() > before {
            if let Some(k) = out.bank.knobs.last_mut() {
                k.bipolar = true;
            }
        }
    }

    // ── Comp (post) ──
    if let Some(b) = find(BlockType::Compressor, "Post Comp") {
        let kids = comp_kids(b, "comp", ["#F3F4F6", "#E5E7EB", "#D1D5DB", "#F9FAFB"]);
        add_parent(
            &mut out,
            "comp",
            "Comp",
            "#E5E7EB",
            Panel {
                layout: "row",
                ..Panel::default()
            },
            kids,
        );
    }

    // ── Mod: depth and mix of the modulation after the amp ──
    {
        let t = post(&|b| {
            matches!(
                b.block_type,
                BlockType::Chorus | BlockType::Flanger | BlockType::Vibrato | BlockType::Phaser
            )
        })
        .into_iter()
        .flat_map(|b| {
            [
                target(b, "depth", Curve::Lin, 1.0, 1.0),
                target(b, "mix", Curve::Lin, 1.0, 1.0),
            ]
        })
        .flatten()
        .collect();
        add_single(&mut out, "mod", "Mod", "#7DD3FC", t, 0.5);
    }

    // ── Motion: the tremolo's depth ──
    {
        let t = post(&|b| b.block_type == BlockType::Trem)
            .into_iter()
            .filter_map(|b| target(b, "depth", Curve::Lin, 1.0, 1.0))
            .collect();
        add_single(&mut out, "motion", "Motion", "#34D399", t, 0.5);
    }

    // ── Boost: the boost block's level, ±6 dB on what the pedal gives ──
    if let Some(b) = find(BlockType::Volume, "Boost") {
        let t = target(b, "gain_db", Curve::Add(6.0), 1.0, 1.0)
            .into_iter()
            .collect();
        add_single(&mut out, "boost", "Boost", "#FAFAF9", t, 0.5);
    }

    // ── Delay / Reverb: a row per block ──
    let dual =
        |blocks: &[&LiveBlock], cols: &dyn Fn(&LiveBlock, usize) -> Vec<Child>| -> Vec<Child> {
            blocks
                .iter()
                .enumerate()
                .flat_map(|(i, b)| cols(b, i + 1))
                .collect()
        };
    {
        // How strong and what character — the Delay knob itself never
        // moves when (no time, no tempo). The panel's Time is the one way
        // in to the rhythm: a pick of the note (1/4, dotted 1/8…, or Free),
        // set outright, not turned by the bar knob.
        let kids = dual(&delays, &|b, n| {
            [
                select_child(
                    b,
                    &format!("delay-type{n}"),
                    &format!("Type {n}"),
                    "#60A5FA",
                    "style",
                    "delay_style",
                ),
                select_child(
                    b,
                    &format!("delay-time{n}"),
                    &format!("Time {n}"),
                    "#A5B4FC",
                    "tap_div_l",
                    "div",
                ),
                rel_child(
                    b,
                    &format!("delay-fb{n}"),
                    &format!("FB {n}"),
                    "#BFDBFE",
                    "feedback",
                    Curve::Lin,
                    1.0,
                    "pct",
                    Some((0.0, 0.65)),
                ),
                rel_child(
                    b,
                    &format!("delay-filter{n}"),
                    &format!("Filter {n}"),
                    "#DBEAFE",
                    "high_cut",
                    Curve::Log,
                    1.0,
                    "hz",
                    Some((0.0, 0.5)),
                ),
                rel_child(
                    b,
                    &format!("delay-level{n}"),
                    &format!("Level {n}"),
                    "#93C5FD",
                    "level",
                    Curve::Add(12.0),
                    1.0,
                    "db",
                    Some((0.0, 0.7)),
                ),
                rel_child(
                    b,
                    &format!("delay-mod{n}"),
                    &format!("Mod {n}"),
                    "#BFDBFE",
                    "mod_depth",
                    Curve::Lin,
                    1.0,
                    "pct",
                    Some((0.0, 0.6)),
                ),
            ]
            .into_iter()
            .flatten()
            .collect()
        });
        let headers = ["Type", "Time", "Feedback", "Filter", "Level", "Mod"]
            .map(String::from)
            .to_vec();
        add_parent(
            &mut out,
            "delay",
            "Delay",
            "#3B82F6",
            Panel {
                layout: "dual",
                headers,
                ..Panel::default()
            },
            kids,
        );
    }
    {
        // Decay is how much of the tail — a strength. Pre-delay is timing,
        // and stays as the patch has it.
        let kids = dual(&verbs, &|b, n| {
            let mut decay = rel_child(
                b,
                &format!("reverb-time{n}"),
                &format!("Decay {n}"),
                "#C4B5FD",
                "decay",
                Curve::Lin,
                1.0,
                "verb_s",
                Some((0.1, 0.9)),
            );
            if let Some(t) = decay.as_mut() {
                t.meta.aux = Some((b.id.clone(), "algorithm".into()));
            }
            [
                select_child(
                    b,
                    &format!("reverb-type{n}"),
                    &format!("Type {n}"),
                    "#A78BFA",
                    "algorithm",
                    "verb_algo",
                ),
                decay,
                // More reverb is an open, less damped tail.
                rel_child(
                    b,
                    &format!("reverb-character{n}"),
                    &format!("Char {n}"),
                    "#EDE9FE",
                    "damping",
                    Curve::Lin,
                    -1.0,
                    "pct",
                    Some((0.0, 0.8)),
                ),
                rel_child(
                    b,
                    &format!("reverb-level{n}"),
                    &format!("Level {n}"),
                    "#C4B5FD",
                    "level",
                    Curve::Add(12.0),
                    1.0,
                    "db",
                    Some((0.0, 0.7)),
                ),
                rel_child(
                    b,
                    &format!("reverb-mod{n}"),
                    &format!("Mod {n}"),
                    "#DDD6FE",
                    "modulation",
                    Curve::Lin,
                    1.0,
                    "pct",
                    Some((0.0, 0.6)),
                ),
            ]
            .into_iter()
            .flatten()
            .collect()
        });
        let headers = ["Type", "Decay", "Character", "Level", "Mod"]
            .map(String::from)
            .to_vec();
        add_parent(
            &mut out,
            "reverb",
            "Reverb",
            "#8B5CF6",
            Panel {
                layout: "dual",
                headers,
                ..Panel::default()
            },
            kids,
        );
    }

    // ── Space: the whole wash — wet levels, a little feedback and decay ──
    {
        let t = delays
            .iter()
            .chain(verbs.iter())
            .flat_map(|b| {
                [
                    target(b, "level", Curve::Add(9.0), 1.0, 1.0),
                    target(b, "feedback", Curve::Lin, 1.0, 0.3),
                    target(b, "decay", Curve::Lin, 1.0, 0.3),
                ]
            })
            .flatten()
            .collect();
        add_single(&mut out, "space", "Space", "#6366F1", t, 0.5);
    }

    // ── Clarity: ducking and a low cut on the wet ──
    {
        let mut kids = Vec::new();
        for (i, b) in delays.iter().enumerate() {
            let n = format!("dly{}", i + 1);
            kids.extend(rel_child(
                b,
                &format!("clarity-{n}-duck"),
                "Duck",
                "#5EEAD4",
                "duck_sens",
                Curve::Lin,
                1.0,
                "db_gain",
                Some((0.1, 0.9)),
            ));
            kids.extend(rel_child(
                b,
                &format!("clarity-{n}-release"),
                "Release",
                "#99F6E4",
                "duck_release",
                Curve::Log,
                1.0,
                "s",
                Some((0.1, 0.9)),
            ));
            kids.extend(rel_child(
                b,
                &format!("clarity-{n}-lowcut"),
                "Lo Cut",
                "#CCFBF1",
                "high_pass",
                Curve::Lin,
                1.0,
                "hz",
                Some((0.1, 0.9)),
            ));
        }
        for (i, b) in verbs.iter().enumerate() {
            let n = format!("verb{}", i + 1);
            kids.extend(rel_child(
                b,
                &format!("clarity-{n}-duck"),
                "Duck",
                "#5EEAD4",
                "duck",
                Curve::Lin,
                1.0,
                "pct",
                Some((0.1, 0.9)),
            ));
            kids.extend(rel_child(
                b,
                &format!("clarity-{n}-thresh"),
                "Thresh",
                "#2DD4BF",
                "duck_threshold",
                Curve::Add(12.0),
                -1.0,
                "db_gain",
                Some((0.1, 0.9)),
            ));
            kids.extend(rel_child(
                b,
                &format!("clarity-{n}-release"),
                "Release",
                "#99F6E4",
                "duck_release",
                Curve::Log,
                1.0,
                "ms",
                Some((0.1, 0.9)),
            ));
            kids.extend(rel_child(
                b,
                &format!("clarity-{n}-lowcut"),
                "Lo Cut",
                "#CCFBF1",
                "low_cut",
                Curve::Log,
                1.0,
                "hz",
                Some((0.1, 0.9)),
            ));
        }
        let anchor = if out.bank.knobs.iter().any(|k| k.id == "delay") {
            "delay"
        } else if out.bank.knobs.iter().any(|k| k.id == "reverb") {
            "reverb"
        } else {
            ""
        };
        add_parent(
            &mut out,
            "clarity",
            "Clarity",
            "#2DD4BF",
            Panel {
                layout: "grouped",
                anchor: anchor.to_string(),
                ..Panel::default()
            },
            kids,
        );
    }

    // ── Width: 0 = mono; rests at the patch's own spread ──
    {
        let mut t: Vec<Target> = blocks
            .iter()
            .filter(|b| b.block_type == BlockType::Volume)
            .filter_map(|b| target(b, "pan", Curve::Spread, 1.0, 1.0))
            .collect();
        for b in &delays {
            t.extend(target(b, "pan", Curve::Spread, 1.0, 1.0));
        }
        for b in &verbs {
            t.extend(target(b, "pan_a", Curve::Spread, 1.0, 1.0));
        }
        for b in post(&|b| {
            matches!(
                b.block_type,
                BlockType::Chorus | BlockType::Flanger | BlockType::Vibrato
            )
        }) {
            t.extend(target(b, "width", Curve::Lin, 1.0, 1.0));
        }
        let rest = spread_of(&t, blocks);
        add_single(&mut out, "width", "Width", "#A3E635", t, rest);
        if out.bank.knobs.last().is_some_and(|k| k.id == "width") {
            out.panels.insert(
                "width".into(),
                Panel {
                    style: "spread",
                    ..Panel::default()
                },
            );
        }
    }

    // ── Output: the patch's level, ±6 dB ──
    if let Some(b) = find(BlockType::Volume, crate::profiles::TRIM_BLOCK) {
        let t = target(b, "gain_db", Curve::Add(6.0), 1.0, 1.0)
            .into_iter()
            .collect();
        add_single(&mut out, "output", "Output", "#6B7280", t, 0.5);
    }

    out
}

/// The Amp EQ's bands for Low, Mid and High: switched-on bells or shelves
/// nearest 150 Hz, 800 Hz and 4 kHz, each band used once.
fn tone_bands(eq: &LiveBlock) -> [Option<usize>; 3] {
    let mut free: Vec<(usize, f32)> = (1..=24)
        .filter(|i| value(eq, &format!("b{i}_on"), 0.0) >= 0.5)
        .filter(|i| {
            // Bell, low shelf, high shelf: the shapes with a gain.
            let shape = value(eq, &format!("b{i}_shape"), 0.0).round();
            shape <= 2.0 && param(eq, &format!("b{i}_gain")).is_some()
        })
        .map(|i| (i, value(eq, &format!("b{i}_freq"), 1000.0).max(1.0)))
        .collect();
    let mut pick = |hz: f32| {
        let best = free
            .iter()
            .enumerate()
            .min_by(|a, b| {
                let da = (a.1.1 / hz).ln().abs();
                let db = (b.1.1 / hz).ln().abs();
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)?;
        Some(free.remove(best).0)
    };
    // High and low first: the mid takes what is left between them.
    let low = pick(150.0);
    let high = pick(4000.0);
    let mid = pick(800.0);
    [low, mid, high]
}

/// How spread the patch is, 0 (mono) .. 1: the mean of each target's
/// distance from mono.
fn spread_of(targets: &[Target], blocks: &[LiveBlock]) -> f32 {
    if targets.is_empty() {
        return 0.5;
    }
    let sum: f32 = targets
        .iter()
        .map(|t| {
            let v = blocks
                .iter()
                .find(|b| b.id == t.block)
                .map_or(0.0, |b| value(b, &t.param, 0.0));
            match t.curve {
                Curve::Spread => (v.abs() / t.lo.abs().max(t.hi.abs()).max(1e-6)).clamp(0.0, 1.0),
                _ => t.norm(v),
            }
        })
        .sum();
    sum / targets.len() as f32
}

// ── The engine ─────────────────────────────────────────────────────────────

/// A live param write: `(block id, param, value)`.
pub type Write = (String, String, f32);

/// The params a choice on `param` sets: a delay's division is both its
/// taps' (`tap_div_l` read, both written — as its face sets it).
#[must_use]
pub fn choice_params(param: &str) -> Vec<&str> {
    match param {
        "tap_div_l" => vec!["tap_div_l", "tap_div_r"],
        p => vec![p],
    }
}

/// The active patch's macro bar, and the baseline it sits on.
#[derive(Clone, Debug, Default)]
pub struct MacroEngine {
    patch: String,
    built: Built,
    /// The patch as dialled, by `(block id, param)`.
    baseline: HashMap<(String, String), f32>,
    /// Each drive stage's bypass as the patch has it, by block id.
    base_bypass: HashMap<String, bool>,
    /// ON/OFF pads pressed since their knob last moved: child id → on.
    pads: HashMap<String, bool>,
    /// Each journey knob's stages, in child order.
    journeys: HashMap<String, Vec<Stage>>,
    /// The presets' responses for this patch ([`responses_for`]).
    responses: Vec<Resolved>,
    /// Its preset snapshot's knob positions (offsets) — what the patch's
    /// own positions are kept against.
    defaults: Vec<MacroValueDef>,
    /// Tune mode's edits, by `(knob id, block id, param)` — live until
    /// saved into a preset, discarded, or the patch changes.
    tuned: HashMap<(String, String, String), Edit>,
    /// Block names by id.
    names: HashMap<String, String>,
    /// The preset snapshot the patch plays (`Fender · Clean`), or empty.
    snapshot: String,
}

/// What a patch brings to the bar besides its chain.
#[derive(Clone, Debug, Default)]
pub struct Context {
    /// The patch's own knob positions.
    pub saved: Vec<MacroValueDef>,
    /// Its preset snapshot's positions, under the patch's.
    pub defaults: Vec<MacroValueDef>,
    /// The presets' responses ([`responses_for`]).
    pub responses: Vec<Resolved>,
    /// The preset snapshot the patch plays (`Fender · Clean`), or empty —
    /// where the bar's positions can be kept.
    pub snapshot: String,
}

impl Context {
    /// Only the patch's own positions.
    #[must_use]
    pub fn saved(saved: &[MacroValueDef]) -> Self {
        Self {
            saved: saved.to_vec(),
            ..Self::default()
        }
    }
}

impl MacroEngine {
    /// Set a baseline the chain does not carry — the boost pedal's level,
    /// which the rig puts on its gain block after the chain is built.
    pub fn set_base(&mut self, block: &str, param: &str, v: f32) {
        self.baseline
            .insert((block.to_string(), param.to_string()), v);
    }

    /// Build the bank again over the same baseline — after a choice that
    /// changes which knobs there are (a delay turned into the Ice machine
    /// brings Pitch in) or where one rests (a pan moved). `blocks` is the
    /// live chain; its values are swapped for the baseline's. Positions are
    /// kept.
    pub fn refresh(&mut self, blocks: &[LiveBlock]) {
        let base: Vec<LiveBlock> = blocks
            .iter()
            .map(|b| {
                let mut b = b.clone();
                for p in &mut b.params {
                    if let Some(v) = self.base(&b.id, &p.name) {
                        p.value = v;
                    }
                }
                if let Some(byp) = self.base_bypass.get(&b.id) {
                    b.bypassed = *byp;
                }
                b
            })
            .collect();
        let patch = self.patch.clone();
        let ctx = Context {
            saved: self.saved(),
            defaults: self.defaults.clone(),
            responses: self.responses.clone(),
            snapshot: self.snapshot.clone(),
        };
        let extra: Vec<((String, String), f32)> = self
            .baseline
            .iter()
            .filter(|((b, p), _)| {
                !base
                    .iter()
                    .any(|x| x.id == *b && x.params.iter().any(|q| q.name == *p))
            })
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        self.rebase(&patch, &base, &ctx);
        self.baseline.extend(extra);
    }

    /// Whether the bar has knob `id` (a bar knob or a panel knob).
    #[must_use]
    pub fn knows(&self, id: &str) -> bool {
        self.built.meta.contains_key(id)
    }

    /// Whether knob `id` has an ON/OFF pad.
    #[must_use]
    pub fn has_pad(&self, id: &str) -> bool {
        self.built
            .meta
            .get(id)
            .is_some_and(|m| m.pad_block.is_some())
    }

    /// Block `id`'s params as the patch has them — no macro on them.
    #[must_use]
    pub fn baseline_params(&self, id: &str) -> Vec<(String, f32)> {
        let mut out: Vec<(String, f32)> = self
            .baseline
            .iter()
            .filter(|((b, _), _)| b == id)
            .map(|((_, p), v)| (p.clone(), *v))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// A drive stage's bypass as the patch has it.
    #[must_use]
    pub fn base_bypassed(&self, id: &str) -> Option<bool> {
        self.base_bypass.get(id).copied()
    }

    /// The preset snapshot the patch plays, or empty.
    #[must_use]
    pub fn snapshot(&self) -> &str {
        &self.snapshot
    }

    /// The patch the bank belongs to.
    #[must_use]
    pub fn patch(&self) -> &str {
        &self.patch
    }

    /// Put `patch` on: its chain as built (`blocks` — the baseline, no
    /// macro applied), its presets' responses and its knob positions.
    pub fn rebase(&mut self, patch: &str, blocks: &[LiveBlock], ctx: &Context) {
        if !self.patch.eq_ignore_ascii_case(patch) {
            self.tuned.clear();
        }
        self.patch = patch.to_string();
        self.responses = ctx.responses.clone();
        self.defaults = ctx.defaults.clone();
        self.snapshot = ctx.snapshot.clone();
        self.baseline = blocks
            .iter()
            .flat_map(|b| {
                b.params
                    .iter()
                    .map(move |p| ((b.id.clone(), p.name.clone()), p.value))
            })
            .collect();
        self.built = build(blocks);
        self.attach(blocks);
        self.base_bypass = self
            .built
            .meta
            .values()
            .filter_map(|m| m.pad_block.clone())
            .map(|id| {
                let byp = blocks
                    .iter()
                    .find(|b| b.id == id)
                    .is_some_and(|b| b.bypassed);
                (id, byp)
            })
            .collect();
        // Selectors rest on the choice the patch makes.
        let rests: Vec<(String, f32)> = self
            .built
            .meta
            .iter()
            .filter_map(|(id, m)| {
                let s = m.select.as_ref()?;
                let cur = self.base(&s.block, &s.param)?;
                Some((id.clone(), select_position(s, cur)))
            })
            .collect();
        for (id, pos) in rests {
            if let Some(m) = self.built.meta.get_mut(&id) {
                m.rest = pos;
            }
            if let Some(k) = self.built.bank.get_knob_mut(&id) {
                k.set_value(pos);
            }
        }
        self.rebuild_rules();
        self.pads.clear();
        // Positions: the preset snapshot's, then the patch's own over them —
        // bar knobs first (they set their children), then the children
        // moved on their own since.
        let is_parent = |id: &str| self.built.bank.knobs.iter().any(|k| k.id == id);
        let mut all: Vec<MacroValueDef> = ctx.defaults.clone();
        for d in &ctx.saved {
            match all.iter_mut().find(|x| x.id == d.id) {
                Some(x) => *x = d.clone(),
                None => all.push(d.clone()),
            }
        }
        let (parents, kids): (Vec<&MacroValueDef>, Vec<&MacroValueDef>) =
            all.iter().partition(|d| is_parent(&d.id));
        for d in parents.into_iter().chain(kids) {
            if let Some(rest) = self.built.meta.get(&d.id).map(|m| m.rest) {
                if self
                    .built
                    .meta
                    .get(&d.id)
                    .is_some_and(|m| m.select.is_none())
                {
                    self.set_position(&d.id, value_of(d.value, rest));
                }
            }
            match d.pad.as_str() {
                "on" => {
                    self.pads.insert(d.id.clone(), true);
                }
                "off" => {
                    self.pads.insert(d.id.clone(), false);
                }
                _ => {}
            }
        }
        self.apply_pads();
    }

    /// The stage rules, from the patch's own bypasses: every child with an
    /// ON/OFF pad is a stage of its parent (the drive stages; the Pitch
    /// block).
    fn rebuild_rules(&mut self) {
        let mut rules = BypassRules::new();
        let mut journeys = HashMap::new();
        for parent in &self.built.bank.knobs {
            let ids: Vec<String> = parent
                .children
                .iter()
                .filter(|c| {
                    self.built
                        .meta
                        .get(&c.id)
                        .is_some_and(|m| m.pad_block.is_some())
                })
                .map(|c| c.id.clone())
                .collect();
            if ids.is_empty() {
                continue;
            }
            let on: Vec<bool> = ids
                .iter()
                .map(|id| {
                    self.built
                        .meta
                        .get(id)
                        .and_then(|m| m.pad_block.as_ref())
                        .is_some_and(|b| !self.base_bypass.get(b).copied().unwrap_or(false))
                })
                .collect();
            let mut stages = journey(&on);
            // A Drive snapshot's stage says where it comes in.
            for (s, id) in stages.iter_mut().zip(&ids) {
                if let Some(e) = self
                    .built
                    .meta
                    .get(id)
                    .and_then(|m| m.targets.first())
                    .and_then(|t| t.resp.as_ref())
                    .and_then(|r| r.enter)
                {
                    s.enter = e.clamp(0.0, 0.99);
                }
            }
            rules.insert(parent.id.clone(), drive_rules(&ids, &stages));
            // Drive's children follow the journey rather than bindings.
            if self.built.panels.get(&parent.id).is_some_and(|p| p.journey) {
                journeys.insert(parent.id.clone(), stages);
            }
        }
        self.built.rules = rules;
        self.journeys = journeys;
    }

    /// Put the presets' responses (and any being tuned) on the targets they
    /// name. A panel knob whose every param is tuned follows its bar knob
    /// end to end (a 0..1 binding), so the bar knob's top is the preset's
    /// top. Drive stages without one get [`stage_fallback`].
    fn attach(&mut self, blocks: &[LiveBlock]) {
        self.names = blocks
            .iter()
            .map(|b| (b.id.clone(), b.name.clone()))
            .collect();
        let mut family: HashMap<String, String> = HashMap::new();
        let mut journey_kids: Vec<String> = Vec::new();
        for p in &self.built.bank.knobs {
            family.insert(p.id.clone(), p.id.clone());
            let journey = self.built.panels.get(&p.id).is_some_and(|x| x.journey);
            for c in &p.children {
                family.insert(c.id.clone(), p.id.clone());
                if journey {
                    journey_kids.push(c.id.clone());
                }
            }
        }
        let Self {
            built,
            responses,
            tuned,
            names,
            baseline,
            ..
        } = self;
        let mut rebind: Vec<(String, String)> = Vec::new();
        for (kid, meta) in &mut built.meta {
            let fam = family.get(kid).cloned().unwrap_or_else(|| kid.clone());
            let mut all = !meta.targets.is_empty() && meta.select.is_none();
            for t in &mut meta.targets {
                let name = names.get(&t.block).cloned().unwrap_or_default();
                t.resp = responses
                    .iter()
                    .find(|r| {
                        r.block.eq_ignore_ascii_case(&name)
                            && r.def.param == t.param
                            && (r.def.knob == *kid || r.def.knob == fam)
                    })
                    .map(|r| Response::of(&r.def, r.source));
                t.edit = tuned
                    .get(&(kid.clone(), t.block.clone(), t.param.clone()))
                    .cloned();
                if t.resp.is_none() && journey_kids.contains(kid) {
                    let r = baseline
                        .get(&(t.block.clone(), t.param.clone()))
                        .copied()
                        .unwrap_or(0.5);
                    t.resp = Some(stage_fallback(r));
                } else if t.resp.is_none() && t.edit.is_none() {
                    all = false;
                }
            }
            if all && fam != *kid && !journey_kids.contains(kid) {
                rebind.push((fam, kid.clone()));
            }
        }
        for (p, c) in rebind {
            if let Some(parent) = built.bank.get_mut(&p) {
                for b in parent
                    .bindings
                    .iter_mut()
                    .filter(|b| b.target.param_id == c)
                {
                    b.min = 0.0;
                    b.max = 1.0;
                }
                if let Some(k) = parent.get_child_mut(&c) {
                    k.set_value(0.5);
                }
            }
            if let Some(m) = built.meta.get_mut(&c) {
                m.rest = 0.5;
            }
        }
    }

    /// Edit one target's response in tune mode — live until saved,
    /// discarded, or the patch changes. `op`: `min`, `max`, `enter`
    /// (`value`), `curve` (`text`), `off` (`value` ≥ 0.5), or a reset back
    /// to what the presets say: `reset_min`, `reset_max`, `reset_curve`,
    /// `reset_off`, `reset_enter`, `reset` (the whole target). `blocks` is
    /// the live chain.
    ///
    /// # Errors
    ///
    /// No such knob, or it does not move that param, or an unknown op.
    pub fn tune_op(
        &mut self,
        knob: &str,
        block: &str,
        param: &str,
        op: &str,
        value: f32,
        text: &str,
        blocks: &[LiveBlock],
    ) -> Result<(), String> {
        if is_timing(param) {
            return Err(format!("macros never move timing ({param})"));
        }
        let meta = self
            .built
            .meta
            .get(knob)
            .ok_or_else(|| format!("no macro knob {knob:?} on this patch"))?;
        if !meta
            .targets
            .iter()
            .any(|t| t.block == block && t.param == param)
        {
            return Err(format!("{knob} does not move {param} on that block"));
        }
        let key = (knob.to_string(), block.to_string(), param.to_string());
        let mut e = self.tuned.get(&key).cloned().unwrap_or_default();
        match op {
            "min" => e.min = Some(value),
            "max" => e.max = Some(value),
            "enter" => e.enter = Some(value.clamp(0.0, 0.99)),
            "curve" => e.shape = Some(Shape::parse(text)),
            "off" => e.off = Some(value >= 0.5),
            "reset_min" => e.min = None,
            "reset_max" => e.max = None,
            "reset_curve" => e.shape = None,
            "reset_off" => e.off = None,
            "reset_enter" => e.enter = None,
            "reset" => e = Edit::default(),
            _ => return Err(format!("unknown tune op {op:?}")),
        }
        if e.is_empty() {
            self.tuned.remove(&key);
        } else {
            self.tuned.insert(key, e);
        }
        self.refresh(blocks);
        Ok(())
    }

    /// The bar knob `parent` and its panel knobs — what one tune panel
    /// covers.
    fn family(&self, parent: &str) -> Vec<String> {
        self.built
            .bank
            .get(parent)
            .map(|p| {
                std::iter::once(p.id.clone())
                    .chain(p.children.iter().map(|c| c.id.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Something on `parent`'s panel is tuned and not saved.
    #[must_use]
    pub fn has_edits(&self, parent: &str) -> bool {
        let fam = self.family(parent);
        self.tuned.keys().any(|(k, _, _)| fam.contains(k))
    }

    /// What is tuned on bar knob `parent`'s panel, as it would be saved:
    /// the whole response in force on each edited target.
    #[must_use]
    pub fn tuned_defs(&self, parent: &str) -> Vec<Tuned> {
        let mut out = Vec::new();
        for id in self.family(parent) {
            let Some(meta) = self.built.meta.get(&id) else {
                continue;
            };
            for t in meta.targets.iter().filter(|t| t.edit.is_some()) {
                let base = self.base(&t.block, &t.param).unwrap_or(0.0);
                let Some(r) = t.effective(base) else { continue };
                out.push(Tuned {
                    block_id: t.block.clone(),
                    block: self.names.get(&t.block).cloned().unwrap_or_default(),
                    def: r.def("", parent, &t.param),
                });
            }
        }
        out
    }

    /// Every `(block name, bar knob, param)` bar knob `parent`'s panel
    /// moves — what its Reset clears from a preset.
    #[must_use]
    pub fn panel_targets(&self, parent: &str) -> Vec<(String, String, String)> {
        let mut out: Vec<(String, String, String)> = Vec::new();
        for id in self.family(parent) {
            let Some(meta) = self.built.meta.get(&id) else {
                continue;
            };
            for t in &meta.targets {
                let e = (
                    self.names.get(&t.block).cloned().unwrap_or_default(),
                    parent.to_string(),
                    t.param.clone(),
                );
                if !out.contains(&e) {
                    out.push(e);
                }
            }
        }
        out
    }

    /// Forget what is tuned on `parent`'s panel (saved, or thrown away).
    pub fn untune(&mut self, parent: &str, blocks: &[LiveBlock]) {
        let fam = self.family(parent);
        self.tuned.retain(|(k, _, _), _| !fam.contains(k));
        self.refresh(blocks);
    }

    /// Double-click in play: a bar knob back to rest, a panel knob back to
    /// where its bar knob puts it (its own offset gone).
    ///
    /// # Errors
    ///
    /// No such knob, or a choice (it has no rest to go back to).
    pub fn reset_position(&mut self, id: &str) -> Result<(), String> {
        let meta = self
            .built
            .meta
            .get(id)
            .ok_or_else(|| format!("no macro knob {id:?} on this patch"))?;
        if meta.select.is_some() {
            return Err("a choice has no rest to go back to".to_string());
        }
        let rest = meta.rest;
        if self.built.bank.get(id).is_some() {
            self.set_position(id, rest);
        } else {
            let put = self
                .built
                .bank
                .knobs
                .iter()
                .find(|p| p.children.iter().any(|c| c.id == id))
                .and_then(|p| {
                    self.implied(p)
                        .into_iter()
                        .find(|(c, _)| c == id)
                        .map(|(_, v)| v)
                })
                .unwrap_or(rest);
            if let Some(k) = self.built.bank.get_knob_mut(id) {
                k.set_value(put);
            }
        }
        self.apply_pads();
        Ok(())
    }

    /// Every knob's position as an offset, off rest — what a preset snapshot
    /// keeps.
    #[must_use]
    pub fn positions(&self) -> Vec<MacroValueDef> {
        self.offsets()
            .into_iter()
            .filter(|(_, m, bar)| !*bar || m.abs() > 1e-4)
            .map(|(id, value, _)| MacroValueDef {
                id,
                value,
                pad: String::new(),
            })
            .collect()
    }

    /// Take `defaults` as the preset snapshot's positions from now on (they
    /// were just saved there), so the patch keeps nothing of its own.
    pub fn set_defaults(&mut self, defaults: Vec<MacroValueDef>) {
        self.defaults = defaults;
    }

    fn base(&self, block: &str, param: &str) -> Option<f32> {
        self.baseline
            .get(&(block.to_string(), param.to_string()))
            .copied()
    }

    /// Set a knob's position (and, for a bar knob, its children's).
    fn set_position(&mut self, id: &str, value: f32) {
        let stages = self.journeys.get(id).cloned();
        let rest = self.built.meta.get(id).map_or(0.5, |m| m.rest);
        let Some(k) = self.built.bank.get_knob_mut(id) else {
            return;
        };
        k.set_value(value);
        let _ = (stages, rest);
        let implied = self
            .built
            .bank
            .get(id)
            .map(|p| self.implied(p))
            .unwrap_or_default();
        if let Some(parent) = self.built.bank.get_mut(id) {
            for (cid, cv) in implied {
                if let Some(c) = parent.get_child_mut(&cid) {
                    c.set_value(cv);
                }
            }
            // The legacy bar: a bar knob move re-applies its rules, so a pad
            // pressed since is forgotten.
            let ids: Vec<String> = parent.children.iter().map(|c| c.id.clone()).collect();
            for cid in ids {
                self.pads.remove(&cid);
            }
            apply_bypass_rules(&mut self.built.bank, &self.built.rules);
        }
    }

    /// Where bar knob `parent`'s panel knobs sit for its present value —
    /// by its bindings, or Drive's journey.
    fn implied(&self, parent: &MacroKnob) -> Vec<(String, f32)> {
        let rest = self.built.meta.get(&parent.id).map_or(0.5, |m| m.rest);
        match self.journeys.get(&parent.id) {
            Some(stages) => {
                let m = offset_of(parent.value, rest);
                parent
                    .children
                    .iter()
                    .zip(stages)
                    .map(|(c, s)| (c.id.clone(), drive_stage_value(m, *s)))
                    .collect()
            }
            None => parent
                .bindings
                .iter()
                .map(|b| (b.target.param_id.clone(), parent.compute_binding_value(b)))
                .collect(),
        }
    }

    /// Every knob's position as an offset — the bar knobs off rest, and the
    /// panel knobs moved away from where their bar knob puts them.
    fn offsets(&self) -> Vec<(String, f32, bool)> {
        let mut out = Vec::new();
        for parent in &self.built.bank.knobs {
            out.push((parent.id.clone(), self.offset(&parent.id), true));
            let implied = self.implied(parent);
            for c in &parent.children {
                let Some(meta) = self.built.meta.get(&c.id).filter(|m| m.select.is_none()) else {
                    continue;
                };
                let rest = meta.rest;
                // Where its bar knob puts it — or, left be by the bar knob,
                // its rest.
                let put = implied
                    .iter()
                    .find(|(id, _)| *id == c.id)
                    .map_or(rest, |(_, v)| *v);
                let own = (put - c.value).abs() > 1e-4;
                if own {
                    out.push((c.id.clone(), offset_of(c.value, rest), false));
                }
            }
        }
        out
    }

    fn apply_pads(&mut self) {
        apply_bypass_rules(&mut self.built.bank, &self.built.rules);
        for (id, on) in &self.pads {
            if let Some(k) = self.built.bank.get_knob_mut(id) {
                k.bypassed = !on;
            }
        }
    }

    /// Move knob `id` to `value`. A selector (Type, Interval) is a choice,
    /// not an offset: it comes back as a write to make as a direct edit.
    pub fn set(&mut self, id: &str, value: f32) -> Option<Write> {
        let meta = self.built.meta.get(id)?.clone();
        if let Some(s) = meta.select {
            let v = select_value(&s, value);
            self.baseline.insert((s.block.clone(), s.param.clone()), v);
            let pos = select_position(&s, v);
            if let Some(k) = self.built.bank.get_knob_mut(id) {
                k.set_value(pos);
            }
            if let Some(m) = self.built.meta.get_mut(id) {
                m.rest = pos;
            }
            return Some((s.block, s.param, v));
        }
        self.set_position(id, value);
        self.apply_pads();
        None
    }

    /// Press a drive stage's pad.
    pub fn set_pad(&mut self, id: &str, on: bool) {
        if self
            .built
            .meta
            .get(id)
            .is_some_and(|m| m.pad_block.is_some())
        {
            self.pads.insert(id.to_string(), on);
            self.apply_pads();
        }
    }

    /// The knob's offset from rest, −1..1.
    fn offset(&self, id: &str) -> f32 {
        match (self.built.bank.get_knob(id), self.built.meta.get(id)) {
            (Some(k), Some(m)) if m.select.is_none() => offset_of(k.value, m.rest),
            _ => 0.0,
        }
    }

    /// Every knob's targets on `(block, param)`, in bank order, with their
    /// offsets — the layers from the baseline up.
    fn layers(&self, block: &str, param: &str) -> Vec<(Target, f32, Option<f32>)> {
        let mut out = Vec::new();
        for parent in &self.built.bank.knobs {
            let journey = self.journeys.contains_key(&parent.id);
            for k in std::iter::once(parent).chain(parent.children.iter()) {
                let Some(meta) = self.built.meta.get(&k.id) else {
                    continue;
                };
                let m = self.offset(&k.id);
                // A drive stage the patch has off fades in from its floor.
                let entry = meta
                    .pad_block
                    .as_ref()
                    .filter(|b| journey && self.base_bypass.get(*b).copied().unwrap_or(false));
                for t in meta
                    .targets
                    .iter()
                    .filter(|t| t.block == block && t.param == param)
                {
                    let from = entry.and_then(|_| t.resp.as_ref()).and_then(|r| r.min);
                    out.push((t.clone(), m, from));
                }
            }
        }
        out
    }

    /// The live value of `(block, param)`: the baseline with every macro on
    /// it applied.
    #[must_use]
    pub fn live(&self, block: &str, param: &str) -> Option<f32> {
        let base = self.base(block, param)?;
        // A knob at rest is no layer at all: the patch's own value, as
        // stored — not a curve evaluated at zero, which a clamp or a
        // round-trip could move by a bit.
        Some(
            self.layers(block, param)
                .iter()
                .filter(|(_, m, _)| *m != 0.0)
                .fold(base, |v, (t, m, e)| t.apply_from(v, *m, *e)),
        )
    }

    /// Every param a macro can move, with its live value.
    #[must_use]
    pub fn live_params(&self) -> Vec<Write> {
        let mut seen: Vec<(String, String)> = Vec::new();
        for meta in self.built.meta.values() {
            for t in &meta.targets {
                let key = (t.block.clone(), t.param.clone());
                if !seen.contains(&key) {
                    seen.push(key);
                }
            }
        }
        seen.sort();
        seen.into_iter()
            .filter_map(|(b, p)| self.live(&b, &p).map(|v| (b, p, v)))
            .collect()
    }

    /// Every drive stage's live bypass: `(block id, bypassed)`.
    #[must_use]
    pub fn live_bypass(&self) -> Vec<(String, bool)> {
        let mut out: Vec<(String, bool)> = self
            .built
            .meta
            .iter()
            .filter_map(|(id, m)| {
                let block = m.pad_block.clone()?;
                let k = self.built.bank.get_knob(id)?;
                Some((block, k.bypassed))
            })
            .collect();
        out.sort();
        out
    }

    /// A direct edit: the player set `(block, param)` to `live` on the
    /// block itself. The value stays as dialled; the baseline becomes
    /// whatever sits under it at the macros' current offsets. Returns that
    /// baseline — what the patch override records.
    pub fn direct_edit(&mut self, block: &str, param: &str, live: f32) -> f32 {
        let layers = self.layers(block, param);
        let old = self.base(block, param);
        let mut v = live;
        for (t, m, entry) in layers.iter().rev().filter(|(_, m, _)| *m != 0.0) {
            let inverse = if entry.is_some() && *m > 0.0 {
                None
            } else {
                t.invert(v, *m)
            };
            match inverse {
                Some(b) => v = b,
                // Pinned to an end: nothing under it can be told apart, so
                // the baseline stays.
                None => {
                    v = old.unwrap_or(live);
                    break;
                }
            }
        }
        self.baseline
            .insert((block.to_string(), param.to_string()), v);
        // A choice made on the block moves its selector's rest with it.
        let moved: Vec<(String, f32)> = self
            .built
            .meta
            .iter()
            .filter_map(|(id, m)| {
                let s = m.select.as_ref()?;
                (s.block == block && s.param == param).then(|| (id.clone(), select_position(s, v)))
            })
            .collect();
        for (id, pos) in moved {
            if let Some(m) = self.built.meta.get_mut(&id) {
                m.rest = pos;
            }
            if let Some(k) = self.built.bank.get_knob_mut(&id) {
                k.set_value(pos);
            }
        }
        v
    }

    /// A direct bypass toggle on a block. For a drive stage it is the
    /// patch's own state from now on, and it holds until the Drive knob
    /// next moves (as a pad press does).
    pub fn direct_bypass(&mut self, block: &str, bypassed: bool) {
        if !self.base_bypass.contains_key(block) {
            return;
        }
        self.base_bypass.insert(block.to_string(), bypassed);
        self.rebuild_rules();
        let child = self
            .built
            .meta
            .iter()
            .find(|(_, m)| m.pad_block.as_deref() == Some(block))
            .map(|(id, _)| id.clone());
        let Some(id) = child else { return };
        self.pads.remove(&id);
        self.apply_pads();
        // Only where the knob, off rest, would say otherwise: at rest the
        // patch's own state is what plays, and no pad needs keeping.
        if self
            .built
            .bank
            .get_knob(&id)
            .is_some_and(|k| k.bypassed != bypassed)
        {
            self.pads.insert(id, !bypassed);
            self.apply_pads();
        }
    }

    /// The positions worth keeping with the patch: every knob away from
    /// where its preset snapshot puts it (rest, when it says nothing) — a
    /// knob turned back to rest over a snapshot that turns it is kept too —
    /// and every pad pressed.
    #[must_use]
    pub fn saved(&self) -> Vec<MacroValueDef> {
        let mut out = Vec::new();
        let offsets = self.offsets();
        let ids: Vec<String> = self
            .built
            .bank
            .knobs
            .iter()
            .flat_map(|p| {
                std::iter::once(p.id.clone()).chain(p.children.iter().map(|c| c.id.clone()))
            })
            .collect();
        for id in ids {
            let own = offsets.iter().find(|(k, _, _)| *k == id);
            let pad = match self.pads.get(&id) {
                Some(true) => "on",
                Some(false) => "off",
                None => "",
            };
            let d = self.defaults.iter().find(|d| d.id == id).map(|d| d.value);
            let m = own.map(|(_, m, _)| *m);
            let keep = match (m, d) {
                (Some(m), Some(d)) => (m - d).abs() > 1e-4,
                // Where its bar knob puts it, but the snapshot moved it.
                (None, Some(_)) => true,
                (Some(m), None) => m.abs() > 1e-4 || own.is_some_and(|(_, _, bar)| !*bar),
                (None, None) => false,
            };
            if keep || !pad.is_empty() {
                let value = m.unwrap_or_else(|| self.offset(&id));
                out.push(MacroValueDef {
                    id,
                    value,
                    pad: pad.to_string(),
                });
            }
        }
        out
    }

    /// Every target bar knob `parent` and its panel move, as tune mode
    /// draws it — grouped by block, in panel order.
    fn tune_views(
        &self,
        parent: &MacroKnob,
        blocks: &[LiveBlock],
    ) -> Vec<crate::proto::MacroTuneView> {
        let stages = self.journeys.get(&parent.id);
        let mut out = Vec::new();
        let kids: Vec<&MacroKnob> = std::iter::once(parent)
            .chain(parent.children.iter())
            .collect();
        for (i, k) in kids.iter().enumerate() {
            let Some(meta) = self.built.meta.get(&k.id).filter(|m| m.select.is_none()) else {
                continue;
            };
            let stage = stages.and_then(|s| i.checked_sub(1).and_then(|j| s.get(j)));
            let off_stage = stage.is_some()
                && meta
                    .pad_block
                    .as_ref()
                    .is_some_and(|b| self.base_bypass.get(b).copied().unwrap_or(false));
            let many = meta.targets.len() > 1;
            for t in &meta.targets {
                let Some(base) = self.base(&t.block, &t.param) else {
                    continue;
                };
                let eff = t.effective(base);
                let entry = eff.as_ref().filter(|_| off_stage).and_then(|r| r.min);
                let group = self.names.get(&t.block).cloned().unwrap_or_default();
                let label = if std::ptr::eq(*k, parent) || many {
                    param_label(&t.param).to_string()
                } else if let Some(p) = stage.and_then(|_| stage_pedal(meta, blocks)) {
                    p.label
                } else {
                    k.label.clone()
                };
                let fmt = if meta.fmt.is_empty() || std::ptr::eq(*k, parent) {
                    param_fmt(&t.param)
                } else {
                    meta.fmt
                };
                let live = blocks
                    .iter()
                    .find(|b| b.id == t.block)
                    .and_then(|b| param(b, &t.param))
                    .map_or(base, |p| p.value);
                let aux = blocks
                    .iter()
                    .find(|b| b.id == t.block)
                    .map_or(0.0, |b| value(b, "algorithm", 0.0));
                let edit = t.edit.clone().unwrap_or_default();
                out.push(crate::proto::MacroTuneView {
                    knob: k.id.clone(),
                    block: t.block.clone(),
                    group,
                    param: t.param.clone(),
                    label,
                    color: k.color.clone().unwrap_or_default(),
                    fmt: fmt.to_string(),
                    aux,
                    live,
                    base,
                    lo: t.apply_from(base, -1.0, None),
                    hi: t.apply_from(base, 1.0, entry),
                    min: t.lo,
                    max: t.hi,
                    curve: eff
                        .as_ref()
                        .map_or(if t.curve == Curve::Log { "log" } else { "lin" }, |r| {
                            r.shape.name()
                        })
                        .to_string(),
                    source: eff.as_ref().map_or("", |r| r.source).to_string(),
                    inherited: t.resp.as_ref().map_or("", |r| r.source).to_string(),
                    enter: stage.map_or(-1.0, |s| s.enter),
                    log: t.curve == Curve::Log
                        || eff.as_ref().is_some_and(|r| r.shape == Shape::Log),
                    off: eff.as_ref().is_some_and(|r| r.off),
                    min_set: edit.min.is_some(),
                    max_set: edit.max.is_some(),
                    edited: t.edit.is_some(),
                });
            }
        }
        out
    }

    /// Every block the knob turns — its own targets and its children's —
    /// is bypassed (and it turns some): a level knob reads `off`.
    fn knob_off(&self, k: &MacroKnob, blocks: &[LiveBlock]) -> bool {
        let ids: Vec<&str> = std::iter::once(&k.id)
            .chain(k.children.iter().map(|c| &c.id))
            .filter_map(|id| self.built.meta.get(id))
            .flat_map(|m| {
                m.targets
                    .iter()
                    .map(|t| t.block.as_str())
                    .chain(m.select.iter().map(|s| s.block.as_str()))
                    .chain(m.show.iter().map(|s| s.0.as_str()))
                    .chain(m.pad_block.as_deref())
            })
            .collect();
        !ids.is_empty()
            && ids.iter().all(|id| {
                blocks
                    .iter()
                    .find(|b| b.id == *id)
                    .is_some_and(|b| b.bypassed)
            })
    }

    /// The bar as the UI draws it; `blocks` is the live chain (values with
    /// the macros applied) for the readouts.
    #[must_use]
    pub fn views(&self, blocks: &[LiveBlock]) -> Vec<MacroKnobView> {
        let live = |at: &Option<(String, String)>| -> f32 {
            at.as_ref()
                .and_then(|(b, p)| {
                    blocks
                        .iter()
                        .find(|x| x.id == *b)
                        .and_then(|x| param(x, p))
                        .map(|p| p.value)
                })
                .unwrap_or(0.0)
        };
        self.built
            .bank
            .knobs
            .iter()
            .map(|k| {
                let panel = self.built.panels.get(&k.id).cloned().unwrap_or_default();
                let rest = self.built.meta.get(&k.id).map_or(0.5, |m| m.rest);
                let readout = if panel.style == "spread" {
                    if k.value < 0.005 {
                        "Mono".to_string()
                    } else {
                        format!("{:.0}%", k.value * 100.0)
                    }
                } else {
                    k.format_value()
                };
                let tune = self.tune_views(k, blocks);
                MacroKnobView {
                    id: k.id.clone(),
                    label: k.label.clone(),
                    color: k.color.clone().unwrap_or_default(),
                    value: k.value,
                    rest,
                    bipolar: k.bipolar,
                    style: panel.style.to_string(),
                    readout,
                    layout: panel.layout.to_string(),
                    headers: panel.headers.clone(),
                    anchor: panel.anchor.clone(),
                    tuned: self.has_edits(&k.id),
                    snapshot: self.snapshot.clone(),
                    scale: knob_scale(&k.id).to_string(),
                    off: self.knob_off(k, blocks),
                    tune,
                    children: k
                        .children
                        .iter()
                        .map(|c| {
                            let meta = self.built.meta.get(&c.id).cloned().unwrap_or_default();
                            let pedal = self
                                .journeys
                                .contains_key(&k.id)
                                .then(|| stage_pedal(&meta, blocks))
                                .flatten();
                            MacroChildView {
                                id: c.id.clone(),
                                label: pedal
                                    .as_ref()
                                    .map_or_else(|| c.label.clone(), |p| p.label.clone()),
                                slot: pedal.as_ref().map(|p| p.slot.clone()).unwrap_or_default(),
                                subtitle: pedal
                                    .as_ref()
                                    .map(|p| p.subtitle.clone())
                                    .unwrap_or_default(),
                                tooltip: pedal
                                    .as_ref()
                                    .map(|p| p.tooltip.clone())
                                    .unwrap_or_default(),
                                empty: pedal.as_ref().is_some_and(|p| p.empty),
                                color: c.color.clone().unwrap_or_default(),
                                value: c.value,
                                rest: meta.rest,
                                group: meta.group.clone(),
                                has_pad: meta.pad_block.is_some(),
                                bypassed: c.bypassed,
                                fmt: meta.fmt.to_string(),
                                param: live(&meta.show),
                                aux: live(&meta.aux),
                                steps: meta.select.as_ref().map_or(0, |s| s.choices.len() as u32),
                            }
                        })
                        .collect(),
                }
            })
            .collect()
    }
}

/// A selector's knob position for the choice `v`.
fn select_position(s: &Select, v: f32) -> f32 {
    let n = s.choices.len();
    if n < 2 {
        return 0.0;
    }
    let i = s
        .choices
        .iter()
        .enumerate()
        .min_by(|a, b| {
            (a.1 - v)
                .abs()
                .partial_cmp(&(b.1 - v).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map_or(0, |(i, _)| i);
    i as f32 / (n - 1) as f32
}

/// The choice a selector's knob position lands on.
fn select_value(s: &Select, pos: f32) -> f32 {
    let n = s.choices.len();
    if n == 0 {
        return 0.0;
    }
    let i = (pos.clamp(0.0, 1.0) * (n - 1) as f32).round() as usize;
    s.choices[i.min(n - 1)]
}

/// A drive stage as the panel names it: the pedal in the slot.
struct StagePedal {
    label: String,
    slot: String,
    subtitle: String,
    tooltip: String,
    empty: bool,
}

/// The pedal a drive stage's slot plays, from the live chain (the rig puts
/// the pedal, its capture and the file on the slot's block).
fn stage_pedal(meta: &Meta, blocks: &[LiveBlock]) -> Option<StagePedal> {
    let b = blocks
        .iter()
        .find(|b| Some(&b.id) == meta.pad_block.as_ref())?;
    let empty = b.empty || b.preset.is_empty();
    Some(StagePedal {
        label: if empty {
            "Empty".to_string()
        } else {
            b.preset.clone()
        },
        slot: b.name.clone(),
        subtitle: if empty {
            String::new()
        } else {
            b.detail.clone()
        },
        tooltip: b.asset.clone(),
        empty,
    })
}

// ── The presets' responses and positions for a patch ──────────────────────

/// Every response the presets give `patch`'s blocks, most important first:
/// its module snapshots' (by block), then its block presets' own, then the
/// seeds for block presets that say nothing ([`seed_responses`]). The
/// engine takes the first that fits each knob and param.
#[must_use]
pub fn responses_for(comp: &Compositions, patch: &PatchDef) -> Vec<Resolved> {
    let mut out = resolve_all(comp, patch);
    let before = out.len();
    out.retain(|r| !is_timing(&r.def.param));
    if out.len() < before {
        tracing::warn!(
            patch = %patch.name,
            ignored = before - out.len(),
            "macro responses on timing params (time, divisions, pre-delay) ignored: macros never move timing"
        );
    }
    out
}

fn resolve_all(comp: &Compositions, patch: &PatchDef) -> Vec<Resolved> {
    let mut out = Vec::new();
    for pick in crate::compose::module_picks(comp, patch) {
        let Some(snap) = comp.module(&pick.module, &pick.preset).and_then(|m| {
            m.snapshots
                .iter()
                .find(|s| !pick.snapshot.is_empty() && s.name.eq_ignore_ascii_case(&pick.snapshot))
                .or_else(|| m.snapshots.first())
        }) else {
            continue;
        };
        for d in &snap.macros {
            out.push(Resolved {
                block: d.block.clone(),
                def: d.clone(),
                source: "module",
            });
        }
    }
    let picks = crate::compose::block_picks(comp, patch);
    for c in &picks {
        if let Some(p) = comp.block_preset(&c.preset) {
            for d in &p.macros {
                out.push(Resolved {
                    block: c.block.clone(),
                    def: d.clone(),
                    source: "block",
                });
            }
        }
    }
    for c in &picks {
        if let Some(p) = comp.block_preset(&c.preset).filter(|p| p.macros.is_empty()) {
            for d in seed_responses(p) {
                out.push(Resolved {
                    block: c.block.clone(),
                    def: d,
                    source: "seed",
                });
            }
        }
    }
    out
}

/// What a save into the presets did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveReport {
    /// Where entries went: block presets and module snapshots, by name.
    pub into: Vec<String>,
    /// Blocks with nothing in that scope to keep their entries: no block
    /// preset on them, or no module snapshot playing their module.
    pub missing: Vec<String>,
}

/// Put `d` into `list`, replacing the entry for the same block, knob and
/// param.
fn put_entry(list: &mut Vec<MacroResponseDef>, d: MacroResponseDef) {
    match list
        .iter_mut()
        .find(|x| x.block.eq_ignore_ascii_case(&d.block) && x.knob == d.knob && x.param == d.param)
    {
        Some(x) => *x = d,
        None => list.push(d),
    }
}

/// The module `block` belongs to, by its type — the most specific one
/// (Delay before Time).
#[must_use]
pub fn module_of(
    comp: &Compositions,
    block: &str,
    chain: &[(String, BlockType)],
) -> Option<String> {
    crate::compose::MODULES.iter().rev().find_map(|m| {
        let bare = crate::profiles::ModuleChoiceDef {
            module: (*m).to_string(),
            preset: String::new(),
            snapshot: String::new(),
        };
        crate::manage::owned_blocks(comp, &bare, chain)
            .iter()
            .any(|b| b.eq_ignore_ascii_case(block))
            .then(|| (*m).to_string())
    })
}

/// The module snapshot `patch` plays that owns `block` (the most specific:
/// Delay over Time), as its pick.
fn owner_pick(
    comp: &Compositions,
    patch: &PatchDef,
    block: &str,
    chain: &[(String, BlockType)],
) -> Option<crate::profiles::ModuleChoiceDef> {
    crate::compose::module_picks(comp, patch)
        .into_iter()
        .rev()
        .find(|p| {
            comp.module(&p.module, &p.preset).is_some()
                && crate::manage::owned_blocks(comp, p, chain)
                    .iter()
                    .any(|b| b.eq_ignore_ascii_case(block))
        })
}

fn snapshot_mut<'a>(
    comp: &'a mut Compositions,
    pick: &crate::profiles::ModuleChoiceDef,
) -> Option<&'a mut crate::compose::ModuleSnapshotDef> {
    comp.modules
        .iter_mut()
        .find(|m| {
            m.module.eq_ignore_ascii_case(&pick.module) && m.name.eq_ignore_ascii_case(&pick.preset)
        })
        .and_then(|m| {
            let i = m
                .snapshots
                .iter()
                .position(|s| {
                    !pick.snapshot.is_empty() && s.name.eq_ignore_ascii_case(&pick.snapshot)
                })
                .unwrap_or(0);
            m.snapshots.get_mut(i)
        })
}

/// Save tuned entries ([`MacroEngine::tuned_defs`]) into the presets
/// `patch` plays: `scope` `module` — the module snapshot that owns each
/// block — or `block` — the block preset on it. A block preset that played
/// on seeds keeps them, written out, beside the new entry. Blocks with no
/// home in that scope are reported, not skipped silently.
pub fn save_tuning(
    comp: &mut Compositions,
    patch: &PatchDef,
    chain: &[(String, BlockType)],
    defs: &[Tuned],
    scope: &str,
) -> SaveReport {
    let mut report = SaveReport::default();
    let into = |name: String, r: &mut SaveReport| {
        if !r.into.contains(&name) {
            r.into.push(name);
        }
    };
    for t in defs {
        if scope == "module" {
            let Some(pick) = owner_pick(comp, patch, &t.block, chain) else {
                if !report.missing.contains(&t.block) {
                    report.missing.push(t.block.clone());
                }
                continue;
            };
            let Some(snap) = snapshot_mut(comp, &pick) else {
                continue;
            };
            let name = format!("{} · {}", pick.preset, snap.name);
            put_entry(
                &mut snap.macros,
                MacroResponseDef {
                    block: t.block.clone(),
                    ..t.def.clone()
                },
            );
            into(name, &mut report);
        } else {
            let picks = crate::compose::block_picks(comp, patch);
            let Some(bp) = picks
                .iter()
                .find(|c| c.block.eq_ignore_ascii_case(&t.block))
                .and_then(|c| {
                    comp.blocks
                        .iter_mut()
                        .find(|b| b.name.eq_ignore_ascii_case(&c.preset))
                })
            else {
                if !report.missing.contains(&t.block) {
                    report.missing.push(t.block.clone());
                }
                continue;
            };
            if bp.macros.is_empty() {
                bp.macros = seed_responses(bp);
            }
            put_entry(
                &mut bp.macros,
                MacroResponseDef {
                    block: String::new(),
                    ..t.def.clone()
                },
            );
            let name = bp.name.clone();
            into(name, &mut report);
        }
    }
    report
}

/// Make a home in `scope` for each block in `missing` before a save: a new
/// block preset named `name` from the block as the patch has it (`homes`:
/// `(block, type, params, bypassed)`), put on the patch — or a new snapshot
/// `name` of the block's module (in the module preset the patch plays, or a
/// new preset `name`), made from the module as the patch has it and played
/// by the patch.
///
/// # Errors
///
/// A name that is taken, or a block no module holds.
pub fn make_homes(
    comp: &mut Compositions,
    patch: &mut PatchDef,
    chain: &[(String, BlockType)],
    homes: &[(String, BlockType, Vec<(String, f32)>, bool)],
    missing: &[String],
    scope: &str,
    name: &str,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("a blank name".to_string());
    }
    if scope == "module" {
        let mut modules: Vec<String> = Vec::new();
        for b in missing {
            let m = module_of(comp, b, chain)
                .ok_or_else(|| format!("{b} belongs to no module a snapshot can hold"))?;
            if !modules.contains(&m) {
                modules.push(m);
            }
        }
        for module in modules {
            let picks = crate::compose::module_picks(comp, patch);
            let current = picks
                .iter()
                .find(|p| {
                    p.module.eq_ignore_ascii_case(&module)
                        && comp.module(&p.module, &p.preset).is_some()
                })
                .cloned();
            let preset = current
                .as_ref()
                .map_or_else(|| name.to_string(), |c| c.preset.clone());
            if comp.module(&module, &preset).is_some_and(|m| {
                m.snapshots
                    .iter()
                    .any(|s| s.name.eq_ignore_ascii_case(name))
            }) {
                return Err(format!(
                    "{module} · {preset} already has a snapshot named {name}"
                ));
            }
            let bare = crate::profiles::ModuleChoiceDef {
                module: module.clone(),
                preset: String::new(),
                snapshot: String::new(),
            };
            let owned = crate::manage::owned_blocks(comp, current.as_ref().unwrap_or(&bare), chain);
            let live = crate::manage::LiveModule {
                current: current.as_ref(),
                owned: &owned,
                picks: &picks,
            };
            crate::manage::save_module_snapshot(comp, patch, &live, &module, &preset, name)?;
        }
    } else {
        let several = missing.len() > 1;
        for b in missing {
            let Some((block, bt, params, bypassed)) =
                homes.iter().find(|h| h.0.eq_ignore_ascii_case(b))
            else {
                return Err(format!("{b} is not on the live chain"));
            };
            let preset = if several {
                format!("{name} {block}")
            } else {
                name.to_string()
            };
            if comp.block_preset(&preset).is_some() {
                return Err(format!("A block preset named {preset} already exists"));
            }
            let state = crate::manage::LiveBlockState {
                name: block,
                block_type: bt.as_str(),
                params: params.clone(),
                bypassed: *bypassed,
                current: None,
            };
            crate::manage::save_block_preset(comp, patch, &state, &preset)?;
        }
    }
    Ok(())
}

/// Clear what a scope says about `targets` (`(block, bar knob, param)`,
/// [`MacroEngine::panel_targets`]) — a panel's Reset: the block presets on
/// those blocks, or the module snapshots that own them, forget their
/// entries for these knobs, so the next layer down plays. Returns where
/// entries were cleared, and how many.
pub fn reset_scope(
    comp: &mut Compositions,
    patch: &PatchDef,
    chain: &[(String, BlockType)],
    targets: &[(String, String, String)],
    scope: &str,
) -> (Vec<String>, usize) {
    let mut names: Vec<String> = Vec::new();
    let mut n = 0;
    let hit = |d: &MacroResponseDef, block: &str, knob: &str, param: &str, keyed: bool| {
        (!keyed || d.block.eq_ignore_ascii_case(block))
            && (d.knob == knob || d.knob.starts_with(&format!("{knob}-")))
            && d.param == param
    };
    for (block, knob, param) in targets {
        if scope == "module" {
            let Some(pick) = owner_pick(comp, patch, block, chain) else {
                continue;
            };
            let Some(snap) = snapshot_mut(comp, &pick) else {
                continue;
            };
            let before = snap.macros.len();
            snap.macros.retain(|d| !hit(d, block, knob, param, true));
            if snap.macros.len() < before {
                n += before - snap.macros.len();
                let name = format!("{} · {}", pick.preset, snap.name);
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        } else {
            let picks = crate::compose::block_picks(comp, patch);
            let Some(bp) = picks
                .iter()
                .find(|c| c.block.eq_ignore_ascii_case(block))
                .and_then(|c| {
                    comp.blocks
                        .iter_mut()
                        .find(|b| b.name.eq_ignore_ascii_case(&c.preset))
                })
            else {
                continue;
            };
            let before = bp.macros.len();
            bp.macros.retain(|d| !hit(d, block, knob, param, false));
            if bp.macros.len() < before {
                n += before - bp.macros.len();
                if !names.contains(&bp.name) {
                    names.push(bp.name.clone());
                }
            }
        }
    }
    (names, n)
}

/// A param as a tune row names it.
#[must_use]
pub fn param_label(param: &str) -> &str {
    match param {
        "level" => "Level",
        "feedback" => "Feedback",
        "decay" => "Decay",
        "pan" | "pan_a" => "Pan",
        "width" => "Width",
        "depth" => "Depth",
        "mix" => "Mix",
        "gain_db" => "Gain",
        "drive" => "Drive",
        "high_cut" => "Hi Cut",
        "low_cut" | "high_pass" => "Lo Cut",
        "time" => "Time",
        "duck" | "duck_sens" => "Duck",
        other => other,
    }
}

/// How a param prints when a knob moves it directly (Space's level, Width's
/// pan).
#[must_use]
pub fn param_fmt(param: &str) -> &'static str {
    match param {
        "level" => "db",
        "gain_db" => "db_gain",
        "high_cut" | "low_cut" | "high_pass" => "hz",
        "time" | "predelay" => "ms",
        "decay" => "verb_s",
        "pan" | "pan_a" => "pan",
        _ => "pct",
    }
}

/// The knob positions `patch`'s preset snapshot carries.
#[must_use]
pub fn preset_positions(comp: &Compositions, patch: &PatchDef) -> Vec<MacroValueDef> {
    comp.preset(&patch.rig_preset)
        .and_then(|p| {
            p.snapshots
                .iter()
                .find(|s| {
                    !patch.snapshot.is_empty() && s.name.eq_ignore_ascii_case(&patch.snapshot)
                })
                .or_else(|| p.snapshots.first())
        })
        .map(|s| s.macros.clone())
        .unwrap_or_default()
}

/// Everything `patch` brings to the bar: its positions over its preset
/// snapshot's, and its presets' responses.
#[must_use]
pub fn context_for(comp: &Compositions, patch: &PatchDef) -> Context {
    let snapshot = comp
        .preset(&patch.rig_preset)
        .and_then(|p| {
            p.snapshots
                .iter()
                .find(|s| {
                    !patch.snapshot.is_empty() && s.name.eq_ignore_ascii_case(&patch.snapshot)
                })
                .or_else(|| p.snapshots.first())
                .map(|s| format!("{} · {}", p.name, s.name))
        })
        .unwrap_or_default();
    Context {
        saved: patch.macros.clone(),
        defaults: preset_positions(comp, patch),
        responses: responses_for(comp, patch),
        snapshot,
    }
}

fn r(knob: &str, param: &str, min: f32, max: f32, curve: &str) -> MacroResponseDef {
    MacroResponseDef {
        knob: knob.into(),
        param: param.into(),
        min: Some(min),
        max: Some(max),
        curve: curve.into(),
        ..MacroResponseDef::default()
    }
}

fn off(knob: &str, param: &str) -> MacroResponseDef {
    MacroResponseDef {
        knob: knob.into(),
        param: param.into(),
        off: true,
        ..MacroResponseDef::default()
    }
}

/// Musical defaults for a block preset that tunes nothing itself, by its
/// character — the shipped seeds:
///
/// - a **slapback** (short and dry, or named so): feedback 0–0.25, level
///   −18..−8 dB, and Space barely moves it (±2 dB, no feedback);
/// - an **ambient** delay (long, or regenerating, or named for it):
///   feedback 0.3–0.75, level −20..−6 dB, and Clarity ducks it up to 10 dB;
/// - a **room**: its decay stays short (0.8×–1.2×), Space leaves it be;
/// - a **hall**, cloud, bloom, shimmer or other ambient reverb: decay
///   0.5×–2× by ratio, on the Reverb knob and on Space, and Clarity cuts its
///   lows up to 400 Hz.
///
/// Each range contains the preset's own value, so rest is always the preset.
#[must_use]
pub fn seed_responses(p: &crate::compose::BlockPresetDef) -> Vec<MacroResponseDef> {
    let v = |name: &str, dflt: f32| {
        p.params
            .iter()
            .find(|x| x.param == name)
            .map_or(dflt, |x| x.value)
    };
    let name = p.name.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    // Keep a range around the preset's own value.
    let around = |x: f32, lo: f32, hi: f32| (lo.min(x), hi.max(x));
    let mut out = Vec::new();
    match p.block_type.as_str() {
        "delay" => {
            let (fb, level, time) = (v("feedback", 0.3), v("level", -16.0), v("time", 350.0));
            // The delay menu's Free (7): the time knob, not a division, sets
            // the time.
            let synced = v("tap_div_l", 7.0).round() != 7.0;
            let slap = has(&["slap"]) || (!synced && time < 180.0 && fb <= 0.3);
            let ambient = !slap
                && (has(&[
                    "ambient", "wash", "swell", "bloom", "shimmer", "pad", "flute",
                ]) || fb >= 0.5
                    || time >= 550.0);
            if slap {
                let (a, b) = around(fb, 0.0, 0.25);
                out.push(r("delay", "feedback", a, b, "lin"));
                let (a, b) = around(level, -18.0, -8.0);
                out.push(r("delay", "level", a, b, "lin"));
                out.push(off("space", "feedback"));
                out.push(r("space", "level", level - 2.0, level + 2.0, "lin"));
            } else if ambient {
                let (a, b) = around(fb, 0.3, 0.75);
                out.push(r("delay", "feedback", a, b, "s"));
                let (a, b) = around(level, -20.0, -6.0);
                out.push(r("delay", "level", a, b, "lin"));
                let (a, b) = around(v("duck_sens", 0.0), 0.0, 10.0);
                out.push(r("clarity", "duck_sens", a, b, "lin"));
            }
        }
        "reverb" => {
            let (decay, level) = (v("decay", 0.4), v("level", -16.0));
            let alg = v("algorithm", 1.0).round() as i32;
            let room = alg == 0 || has(&["room", "tight", "small"]);
            let big = !room
                && (matches!(alg, 1 | 4 | 5 | 6 | 7 | 12)
                    || has(&[
                        "hall",
                        "ambient",
                        "cathedral",
                        "wash",
                        "swell",
                        "arena",
                        "bloom",
                        "shimmer",
                    ]));
            if room {
                out.push(r(
                    "reverb",
                    "decay",
                    decay * 0.8,
                    (decay * 1.2).min(1.0),
                    "lin",
                ));
                out.push(off("space", "decay"));
            } else if big && decay > 0.0 {
                let (a, b) = (decay * 0.5, (decay * 2.0).min(1.0));
                out.push(r("reverb", "decay", a, b, "log"));
                out.push(r("space", "decay", a, b, "log"));
                let (a, b) = around(level, level - 8.0, level + 6.0);
                out.push(r("reverb", "level", a, b.min(12.0), "lin"));
                let lc = v("low_cut", 20.0);
                let (a, b) = around(lc, 20.0, 400.0);
                out.push(r("clarity", "low_cut", a, b, "log"));
            }
        }
        _ => {}
    }
    out
}

/// Put `patch`'s stored knob positions on its built chain, as the live rig
/// does — what levelling measures, so a preset's knob positions are part of
/// the loudness it is levelled to.
#[cfg(not(target_arch = "wasm32"))]
pub fn apply_positions(def: &PatchDef, comp: &Compositions, patch: &mut sampler::RigPatch) {
    apply_ctx(context_for(comp, def), def, patch);
}

/// Knobs levelling leaves at rest: Output is the player's own per-patch
/// level offset on top of the levelled patch — measuring it in would have
/// the next levelling pass cancel it.
pub const LEVEL_EXEMPT: &[&str] = &["output"];

/// [`apply_positions`] for a loudness measurement: every knob where the
/// patch keeps it except the [`LEVEL_EXEMPT`] ones, which stay at rest.
#[cfg(not(target_arch = "wasm32"))]
pub fn apply_positions_for_level(
    def: &PatchDef,
    comp: &Compositions,
    patch: &mut sampler::RigPatch,
) {
    let mut ctx = context_for(comp, def);
    ctx.saved
        .retain(|m| !LEVEL_EXEMPT.iter().any(|k| m.id.eq_ignore_ascii_case(k)));
    ctx.defaults
        .retain(|m| !LEVEL_EXEMPT.iter().any(|k| m.id.eq_ignore_ascii_case(k)));
    apply_ctx(ctx, def, patch);
}

#[cfg(not(target_arch = "wasm32"))]
fn apply_ctx(ctx: Context, def: &PatchDef, patch: &mut sampler::RigPatch) {
    if ctx.saved.is_empty() && ctx.defaults.is_empty() {
        return;
    }
    let blocks = crate::session::chain_as_live(&patch.chain);
    let mut e = MacroEngine::default();
    e.rebase(&def.name, &blocks, &ctx);
    let writes = e.live_params();
    let bypass = e.live_bypass();
    let index = |id: &str| {
        id.strip_prefix("chain-")
            .and_then(|i| i.parse::<usize>().ok())
    };
    for (id, param, v) in writes {
        let Some(b) = index(&id).and_then(|i| patch.chain.get_mut(i)) else {
            continue;
        };
        match b.params.iter_mut().find(|p| p.name == param) {
            Some(p) => p.value = format!("{v}"),
            None => b.params.push(sampler::rig_node::Param {
                name: param,
                value: format!("{v}"),
            }),
        }
    }
    for (id, byp) in bypass {
        if let Some(b) = index(&id).and_then(|i| patch.chain.get_mut(i)) {
            b.bypassed = byp;
        }
    }
}
