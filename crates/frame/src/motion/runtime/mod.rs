//! Runs the motions on a document's nodes.
//!
//! An [`Animator`] is compiled from a [`Scene`] (every `Node::motion`),
//! then driven by the host once per frame:
//!
//! 1. [`Animator::advance`]`(t, inputs)` — samples every source the
//!    motions read once (params, feeds, gestures; the host implements
//!    [`Inputs`]), evaluates each motion's input and bound dynamics
//!    parameters, and integrates the dynamics with a **fixed 1 ms step**
//!    (an accumulator carries the remainder, a stall is capped at 100 ms
//!    of simulation), so a spring behaves the same at 30 Hz and 240 Hz and
//!    an offline render at a fixed frame rate is deterministic.
//! 2. [`Animator::apply`]`(scene)` — writes each output into its node on
//!    the render copy: only animated nodes change.
//!
//! [`Animator::is_animating`] says whether another frame would differ (a
//! spring still moving, an LFO running); settled springs let hosts sleep.
//! [`Animator::swap`] rebuilds from an edited document and keeps the state
//! of every motion whose node (and motion slot and dynamics kind) is still
//! there — live reload doesn't restart a ringing spring.
//!
//! Clock rules: the first frame (and a frame earlier than the last one)
//! starts every motion *at rest on its input* — a spring at its target, a
//! resonator still — and puts time-driven ones (LFO, noise, spin) at the
//! phase `rate × t`, so a single offline frame at `--time T` is
//! deterministic. A source that appears (a feed's first frame) snaps the
//! motions reading it to rest there too.

use std::collections::{BTreeMap, HashMap};

use crate::document::proto::{Node, NodeId, NodeKind, Paint, PaintKind, Scene, Transform};
use crate::motion::proto::{Dynamics, Easing, Expr, Func, Motion, Num, Source, Target, Wave, expr};

mod morph;
pub mod shapes;

pub use shapes::{ShapeCtx, ShapeFn, ShapeRegistry};

/// The fixed integration step, seconds.
pub const STEP: f64 = 0.001;
/// The most simulated time one frame catches up on after a stall, seconds.
pub const MAX_CATCH_UP: f64 = 0.1;

/// What the host samples for the motions. Only called for sources some
/// motion reads, once per frame each.
pub trait Inputs {
    /// A param or feed reading; `None` while it has no value (a feed
    /// before its first frame, an address the engine doesn't have).
    fn source(&self, source: &Source) -> Option<f64>;
    /// Whether the pointer is over the control `node` belongs to.
    fn hover(&self, _node: NodeId) -> bool {
        false
    }
    /// Whether the control `node` belongs to is being pressed / dragged.
    fn press(&self, _node: NodeId) -> bool {
        false
    }
}

/// No sources at all: every param and feed is missing.
impl Inputs for () {
    fn source(&self, _: &Source) -> Option<f64> {
        None
    }
}

/// Sources from a map (tests, tools).
impl Inputs for HashMap<Source, f64> {
    fn source(&self, s: &Source) -> Option<f64> {
        self.get(s).copied()
    }
}

// ── compiled expressions ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Gesture {
    Hover,
    Press,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Source(Source),
    Gesture(Gesture, NodeId),
}

#[derive(Debug, Clone)]
enum C {
    Num(f64),
    Time,
    Sample(usize),
    /// Another motion's output (`None` = no such motion: missing).
    Motion(Option<usize>),
    Neg(Box<C>),
    Add(Box<C>, Box<C>),
    Sub(Box<C>, Box<C>),
    Mul(Box<C>, Box<C>),
    Div(Box<C>, Box<C>),
    Call(Func, Vec<C>),
}

struct Env<'a> {
    time: f64,
    samples: &'a [Option<f64>],
    outputs: &'a [Option<f64>],
}

