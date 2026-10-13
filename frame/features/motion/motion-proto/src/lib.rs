//! The motion contract: animation as data on a document node.
//!
//! A [`Motion`] drives one property of the node it sits on
//! (`Node::motion`). Every frame the runtime reads its **input** (an
//! [expression](expr) over params, feeds, time, gestures and other
//! motions), runs it through its **dynamics** (a spring, a struck
//! resonator, an LFO, noise, ballistics, keyframes, spin — or nothing),
//! maps the result `0 → from`, `1 → to`, and writes it into the
//! **target** (translate, rotate, scale, stretch, skew, opacity, colour,
//! emission, dash offset, a path morph or a shape provider's geometry).
//!
//! ```json
//! {"target":"rotate","input":"norm(wah/position)","dynamics":{"spring":{"stiffness":300,"damping":14}},"from":-20,"to":20}
//! {"name":"ring","target":"none","dynamics":{"oscillator":{"freq":"4 + norm(spring/tension)*6","decay":"0.4 + 3*norm(spring/dwell)","excite":"level(spring/in)"}}}
//! {"target":"rotate","input":"0.5 + 1.5*norm(tape/speed)","dynamics":"spin","to":360}
//! {"target":"emission","input":"0.6 + 0.6*norm(tube/drive)","dynamics":{"noise":{"rate":9,"amount":"0.25*norm(tube/drive)"}}}
//! ```
//!
//! Everything here is plain data (`serde` + `facet`); the evaluator is
//! `motion-runtime`.

use std::collections::BTreeMap;

use facet::Facet;
use serde::{Deserialize, Serialize};

pub mod expr;

pub use expr::{Expr, ExprError, FeedRead, Func, Source};

fn one() -> f64 {
    1.0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_one(v: &f64) -> bool {
    *v == 1.0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(v: &bool) -> bool {
    !*v
}

/// One animated property of a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Motion {
    /// A name other motions read with `motion(name)` (looked up on the
    /// same node, then its ancestors, then anywhere).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub name: Option<String>,
    /// The property it writes.
    pub target: Target,
    /// The input [expression](expr), e.g. `norm(room/size)`,
    /// `level(spring/in)*2`, `time`. Empty = the dynamics' natural input:
    /// `time` for keyframes, `1` for LFO / noise / spin, `0` otherwise.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[facet(default)]
    pub input: String,
    /// How the output follows the input.
    #[serde(default, skip_serializing_if = "Dynamics::is_follow")]
    #[facet(default)]
    pub dynamics: Dynamics,
    /// The target value at output 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    #[facet(default)]
    pub from: f64,
    /// The target value at output 1 (outputs outside 0..1 extrapolate).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    #[facet(default = 1.0)]
    pub to: f64,
    /// Drive only the dynamics' deviation from the input (`output −
    /// input`), added to what the node already shows: layers a spring's
    /// overshoot on a part a control already moves (a needle, a treadle).
    #[serde(default, skip_serializing_if = "is_false")]
    #[facet(default)]
    pub lag: bool,
    /// Where rotate / scale / stretch / skew turn about, as fractions of
    /// the node's own box (default its centre).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[facet(default)]
    pub pivot: Option<Pivot>,
}

impl Motion {
    /// A motion on `target` with no input, instant dynamics, output 0..1.
    #[must_use]
    pub fn new(target: Target) -> Self {
        Self { name: None, target, input: String::new(), dynamics: Dynamics::Follow, from: 0.0, to: 1.0, lag: false, pivot: None }
    }

    /// The same motion on another target.
    #[must_use]
    pub fn on(mut self, target: Target) -> Self {
        self.target = target;
        self
    }

    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    #[must_use]
    pub fn input(mut self, expr: impl Into<String>) -> Self {
        self.input = expr.into();
        self
    }

    #[must_use]
    pub fn dynamics(mut self, d: Dynamics) -> Self {
        self.dynamics = d;
        self
    }

    /// Map output 0 → `from`, 1 → `to`.
    #[must_use]
    pub fn range(mut self, from: f64, to: f64) -> Self {
        self.from = from;
        self.to = to;
        self
    }

    #[must_use]
    pub fn lag(mut self) -> Self {
        self.lag = true;
        self
    }

    #[must_use]
    pub fn pivot(mut self, x: f64, y: f64) -> Self {
        self.pivot = Some(Pivot { x, y });
        self
    }

    /// The input expression the runtime evaluates (the dynamics' natural
    /// input when [`Self::input`] is empty).
    #[must_use]
    pub fn input_expr(&self) -> &str {
        if !self.input.trim().is_empty() {
            return &self.input;
        }
        match self.dynamics {
            Dynamics::Keyframes { .. } => "time",
            Dynamics::Lfo { .. } | Dynamics::Noise { .. } | Dynamics::Spin => "1",
            _ => "0",
        }
    }

    /// Every expression this motion evaluates, with where it sits (for
    /// lint messages): the input, the dynamics' bound numbers, a shape's
    /// inputs.
    #[must_use]
    pub fn expressions(&self) -> Vec<(String, &str)> {
        let mut out = vec![("input".to_string(), self.input_expr())];
        for (k, n) in self.dynamics.nums() {
            if let Num::Expr(e) = n {
                out.push((format!("dynamics.{k}"), e.as_str()));
            }
        }
        if let Some(e) = self.dynamics.excite() {
            out.push(("dynamics.excite".into(), e));
        }
        if let Target::Shape { inputs, .. } = &self.target {
            for (k, n) in inputs {
                if let Num::Expr(e) = n {
                    out.push((format!("target.inputs.{k}"), e.as_str()));
                }
            }
        }
        out
    }

    /// Structural problems (bad expressions, a morph with < 2 shapes, a
    /// colour mix with a bad colour, keyframes out of order) — not
    /// whether the names resolve; that needs a catalog (`design-lint`).
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (at, e) in self.expressions() {
            if let Err(err) = expr::parse(e) {
                out.push(format!("{at}: `{e}`: {err}"));
            }
        }
        match &self.target {
            Target::Fill { colors } | Target::Stroke { colors } => {
                if colors.is_empty() {
                    out.push("target: a colour mix needs at least one colour".into());
                }
                for c in colors {
                    if parse_hex(c).is_none() {
                        out.push(format!("target: `{c}` is not a colour (#rrggbb or #rrggbbaa)"));
                    }
                }
            }
            Target::Morph { shapes } if shapes.len() < 2 => out.push("target: a morph needs at least two shapes".into()),
            _ => {}
        }
        if let Dynamics::Keyframes { times, values, .. } = &self.dynamics {
            if times.is_empty() || times.len() != values.len() {
                out.push(format!("dynamics.keyframes: {} times but {} values", times.len(), values.len()));
            }
            if times.windows(2).any(|w| w[1] < w[0]) {
                out.push("dynamics.keyframes: times must not decrease".into());
            }
        }
        out
    }
}

