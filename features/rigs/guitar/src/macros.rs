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

use signal_guitar_proto::{BlockParam, LiveBlock, MacroChildView, MacroKnobView};
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
    /// Steps along the synced tempo divisions, shortest to longest.
    Div,
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
            self.max.map_or(base, |to| self.shape.between(entry.unwrap_or(base), to, m))
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
}

/// Synced divisions, shortest first — indices into the delay's
/// `TapDivision` menu (1/16, 1/8T, Silver, 1/8, Golden, 1/4T, 1/8., 1/4,
/// 1/4., 1/2). Free (7) is not a division.
const DIV_ORDER: [usize; 10] = [4, 3, 6, 2, 5, 10, 1, 0, 8, 9];
/// The delay menu's Free: the time knob, not a division, sets the time.
const DIV_FREE: f32 = 7.0;
/// How many divisions a Time knob all the way up (or down) moves.
const DIV_STEPS: f32 = 3.0;

impl Target {
    fn effect(&self, m: f32) -> f32 {
        (m * self.dir * self.depth).clamp(-1.0, 1.0)
    }

    fn log_floor(&self) -> f32 {
        if self.lo > 0.0 { self.lo } else { (self.hi / 1000.0).max(1e-3) }
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
                if self.hi > self.lo { ((v - self.lo) / (self.hi - self.lo)).clamp(0.0, 1.0) } else { 0.0 }
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
                if t <= 0.0 { self.lo } else { f * (self.hi / f).powf(t) }
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
        if let Some(r) = &self.resp {
            return r.apply(base, m, entry).clamp(self.lo.min(self.hi), self.hi.max(self.lo));
        }
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
            Curve::Div => div_shift(base, (e * DIV_STEPS).round()),
        }
    }

    /// The baseline under a live value `live` with the knob at offset `m` —
    /// `apply`'s inverse. `None` where there is none: a knob all the way to
    /// one end pins every baseline to that end, so the baseline is kept.
    #[must_use]
    pub fn invert(&self, live: f32, m: f32) -> Option<f32> {
        if let Some(r) = &self.resp {
            return r.invert(live, m).map(|v| v.clamp(self.lo.min(self.hi), self.hi.max(self.lo)));
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
            Curve::Div => Some(div_shift(live, -(e * DIV_STEPS).round())),
        }
    }
}

/// `t` moved toward 1 (e > 0) or 0 (e < 0) by the fraction `|e|` of the way.
fn toward(t: f32, e: f32) -> f32 {
    if e >= 0.0 { (1.0 - t).mul_add(e, t) } else { t.mul_add(e, t) }
}

/// [`toward`]'s inverse.
fn away(t: f32, e: f32) -> Option<f32> {
    if e >= 0.0 {
        (e < 0.999).then(|| ((t - e) / (1.0 - e)).clamp(0.0, 1.0))
    } else {
        (e > -0.999).then(|| (t / (1.0 + e)).clamp(0.0, 1.0))
    }
}