impl C {
    fn eval(&self, env: &Env<'_>) -> Option<f64> {
        let bin = |a: &C, b: &C, f: fn(f64, f64) -> f64| Some(f(a.eval(env)?, b.eval(env)?));
        match self {
            C::Num(v) => Some(*v),
            C::Time => Some(env.time),
            C::Sample(i) => env.samples[*i],
            C::Motion(i) => i.and_then(|i| env.outputs[i]),
            C::Neg(a) => a.eval(env).map(|v| -v),
            C::Add(a, b) => bin(a, b, |x, y| x + y),
            C::Sub(a, b) => bin(a, b, |x, y| x - y),
            C::Mul(a, b) => bin(a, b, |x, y| x * y),
            C::Div(a, b) => bin(a, b, |x, y| if y.abs() < 1e-12 { 0.0 } else { x / y }),
            C::Call(Func::Or, args) => args.iter().find_map(|a| a.eval(env)),
            C::Call(f, args) => {
                let mut v = [0.0; 8];
                let n = args.len().min(8);
                for (slot, a) in v.iter_mut().zip(args) {
                    *slot = a.eval(env)?;
                }
                let r = f.apply(&v[..n]);
                r.is_finite().then_some(r)
            }
        }
    }
}

/// Compiling: sources interned into the animator's sample table, motion
/// names resolved to program indices.
struct Compiler<'a> {
    keys: &'a mut Vec<Key>,
    index: &'a mut HashMap<Key, usize>,
    node: NodeId,
    resolve: &'a dyn Fn(NodeId, &str) -> Option<usize>,
    deps: Vec<usize>,
}

impl Compiler<'_> {
    fn key(&mut self, k: Key) -> usize {
        if let Some(i) = self.index.get(&k) {
            return *i;
        }
        let i = self.keys.len();
        self.keys.push(k.clone());
        self.index.insert(k, i);
        i
    }

    fn compile(&mut self, e: &Expr) -> C {
        let b = |c: &mut Self, e: &Expr| Box::new(c.compile(e));
        match e {
            Expr::Num(v) => C::Num(*v),
            Expr::Time => C::Time,
            Expr::Hover => C::Sample(self.key(Key::Gesture(Gesture::Hover, self.node))),
            Expr::Press => C::Sample(self.key(Key::Gesture(Gesture::Press, self.node))),
            Expr::Src(s) => C::Sample(self.key(Key::Source(s.clone()))),
            Expr::Motion(name) => {
                let i = (self.resolve)(self.node, name);
                self.deps.extend(i);
                C::Motion(i)
            }
            Expr::Neg(a) => C::Neg(b(self, a)),
            Expr::Add(x, y) => C::Add(b(self, x), b(self, y)),
            Expr::Sub(x, y) => C::Sub(b(self, x), b(self, y)),
            Expr::Mul(x, y) => C::Mul(b(self, x), b(self, y)),
            Expr::Div(x, y) => C::Div(b(self, x), b(self, y)),
            Expr::Call(f, args) => C::Call(*f, args.iter().map(|a| self.compile(a)).collect()),
        }
    }

    /// A bad expression compiles to "missing" (lint reports it).
    fn text(&mut self, src: &str) -> (C, bool) {
        match expr::parse(src) {
            Ok(e) => (self.compile(&e), e.uses_time()),
            Err(_) => (C::Motion(None), false),
        }
    }

    fn num(&mut self, n: &Num) -> (C, bool) {
        match n {
            Num::Value(v) => (C::Num(*v), false),
            Num::Expr(s) => self.text(s),
        }
    }
}

// ── programs and their state ─────────────────────────────────────────────

struct Prog {
    node: NodeId,
    slot: usize,
    motion: Motion,
    input: C,
    /// The dynamics' bound numbers, in `Dynamics::nums` order.
    params: Vec<C>,
    excite: Option<C>,
    shape_inputs: Vec<(String, C)>,
    /// Some expression reads `time`.
    timed: bool,
    seed: u64,
}

impl Prog {
    fn sig(&self) -> u8 {
        dyn_sig(&self.motion.dynamics)
    }
}

fn dyn_sig(d: &Dynamics) -> u8 {
    match d {
        Dynamics::Follow => 0,
        Dynamics::Spring { .. } => 1,
        Dynamics::Oscillator { .. } => 2,
        Dynamics::Lfo { .. } => 3,
        Dynamics::Noise { .. } => 4,
        Dynamics::Ballistic { .. } => 5,
        Dynamics::Keyframes { .. } => 6,
        Dynamics::Spin => 7,
    }
}