/// A point in a node's box, as fractions of its width and height.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct Pivot {
    pub x: f64,
    pub y: f64,
}

impl Default for Pivot {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

/// The property a motion writes. Geometric targets compose onto the node's
/// transform (after any control has moved it), in the order the motions
/// are listed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Target {
    /// Writes nothing: a named signal other motions read (`motion(name)`).
    None,
    /// Move right by the value, px.
    X,
    /// Move down by the value, px.
    Y,
    /// Turn clockwise by the value, degrees, about the pivot.
    Rotate,
    /// Scale by the value (1 = as drawn) about the pivot.
    Scale,
    /// Stretch along x by the value (1 = as drawn), anchored at the pivot:
    /// a spring or coil pulled out.
    StretchX,
    /// Stretch along y by the value (1 = as drawn), anchored at the pivot.
    StretchY,
    /// Shear x by y, degrees.
    SkewX,
    /// Shear y by x, degrees.
    SkewY,
    /// Set opacity (0..1) — overrides a control's reading unless `lag`.
    Opacity,
    /// Solid fill colour: the value (0..1) blends along `colors`.
    Fill { colors: Vec<String> },
    /// Stroke colour: as `Fill`.
    Stroke { colors: Vec<String> },
    /// Scale the node's emission intensity by the value (a lamp without
    /// light stays dark).
    Emission,
    /// Shift the stroke's dash pattern by the value, px (marching ants,
    /// tape moving).
    DashOffset,
    /// Blend between keyed outlines: value 0 = `shapes[0]`, 1 =
    /// `shapes[1]`, … (paths with the same commands blend point by point;
    /// others switch at the midpoint).
    Morph { shapes: Vec<String> },
    /// Geometry from a registered shape provider (`coil`, `reverb.room`,
    /// `wave`, …) fed `inputs` (numbers or expressions), plus `value` —
    /// this motion's output. `part` picks one of the provider's outlines.
    Shape {
        provider: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[facet(default)]
        part: Option<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        #[facet(default)]
        inputs: BTreeMap<String, Num>,
    },
}

