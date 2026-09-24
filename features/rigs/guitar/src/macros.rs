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

use std::collections::HashMap;

use signal_guitar_proto::{BlockParam, LiveBlock, MacroChildView, MacroKnobView};
use signal_macromod::{MacroBank, MacroBinding, MacroKnob};
use signal_proto::block::BlockType;

use crate::profiles::MacroValueDef;

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
/// on — every stage all the way driven at the top. Below rest the stages
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
/// patch) at rest; going up, each stage ramps from where it is on to all the
/// way driven at the top — the first across the whole upper half, a late one
/// only once it is in; going down, each on stage lowers toward a gentle 30 %
/// of its drive until it drops out.
#[must_use]
pub fn drive_stage_value(m: f32, s: Stage) -> f32 {
    if m >= 0.0 {
        let u = ((m - s.enter) / (1.0 - s.enter).max(1e-6)).clamp(0.0, 1.0);
        0.5 + 0.5 * u
    } else if s.on {
        let u = (-m / s.drop.max(1e-6)).clamp(0.0, 1.0);
        0.5 - 0.35 * u
    } else {
        0.5
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
        let (patch, saved) = (self.patch.clone(), self.saved());
        let extra: Vec<((String, String), f32)> = self
            .baseline
            .iter()
            .filter(|((b, p), _)| !base.iter().any(|x| x.id == *b && x.params.iter().any(|q| q.name == *p)))
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        self.rebase(&patch, &base, &saved);
        self.baseline.extend(extra);
    }

    /// The patch the bank belongs to.
    #[must_use]
    pub fn patch(&self) -> &str {
        &self.patch
    }

    /// Put `patch` on: its chain as built (`blocks` — the baseline, no
    /// macro applied) and its saved knob positions.
    pub fn rebase(&mut self, patch: &str, blocks: &[LiveBlock], saved: &[MacroValueDef]) {
        self.patch = patch.to_string();
        self.baseline = blocks
            .iter()
            .flat_map(|b| b.params.iter().map(move |p| ((b.id.clone(), p.name.clone()), p.value)))
            .collect();
        self.built = build(blocks);
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
        // Saved positions: bar knobs first (they set their children), then
        // the children moved on their own since.
        let is_parent = |id: &str| self.built.bank.knobs.iter().any(|k| k.id == id);
        let (parents, kids): (Vec<&MacroValueDef>, Vec<&MacroValueDef>) =
            saved.iter().partition(|d| is_parent(&d.id));
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
            let stages = journey(&on);
            rules.insert(parent.id.clone(), drive_rules(&ids, &stages));
            // Drive's children follow the journey rather than bindings.
            if self.built.panels.get(&parent.id).is_some_and(|p| p.journey) {
                journeys.insert(parent.id.clone(), stages);
            }
        }
        self.built.rules = rules;
        self.journeys = journeys;
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
        if let Some(parent) = self.built.bank.get_mut(id) {
            let v = parent.value;
            if let Some(stages) = stages {
                let m = offset_of(v, rest);
                for (child, s) in parent.children.iter_mut().zip(stages) {
                    child.set_value(drive_stage_value(m, s));
                }
            } else {
                let values: Vec<(String, f32)> = parent
                    .bindings
                    .iter()
                    .map(|b| (b.target.param_id.clone(), parent.compute_binding_value(b)))
                    .collect();
                for (cid, cv) in values {
                    if let Some(c) = parent.get_child_mut(&cid) {
                        c.set_value(cv);
                    }
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
    fn layers(&self, block: &str, param: &str) -> Vec<(Target, f32)> {
        let mut out = Vec::new();
        for parent in &self.built.bank.knobs {
            for k in std::iter::once(parent).chain(parent.children.iter()) {
                let Some(meta) = self.built.meta.get(&k.id) else { continue };
                let m = self.offset(&k.id);
                for t in meta.targets.iter().filter(|t| t.block == block && t.param == param) {
                    out.push((t.clone(), m));
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
        Some(self.layers(block, param).iter().fold(base, |v, (t, m)| t.apply(v, *m)))
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
        for (t, m) in layers.iter().rev() {
            match t.invert(v, *m) {
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

    /// The positions worth keeping with the patch: every knob off rest, and
    /// every pad pressed.
    #[must_use]
    pub fn saved(&self) -> Vec<MacroValueDef> {
        let mut out = Vec::new();
        for parent in &self.built.bank.knobs {
            for k in std::iter::once(parent).chain(parent.children.iter()) {
                let m = self.offset(&k.id);
                let pad = match self.pads.get(&k.id) {
                    Some(true) => "on",
                    Some(false) => "off",
                    None => "",
                };
                if m.abs() > 1e-4 || !pad.is_empty() {
                    out.push(MacroValueDef { id: k.id.clone(), value: m, pad: pad.to_string() });
                }
            }
        }
        out
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
        e.rebase("Test", &chain(), &[]);
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
    /// going up the off stages come in, in chain order, and every stage ends
    /// all the way driven; going down they drop out, last first, until none
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
        e.set("drive", 0.75);
        assert_eq!(byp(&e), vec![false, false, true], "then stage 2 comes in");
        e.set("drive", 0.9);
        assert_eq!(byp(&e), vec![false, false, false], "then stage 3");
        e.set("drive", 1.0);
        for id in ["d1", "d2", "d3"] {
            assert!(approx(e.live(id, "drive").unwrap(), 1.0), "{id} all the way driven");
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
        again.rebase("Test", &chain(), &saved);
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
        e.rebase("Sync", &[b], &[]);
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
        e.rebase("POG", &[pog], &[]);
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
        e.rebase("Flute", &[ice], &[]);
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
        let t = |curve| Target { block: "b".into(), param: "p".into(), lo: 20.0, hi: 2000.0, curve, dir: 1.0, depth: 1.0 };
        for curve in [Curve::Lin, Curve::Log, Curve::Add(12.0)] {
            let t = t(curve);
            for m in [-0.8, -0.3, 0.4, 0.9] {
                let live = t.apply(300.0, m);
                assert!((t.invert(live, m).unwrap() - 300.0).abs() < 0.5, "{curve:?} {m}");
            }
        }
    }
}