#[derive(Debug, Clone, Default)]
struct State {
    started: bool,
    /// Position (spring, resonator, ballistic) or phase in turns (LFO,
    /// noise, spin).
    x: f64,
    v: f64,
    input: Option<f64>,
    excite: Option<f64>,
    settled: bool,
    shape: Vec<f64>,
}

/// Evaluates a scene's motions. See the crate docs.
pub struct Animator {
    progs: Vec<Prog>,
    /// Evaluation order: a motion after the motions it reads.
    order: Vec<usize>,
    keys: Vec<Key>,
    samples: Vec<Option<f64>>,
    states: Vec<State>,
    outputs: Vec<Option<f64>>,
    /// Programs per node, in the node's motion order.
    by_node: BTreeMap<NodeId, Vec<usize>>,
    clock: Option<f64>,
    acc: f64,
    animating: bool,
    shapes: ShapeRegistry,
    shape_cache: ShapeCache,
}

/// Memoised provider output per program: (inputs, node size, outlines).
type ShapeCache = HashMap<usize, (Vec<f64>, (f64, f64), Vec<String>)>;

impl std::fmt::Debug for Animator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Animator")
            .field("motions", &self.progs.len())
            .field("clock", &self.clock)
            .field("animating", &self.animating)
            .finish_non_exhaustive()
    }
}

impl Default for Animator {
    fn default() -> Self {
        Self::new(&Scene::new("empty"))
    }
}

impl Animator {
    /// Compile every motion in `scene` (builtin shape providers).
    #[must_use]
    pub fn new(scene: &Scene) -> Self {
        Self::with_shapes(scene, ShapeRegistry::builtin())
    }

    /// Compile with a shape registry of the host's.
    #[must_use]
    pub fn with_shapes(scene: &Scene, shapes: ShapeRegistry) -> Self {
        let mut a = Self {
            progs: Vec::new(),
            order: Vec::new(),
            keys: Vec::new(),
            samples: Vec::new(),
            states: Vec::new(),
            outputs: Vec::new(),
            by_node: BTreeMap::new(),
            clock: None,
            acc: 0.0,
            animating: false,
            shapes,
            shape_cache: HashMap::new(),
        };
        a.compile(scene);
        a
    }

    /// Register (or override) a shape provider.
    pub fn register_shape(&mut self, name: impl Into<String>, f: ShapeFn) {
        self.shapes.register(name, f);
        self.shape_cache.clear();
    }

    #[must_use]
    pub fn shapes(&self) -> &ShapeRegistry {
        &self.shapes
    }