/// A number, or an expression evaluated each frame (a dynamics parameter
/// bound to a param: `"damping": "0.1 + norm(spring/damping)"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[serde(untagged)]
#[repr(u8)]
pub enum Num {
    Value(f64),
    Expr(String),
}

impl From<f64> for Num {
    fn from(v: f64) -> Self {
        Self::Value(v)
    }
}

impl From<&str> for Num {
    fn from(v: &str) -> Self {
        Self::Expr(v.into())
    }
}

impl From<String> for Num {
    fn from(v: String) -> Self {
        Self::Expr(v)
    }
}

/// How a motion's output follows its input. Every parameter is a [`Num`]:
/// a constant or an expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet, Default)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Dynamics {
    /// Output = input.
    #[default]
    Follow,
    /// A mass on a spring pulled toward the input: `stiffness` (N/m),
    /// `damping` (N·s/m), `mass` (kg). The defaults (170, 26, 1) settle
    /// quickly with no overshoot to speak of; lower damping bounces
    /// (critical damping is `2·√(stiffness·mass)`).
    Spring {
        #[serde(default = "Num::stiffness")]
        stiffness: Num,
        #[serde(default = "Num::damping")]
        damping: Num,
        #[serde(default = "Num::one")]
        mass: Num,
    },
    /// A struck resonator resting at the input: rings at `freq` Hz when
    /// `excite` rises (each rise kicks it by `gain` × the rise), dying away
    /// over `decay` seconds (to −60 dB) — or with damping ratio `damping`
    /// when `decay` is absent. A reverb spring hit by the input level.
    Oscillator {
        #[serde(default = "Num::freq")]
        freq: Num,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[facet(default)]
        decay: Option<Num>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[facet(default)]
        damping: Option<Num>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        #[facet(default)]
        excite: String,
        #[serde(default = "Num::one")]
        gain: Num,
    },
    /// input × a wave at `rate` Hz, 0..1 (`phase` in turns).
    Lfo {
        #[serde(default = "Num::one")]
        rate: Num,
        #[serde(default)]
        #[facet(default)]
        shape: Wave,
        #[serde(default, skip_serializing_if = "is_zero")]
        #[facet(default)]
        phase: f64,
    },
    /// input + `amount` × smooth noise (−1..1) changing at `rate` Hz:
    /// flicker, drift, hum.
    Noise {
        #[serde(default = "Num::rate")]
        rate: Num,
        #[serde(default = "Num::amount")]
        amount: Num,
    },
    /// A meter's movement: rises with time constant `attack`, falls with
    /// `release` (seconds).
    Ballistic {
        #[serde(default = "Num::attack")]
        attack: Num,
        #[serde(default = "Num::release")]
        release: Num,
    },
    /// The input (by default `time`, seconds) looked up in a keyed curve:
    /// `values[i]` at `times[i]`, eased between; `loop` wraps the input.
    Keyframes {
        times: Vec<f64>,
        values: Vec<f64>,
        #[serde(default)]
        #[facet(default)]
        easing: Easing,
        #[serde(default, rename = "loop", skip_serializing_if = "is_false")]
        #[facet(default)]
        looping: bool,
    },
    /// Turn continuously: the output advances by the input (turns per
    /// second) each second, wrapping at 1 — `to: 360` for degrees. Reels,
    /// fans, a rotating speaker.
    Spin,
}

impl Num {
    fn one() -> Self {
        Self::Value(1.0)
    }
    fn stiffness() -> Self {
        Self::Value(170.0)
    }
    fn damping() -> Self {
        Self::Value(26.0)
    }
    fn freq() -> Self {
        Self::Value(4.0)
    }
    fn rate() -> Self {
        Self::Value(8.0)
    }
    fn amount() -> Self {
        Self::Value(0.1)
    }
    fn attack() -> Self {
        Self::Value(0.01)
    }
    fn release() -> Self {
        Self::Value(0.3)
    }
}

impl Dynamics {
    #[must_use]
    pub fn is_follow(&self) -> bool {
        matches!(self, Self::Follow)
    }

    /// A spring with the default stiffness and mass and `damping`.
    #[must_use]
    pub fn spring(stiffness: impl Into<Num>, damping: impl Into<Num>) -> Self {
        Self::Spring { stiffness: stiffness.into(), damping: damping.into(), mass: Num::one() }
    }