/// A division index moved `steps` along [`DIV_ORDER`]; Free stays Free.
fn div_shift(index: f32, steps: f32) -> f32 {
    let Some(pos) = DIV_ORDER.iter().position(|&d| d as f32 == index.round()) else {
        return index;
    };
    let to = (pos as f32 + steps).clamp(0.0, (DIV_ORDER.len() - 1) as f32) as usize;
    DIV_ORDER[to] as f32
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
    if m >= 0.0 { (1.0 - rest).mul_add(m, rest) } else { rest.mul_add(m, rest) }
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
                Stage { on: true, enter, drop }
            } else {
                Stage { on: false, enter, drop: 0.0 }
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
            let lo = if s.on { 0.5 - s.drop / 2.0 } else { 0.5 + s.enter / 2.0 } + 1e-4;
            BypassRule { child_id: id.clone(), active_ranges: vec![(lo, 1.01)] }
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
        Self { knob: knob(id, label, color), meta, range }
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
fn select_child(b: &LiveBlock, id: &str, label: &str, color: &str, pname: &str, fmt: &'static str) -> Option<Child> {
    let p = param(b, pname)?;
    let choices: Vec<f32> = (p.min.round() as i32..=p.max.round() as i32).map(|i| i as f32).collect();
    Some(Child::new(
        id,
        label,
        color,
        Meta {
            select: Some(Select { block: b.id.clone(), param: pname.to_string(), choices }),
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
        blocks.iter().find(|b| b.block_type == bt && b.name.eq_ignore_ascii_case(name))
    };
    let post = |pred: &dyn Fn(&LiveBlock) -> bool| -> Vec<&LiveBlock> {
        blocks.iter().filter(|b| !is_pre_fx(b) && pred(b)).collect()
    };
    let delays = post(&|b| b.block_type == BlockType::Delay);
    let verbs = post(&|b| b.block_type == BlockType::Reverb);
    let delays: Vec<&LiveBlock> = delays.into_iter().take(2).collect();
    let verbs: Vec<&LiveBlock> = verbs.into_iter().take(2).collect();

    // A knob with its own targets and no panel.
    let add_single = |out: &mut Built, id: &str, label: &str, color: &str, targets: Vec<Target>, rest: f32| {
        if targets.is_empty() {
            return;
        }
        let mut k = knob(id, label, color);
        k.set_value(rest);
        out.bank.knobs.push(k);
        out.meta.insert(id.to_string(), Meta { targets, rest, ..Meta::default() });
    };
    // A bar knob with a panel of children.
    let add_parent = |out: &mut Built, id: &str, label: &str, color: &str, panel: Panel, children: Vec<Child>| {
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
        out.meta.insert(id.to_string(), Meta { rest: 0.5, ..Meta::default() });
        out.panels.insert(id.to_string(), panel);
        out.bank.knobs.push(parent);
    };

    // ── Input: an input trim, where the chain has one ──
    if let Some(b) = blocks.iter().find(|b| {
        b.block_type == BlockType::Volume
            && (b.name.eq_ignore_ascii_case("Input") || b.name.eq_ignore_ascii_case("Input Trim"))
    }) {
        let t: Vec<Target> = target(b, "gain_db", Curve::Add(12.0), 1.0, 1.0).into_iter().collect();
        add_single(&mut out, "input", "Input", "#6B7280", t, 0.5);
    }

    // ── Gate ──
    if let Some(b) = blocks.iter().find(|b| b.block_type == BlockType::Gate) {
        let spec: [(&str, &str, &str, &str, Curve, (f32, f32)); 5] = [
            ("threshold", "Threshold", "#CBD5E1", "db_gain", Curve::Add(12.0), (0.0, 0.8)),
            ("range", "Range", "#E2E8F0", "db_gain", Curve::Lin, (0.0, 0.9)),
            ("attack", "Attack", "#F1F5F9", "ms", Curve::Log, (0.0, 0.5)),
            ("release", "Release", "#CBD5E1", "ms", Curve::Log, (0.1, 0.8)),
            ("hold", "Hold", "#CBD5E1", "ms", Curve::Log, (0.0, 0.6)),
        ];
        let kids = spec
            .iter()
            .filter_map(|(p, l, c, f, curve, r)| {
                rel_child(b, &format!("gate-{p}"), l, c, p, *curve, 1.0, f, Some(*r))
            })
            .collect();
        add_parent(&mut out, "gate", "Gate", "#94A3B8", Panel { layout: "row", ..Panel::default() }, kids);
    }

    // ── Pre-Comp / Comp: more = more compression ──
    let comp_kids = |b: &LiveBlock, pfx: &str, colors: [&str; 4]| -> Vec<Child> {
        let spec: [(&str, &str, &str, Curve, f32, (f32, f32)); 4] = [
            ("threshold", "Threshold", "db_gain", Curve::Add(12.0), -1.0, (0.0, 0.8)),
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
        add_parent(&mut out, "pre-comp", "Pre-Comp", "#E5E7EB", Panel { layout: "row", ..Panel::default() }, kids);
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
                    if let Some(mut mix) = rel_child(b, &format!("pitch-mix{tag}"), "Mix", "#FDE047", "mix", Curve::Lin, 1.0, "pct", r) {
                        mix.meta.pad_block = Some(b.id.clone());
                        kids.push(mix);
                    }
                    kids.extend(rel_child(b, &format!("pitch-down{tag}"), "Oct Down", "#EAB308", "b_level", Curve::Lin, 1.0, "pct", r));
                    kids.extend(rel_child(b, &format!("pitch-up{tag}"), "Oct Up", "#FEF08A", "a_level", Curve::Lin, 1.0, "pct", r));
                    kids.extend(rel_child(b, &format!("pitch-dry{tag}"), "Dry", "#FEFCE8", "dry", Curve::Lin, 1.0, "pct", None));
                    kids.extend(select_child(b, &format!("pitch-a{tag}"), "Interval A", "#FEF9C3", "semitones", "semitones"));
                    kids.extend(select_child(b, &format!("pitch-b{tag}"), "Interval B", "#FEF9C3", "b_semitones", "semitones"));
                }
                // The Ice machine: its blend is how much of the repeats is
                // shifted.
                BlockType::Delay if !is_pre_fx(b) && value(b, "style", -1.0).round() == 6.0 => {
                    kids.extend(rel_child(b, &format!("pitch-mix{tag}"), "Blend", "#FDE047", "blend", Curve::Lin, 1.0, "pct", r));
                    kids.extend(select_child(b, &format!("pitch-a{tag}"), "Interval", "#FEF9C3", "interval", "interval"));
                }
                // A shimmer delay has no amount of its own: its level is it.
                BlockType::Delay if !is_pre_fx(b) && value(b, "style", -1.0).round() == 4.0 => {
                    kids.extend(rel_child(b, &format!("pitch-mix{tag}"), "Shimmer", "#FDE047", "level", Curve::Add(12.0), 1.0, "db", r));
                }
                BlockType::Reverb if !is_pre_fx(b) && value(b, "algorithm", -1.0).round() == 6.0 => {
                    kids.extend(rel_child(b, &format!("pitch-mix{tag}"), "Shimmer", "#FDE047", "shim_amount", Curve::Lin, 1.0, "pct", r));
                    kids.extend(select_child(b, &format!("pitch-a{tag}"), "Shift", "#FEF9C3", "shim_shift1", "semitones"));
                }
                _ => {}
            }
            if kids.len() > before {
                n += 1;
            }
        }
        add_parent(&mut out, "pitch", "Pitch", "#FACC15", Panel { layout: "grouped", ..Panel::default() }, kids);
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
                let mut c = rel_child(b, &format!("drive-{}", k + 1), &b.name, colors[k % colors.len()], "drive", Curve::Lin, 1.0, "pct", None)?;
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
            Panel { layout: "row", journey: true, ..Panel::default() },
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
                    kids.extend(rel_child(eq, &format!("tone-{key}"), label, color, &format!("b{i}_gain"), Curve::Add(9.0), 1.0, "db_gain", Some(*r)));
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
        add_parent(&mut out, "tone", "Tone", "#22C55E", Panel { layout: "row", ..Panel::default() }, kids);
        if out.bank.knobs.len() > before {
            if let Some(k) = out.bank.knobs.last_mut() {
                k.bipolar = true;
            }
        }
    }

    // ── Comp (post) ──
    if let Some(b) = find(BlockType::Compressor, "Post Comp") {
        let kids = comp_kids(b, "comp", ["#F3F4F6", "#E5E7EB", "#D1D5DB", "#F9FAFB"]);
        add_parent(&mut out, "comp", "Comp", "#E5E7EB", Panel { layout: "row", ..Panel::default() }, kids);
    }

    // ── Mod: depth and mix of the modulation after the amp ──
    {
        let t = post(&|b| matches!(b.block_type, BlockType::Chorus | BlockType::Flanger | BlockType::Vibrato | BlockType::Phaser))
            .into_iter()
            .flat_map(|b| {
                [target(b, "depth", Curve::Lin, 1.0, 1.0), target(b, "mix", Curve::Lin, 1.0, 1.0)]
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
        add_single(&mut out, "motion", "Motion", "#EC4899", t, 0.5);
    }

    // ── Boost: the boost block's level, ±6 dB on what the pedal gives ──
    if let Some(b) = find(BlockType::Volume, "Boost") {
        let t = target(b, "gain_db", Curve::Add(6.0), 1.0, 1.0).into_iter().collect();
        add_single(&mut out, "boost", "Boost", "#FAFAF9", t, 0.5);
    }

    // ── Delay / Reverb: a row per block ──
    let dual = |blocks: &[&LiveBlock], cols: &dyn Fn(&LiveBlock, usize) -> Vec<Child>| -> Vec<Child> {
        blocks.iter().enumerate().flat_map(|(i, b)| cols(b, i + 1)).collect()
    };
    {
        let kids = dual(&delays, &|b, n| {
            let synced = value(b, "tap_div_l", DIV_FREE).round() != DIV_FREE;
            let mut time = if synced {
                let targets: Vec<Target> = ["tap_div_l", "tap_div_r"]
                    .iter()
                    .filter_map(|p| target(b, p, Curve::Div, 1.0, 1.0))
                    .collect();
                Child::new(
                    &format!("delay-time{n}"),
                    &format!("Time {n}"),
                    "#93C5FD",
                    Meta { targets, fmt: "div", show: Some((b.id.clone(), "tap_div_l".into())), group: b.name.clone(), ..Meta::default() },
                    Some((0.1, 0.8)),
                )
            } else {
                match rel_child(b, &format!("delay-time{n}"), &format!("Time {n}"), "#93C5FD", "time", Curve::Log, 1.0, "ms", Some((0.1, 0.8))) {
                    Some(c) => c,
                    None => return Vec::new(),
                }
            };
            time.meta.group = b.name.clone();
            [
                select_child(b, &format!("delay-type{n}"), &format!("Type {n}"), "#60A5FA", "style", "delay_style"),
                Some(time),
                rel_child(b, &format!("delay-fb{n}"), &format!("FB {n}"), "#BFDBFE", "feedback", Curve::Lin, 1.0, "pct", Some((0.0, 0.65))),
                rel_child(b, &format!("delay-filter{n}"), &format!("Filter {n}"), "#DBEAFE", "high_cut", Curve::Log, 1.0, "hz", Some((0.0, 0.5))),
                rel_child(b, &format!("delay-level{n}"), &format!("Level {n}"), "#93C5FD", "level", Curve::Add(12.0), 1.0, "db", Some((0.0, 0.7))),
            ]
            .into_iter()
            .flatten()
            .collect()
        });
        let headers = ["Type", "Time", "Feedback", "Filter", "Level"].map(String::from).to_vec();
        add_parent(&mut out, "delay", "Delay", "#3B82F6", Panel { layout: "dual", headers, ..Panel::default() }, kids);
    }
    {
        let kids = dual(&verbs, &|b, n| {
            let mut time = rel_child(b, &format!("reverb-time{n}"), &format!("Time {n}"), "#C4B5FD", "decay", Curve::Lin, 1.0, "verb_s", Some((0.1, 0.9)));
            if let Some(t) = time.as_mut() {
                t.meta.aux = Some((b.id.clone(), "algorithm".into()));
            }
            [
                select_child(b, &format!("reverb-type{n}"), &format!("Type {n}"), "#A78BFA", "algorithm", "verb_algo"),
                time,
                rel_child(b, &format!("reverb-predelay{n}"), &format!("Pre-Dly {n}"), "#DDD6FE", "predelay", Curve::Lin, 1.0, "ms", Some((0.0, 0.5))),
                // More reverb is an open, less damped tail.
                rel_child(b, &format!("reverb-character{n}"), &format!("Char {n}"), "#EDE9FE", "damping", Curve::Lin, -1.0, "pct", Some((0.0, 0.8))),
                rel_child(b, &format!("reverb-level{n}"), &format!("Level {n}"), "#C4B5FD", "level", Curve::Add(12.0), 1.0, "db", Some((0.0, 0.7))),
            ]
            .into_iter()
            .flatten()
            .collect()
        });
        let headers = ["Type", "Time", "Pre-Delay", "Character", "Level"].map(String::from).to_vec();
        add_parent(&mut out, "reverb", "Reverb", "#8B5CF6", Panel { layout: "dual", headers, ..Panel::default() }, kids);
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
            kids.extend(rel_child(b, &format!("clarity-{n}-duck"), "Duck", "#5EEAD4", "duck_sens", Curve::Lin, 1.0, "db_gain", Some((0.1, 0.9))));
            kids.extend(rel_child(b, &format!("clarity-{n}-release"), "Release", "#99F6E4", "duck_release", Curve::Log, 1.0, "s", Some((0.1, 0.9))));
            kids.extend(rel_child(b, &format!("clarity-{n}-lowcut"), "Lo Cut", "#CCFBF1", "high_pass", Curve::Lin, 1.0, "hz", Some((0.1, 0.9))));
        }
        for (i, b) in verbs.iter().enumerate() {
            let n = format!("verb{}", i + 1);
            kids.extend(rel_child(b, &format!("clarity-{n}-duck"), "Duck", "#5EEAD4", "duck", Curve::Lin, 1.0, "pct", Some((0.1, 0.9))));
            kids.extend(rel_child(b, &format!("clarity-{n}-thresh"), "Thresh", "#2DD4BF", "duck_threshold", Curve::Add(12.0), -1.0, "db_gain", Some((0.1, 0.9))));
            kids.extend(rel_child(b, &format!("clarity-{n}-release"), "Release", "#99F6E4", "duck_release", Curve::Log, 1.0, "ms", Some((0.1, 0.9))));
            kids.extend(rel_child(b, &format!("clarity-{n}-lowcut"), "Lo Cut", "#CCFBF1", "low_cut", Curve::Log, 1.0, "hz", Some((0.1, 0.9))));
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
            Panel { layout: "grouped", anchor: anchor.to_string(), ..Panel::default() },
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
        for b in post(&|b| matches!(b.block_type, BlockType::Chorus | BlockType::Flanger | BlockType::Vibrato)) {
            t.extend(target(b, "width", Curve::Lin, 1.0, 1.0));
        }
        let rest = spread_of(&t, blocks);
        add_single(&mut out, "width", "Width", "#A3E635", t, rest);
        if out.bank.knobs.last().is_some_and(|k| k.id == "width") {
            out.panels.insert("width".into(), Panel { style: "spread", ..Panel::default() });
        }
    }

    // ── Output: the patch's level, ±6 dB ──
    if let Some(b) = find(BlockType::Volume, crate::profiles::TRIM_BLOCK) {
        let t = target(b, "gain_db", Curve::Add(6.0), 1.0, 1.0).into_iter().collect();
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
    /// Responses being tuned in the panel, by knob id — live until saved
    /// into a preset or the patch changes.
    tuned: HashMap<String, Response>,
    /// Block names by id.
    names: HashMap<String, String>,
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
}

impl Context {
    /// Only the patch's own positions.
    #[must_use]
    pub fn saved(saved: &[MacroValueDef]) -> Self {
        Self { saved: saved.to_vec(), ..Self::default() }
    }
}

impl MacroEngine {
    /// Set a baseline the chain does not carry — the boost pedal's level,
    /// which the rig puts on its gain block after the chain is built.
    pub fn set_base(&mut self, block: &str, param: &str, v: f32) {
        self.baseline.insert((block.to_string(), param.to_string()), v);
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
        };
        let extra: Vec<((String, String), f32)> = self
            .baseline
            .iter()
            .filter(|((b, p), _)| !base.iter().any(|x| x.id == *b && x.params.iter().any(|q| q.name == *p)))
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        self.rebase(&patch, &base, &ctx);
        self.baseline.extend(extra);
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
        self.baseline = blocks
            .iter()
            .flat_map(|b| b.params.iter().map(move |p| ((b.id.clone(), p.name.clone()), p.value)))
            .collect();
        self.built = build(blocks);
        self.attach(blocks);
        self.base_bypass = self
            .built
            .meta
            .values()
            .filter_map(|m| m.pad_block.clone())
            .map(|id| {
                let byp = blocks.iter().find(|b| b.id == id).is_some_and(|b| b.bypassed);
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
                if self.built.meta.get(&d.id).is_some_and(|m| m.select.is_none()) {
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
                .filter(|c| self.built.meta.get(&c.id).is_some_and(|m| m.pad_block.is_some()))
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
        self.names = blocks.iter().map(|b| (b.id.clone(), b.name.clone())).collect();
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
        let Self { built, responses, tuned, names, baseline, .. } = self;
        let mut rebind: Vec<(String, String)> = Vec::new();
        for (kid, meta) in &mut built.meta {
            let fam = family.get(kid).cloned().unwrap_or_else(|| kid.clone());
            let mut all = !meta.targets.is_empty() && meta.select.is_none();
            for t in &mut meta.targets {
                let name = names.get(&t.block).cloned().unwrap_or_default();
                t.resp = tuned.get(kid).cloned().or_else(|| {
                    responses
                        .iter()
                        .find(|r| {
                            r.block.eq_ignore_ascii_case(&name)
                                && r.def.param == t.param
                                && (r.def.knob == *kid || r.def.knob == fam)
                        })
                        .map(|r| Response::of(&r.def, r.source))
                });
                if t.resp.is_none() && journey_kids.contains(kid) {
                    let r = baseline.get(&(t.block.clone(), t.param.clone())).copied().unwrap_or(0.5);
                    t.resp = Some(stage_fallback(r));
                } else if t.resp.is_none() {
                    all = false;
                }
            }
            if all && fam != *kid && !journey_kids.contains(kid) {
                rebind.push((fam, kid.clone()));
            }
        }
        for (p, c) in rebind {
            if let Some(parent) = built.bank.get_mut(&p) {
                for b in parent.bindings.iter_mut().filter(|b| b.target.param_id == c) {
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

    /// Tune knob `id`'s response live (the panel's tune mode) — until it is
    /// saved into a preset, or the patch changes. `blocks` is the live chain.
    pub fn tune(&mut self, id: &str, r: Response, blocks: &[LiveBlock]) {
        if self.built.meta.get(id).is_some_and(|m| !m.targets.is_empty()) {
            self.tuned.insert(id.to_string(), r);
            self.refresh(blocks);
        }
    }

    /// The responses being tuned on bar knob `parent`'s panel, as they would
    /// be saved: `(chain block, entry)` — the entry keyed to the bar knob,
    /// so it applies to whichever row the block sits in.
    #[must_use]
    pub fn tuned_defs(&self, parent: &str) -> Vec<(String, MacroResponseDef)> {
        let Some(p) = self.built.bank.get(parent) else { return Vec::new() };
        let mut out = Vec::new();
        for c in &p.children {
            let (Some(r), Some(meta)) = (self.tuned.get(&c.id), self.built.meta.get(&c.id)) else { continue };
            for t in &meta.targets {
                let block = self.names.get(&t.block).cloned().unwrap_or_default();
                out.push((block, r.def("", parent, &t.param)));
            }
        }
        out
    }

    /// Forget the responses being tuned on `parent`'s panel (saved, or
    /// thrown away).
    pub fn untune(&mut self, parent: &str, blocks: &[LiveBlock]) {
        let kids: Vec<String> = self
            .built
            .bank
            .get(parent)
            .map(|p| p.children.iter().map(|c| c.id.clone()).collect())
            .unwrap_or_default();
        self.tuned.retain(|k, _| !kids.contains(k));
        self.refresh(blocks);
    }

    /// Every knob's position as an offset, off rest — what a preset snapshot
    /// keeps.
    #[must_use]
    pub fn positions(&self) -> Vec<MacroValueDef> {
        self.offsets()
            .into_iter()
            .filter(|(_, m, bar)| !*bar || m.abs() > 1e-4)
            .map(|(id, value, _)| MacroValueDef { id, value, pad: String::new() })
            .collect()
    }

    /// Take `defaults` as the preset snapshot's positions from now on (they
    /// were just saved there), so the patch keeps nothing of its own.
    pub fn set_defaults(&mut self, defaults: Vec<MacroValueDef>) {
        self.defaults = defaults;
    }

    fn base(&self, block: &str, param: &str) -> Option<f32> {
        self.baseline.get(&(block.to_string(), param.to_string())).copied()
    }

    /// Set a knob's position (and, for a bar knob, its children's).
    fn set_position(&mut self, id: &str, value: f32) {
        let stages = self.journeys.get(id).cloned();
        let rest = self.built.meta.get(id).map_or(0.5, |m| m.rest);
        let Some(k) = self.built.bank.get_knob_mut(id) else { return };
        k.set_value(value);
        let _ = (stages, rest);
        let implied = self.built.bank.get(id).map(|p| self.implied(p)).unwrap_or_default();
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
                let Some(meta) = self.built.meta.get(&c.id).filter(|m| m.select.is_none()) else { continue };
                let rest = meta.rest;
                // Where its bar knob puts it — or, left be by the bar knob,
                // its rest.
                let put = implied.iter().find(|(id, _)| *id == c.id).map_or(rest, |(_, v)| *v);
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
        if self.built.meta.get(id).is_some_and(|m| m.pad_block.is_some()) {
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
                let Some(meta) = self.built.meta.get(&k.id) else { continue };
                let m = self.offset(&k.id);
                // A drive stage the patch has off fades in from its floor.
                let entry = meta
                    .pad_block
                    .as_ref()
                    .filter(|b| journey && self.base_bypass.get(*b).copied().unwrap_or(false));
                for t in meta.targets.iter().filter(|t| t.block == block && t.param == param) {
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
        Some(self.layers(block, param).iter().fold(base, |v, (t, m, e)| t.apply_from(v, *m, *e)))
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
        for (t, m, entry) in layers.iter().rev() {
            let inverse = if entry.is_some() && *m > 0.0 { None } else { t.invert(v, *m) };
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
        self.baseline.insert((block.to_string(), param.to_string()), v);
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
        if self.built.bank.get_knob(&id).is_some_and(|k| k.bypassed != bypassed) {
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
            .flat_map(|p| std::iter::once(p.id.clone()).chain(p.children.iter().map(|c| c.id.clone())))
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
                out.push(MacroValueDef { id, value, pad: pad.to_string() });
            }
        }
        out
    }

    /// A panel knob's response as tune mode draws it: where its param sits
    /// at the patch's value, the knob's bottom and top, on which curve.
    fn tune_view(&self, id: &str, meta: &Meta) -> Option<signal_guitar_proto::MacroTuneView> {
        let t = meta.targets.first().filter(|_| meta.select.is_none())?;
        let base = self.base(&t.block, &t.param)?;
        let stage = self
            .journeys
            .keys()
            .filter_map(|p| self.built.bank.get(p))
            .any(|p| p.children.iter().any(|c| c.id == id));
        let off_stage = stage
            && meta.pad_block.as_ref().is_some_and(|b| self.base_bypass.get(b).copied().unwrap_or(false));
        let entry = t.resp.as_ref().filter(|_| off_stage).and_then(|r| r.min);
        let (curve, source, enter) = match &t.resp {
            Some(r) => (r.shape.name(), r.source, r.enter.unwrap_or(-1.0)),
            None => (if t.curve == Curve::Log { "log" } else { "lin" }, "", -1.0),
        };
        let stage_enter = self
            .journeys
            .iter()
            .find_map(|(p, stages)| {
                let parent = self.built.bank.get(p)?;
                let i = parent.children.iter().position(|c| c.id == id)?;
                stages.get(i).map(|s| s.enter)
            })
            .unwrap_or(enter);
        Some(signal_guitar_proto::MacroTuneView {
            base,
            lo: t.apply_from(base, -1.0, None),
            hi: t.apply_from(base, 1.0, entry),
            min: t.lo,
            max: t.hi,
            curve: curve.to_string(),
            source: source.to_string(),
            enter: stage_enter,
            log: t.curve == Curve::Log || matches!(t.resp.as_ref().map(|r| r.shape), Some(Shape::Log)),
        })
    }

    /// The bar as the UI draws it; `blocks` is the live chain (values with
    /// the macros applied) for the readouts.
    #[must_use]
    pub fn views(&self, blocks: &[LiveBlock]) -> Vec<MacroKnobView> {
        let live = |at: &Option<(String, String)>| -> f32 {
            at.as_ref()
                .and_then(|(b, p)| blocks.iter().find(|x| x.id == *b).and_then(|x| param(x, p)).map(|p| p.value))
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
                    if k.value < 0.005 { "Mono".to_string() } else { format!("{:.0}%", k.value * 100.0) }
                } else {
                    k.format_value()
                };
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
                    children: k
                        .children
                        .iter()
                        .map(|c| {
                            let meta = self.built.meta.get(&c.id).cloned().unwrap_or_default();
                            MacroChildView {
                                id: c.id.clone(),
                                label: c.label.clone(),
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
                                tune: self.tune_view(&c.id, &meta),
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
        .min_by(|a, b| (a.1 - v).abs().partial_cmp(&(b.1 - v).abs()).unwrap_or(std::cmp::Ordering::Equal))
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

// ── The presets' responses and positions for a patch ──────────────────────

/// Every response the presets give `patch`'s blocks, most important first:
/// its module snapshots' (by block), then its block presets' own, then the
/// seeds for block presets that say nothing ([`seed_responses`]). The
/// engine takes the first that fits each knob and param.
#[must_use]
pub fn responses_for(comp: &Compositions, patch: &PatchDef) -> Vec<Resolved> {
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
            out.push(Resolved { block: d.block.clone(), def: d.clone(), source: "module" });
        }
    }
    let picks = crate::compose::block_picks(comp, patch);
    for c in &picks {
        if let Some(p) = comp.block_preset(&c.preset) {
            for d in &p.macros {
                out.push(Resolved { block: c.block.clone(), def: d.clone(), source: "block" });
            }
        }
    }
    for c in &picks {
        if let Some(p) = comp.block_preset(&c.preset).filter(|p| p.macros.is_empty()) {
            for d in seed_responses(p) {
                out.push(Resolved { block: c.block.clone(), def: d, source: "seed" });
            }
        }
    }
    out
}

/// Save tuned entries (`(chain block, entry)`, from
/// [`MacroEngine::tuned_defs`]) into the presets `patch` plays: `scope`
/// `module` — the module snapshot that owns the block (the most specific:
/// Delay over Time) — or `block` — the block preset on it. A block preset
/// that played on seeds keeps them, written out, beside the new entry.
/// Returns how many entries were written.
pub fn save_tuning(
    comp: &mut Compositions,
    patch: &PatchDef,
    chain: &[(String, BlockType)],
    defs: &[(String, MacroResponseDef)],
    scope: &str,
) -> usize {
    fn put(list: &mut Vec<MacroResponseDef>, d: MacroResponseDef) {
        match list
            .iter_mut()
            .find(|x| x.block.eq_ignore_ascii_case(&d.block) && x.knob == d.knob && x.param == d.param)
        {
            Some(x) => *x = d,
            None => list.push(d),
        }
    }
    let mut n = 0;
    for (block, def) in defs {
        if scope == "module" {
            let picks = crate::compose::module_picks(comp, patch);
            let Some(owner) = picks
                .iter()
                .rev()
                .find(|p| {
                    crate::manage::owned_blocks(comp, p, chain)
                        .iter()
                        .any(|b| b.eq_ignore_ascii_case(block))
                })
                .cloned()
            else {
                continue;
            };
            let Some(snap) = comp
                .modules
                .iter_mut()
                .find(|m| m.module.eq_ignore_ascii_case(&owner.module) && m.name.eq_ignore_ascii_case(&owner.preset))
                .and_then(|m| {
                    let i = m
                        .snapshots
                        .iter()
                        .position(|s| !owner.snapshot.is_empty() && s.name.eq_ignore_ascii_case(&owner.snapshot))
                        .unwrap_or(0);
                    m.snapshots.get_mut(i)
                })
            else {
                continue;
            };
            put(&mut snap.macros, MacroResponseDef { block: block.clone(), ..def.clone() });
            n += 1;
        } else {
            let picks = crate::compose::block_picks(comp, patch);
            let Some(choice) = picks.iter().find(|c| c.block.eq_ignore_ascii_case(block)) else { continue };
            let Some(bp) = comp.blocks.iter_mut().find(|b| b.name.eq_ignore_ascii_case(&choice.preset)) else {
                continue;
            };
            if bp.macros.is_empty() {
                bp.macros = seed_responses(bp);
            }
            put(&mut bp.macros, MacroResponseDef { block: String::new(), ..def.clone() });
            n += 1;
        }
    }
    n
}

/// The knob positions `patch`'s preset snapshot carries.
#[must_use]
pub fn preset_positions(comp: &Compositions, patch: &PatchDef) -> Vec<MacroValueDef> {
    comp.preset(&patch.rig_preset)
        .and_then(|p| {
            p.snapshots
                .iter()
                .find(|s| !patch.snapshot.is_empty() && s.name.eq_ignore_ascii_case(&patch.snapshot))
                .or_else(|| p.snapshots.first())
        })
        .map(|s| s.macros.clone())
        .unwrap_or_default()
}

/// Everything `patch` brings to the bar: its positions over its preset
/// snapshot's, and its presets' responses.
#[must_use]
pub fn context_for(comp: &Compositions, patch: &PatchDef) -> Context {
    Context {
        saved: patch.macros.clone(),
        defaults: preset_positions(comp, patch),
        responses: responses_for(comp, patch),
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
    MacroResponseDef { knob: knob.into(), param: param.into(), off: true, ..MacroResponseDef::default() }
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
    let v = |name: &str, dflt: f32| p.params.iter().find(|x| x.param == name).map_or(dflt, |x| x.value);
    let name = p.name.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    // Keep a range around the preset's own value.
    let around = |x: f32, lo: f32, hi: f32| (lo.min(x), hi.max(x));
    let mut out = Vec::new();
    match p.block_type.as_str() {
        "delay" => {
            let (fb, level, time) = (v("feedback", 0.3), v("level", -16.0), v("time", 350.0));
            let synced = v("tap_div_l", DIV_FREE).round() != DIV_FREE;
            let slap = has(&["slap"]) || (!synced && time < 180.0 && fb <= 0.3);
            let ambient = !slap && (has(&["ambient", "wash", "swell", "bloom", "shimmer", "pad", "flute"]) || fb >= 0.5 || time >= 550.0);
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
            let big = !room && (matches!(alg, 1 | 4 | 5 | 6 | 7 | 12) || has(&["hall", "ambient", "cathedral", "wash", "swell", "arena", "bloom", "shimmer"]));
            if room {
                out.push(r("reverb", "decay", decay * 0.8, (decay * 1.2).min(1.0), "lin"));
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
pub fn apply_positions(def: &PatchDef, comp: &Compositions, patch: &mut signal_sampler::RigPatch) {
    let ctx = context_for(comp, def);
    if ctx.saved.is_empty() && ctx.defaults.is_empty() {
        return;
    }
    let blocks = crate::session::chain_as_live(&patch.chain);
    let mut e = MacroEngine::default();
    e.rebase(&def.name, &blocks, &ctx);
    let writes = e.live_params();
    let bypass = e.live_bypass();
    let index = |id: &str| id.strip_prefix("chain-").and_then(|i| i.parse::<usize>().ok());
    for (id, param, v) in writes {
        let Some(b) = index(&id).and_then(|i| patch.chain.get_mut(i)) else { continue };
        match b.params.iter_mut().find(|p| p.name == param) {
            Some(p) => p.value = format!("{v}"),
            None => b.params.push(signal_sampler::rig_node::Param { name: param, value: format!("{v}") }),
        }
    }
    for (id, byp) in bypass {
        if let Some(b) = index(&id).and_then(|i| patch.chain.get_mut(i)) {
            b.bypassed = byp;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, value: f32, min: f32, max: f32) -> BlockParam {
        BlockParam { name: name.into(), value, min, max, overridden: false }
    }

    fn block(id: &str, bt: BlockType, name: &str, params: Vec<BlockParam>) -> LiveBlock {
        LiveBlock {
            id: id.into(),
            block_type: bt,
            name: name.into(),
            bypassed: false,
            param_name: None,
            param_value: 0.0,
            param_min: 0.0,
            param_max: 0.0,
            params,
            preset: String::new(),
            options: Vec::new(),
            option: 0,
            overridden: false,
            output_level_db: None,
        }
    }

    fn drive(id: &str, name: &str, d: f32, bypassed: bool) -> LiveBlock {
        let mut b = block(id, BlockType::Drive, name, vec![p("drive", d, 0.0, 1.0)]);
        b.preset = format!("{name} pedal");
        b.bypassed = bypassed;
        b
    }

    fn delay(id: &str, name: &str) -> LiveBlock {
        block(
            id,
            BlockType::Delay,
            name,
            vec![
                p("level", -14.0, -60.0, 12.0),
                p("time", 350.0, 20.0, 2500.0),
                p("feedback", 0.3, 0.0, 0.95),
                p("style", 0.0, 0.0, 12.0),
                p("tap_div_l", 7.0, 0.0, 10.0),
                p("tap_div_r", 7.0, 0.0, 10.0),
                p("high_pass", 40.0, 0.0, 900.0),
                p("pan", 0.5, -1.0, 1.0),
                p("high_cut", 8000.0, 500.0, 20000.0),
                p("duck_sens", 3.0, 0.0, 18.0),
                p("duck_release", 0.2, 0.05, 1.0),
            ],
        )
    }

    fn verb(id: &str, name: &str) -> LiveBlock {
        block(
            id,
            BlockType::Reverb,
            name,
            vec![
                p("level", -18.0, -60.0, 12.0),
                p("decay", 0.4, 0.0, 1.0),
                p("algorithm", 1.0, 0.0, 14.0),
                p("damping", 0.5, 0.0, 1.0),
                p("pan_a", -0.3, -1.0, 1.0),
                p("predelay", 20.0, 0.0, 200.0),
                p("low_cut", 100.0, 20.0, 2000.0),
                p("high_cut", 6000.0, 1000.0, 20000.0),
                p("duck", 0.2, 0.0, 1.0),
                p("duck_threshold", -20.0, -60.0, 0.0),
                p("duck_release", 120.0, 20.0, 2000.0),
            ],
        )
    }

    fn eq() -> LiveBlock {
        let mut params = Vec::new();
        for (i, hz, shape) in [(1, 80.0, 3.0), (2, 200.0, 0.0), (3, 700.0, 0.0), (4, 4500.0, 2.0)] {
            params.push(p(&format!("b{i}_on"), 1.0, 0.0, 1.0));
            params.push(p(&format!("b{i}_freq"), hz, 10.0, 30000.0));
            params.push(p(&format!("b{i}_gain"), 0.0, -30.0, 30.0));
            params.push(p(&format!("b{i}_shape"), shape, 0.0, 12.0));
        }
        block("eq", BlockType::Eq, "Amp EQ", params)
    }

    fn chain() -> Vec<LiveBlock> {
        vec![
            drive("d1", "Drive 1", 0.4, false),
            drive("d2", "Drive 2", 0.5, true),
            drive("d3", "Drive 3", 0.6, true),
            eq(),
            block("trim", BlockType::Volume, "Patch Trim", vec![p("gain_db", 0.0, -24.0, 24.0), p("pan", 0.0, -1.0, 1.0)]),
            block(
                "cho",
                BlockType::Chorus,
                "Chorus",
                vec![p("mix", 0.5, 0.0, 1.0), p("depth", 0.4, 0.0, 1.0), p("width", 0.8, 0.0, 1.0)],
            ),
            delay("dly1", "DLY 1"),
            delay("dly2", "DLY 2"),
            verb("v1", "VERB 1"),
            verb("v2", "VERB 2"),
        ]
    }

    fn engine() -> MacroEngine {
        let mut e = MacroEngine::default();
        e.rebase("Test", &chain(), &Context::default());
        e
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    /// A child follows its parent as `min + (max − min) × parent` — the
    /// legacy binding — including a range that runs backwards (Tone's Low,
    /// 0.9 → 0.1) and one that does not move at all (its Mid, 0.35 → 0.35).
    #[test]
    fn children_follow_the_parent_including_inverted_ranges() {
        let mut e = engine();
        e.set("tone", 1.0);
        let v = |id: &str| e.built.bank.get_knob(id).unwrap().value;
        assert!(approx(v("tone-low"), 0.1));
        assert!(approx(v("tone-mid"), 0.35));
        assert!(approx(v("tone-high"), 0.9));
        e.set("tone", 0.0);
        let v = |id: &str| e.built.bank.get_knob(id).unwrap().value;
        assert!(approx(v("tone-low"), 0.9));
        assert!(approx(v("tone-high"), 0.1));
    }

    /// Tone is a bipolar tilt: up is bright (low down, high up), down is
    /// dark, and at rest — its middle — every band is as the patch has it.
    #[test]
    fn tone_is_a_bipolar_tilt_around_the_patch() {
        let mut e = engine();
        assert!(e.built.bank.get("tone").unwrap().bipolar);
        assert_eq!(e.built.bank.get("tone").unwrap().format_value(), "0%");
        let (low, mid, high) = ("b2_gain", "b3_gain", "b4_gain");
        for p in [low, mid, high] {
            assert!(approx(e.live("eq", p).unwrap(), 0.0), "{p} moved at rest");
        }
        e.set("tone", 1.0);
        assert!(e.live("eq", low).unwrap() < -5.0);
        assert!(approx(e.live("eq", mid).unwrap(), 0.0));
        assert!(e.live("eq", high).unwrap() > 5.0);
        assert_eq!(e.built.bank.get("tone").unwrap().format_value(), "+100%");
        e.set("tone", 0.0);
        assert!(e.live("eq", low).unwrap() > 5.0);
        assert!(e.live("eq", high).unwrap() < -5.0);
        // The low-cut band at 80 Hz is not a tilt band.
        assert!(!e.built.meta.values().any(|m| m.targets.iter().any(|t| t.param == "b1_gain")));
    }

    /// Relative, not absolute: every knob at rest leaves every param as the
    /// patch has it, and returning to rest after a move restores it exactly.
    #[test]
    fn rest_changes_nothing_and_coming_back_restores_the_patch() {
        let mut e = engine();
        let before = e.live_params();
        for (_, _, v) in &before {
            assert!(v.is_finite());
        }
        for k in ["delay", "reverb", "space", "clarity", "tone", "mod"] {
            e.set(k, 0.93);
            e.set(k, 0.12);
            e.set(k, 0.5);
        }
        let after = e.live_params();
        assert_eq!(before.len(), after.len());
        for (a, b) in before.iter().zip(after.iter()) {
            assert!(approx(a.2, b.2), "{}.{}: {} → {}", a.0, a.1, a.2, b.2);
        }
        assert!(e.saved().is_empty(), "rest is not stored: {:?}", e.saved());
    }

    /// Up is more of what the patch has, around each param's own value.
    #[test]
    fn delay_up_is_more_of_the_patch() {
        let mut e = engine();
        e.set("delay", 1.0);
        assert!(e.live("dly1", "time").unwrap() > 350.0);
        assert!(e.live("dly1", "feedback").unwrap() > 0.3);
        assert!(e.live("dly1", "level").unwrap() > -14.0);
        e.set("delay", 0.0);
        assert!(e.live("dly1", "time").unwrap() < 350.0);
        assert!(e.live("dly1", "feedback").unwrap() < 0.3);
        // Type is a choice, not scaled by the parent.
        assert!(approx(e.live("dly1", "style").unwrap_or(0.0), 0.0));
    }

    /// Drive is a journey: at rest the stages are as the patch has them;
    /// going up the off stages come in, in chain order, fading in from their
    /// floor, and every stage ends at a sensible top; going down they drop out, last first, until none
    /// is left.
    #[test]
    fn drive_brings_the_stages_in_and_out_in_order() {
        let mut e = engine();
        let byp = |e: &MacroEngine| e.live_bypass().into_iter().map(|(_, b)| b).collect::<Vec<_>>();
        assert_eq!(byp(&e), vec![false, true, true], "rest = the patch");
        e.set("drive", 0.6);
        assert_eq!(byp(&e), vec![false, true, true], "stage 1 ramps first");
        assert!(e.live("d1", "drive").unwrap() > 0.4);
        assert!(approx(e.live("d2", "drive").unwrap(), 0.5), "stage 2 not yet");
        e.set("drive", 0.7);
        assert_eq!(byp(&e), vec![false, false, true], "then stage 2 comes in");
        assert!(e.live("d2", "drive").unwrap() < 0.25, "fading in from its floor, not jumping to 0.5");
        e.set("drive", 0.9);
        assert_eq!(byp(&e), vec![false, false, false], "then stage 3");
        e.set("drive", 1.0);
        // Each at a sensible top — 60 % of the way from its drive to full.
        for (id, d) in [("d1", 0.4), ("d2", 0.5), ("d3", 0.6)] {
            let top = 0.6f32.mul_add(1.0 - d, d);
            assert!(approx(e.live(id, "drive").unwrap(), top), "{id} at its top");
        }
        e.set("drive", 0.3);
        assert_eq!(byp(&e), vec![false, true, true]);
        assert!(e.live("d1", "drive").unwrap() < 0.4, "stage 1 gentler below rest");
        e.set("drive", 0.0);
        assert_eq!(byp(&e), vec![true, true, true], "the bottom is dry");
    }

    /// Four stages come in progressively too — the legacy three-stage rule,
    /// generalised.
    #[test]
    fn four_stage_rules_open_one_by_one() {
        let ids: Vec<String> = (1..=4).map(|i| format!("s{i}")).collect();
        let rules = drive_rules(&ids, &journey(&[false; 4]));
        let on_at = |v: f32| -> usize {
            rules
                .iter()
                .filter(|r| r.active_ranges.iter().any(|&(lo, hi)| v >= lo && v < hi))
                .count()
        };
        assert_eq!(on_at(0.5), 0);
        assert_eq!(on_at(0.6), 1);
        assert_eq!(on_at(0.7), 2);
        assert_eq!(on_at(0.85), 3);
        assert_eq!(on_at(1.0), 4);
        // All four on in the patch: they drop out last first going down.
        let rules = drive_rules(&ids, &journey(&[true; 4]));
        let on = |v: f32| -> Vec<bool> {
            rules.iter().map(|r| r.active_ranges.iter().any(|&(lo, hi)| v >= lo && v < hi)).collect()
        };
        assert_eq!(on(0.5), vec![true; 4]);
        assert_eq!(on(0.3), vec![true, true, true, false]);
        assert_eq!(on(0.2), vec![true, true, false, false]);
        assert_eq!(on(0.0), vec![false; 4]);
    }

    /// A pad forces its stage until the Drive knob next moves.
    #[test]
    fn a_pad_holds_until_the_knob_moves() {
        let mut e = engine();
        e.set_pad("drive-3", true);
        assert_eq!(e.live_bypass()[2], ("d3".to_string(), false));
        assert_eq!(e.saved().iter().find(|d| d.id == "drive-3").unwrap().pad, "on");
        e.set("drive", 0.5);
        assert_eq!(e.live_bypass()[2], ("d3".to_string(), true));
    }

    /// Switching a stage by hand is the patch's own state: at rest it needs
    /// no pad (so nothing is stored), off rest it holds until the knob moves.
    #[test]
    fn a_hand_switched_stage_is_the_patch() {
        let mut e = engine();
        e.direct_bypass("d2", false);
        assert_eq!(e.live_bypass()[1], ("d2".to_string(), false));
        assert!(e.saved().is_empty(), "at rest nothing to keep: {:?}", e.saved());
        // Down here the knob has stage 2 out; switching it on holds.
        e.set("drive", 0.2);
        assert_eq!(e.live_bypass()[1], ("d2".to_string(), true));
        e.direct_bypass("d2", false);
        assert_eq!(e.live_bypass()[1], ("d2".to_string(), false));
        assert!(e.saved().iter().any(|d| d.id == "drive-2" && d.pad == "on"));
    }

    /// Clarity: more ducking and more low cut on the wet above rest, less
    /// below, none of it moved at rest.
    #[test]
    fn clarity_ducks_and_cuts_around_the_patch() {
        let mut e = engine();
        assert!(approx(e.live("dly1", "duck_sens").unwrap(), 3.0));
        assert!(approx(e.live("v1", "low_cut").unwrap(), 100.0));
        e.set("clarity", 1.0);
        assert!(e.live("dly1", "duck_sens").unwrap() > 3.0);
        assert!(e.live("dly1", "high_pass").unwrap() > 40.0);
        assert!(e.live("v1", "duck").unwrap() > 0.2);
        assert!(e.live("v1", "duck_threshold").unwrap() < -20.0, "a lower threshold ducks more");
        assert!(e.live("v1", "low_cut").unwrap() > 100.0);
        e.set("clarity", 0.0);
        assert!(e.live("dly1", "duck_sens").unwrap() < 3.0);
        assert!(e.live("v1", "low_cut").unwrap() < 100.0);
        assert!(e.live("v1", "duck_threshold").unwrap() > -20.0);
        e.set("clarity", 0.5);
        assert!(approx(e.live("v1", "low_cut").unwrap(), 100.0));
        // Its panel hangs under the Delay knob, a row per block.
        let v = e.views(&chain());
        let clarity = v.iter().find(|k| k.id == "clarity").unwrap();
        assert_eq!(clarity.anchor, "delay");
        assert_eq!(clarity.layout, "grouped");
        let groups: Vec<&str> = clarity.children.iter().map(|c| c.group.as_str()).collect();
        assert_eq!(groups.first(), Some(&"DLY 1"));
        assert_eq!(groups.last(), Some(&"VERB 2"));
    }

    /// Width: 0 is mono — every pan centred, every width shut — its rest is
    /// the patch's own spread, and the top is as wide as the params go.
    #[test]
    fn width_zero_is_mono_and_rest_is_the_patch() {
        let mut e = engine();
        let rest = e.built.meta["width"].rest;
        assert!(rest > 0.0 && rest < 1.0);
        assert!(approx(e.built.bank.get("width").unwrap().value, rest));
        assert!(approx(e.live("dly1", "pan").unwrap(), 0.5));
        assert!(approx(e.live("v1", "pan_a").unwrap(), -0.3));
        e.set("width", 0.0);
        for (b, p) in [("dly1", "pan"), ("v1", "pan_a"), ("cho", "width"), ("trim", "pan")] {
            assert!(approx(e.live(b, p).unwrap(), 0.0), "{b}.{p} not mono");
        }
        assert_eq!(e.views(&chain()).iter().find(|k| k.id == "width").unwrap().readout, "Mono");
        e.set("width", 1.0);
        assert!(approx(e.live("dly1", "pan").unwrap(), 1.0));
        assert!(approx(e.live("v1", "pan_a").unwrap(), -1.0), "a pan widens to its own side");
        assert!(approx(e.live("cho", "width").unwrap(), 1.0));
        e.set("width", rest);
        assert!(approx(e.live("dly1", "pan").unwrap(), 0.5));
        assert!(approx(e.live("cho", "width").unwrap(), 0.8));
        assert!(e.saved().is_empty());
    }

    /// A direct edit keeps the value the player dialled and moves the
    /// baseline under it, so the macro stays an offset on top: back at rest
    /// the param sits at the new baseline, not where the macro had it.
    #[test]
    fn a_direct_edit_moves_the_baseline_under_the_macro() {
        let mut e = engine();
        e.set("space", 1.0);
        let base = e.direct_edit("dly1", "level", -6.0);
        assert!(approx(e.live("dly1", "level").unwrap(), -6.0));
        assert!(base < -6.0);
        e.set("space", 0.5);
        assert!(approx(e.live("dly1", "level").unwrap(), base));
    }

    /// Saved offsets come back with the patch — children moved on their
    /// own after their parent included.
    #[test]
    fn positions_come_back_with_the_patch() {
        let mut e = engine();
        e.set("delay", 0.8);
        e.set("delay-fb2", 0.1);
        let saved = e.saved();
        let live = e.live_params();
        let mut again = MacroEngine::default();
        again.rebase("Test", &chain(), &Context::saved(&saved));
        for (a, b) in live.iter().zip(again.live_params().iter()) {
            assert!(approx(a.2, b.2), "{}.{}", a.0, a.1);
        }
    }

    /// A selector is a choice: it writes the param itself and becomes the
    /// patch's own, so its rest follows it.
    #[test]
    fn a_type_knob_is_a_direct_choice() {
        let mut e = engine();
        let w = e.set("delay-type1", 1.0).expect("a write");
        assert_eq!(w, ("dly1".to_string(), "style".to_string(), 12.0));
        assert!(approx(e.built.meta["delay-type1"].rest, 1.0));
        assert!(e.saved().iter().all(|d| d.id != "delay-type1"));
    }

    /// A synced delay's time steps through the divisions, longer up.
    #[test]
    fn a_synced_time_steps_through_divisions() {
        let mut b = delay("dly1", "DLY 1");
        for p in b.params.iter_mut().filter(|p| p.name.starts_with("tap_div")) {
            p.value = 2.0; // 1/8
        }
        let mut e = MacroEngine::default();
        e.rebase("Sync", &[b], &Context::default());
        e.set("delay-time1", 1.0);
        let up = e.live("dly1", "tap_div_l").unwrap();
        assert!([5.0, 10.0, 1.0].contains(&up), "a longer division: {up}");
        assert_eq!(e.live("dly1", "tap_div_r"), Some(up));
        e.set("delay-time1", 0.0);
        assert!([3.0, 6.0, 4.0].contains(&e.live("dly1", "tap_div_l").unwrap()));
    }

    /// Pitch appears only with a pitch-bearing block: the Ice delay, a
    /// shimmer reverb or the Pitch block.
    #[test]
    fn pitch_needs_a_pitch_block() {
        // The octaver: bypassed in the patch, it comes in above rest with
        // more of both octaves, and its intervals are left be.
        let mut pog = block(
            "pog",
            BlockType::Pitch,
            "Pitch",
            vec![
                p("semitones", 12.0, -24.0, 24.0),
                p("b_semitones", -12.0, -24.0, 24.0),
                p("mix", 0.5, 0.0, 1.0),
                p("a_level", 0.7, 0.0, 1.0),
                p("b_level", 0.7, 0.0, 1.0),
                p("dry", 1.0, 0.0, 1.0),
            ],
        );
        pog.bypassed = true;
        let mut e = MacroEngine::default();
        e.rebase("POG", &[pog], &Context::default());
        let kids: Vec<String> = e.built.bank.get("pitch").unwrap().children.iter().map(|c| c.label.clone()).collect();
        assert_eq!(kids, ["Mix", "Oct Down", "Oct Up", "Dry", "Interval A", "Interval B"]);
        assert_eq!(e.live_bypass(), vec![("pog".to_string(), true)]);
        e.set("pitch", 0.7);
        assert_eq!(e.live_bypass(), vec![("pog".to_string(), false)], "up engages it");
        assert!(e.live("pog", "a_level").unwrap() > 0.7);
        assert!(e.live("pog", "b_level").unwrap() > 0.7);
        assert!(e.live("pog", "mix").unwrap() > 0.5);
        assert!(approx(e.live("pog", "semitones").unwrap(), 12.0));
        e.set("pitch", 0.2);
        assert_eq!(e.live_bypass(), vec![("pog".to_string(), true)]);
        e.set_pad("pitch-mix1", true);
        assert_eq!(e.live_bypass(), vec![("pog".to_string(), false)], "the pad engages it");
        let w = e.set("pitch-b1", 31.0 / 48.0).expect("a choice");
        assert_eq!((w.1.as_str(), w.2), ("b_semitones", 7.0));

        assert!(engine().built.bank.get("pitch").is_none());
        let mut ice = delay("dly1", "DLY 1");
        ice.params.iter_mut().find(|p| p.name == "style").unwrap().value = 6.0;
        ice.params.push(p("interval", 27.0, 0.0, 30.0));
        ice.params.push(p("blend", 0.6, 0.0, 1.0));
        let mut e = MacroEngine::default();
        e.rebase("Flute", &[ice], &Context::default());
        let pitch = e.built.bank.get("pitch").expect("pitch");
        assert_eq!(pitch.children.len(), 2);
        e.set("pitch", 1.0);
        assert!(e.live("dly1", "blend").unwrap() > 0.6);
        assert!(approx(e.live("dly1", "interval").unwrap(), 27.0), "the interval is left be");
    }

    /// The positions ride on the patch in the styx library: a profile
    /// written before macros existed still parses (the field defaults),
    /// and positions written come back.
    #[test]
    fn positions_persist_with_the_patch() {
        let mut def: crate::profiles::ProfileDef =
            facet_styx::from_str(crate::library::DEFAULT_PROFILE).expect("a profile without macros parses");
        assert!(def.patches.iter().all(|p| p.macros.is_empty()));
        def.patches[0].macros = vec![
            MacroValueDef { id: "drive".into(), value: 0.4, pad: String::new() },
            MacroValueDef { id: "drive-2".into(), value: 0.0, pad: "off".into() },
        ];
        let text = facet_styx::to_string(&def).expect("write");
        let back: crate::profiles::ProfileDef = facet_styx::from_str(&text).expect("read back");
        assert_eq!(back.patches[0].macros, def.patches[0].macros);
    }

    // ── Preset-tuned responses ─────────────────────────────────────────

    /// The curves between rest and an end: linear, by ratio, slow-then-fast
    /// and S; each lands on the end and starts at rest; a side left out
    /// does not move.
    #[test]
    fn response_curves_and_one_sided_ranges() {
        let resp = |curve: &str, min: Option<f32>, max: Option<f32>| Response {
            min,
            max,
            shape: Shape::parse(curve),
            off: false,
            enter: None,
            source: "block",
        };
        let lin = resp("lin", Some(0.0), Some(1.0));
        assert!(approx(lin.apply(0.5, 0.5, None), 0.75));
        assert!(approx(lin.apply(0.5, -0.5, None), 0.25));
        // By ratio: halfway from 1 s to 4 s is 2 s, not 2.5 s.
        let log = resp("log", Some(0.25), Some(4.0));
        assert!(approx(log.apply(1.0, 0.5, None), 2.0));
        assert!(approx(log.apply(1.0, -0.5, None), 0.5));
        // Slow, then fast: a quarter of the way at halfway.
        let exp = resp("exp", None, Some(1.0));
        assert!(approx(exp.apply(0.0, 0.5, None), 0.25));
        // S: half at half, gentle at the ends.
        let s = resp("s", None, Some(1.0));
        assert!(approx(s.apply(0.0, 0.5, None), 0.5));
        assert!(s.apply(0.0, 0.1, None) < 0.1);
        for r in [&lin, &log, &exp, &s] {
            assert!(approx(r.apply(0.5, 1.0, None), r.max.unwrap()), "lands on its top");
            assert!(approx(r.apply(0.5, 0.0, None), 0.5), "rest is the patch");
        }
        // One-sided: no min, so down changes nothing.
        assert!(approx(exp.apply(0.3, -1.0, None), 0.3));
        // Off: never moves.
        let mut o = lin.clone();
        o.off = true;
        assert!(approx(o.apply(0.3, 1.0, None), 0.3));
        // Every curve inverts.
        for r in [&lin, &log, &exp, &s] {
            for m in [-0.7, -0.2, 0.3, 0.8] {
                if (m < 0.0 && r.min.is_none()) || (m > 0.0 && r.max.is_none()) {
                    continue;
                }
                let live = r.apply(0.6, m, None);
                assert!((r.invert(live, m).unwrap() - 0.6).abs() < 1e-3, "{:?} {m}", r.shape);
            }
        }
    }

    fn patch_def(name: &str) -> PatchDef {
        PatchDef {
            name: name.into(),
            song: String::new(),
            preset: String::new(),
            preset2: String::new(),
            rig_preset: String::new(),
            snapshot: String::new(),
            modules: Vec::new(),
            blocks: Vec::new(),
            drives: Vec::new(),
            trim_db: 0.0,
            level_db: 0.0,
            boost_db: 0.0,
            overrides: Vec::new(),
            macros: Vec::new(),
        }
    }

    fn entry(block: &str, knob: &str, param: &str, min: f32, max: f32) -> MacroResponseDef {
        MacroResponseDef {
            block: block.into(),
            knob: knob.into(),
            param: param.into(),
            min: Some(min),
            max: Some(max),
            ..MacroResponseDef::default()
        }
    }

    /// Who decides how a macro moves a param: the module snapshot playing,
    /// over the block preset on the block, over a seed for a block preset
    /// that says nothing, over the engine's own response.
    #[test]
    fn a_module_snapshot_beats_a_block_preset_beats_a_seed() {
        use crate::compose::{BlockChoiceDef, BlockPresetDef, ModulePresetDef, ModuleSnapshotDef, ParamSetDef};
        use crate::profiles::ModuleChoiceDef;
        let mut comp = Compositions::default();
        comp.blocks.push(BlockPresetDef {
            block_type: "delay".into(),
            name: "Tuned".into(),
            macros: vec![entry("", "delay", "feedback", 0.1, 0.4), entry("", "delay", "level", -20.0, -10.0)],
            ..BlockPresetDef::default()
        });
        comp.blocks.push(BlockPresetDef {
            block_type: "delay".into(),
            name: "Slapback".into(),
            params: vec![ParamSetDef { param: "feedback".into(), value: 0.1 }, ParamSetDef { param: "time".into(), value: 110.0 }],
            ..BlockPresetDef::default()
        });
        comp.modules.push(ModulePresetDef {
            module: "Delay".into(),
            name: "Rig".into(),
            snapshots: vec![ModuleSnapshotDef {
                name: "A".into(),
                blocks: vec![
                    BlockChoiceDef { block: "DLY 1".into(), preset: "Tuned".into() },
                    BlockChoiceDef { block: "DLY 2".into(), preset: "Slapback".into() },
                ],
                macros: vec![entry("DLY 1", "delay", "feedback", 0.0, 0.9)],
                ..ModuleSnapshotDef::default()
            }],
        });
        let mut patch = patch_def("P");
        patch.modules.push(ModuleChoiceDef { module: "Delay".into(), preset: "Rig".into(), snapshot: "A".into() });
        let ctx = context_for(&comp, &patch);
        let mut e = MacroEngine::default();
        e.rebase("P", &chain(), &ctx);
        let src = |id: &str| e.built.meta[id].targets[0].resp.as_ref().map(|r| r.source);
        assert_eq!(src("delay-fb1"), Some("module"), "the module snapshot's feedback");
        assert_eq!(src("delay-level1"), Some("block"), "the block preset's level");
        assert_eq!(src("delay-fb2"), Some("seed"), "the slapback's seed");
        assert_eq!(src("delay-time1"), None, "nothing says: the engine's own");
        // A tuned knob follows its bar knob end to end: the bar knob's top
        // is the preset's top.
        e.set("delay", 1.0);
        assert!(approx(e.live("dly1", "feedback").unwrap(), 0.9));
        assert!(approx(e.live("dly1", "level").unwrap(), -10.0));
        assert!(approx(e.live("dly2", "feedback").unwrap(), 0.25), "a slapback tops out at 0.25");
        e.set("delay", 0.0);
        assert!(approx(e.live("dly1", "feedback").unwrap(), 0.0));
        assert!(approx(e.live("dly1", "level").unwrap(), -20.0));
        e.set("delay", 0.5);
        assert!(approx(e.live("dly1", "feedback").unwrap(), 0.3), "rest is the patch");
        // Space: the slapback opts its feedback out.
        e.set("space", 1.0);
        assert!(approx(e.live("dly2", "feedback").unwrap(), 0.3), "Space leaves a slapback's feedback be");
        assert!(e.live("dly1", "feedback").unwrap() > 0.3);
    }

    /// A Drive snapshot shapes the journey per stage: where it comes in, the
    /// drive it fades in from, and where it tops out.
    #[test]
    fn a_drive_snapshot_shapes_the_journey() {
        let mut ctx = Context::default();
        for (slot, enter, min, max) in [("Drive 2", 0.1, 0.2, 0.55), ("Drive 3", 0.8, 0.35, 0.7)] {
            let mut d = entry(slot, "drive", "drive", min, max);
            d.enter = Some(enter);
            ctx.responses.push(Resolved { block: slot.into(), def: d, source: "module" });
        }
        let mut e = MacroEngine::default();
        e.rebase("Drive", &chain(), &ctx);
        let byp = |e: &MacroEngine| e.live_bypass().into_iter().map(|(_, b)| b).collect::<Vec<_>>();
        e.set("drive", 0.56);
        assert_eq!(byp(&e), vec![false, false, true], "stage 2 comes in early");
        let d2 = e.live("d2", "drive").unwrap();
        assert!(d2 > 0.2 && d2 < 0.3, "fading in from 0.2: {d2}");
        e.set("drive", 0.85);
        assert_eq!(byp(&e), vec![false, false, true], "stage 3 only at 80 % up");
        e.set("drive", 1.0);
        assert_eq!(byp(&e), vec![false, false, false]);
        assert!(approx(e.live("d2", "drive").unwrap(), 0.55), "its own top, not full");
        assert!(approx(e.live("d3", "drive").unwrap(), 0.7));
        let tv = e.views(&chain());
        let stage = tv.iter().find(|k| k.id == "drive").unwrap().children[2].tune.clone().unwrap();
        assert!(approx(stage.enter, 0.8));
        assert_eq!(stage.source, "module");
    }

    /// A patch's own knob positions win over its preset snapshot's, which
    /// win over rest; the patch keeps only what differs from the snapshot —
    /// a knob turned back to rest over a snapshot that turns it included.
    #[test]
    fn positions_patch_over_snapshot_over_rest() {
        let pos = |id: &str, v: f32| MacroValueDef { id: id.into(), value: v, pad: String::new() };
        let ctx = Context {
            saved: vec![pos("delay", -0.5)],
            defaults: vec![pos("delay", 0.4), pos("space", 0.6)],
            responses: Vec::new(),
        };
        let mut e = MacroEngine::default();
        e.rebase("P", &chain(), &ctx);
        assert!(approx(offset_of(e.built.bank.get("delay").unwrap().value, 0.5), -0.5), "the patch's");
        assert!(approx(offset_of(e.built.bank.get("space").unwrap().value, 0.5), 0.6), "the snapshot's");
        assert!(approx(e.built.bank.get("reverb").unwrap().value, 0.5), "rest");
        assert_eq!(e.saved(), vec![pos("delay", -0.5)]);
        // Back to the snapshot's position: nothing of its own.
        e.set("delay", value_of(0.4, 0.5));
        assert!(e.saved().is_empty(), "{:?}", e.saved());
        // To rest, over a snapshot that turns it: kept, as rest.
        e.set("space", 0.5);
        assert_eq!(e.saved(), vec![pos("space", 0.0)]);
        // What a preset snapshot would keep: every knob off rest.
        let kept = e.positions();
        assert!(kept.iter().any(|d| d.id == "delay" && approx(d.value, 0.4)));
        assert!(!kept.iter().any(|d| d.id == "space"));
        assert!(!kept.iter().any(|d| d.id.starts_with("delay-")), "panel knobs where their bar knob puts them");
    }

    /// Levelling measures a patch with its knobs where it keeps them: the
    /// built chain carries the positions.
    #[test]
    fn levelling_measures_the_stored_positions() {
        use signal_sampler::{RigBlock, RigPatch};
        let dly = RigBlock::of_type(BlockType::Delay)
            .named("DLY 1")
            .with_param("feedback", "0.3")
            .with_param("level", "-14");
        let mut built = RigPatch::new("P").with_block(dly);
        let mut def = patch_def("P");
        def.macros = vec![MacroValueDef { id: "space".into(), value: 1.0, pad: String::new() }];
        apply_positions(&def, &Compositions::default(), &mut built);
        let level = built.chain[0].param_f32("level").unwrap();
        assert!(level > -14.0, "Space up raised the wet level the meter hears: {level}");
        // A patch with no positions is measured as built.
        let mut plain = RigPatch::new("P").with_block(
            RigBlock::of_type(BlockType::Delay).named("DLY 1").with_param("level", "-14"),
        );
        apply_positions(&patch_def("P"), &Compositions::default(), &mut plain);
        assert!(approx(plain.chain[0].param_f32("level").unwrap(), -14.0));
    }

    /// The new fields are defaulted: libraries written before them parse,
    /// and what is written comes back.
    #[test]
    fn preset_responses_and_positions_round_trip_through_styx() {
        use crate::compose::{BlockLib, ModuleLib, PresetLib};
        let old_blocks = r#"presets ({block_type delay, name Slap, params ({param feedback, value 0.1}), bypass false, target_gr_db 0})"#;
        let lib: BlockLib = facet_styx::from_str(old_blocks).expect("an old blocks.styx parses");
        assert!(lib.presets[0].macros.is_empty());
        let old_modules = r#"presets ({module Delay, name Rig, snapshots ({name A, nam "", cab "", nam2 "", cab2 "", level_db 0, level2_db 0, drives (), blocks (), modules (), overrides ()})})"#;
        let m: ModuleLib = facet_styx::from_str(old_modules).expect("an old modules.styx parses");
        assert!(m.presets[0].snapshots[0].macros.is_empty());
        let old_presets = r#"presets ({name Fender, snapshots ({name Clean, modules (), blocks (), overrides (), level_db 0})})"#;
        let p: PresetLib = facet_styx::from_str(old_presets).expect("an old presets.styx parses");
        assert!(p.presets[0].snapshots[0].macros.is_empty());

        let mut lib = lib;
        let mut one_sided = entry("", "space", "level", -16.0, -8.0);
        one_sided.min = None;
        one_sided.curve = "log".into();
        lib.presets[0].macros = vec![entry("", "delay", "feedback", 0.0, 0.25), one_sided];
        let back: BlockLib = facet_styx::from_str(&facet_styx::to_string(&lib).unwrap()).unwrap();
        assert_eq!(back.presets[0].macros, lib.presets[0].macros);
        let mut p = p;
        p.presets[0].snapshots[0].macros = vec![MacroValueDef { id: "drive".into(), value: 0.3, pad: String::new() }];
        let back: PresetLib = facet_styx::from_str(&facet_styx::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.presets[0].snapshots[0].macros, p.presets[0].snapshots[0].macros);
    }

    /// Saving a tuning writes it where it was asked: the block preset on the
    /// block (keeping the seeds it played on), or the module snapshot.
    #[test]
    fn tuning_saves_into_the_block_preset_or_the_module_snapshot() {
        use crate::compose::{BlockChoiceDef, BlockPresetDef, ModulePresetDef, ModuleSnapshotDef, ParamSetDef};
        use crate::profiles::ModuleChoiceDef;
        let mut comp = Compositions::default();
        comp.blocks.push(BlockPresetDef {
            block_type: "delay".into(),
            name: "Slapback".into(),
            params: vec![ParamSetDef { param: "time".into(), value: 110.0 }],
            ..BlockPresetDef::default()
        });
        comp.modules.push(ModulePresetDef {
            module: "Delay".into(),
            name: "Rig".into(),
            snapshots: vec![ModuleSnapshotDef {
                name: "A".into(),
                blocks: vec![BlockChoiceDef { block: "DLY 1".into(), preset: "Slapback".into() }],
                ..ModuleSnapshotDef::default()
            }],
        });
        let mut patch = patch_def("P");
        patch.modules.push(ModuleChoiceDef { module: "Delay".into(), preset: "Rig".into(), snapshot: "A".into() });
        let mut e = MacroEngine::default();
        e.rebase("P", &chain(), &context_for(&comp, &patch));
        let r = Response { min: Some(0.05), max: Some(0.2), shape: Shape::S, off: false, enter: None, source: "tuning" };
        e.tune("delay-fb1", r, &chain());
        e.set("delay", 1.0);
        assert!(approx(e.live("dly1", "feedback").unwrap(), 0.2), "live while tuning");
        let defs = e.tuned_defs("delay");
        assert_eq!(defs[0].0, "DLY 1");
        let chain_types: Vec<(String, BlockType)> = chain().iter().map(|b| (b.name.clone(), b.block_type)).collect();
        let mut by_block = comp.clone();
        assert_eq!(save_tuning(&mut by_block, &patch, &chain_types, &defs, "block"), 1);
        let saved = &by_block.block_preset("Slapback").unwrap().macros;
        assert!(saved.iter().any(|d| d.knob == "delay" && d.param == "feedback" && d.max == Some(0.2) && d.curve == "s"));
        assert!(saved.iter().any(|d| d.knob == "space" && d.off), "the seeds it played on, kept");
        let mut by_module = comp.clone();
        assert_eq!(save_tuning(&mut by_module, &patch, &chain_types, &defs, "module"), 1);
        let snap = &by_module.module("Delay", "Rig").unwrap().snapshots[0];
        assert_eq!(snap.macros[0].block, "DLY 1");
        assert_eq!(snap.macros[0].min, Some(0.05));
    }

    /// The seeds are musical and always contain the preset's own value.
    #[test]
    fn seeds_fit_the_presets_character() {
        use crate::compose::{BlockPresetDef, ParamSetDef};
        let preset = |ty: &str, name: &str, params: &[(&str, f32)]| BlockPresetDef {
            block_type: ty.into(),
            name: name.into(),
            params: params.iter().map(|(p, v)| ParamSetDef { param: (*p).into(), value: *v }).collect(),
            ..BlockPresetDef::default()
        };
        let find = |list: &[MacroResponseDef], k: &str, p: &str| list.iter().find(|d| d.knob == k && d.param == p).cloned();
        let slap = seed_responses(&preset("delay", "Slap", &[("feedback", 0.12), ("level", -12.0), ("time", 100.0)]));
        assert_eq!(find(&slap, "delay", "feedback").unwrap().max, Some(0.25));
        assert!(find(&slap, "space", "feedback").unwrap().off);
        let amb = seed_responses(&preset("delay", "Ambient Wash", &[("feedback", 0.6), ("level", -10.0)]));
        let fb = find(&amb, "delay", "feedback").unwrap();
        assert_eq!((fb.min, fb.max), (Some(0.3), Some(0.75)));
        let hall = seed_responses(&preset("reverb", "Big Hall", &[("decay", 0.4), ("algorithm", 1.0)]));
        let d = find(&hall, "reverb", "decay").unwrap();
        assert!(approx(d.min.unwrap(), 0.2) && approx(d.max.unwrap(), 0.8) && d.curve == "log");
        let room = seed_responses(&preset("reverb", "Room", &[("decay", 0.3), ("algorithm", 0.0)]));
        assert!(find(&room, "reverb", "decay").unwrap().max.unwrap() <= 0.36 + 1e-6, "a room stays short");
        for list in [&slap, &amb, &hall, &room] {
            for d in list.iter().filter(|d| !d.off) {
                assert!(d.min.unwrap() <= d.max.unwrap());
            }
        }
        assert!(seed_responses(&preset("compressor", "Glue", &[])).is_empty());
    }

    #[test]
    fn offsets_round_trip() {
        for rest in [0.0, 0.3, 0.5, 1.0] {
            for m in [-1.0, -0.4, 0.0, 0.7, 1.0] {
                let v = value_of(m, rest);
                let back = offset_of(v, rest);
                if (rest > 0.0 || m >= 0.0) && (rest < 1.0 || m <= 0.0) {
                    assert!(approx(back, m), "rest {rest} m {m} → {v} → {back}");
                }
            }
        }
    }

    #[test]
    fn curves_invert() {
        let t = |curve| Target { block: "b".into(), param: "p".into(), lo: 20.0, hi: 2000.0, curve, dir: 1.0, depth: 1.0, resp: None };
        for curve in [Curve::Lin, Curve::Log, Curve::Add(12.0)] {
            let t = t(curve);
            for m in [-0.8, -0.3, 0.4, 0.9] {
                let live = t.apply(300.0, m);
                assert!((t.invert(live, m).unwrap() - 300.0).abs() < 0.5, "{curve:?} {m}");
            }
        }
    }
}