    /// How many motions there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.progs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.progs.is_empty()
    }

    /// Whether the next frame would look different: a spring or resonator
    /// still moving, an LFO / noise / spin running, an input reading
    /// `time`. False before the first frame.
    #[must_use]
    pub fn is_animating(&self) -> bool {
        self.animating
    }

    /// Motion `slot` of `node`'s output (mapped), once it has one.
    #[must_use]
    pub fn output(&self, node: NodeId, slot: usize) -> Option<f64> {
        self.by_node
            .get(&node)
            .and_then(|v| v.get(slot))
            .and_then(|i| self.outputs[*i])
    }

    /// Rebuild from an edited scene, keeping each motion's state when its
    /// node, slot and dynamics kind are unchanged (and the clock).
    pub fn swap(&mut self, scene: &Scene) {
        let old: HashMap<(NodeId, usize), (u8, State, Option<f64>)> = self
            .progs
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    (p.node, p.slot),
                    (
                        p.sig(),
                        std::mem::take(&mut self.states[i]),
                        self.outputs[i],
                    ),
                )
            })
            .collect();
        self.compile(scene);
        // A scene left with no motions is still: nothing will step it to
        // say so (hosts skip an empty animator), so say it here.
        if self.progs.is_empty() {
            self.animating = false;
        }
        let mut old = old;
        for (i, p) in self.progs.iter().enumerate() {
            if let Some((sig, st, out)) = old.remove(&(p.node, p.slot))
                && sig == p.sig()
            {
                self.states[i] = st;
                self.outputs[i] = out;
            }
        }
        self.shape_cache.clear();
    }

    fn compile(&mut self, scene: &Scene) {
        self.progs.clear();
        self.keys.clear();
        self.by_node.clear();
        // Every (node, slot) with a motion, in document order.
        let order = scene.walk(scene.root);
        let mut slots: Vec<(NodeId, usize)> = Vec::new();
        for id in &order {
            if let Some(n) = scene.node(*id) {
                for s in 0..n.motion.len() {
                    slots.push((*id, s));
                }
            }
        }
        let index_of: HashMap<(NodeId, usize), usize> =
            slots.iter().enumerate().map(|(i, k)| (*k, i)).collect();
        // Names: on the node, then up its ancestors, then anywhere.
        let mut global: HashMap<&str, usize> = HashMap::new();
        for (i, (id, s)) in slots.iter().enumerate() {
            if let Some(name) = scene.node(*id).and_then(|n| n.motion[*s].name.as_deref()) {
                global.entry(name).or_insert(i);
            }
        }
        let resolve = |from: NodeId, name: &str| -> Option<usize> {
            let mut at = Some(from);
            while let Some(id) = at {
                let n = scene.node(id)?;
                if let Some(s) = n
                    .motion
                    .iter()
                    .position(|m| m.name.as_deref() == Some(name))
                {
                    return index_of.get(&(id, s)).copied();
                }
                at = n.parent;
            }
            global.get(name).copied()
        };
        let mut index = HashMap::new();
        let mut deps: Vec<Vec<usize>> = Vec::new();
        for (id, s) in &slots {
            let Some(node) = scene.node(*id) else {
                continue;
            };
            let m = node.motion[*s].clone();
            let mut c = Compiler {
                keys: &mut self.keys,
                index: &mut index,
                node: *id,
                resolve: &resolve,
                deps: Vec::new(),
            };
            let (input, mut timed) = c.text(m.input_expr());
            let mut params = Vec::new();
            for (_, n) in m.dynamics.nums() {
                let (p, t) = c.num(n);
                timed |= t;
                params.push(p);
            }
            let excite = m.dynamics.excite().map(|e| {
                let (x, t) = c.text(e);
                timed |= t;
                x
            });
            let shape_inputs = match &m.target {
                Target::Shape { inputs, .. } => inputs
                    .iter()
                    .map(|(k, n)| {
                        let (x, t) = c.num(n);
                        timed |= t;
                        (k.clone(), x)
                    })
                    .collect(),
                _ => Vec::new(),
            };
            deps.push(std::mem::take(&mut c.deps));
            let seed = seed_of(*id, *s);
            self.by_node.entry(*id).or_default().push(self.progs.len());
            self.progs.push(Prog {
                node: *id,
                slot: *s,
                motion: m,
                input,
                params,
                excite,
                shape_inputs,
                timed,
                seed,
            });
        }
        self.order = topo(&deps);
        self.samples = vec![None; self.keys.len()];
        self.states = vec![State::default(); self.progs.len()];
        self.outputs = vec![None; self.progs.len()];
        self.shape_cache.clear();
    }

    /// Advance to `t` seconds, sampling sources from `inputs`.
    pub fn advance(&mut self, t: f64, inputs: &dyn Inputs) {
        for (k, s) in self.keys.iter().zip(self.samples.iter_mut()) {
            *s = match k {
                Key::Source(src) => inputs.source(src).filter(|v| v.is_finite()),
                Key::Gesture(Gesture::Hover, n) => Some(f64::from(u8::from(inputs.hover(*n)))),
                Key::Gesture(Gesture::Press, n) => Some(f64::from(u8::from(inputs.press(*n)))),
            };
        }
        // The frame's elapsed time: full for phase-driven dynamics, fixed
        // steps (capped) for integrated ones. A fresh clock (or one that
        // went back) starts everything at rest.
        let restart = self.clock.is_none_or(|c| t < c - 1e-6);
        let dt = if restart {
            0.0
        } else {
            (t - self.clock.unwrap_or(t)).max(0.0)
        };
        if restart {
            for s in &mut self.states {
                s.started = false;
            }
            self.acc = 0.0;
        } else if self.animating {
            self.acc = (self.acc + dt).min(MAX_CATCH_UP);
        } else {
            // Everything was at rest: whatever changed starts moving now,
            // not as if it had been moving since the last frame.
            self.acc = 0.0;
        }
        let steps = (self.acc / STEP + 1e-9).floor();
        self.acc = (self.acc - steps * STEP).max(0.0);
        let steps = steps as u32;
        self.clock = Some(t);

        let mut animating = false;
        let order = std::mem::take(&mut self.order);
        for &i in &order {
            let (y, moving, shape) = {
                let env = Env {
                    time: t,
                    samples: &self.samples,
                    outputs: &self.outputs,
                };
                let p = &self.progs[i];
                let input = p.input.eval(&env);
                let params: Vec<Option<f64>> = p.params.iter().map(|c| c.eval(&env)).collect();
                let excite = p.excite.as_ref().and_then(|c| c.eval(&env));
                let shape: Vec<f64> = p
                    .shape_inputs
                    .iter()
                    .map(|(_, c)| c.eval(&env).unwrap_or(f64::NAN))
                    .collect();
                let st = &mut self.states[i];
                let (y, moving) = step(
                    &p.motion.dynamics,
                    st,
                    Step {
                        input,
                        params: &params,
                        excite,
                        dt,
                        steps,
                        t,
                        seed: p.seed,
                    },
                );
                (
                    y.map(|y| {
                        if p.motion.lag {
                            y - input.unwrap_or(0.0)
                        } else {
                            y
                        }
                    }),
                    moving || p.timed,
                    shape,
                )
            };
            let p = &self.progs[i];
            animating |= moving;
            self.states[i].shape = shape;
            self.outputs[i] = y.map(|y| p.motion.from + y * (p.motion.to - p.motion.from));
        }
        self.order = order;
        self.animating = animating;
    }

    /// Write every output into its node in `scene` (the render copy).
    /// Geometric targets compose onto the node's current transform (after
    /// controls), in motion order.
    pub fn apply(&mut self, scene: &mut Scene) {
        for (node_id, progs) in &self.by_node {
            let Some(node) = scene.nodes.get_mut(node_id) else {
                continue;
            };
            for &i in progs {
                let Some(v) = self.outputs[i] else { continue };
                let p = &self.progs[i];
                apply_target(
                    node,
                    &p.motion,
                    v,
                    i,
                    &self.states[i].shape,
                    &self.shapes,
                    &mut self.shape_cache,
                    &p.shape_inputs,
                );
            }
        }
    }
}