    /// The bound numbers, by field name.
    #[must_use]
    pub fn nums(&self) -> Vec<(&'static str, &Num)> {
        match self {
            Self::Spring { stiffness, damping, mass } => vec![("stiffness", stiffness), ("damping", damping), ("mass", mass)],
            Self::Oscillator { freq, decay, damping, gain, .. } => {
                let mut v = vec![("freq", freq), ("gain", gain)];
                if let Some(d) = decay {
                    v.push(("decay", d));
                }
                if let Some(d) = damping {
                    v.push(("damping", d));
                }
                v
            }
            Self::Lfo { rate, .. } => vec![("rate", rate)],
            Self::Noise { rate, amount } => vec![("rate", rate), ("amount", amount)],
            Self::Ballistic { attack, release } => vec![("attack", attack), ("release", release)],
            Self::Follow | Self::Keyframes { .. } | Self::Spin => Vec::new(),
        }
    }

    /// An oscillator's excitation expression.
    #[must_use]
    pub fn excite(&self) -> Option<&str> {
        match self {
            Self::Oscillator { excite, .. } if !excite.trim().is_empty() => Some(excite),
            _ => None,
        }
    }
}

/// An LFO's wave, 0..1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet, Default)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Wave {
    /// 0 at phase 0, 1 at ½.
    #[default]
    Sine,
    Triangle,
    /// Ramp up.
    Saw,
    Square,
    /// A new random level each cycle.
    SampleHold,
}

/// Easing between keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet, Default)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Easing {
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Hold each value until the next key.
    Step,
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa` → straight RGBA 0..1.
#[must_use]
pub fn parse_hex(s: &str) -> Option<[f64; 4]> {
    let h = s.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok().map(|b| f64::from(b) / 255.0);
    match h.len() {
        3 => {
            let n = |i: usize| u8::from_str_radix(h.get(i..=i)?, 16).ok().map(|b| f64::from(b * 17) / 255.0);
            Some([n(0)?, n(1)?, n(2)?, 1.0])
        }
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 1.0]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_is_terse_and_round_trips() {
        let m = Motion::new(Target::Rotate)
            .input("norm(wah/position)")
            .dynamics(Dynamics::spring(300.0, "4 + 10*norm(wah/damp)"))
            .range(-20.0, 20.0)
            .pivot(0.5, 1.0);
        let j = serde_json::to_string(&m).unwrap();
        assert_eq!(
            j,
            r#"{"target":"rotate","input":"norm(wah/position)","dynamics":{"spring":{"stiffness":300.0,"damping":"4 + 10*norm(wah/damp)","mass":1.0}},"from":-20.0,"to":20.0,"pivot":{"x":0.5,"y":1.0}}"#
        );
        assert_eq!(serde_json::from_str::<Motion>(&j).unwrap(), m);
        // Hand-written: defaults fill in.
        let m: Motion = serde_json::from_str(r#"{"target":"opacity","dynamics":{"lfo":{"rate":2}}}"#).unwrap();
        assert_eq!(m.dynamics, Dynamics::Lfo { rate: Num::Value(2.0), shape: Wave::Sine, phase: 0.0 });
        assert_eq!(m.input_expr(), "1");
        assert_eq!((m.from, m.to), (0.0, 1.0));
        let m: Motion = serde_json::from_str(r#"{"target":"x","dynamics":"spin"}"#).unwrap();
        assert_eq!(m.dynamics, Dynamics::Spin);
        let m: Motion = serde_json::from_str(r#"{"target":{"shape":{"provider":"reverb.room","part":"rays","inputs":{"size":"norm(room/size)","n":3}}}}"#).unwrap();
        assert!(matches!(&m.target, Target::Shape { inputs, .. } if inputs["n"] == Num::Value(3.0)));
    }

    #[test]
    fn problems_name_the_bad_expression() {
        let m = Motion::new(Target::Opacity).input("norm(a/b").dynamics(Dynamics::Oscillator {
            freq: "2 +".into(),
            decay: None,
            damping: None,
            excite: "level(x/in)".into(),
            gain: Num::one(),
        });
        let p = m.problems();
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(p[0].starts_with("input"), "{p:?}");
        assert!(p[1].starts_with("dynamics.freq"), "{p:?}");
        assert!(Motion::new(Target::Fill { colors: vec!["#zz0000".into()] }).problems()[0].contains("not a colour"));
    }

    #[test]
    fn hex() {
        assert_eq!(parse_hex("#ff0000"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_hex("#fff"), Some([1.0, 1.0, 1.0, 1.0]));
        assert_eq!(parse_hex("#00000080").map(|c| (c[3] * 255.0).round()), Some(128.0));
        assert_eq!(parse_hex("red"), None);
    }
}