/// Topological order over "reads" edges (cycles fall back to document
/// order: the later motion reads the earlier one's previous frame).
fn topo(deps: &[Vec<usize>]) -> Vec<usize> {
    let n = deps.len();
    let mut state = vec![0u8; n];
    let mut out = Vec::with_capacity(n);
    fn visit(i: usize, deps: &[Vec<usize>], state: &mut [u8], out: &mut Vec<usize>) {
        if state[i] != 0 {
            return;
        }
        state[i] = 1;
        for &d in &deps[i] {
            if d != i && state[d] == 0 {
                visit(d, deps, state, out);
            }
        }
        state[i] = 2;
        out.push(i);
    }
    for i in 0..n {
        visit(i, deps, &mut state, &mut out);
    }
    out
}

fn seed_of(id: NodeId, slot: usize) -> u64 {
    let b = id.0.as_u128();
    hash64(
        (b as u64)
            ^ ((b >> 64) as u64).rotate_left(17)
            ^ (slot as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
    )
}

fn hash64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

/// A lattice value in −1..1.
fn lattice(seed: u64, i: i64) -> f64 {
    (hash64(seed ^ (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)) >> 11) as f64
        / (1u64 << 53) as f64
        * 2.0
        - 1.0
}

/// Smooth value noise, −1..1, one new value per unit of `x`.
fn noise(seed: u64, x: f64) -> f64 {
    let n1 = |x: f64, s: u64| {
        let i = x.floor();
        let f = x - i;
        let f = f * f * (3.0 - 2.0 * f);
        let a = lattice(s, i as i64);
        let b = lattice(s, i as i64 + 1);
        a + (b - a) * f
    };
    (0.7 * n1(x, seed) + 0.3 * n1(x * 2.7 + 13.1, seed ^ 0xabcd)).clamp(-1.0, 1.0)
}

fn wave(shape: Wave, phase: f64, seed: u64) -> f64 {
    let p = phase - phase.floor();
    match shape {
        Wave::Sine => 0.5 - 0.5 * (std::f64::consts::TAU * p).cos(),
        Wave::Triangle => 1.0 - (2.0 * p - 1.0).abs(),
        Wave::Saw => p,
        Wave::Square => f64::from(u8::from(p >= 0.5)),
        Wave::SampleHold => lattice(seed, phase.floor() as i64) * 0.5 + 0.5,
    }
}

fn ease(e: Easing, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match e {
        Easing::Linear => t,
        Easing::EaseIn => t * t * t,
        Easing::EaseOut => 1.0 - (1.0 - t).powi(3),
        Easing::EaseInOut => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        Easing::Step => 0.0,
    }
}

fn keyframes(times: &[f64], values: &[f64], easing: Easing, looping: bool, x: f64) -> f64 {
    let n = times.len().min(values.len());
    if n == 0 {
        return 0.0;
    }
    let (t0, t1) = (times[0], times[n - 1]);
    let x = if looping && t1 > t0 {
        t0 + (x - t0).rem_euclid(t1 - t0)
    } else {
        x
    };
    if x <= t0 {
        return values[0];
    }
    if x >= t1 {
        return values[n - 1];
    }
    let k = times[..n].partition_point(|t| *t <= x).max(1);
    let (a, b) = (times[k - 1], times[k]);
    let u = if b > a { (x - a) / (b - a) } else { 1.0 };
    values[k - 1] + (values[k] - values[k - 1]) * ease(easing, u)
}

struct Step<'a> {
    input: Option<f64>,
    params: &'a [Option<f64>],
    excite: Option<f64>,
    /// The frame's elapsed seconds (phase-driven dynamics).
    dt: f64,
    /// Fixed steps to integrate (integrated dynamics).
    steps: u32,
    t: f64,
    seed: u64,
}

impl Step<'_> {
    fn param(&self, i: usize, default: f64) -> f64 {
        self.params
            .get(i)
            .copied()
            .flatten()
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }
}

const EPS: f64 = 1e-4;

/// One frame of one motion's dynamics: its output (before lag/mapping;
/// `None` while it has never had an input) and whether it's still moving.
fn step(d: &Dynamics, st: &mut State, s: Step<'_>) -> (Option<f64>, bool) {
    // A source that appears (or a fresh clock) starts at rest on it.
    let fresh = !st.started || (st.input.is_none() && s.input.is_some());
    st.input = s.input.or(st.input);
    let Some(input) = st.input else {
        return (None, false);
    };
    let h = STEP;
    let n = s.steps;
    match d {
        Dynamics::Follow => {
            st.started = true;
            (Some(input), false)
        }
        Dynamics::Spring { .. } => {
            let k = s.param(0, 170.0).max(0.0);
            let c = s.param(1, 26.0).max(0.0);
            let m = s.param(2, 1.0).max(1e-6);
            // Keep the explicit step stable: ω·h and (c/m)·h below ~1.8.
            let (k, c) = ((k / m).min((1.8 / h).powi(2)), (c / m).min(1.8 / h));
            if fresh {
                (st.x, st.v, st.started) = (input, 0.0, true);
            }
            for _ in 0..n {
                let a = k * (input - st.x) - c * st.v;
                st.v += a * h;
                st.x += st.v * h;
            }
            let scale = input.abs().max(1.0);
            st.settled = (st.x - input).abs() < EPS * scale && st.v.abs() < EPS * scale;
            if st.settled {
                (st.x, st.v) = (input, 0.0);
            }
            (Some(st.x), !st.settled)
        }
        Dynamics::Oscillator { decay, .. } => {
            let f = s.param(0, 4.0).clamp(0.0, 250.0);
            let gain = s.param(1, 1.0);
            let w = std::f64::consts::TAU * f;
            // `decay` (T60, s) wins over `damping` (ζ); params after
            // freq, gain are decay then damping, as present.
            let mut idx = 2;
            let zeta = match decay {
                Some(_) => {
                    let t60 = s.param(idx, 1.0).max(1e-3);
                    idx += 1;
                    6.907_755 / (w.max(1e-6) * t60)
                }
                None => s.param(idx, 0.05),
            };
            let _ = idx;
            let zw = (zeta * w).clamp(0.0, 0.9 / h);
            if fresh {
                (st.x, st.v, st.started) = (input, 0.0, true);
                st.excite = s.excite;
            } else if let Some(e) = s.excite {
                let rise = (e - st.excite.unwrap_or(e)).max(0.0);
                st.v += gain * rise * w;
                st.excite = Some(e);
            }
            for _ in 0..n {
                let a = -w * w * (st.x - input) - 2.0 * zw * st.v;
                st.v += a * h;
                st.x += st.v * h;
            }
            // Settled when the ring's energy is below the threshold.
            let amp = ((st.x - input).powi(2) + (st.v / w.max(1e-6)).powi(2)).sqrt();
            st.settled = amp < EPS;
            if st.settled {
                (st.x, st.v) = (input, 0.0);
            }
            (Some(st.x), !st.settled)
        }
        Dynamics::Lfo { shape, phase, .. } => {
            let rate = s.param(0, 1.0);
            if fresh {
                (st.x, st.started) = (rate * s.t, true);
            } else {
                st.x = (st.x + rate * s.dt).rem_euclid(1e6);
            }
            (
                Some(input * wave(*shape, st.x + phase, s.seed)),
                rate != 0.0 && input != 0.0,
            )
        }
        Dynamics::Noise { .. } => {
            let rate = s.param(0, 8.0);
            let amount = s.param(1, 0.1);
            if fresh {
                (st.x, st.started) = (rate * s.t, true);
            } else {
                st.x = (st.x + rate * s.dt).rem_euclid(1e6);
            }
            (
                Some(input + amount * noise(s.seed, st.x)),
                rate != 0.0 && amount != 0.0,
            )
        }
        Dynamics::Ballistic { .. } => {
            let attack = s.param(0, 0.01).max(1e-4);
            let release = s.param(1, 0.3).max(1e-4);
            if fresh {
                (st.x, st.started) = (input, true);
            }
            let (ka, kr) = (1.0 - (-h / attack).exp(), 1.0 - (-h / release).exp());
            for _ in 0..n {
                let k = if input > st.x { ka } else { kr };
                st.x += (input - st.x) * k;
            }
            st.settled = (st.x - input).abs() < EPS * input.abs().max(1.0);
            if st.settled {
                st.x = input;
            }
            (Some(st.x), !st.settled)
        }
        Dynamics::Keyframes {
            times,
            values,
            easing,
            looping,
        } => {
            st.started = true;
            (
                Some(keyframes(times, values, *easing, *looping, input)),
                false,
            )
        }
        Dynamics::Spin => {
            if fresh {
                (st.x, st.started) = ((input * s.t).rem_euclid(1.0), true);
            } else {
                st.x = (st.x + input * s.dt).rem_euclid(1.0);
            }
            (Some(st.x), input != 0.0)
        }
    }
}

// ── targets ──────────────────────────────────────────────────────────────

/// `p ∘ q`: apply `q`, then `p`.
fn compose(p: &Transform, q: &Transform) -> Transform {
    Transform {
        a: p.a * q.a + p.b * q.c,
        b: p.a * q.b + p.b * q.d,
        tx: p.a * q.tx + p.b * q.ty + p.tx,
        c: p.c * q.a + p.d * q.c,
        d: p.c * q.b + p.d * q.d,
        ty: p.c * q.tx + p.d * q.ty + p.ty,
    }
}

/// `lin` about the local point (`px`, `py`), then the node's transform.
fn about(t: &Transform, (a, b, c, d): (f64, f64, f64, f64), (px, py): (f64, f64)) -> Transform {
    let l = Transform {
        a,
        b,
        tx: px - (a * px + b * py),
        c,
        d,
        ty: py - (c * px + d * py),
    };
    compose(t, &l)
}

fn mix(colors: &[String], v: f64) -> Option<crate::document::proto::Color> {
    let cs: Vec<[f64; 4]> = colors
        .iter()
        .filter_map(|c| crate::motion::proto::parse_hex(c))
        .collect();
    let (first, last) = (cs.first()?, cs.last()?);
    let c = if cs.len() == 1 || v <= 0.0 {
        *first
    } else if v >= 1.0 {
        *last
    } else {
        let x = v * (cs.len() - 1) as f64;
        let i = (x.floor() as usize).min(cs.len() - 2);
        let f = x - i as f64;
        let (a, b) = (cs[i], cs[i + 1]);
        [
            a[0] + (b[0] - a[0]) * f,
            a[1] + (b[1] - a[1]) * f,
            a[2] + (b[2] - a[2]) * f,
            a[3] + (b[3] - a[3]) * f,
        ]
    };
    Some(crate::document::proto::Color::rgba(c[0], c[1], c[2], c[3]))
}

fn recolor(paints: &mut Vec<Paint>, color: crate::document::proto::Color) {
    let mut hit = false;
    for p in paints.iter_mut() {
        if let PaintKind::Solid { color: c } = &mut p.kind {
            *c = color;
            hit = true;
        }
    }
    if !hit {
        *paints = vec![Paint::solid(color)];
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_target(
    node: &mut Node,
    m: &Motion,
    v: f64,
    i: usize,
    shape_args: &[f64],
    shapes: &ShapeRegistry,
    cache: &mut ShapeCache,
    names: &[(String, C)],
) {
    let pivot = m.pivot.unwrap_or_default();
    let p = (pivot.x * node.size.width, pivot.y * node.size.height);
    let factor = if m.lag { 1.0 + v } else { v };
    match &m.target {
        Target::None => {}
        Target::X => node.transform.tx += v,
        Target::Y => node.transform.ty += v,
        Target::Rotate => {
            let (s, c) = v.to_radians().sin_cos();
            node.transform = about(&node.transform, (c, -s, s, c), p);
        }
        Target::Scale => node.transform = about(&node.transform, (factor, 0.0, 0.0, factor), p),
        Target::StretchX => node.transform = about(&node.transform, (factor, 0.0, 0.0, 1.0), p),
        Target::StretchY => node.transform = about(&node.transform, (1.0, 0.0, 0.0, factor), p),
        Target::SkewX => {
            node.transform = about(&node.transform, (1.0, v.to_radians().tan(), 0.0, 1.0), p)
        }
        Target::SkewY => {
            node.transform = about(&node.transform, (1.0, 0.0, v.to_radians().tan(), 1.0), p)
        }
        Target::Opacity => {
            node.opacity = (if m.lag { node.opacity + v } else { v }).clamp(0.0, 1.0)
        }
        Target::Fill { colors } => {
            if let Some(c) = mix(colors, v) {
                recolor(&mut node.fills, c);
            }
        }
        Target::Stroke { colors } => {
            if let Some(c) = mix(colors, v) {
                recolor(&mut node.stroke.paints, c);
            }
        }
        Target::Emission => {
            if let Some(e) = node.emission.as_mut() {
                e.intensity = (if m.lag {
                    e.intensity + v
                } else {
                    e.intensity * v
                })
                .max(0.0);
            }
        }
        Target::DashOffset => node.stroke.dash_offset += v,
        Target::Morph { shapes: keyed } => {
            if let Some(d) = morph::blend(keyed, v) {
                set_geometry(node, vec![d]);
            }
        }
        Target::Shape { provider, part, .. } => {
            let size = (node.size.width, node.size.height);
            let mut key: Vec<f64> = shape_args.to_vec();
            key.push(v);
            if let Some((k, s, g)) = cache.get(&i)
                && *k == key
                && *s == size
            {
                let g = g.clone();
                set_geometry(node, g);
                return;
            }
            let mut args: Vec<(&str, f64)> = names
                .iter()
                .map(|(k, _)| k.as_str())
                .zip(shape_args.iter().copied())
                .filter(|(_, v)| v.is_finite())
                .collect();
            args.push(("value", v));
            let ctx = ShapeCtx {
                part: part.as_deref().unwrap_or(""),
                width: size.0,
                height: size.1,
                args: &args,
            };
            if let Some(g) = shapes.get(provider).and_then(|f| f(&ctx)) {
                cache.insert(i, (key, size, g.clone()));
                set_geometry(node, g);
            }
        }
    }
}

fn set_geometry(node: &mut Node, g: Vec<String>) {
    if !matches!(node.kind, NodeKind::Vector) {
        node.kind = NodeKind::Vector;
    }
    node.fill_geometry = g;
}

#[cfg(test)]
mod tests;
