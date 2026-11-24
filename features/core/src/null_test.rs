//! The null test: a block compiled in against its CLAP, with the same
//! macro automation (signal#26: "macros work 100% on either backend").
//!
//! [`NullCase::for_block`] makes, for one of Signal's blocks, a preset of
//! that block alone with every parameter on a macro: half of them set, half
//! modulated, so both `CLAP_EVENT_PARAM_VALUE` and `CLAP_EVENT_PARAM_MOD`
//! are exercised. Its automation moves the macros at frames that don't
//! fall on block boundaries, so sample accuracy is too. [`null`] renders
//! it twice, through two sets of factories, and reports how far apart the
//! outputs are.
//!
//! - **Compiled in**: `[&BuiltinFactory]`.
//! - **Its CLAP**: a factory that builds the block's CLAP form as
//!   [`Backend::Clap`]. On the desktop that's the CLAP factory over
//!   `signal-plugin-host`, loading the block's bundle from
//!   FastTrackStudios/plugins (the follow-up). Until then
//!   [`LoopbackClapFactory`] stands in: the compiled-in processor behind a
//!   [`ClapInstance`] that takes parameter events exactly as nice-plug's
//!   CLAP wrapper does (normalized values, modulation offsets, split at
//!   each event). It tests Signal's side of the boundary: the translation,
//!   the ids, the modulation arithmetic.
//!
//! A block that doesn't null is a bug in its CLAP form.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::block::{
    Backend, BlockFactory, BlockKind, BlockProcessor, BuildContext, Built, ClapInstance,
    ClapParamEvent, ClapParamKind, HostedPlugin, Unavailable,
};
use crate::engine::{HostParam, HostParamEvent, Processor};
use crate::graph::{BuildOptions, BuildReport};
use crate::host::{NoHost, SignalHost};
use crate::param::{MACROS, ParamSpec};
use crate::platform::Platforms;
use crate::state::{BlockSpec, LaneSpec, MacroMode, MacroSpec, MacroTarget, Preset};

/// Below this peak difference (dBFS) two renders null.
pub const NULL_DBFS: f32 = -100.0;

/// A macro moving, at a frame of the render.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MacroMove {
    pub frame: u32,
    pub index: u8,
    /// 0–1.
    pub value: f32,
}

/// What to render.
#[derive(Clone, Debug)]
pub struct NullCase {
    pub preset: Preset,
    pub automation: Vec<MacroMove>,
    /// Fed to every input channel.
    pub input: Vec<f32>,
    pub sample_rate: f32,
    /// The host's block size (the processor splits it further).
    pub host_block: usize,
}

/// How two renders compared.
#[derive(Clone, Debug)]
pub struct NullResult {
    /// Peak absolute difference, linear and in dBFS.
    pub peak_diff: f32,
    pub peak_diff_dbfs: f32,
    /// The first render's peak, so a silent render can't pass by accident.
    pub peak: f32,
    pub frames: usize,
    pub report_a: BuildReport,
    pub report_b: BuildReport,
}

impl NullResult {
    /// The two null, and the first isn't silence.
    #[must_use]
    pub fn nulls(&self) -> bool {
        self.peak > 1e-4 && self.peak_diff_dbfs < NULL_DBFS
    }
}

/// A test signal: a decaying, pitch-swept tone with transients, so gates
/// open, compressors work and tails ring.
#[must_use]
pub fn test_signal(frames: usize, sample_rate: f32) -> Vec<f32> {
    let mut phase = 0.0_f32;
    (0..frames)
        .map(|i| {
            let t = crate::util::count_f32(i) / sample_rate;
            let beat = t * 4.0 - libm::floorf(t * 4.0);
            let env = libm::expf(-beat * 8.0);
            phase += core::f32::consts::TAU * (110.0 + 220.0 * beat) / sample_rate;
            0.6 * env * libm::sinf(phase)
        })
        .collect()
}

impl NullCase {
    /// One of Signal's blocks alone, every parameter on a macro (set and
    /// modulated in turn), automated off the block grid. `None` for a block
    /// with no parameters, or more than the macros can drive at once (the
    /// rest are then driven in further cases by `offset`).
    #[must_use]
    pub fn for_block(
        name: &str,
        params: &'static [ParamSpec],
        offset: usize,
        asset: Option<crate::host::AssetRef>,
    ) -> Option<Self> {
        Self::for_algorithm(name, None, params, offset, asset)
    }

    /// [`for_block`](Self::for_block) on algorithm `algorithm` of family
    /// `name`, whose controls are `params`.
    #[must_use]
    pub fn for_algorithm(
        name: &str,
        algorithm: Option<&str>,
        params: &'static [ParamSpec],
        offset: usize,
        asset: Option<crate::host::AssetRef>,
    ) -> Option<Self> {
        let sample_rate = 48_000.0;
        let driven: Vec<(usize, &ParamSpec)> = params
            .iter()
            .enumerate()
            .skip(offset)
            .take(MACROS)
            .collect();
        if driven.is_empty() {
            return None;
        }
        let macros = driven
            .iter()
            .enumerate()
            .map(|(m, (_, p))| {
                let modulate = m % 2 == 1;
                MacroSpec {
                    name: alloc::format!("{}", p.id),
                    targets: vec![MacroTarget {
                        lane: "Main".into(),
                        block: "b".into(),
                        param: p.id.into(),
                        mode: if modulate {
                            MacroMode::Modulate
                        } else {
                            MacroMode::Set
                        },
                        // Set: the whole range. Modulate: ±a quarter of it.
                        min: if modulate { -0.25 } else { p.min },
                        max: if modulate { 0.25 } else { p.max },
                    }],
                }
            })
            .collect();
        let mut block = BlockSpec::signal("b", name);
        block.asset = asset;
        block.algorithm = algorithm.map(Into::into);
        let preset = Preset {
            name: alloc::format!("null: {name}"),
            inputs: 2,
            outputs: 2,
            lanes: vec![LaneSpec {
                name: "Main".into(),
                port: None,
                input: vec![0, 1],
                output: 0,
                gain_db: 0.0,
                mute: false,
                blocks: vec![block],
            }],
            macros,
        };
        let frames = 48_000_usize;
        // Every macro moves every 997 frames (prime: never on a block
        // boundary), each through its own pattern.
        let mut automation = Vec::new();
        let mut frame = 0_u32;
        let mut step = 0_u32;
        while usize::try_from(frame).unwrap_or(usize::MAX) < frames {
            for m in 0..driven.len() {
                let mi = u32::try_from(m).unwrap_or(0);
                let k = step.wrapping_mul(7).wrapping_add(mi.wrapping_mul(3)) % 11;
                automation.push(MacroMove {
                    frame,
                    index: u8::try_from(m).unwrap_or(0),
                    value: f32::from(u16::try_from(k).unwrap_or(0)) / 10.0,
                });
            }
            frame = frame.saturating_add(997);
            step = step.saturating_add(1);
        }
        Some(Self {
            preset,
            automation,
            input: test_signal(frames, sample_rate),
            sample_rate,
            host_block: 256,
        })
    }
}

/// Render `case` with `factories`: its outputs, per channel.
#[must_use]
pub fn render(
    case: &NullCase,
    factories: &[&dyn BlockFactory],
    host: &dyn SignalHost,
) -> (Vec<Vec<f32>>, BuildReport) {
    let opts = BuildOptions {
        sample_rate: case.sample_rate,
        max_block: case.host_block,
        port: None,
        factories,
        host,
    };
    let (graph, report) = crate::graph::build(&case.preset, &opts);
    let mut proc = Processor::new();
    let _ = proc.install(graph);
    let channels = usize::from(case.preset.outputs.max(case.preset.inputs)).max(1);
    let frames = case.input.len();
    let mut out: Vec<Vec<f32>> = vec![vec![0.0; frames]; channels];
    let mut events: Vec<HostParamEvent> = Vec::new();
    let mut start = 0_usize;
    while start < frames {
        let end = start.saturating_add(case.host_block).min(frames);
        for ch in &mut out {
            if let (Some(d), Some(s)) = (ch.get_mut(start..end), case.input.get(start..end)) {
                d.copy_from_slice(s);
            }
        }
        events.clear();
        for mv in &case.automation {
            let f = usize::try_from(mv.frame).unwrap_or(usize::MAX);
            if f >= start && f < end {
                events.push(HostParamEvent {
                    offset: u32::try_from(f.saturating_sub(start)).unwrap_or(0),
                    param: HostParam::Macro(mv.index),
                    value: mv.value,
                });
            }
        }
        events.sort_by_key(|e| e.offset);
        let mut io: Vec<&mut [f32]> = out
            .iter_mut()
            .filter_map(|c| c.get_mut(start..end))
            .collect();
        let inputs = usize::from(case.preset.inputs);
        proc.process(&mut io, inputs, end.saturating_sub(start), &[], &events);
        start = end;
    }
    (out, report)
}

/// Render `case` through `a` and through `b`, and compare.
#[must_use]
pub fn null(
    case: &NullCase,
    a: &[&dyn BlockFactory],
    b: &[&dyn BlockFactory],
    host: &dyn SignalHost,
) -> NullResult {
    let (out_a, report_a) = render(case, a, host);
    let (out_b, report_b) = render(case, b, host);
    let mut peak = 0.0_f32;
    let mut peak_diff = 0.0_f32;
    for (ca, cb) in out_a.iter().zip(&out_b) {
        for (x, y) in ca.iter().zip(cb) {
            peak = peak.max(x.abs());
            peak_diff = peak_diff.max((x - y).abs());
        }
    }
    let peak_diff_dbfs = if peak_diff > 0.0 {
        20.0 * libm::log10f(peak_diff)
    } else {
        f32::NEG_INFINITY
    };
    NullResult {
        peak_diff,
        peak_diff_dbfs,
        peak,
        frames: case.input.len(),
        report_a,
        report_b,
    }
}

/// Null every one of the plugins' blocks (each parameter on a macro),
/// compiled in against `clap`'s form of it. One result per case.
#[must_use]
pub fn null_every_block(clap: &dyn BlockFactory) -> Vec<(String, NullResult)> {
    let builtin = crate::block::BuiltinFactory;
    let mut results = Vec::new();
    for name in plugin_processors::NAMES {
        // Every algorithm of the family: a face over an engine, or an
        // engine's own DSP, each null-tested on its own controls.
        for algo in plugin_processors::algorithms::family(name) {
            let mut offset = 0;
            while let Some(case) =
                NullCase::for_algorithm(name, Some(algo.id), algo.controls, offset, None)
            {
                let r = null(&case, &[&builtin], &[clap], &NoHost);
                results.push((alloc::format!("{name}/{}[{offset}..]", algo.id), r));
                offset = offset.saturating_add(MACROS);
            }
        }
    }
    results
}

// ── The loopback CLAP ───────────────────────────────────────────────────

/// A compiled-in processor behind the CLAP boundary, taking events as
/// nice-plug's CLAP wrapper does: a normalized value per parameter, a
/// normalized modulation offset that replaces the last one, the value it
/// runs at `denormalize(value + modulation)`, and the block split at each
/// event (sample-accurate automation).
pub struct LoopbackClap {
    inner: Box<dyn BlockProcessor>,
    specs: &'static [ParamSpec],
    value: Vec<f64>,
    modulation: Vec<f64>,
}

impl LoopbackClap {
    #[must_use]
    pub fn new(inner: Box<dyn BlockProcessor>) -> Self {
        let specs = inner.params();
        Self {
            value: specs
                .iter()
                .map(|s| f64::from(s.normalize(s.default)))
                .collect(),
            modulation: vec![0.0; specs.len()],
            specs,
            inner,
        }
    }

    /// Take one event into the stored values (nice-plug: the parameter's
    /// normalized value, or its modulation offset, which replaces the last).
    fn take(&mut self, e: &ClapParamEvent) {
        let Ok(i) = usize::try_from(e.param_id) else {
            return;
        };
        let slot = match e.kind {
            ClapParamKind::Value(v) => self.value.get_mut(i).map(|x| (x, v)),
            ClapParamKind::Mod(m) => self.modulation.get_mut(i).map(|x| (x, m)),
        };
        if let Some((x, v)) = slot {
            *x = v;
        }
    }

    /// Set the processor to what the stored values now mean: nice-plug's
    /// `process()` reads each parameter's final value at a change point,
    /// so an intermediate (a value before its modulation) is never set.
    fn apply_all(&mut self) {
        for (i, ((spec, v), m)) in self
            .specs
            .iter()
            .zip(&self.value)
            .zip(&self.modulation)
            .enumerate()
        {
            let norm = signal_dsp_core::num::narrow((v + m).clamp(0.0, 1.0));
            self.inner.set_param(i, spec.denormalize(norm));
        }
    }
}

impl ClapInstance for LoopbackClap {
    fn process(&mut self, left: &mut [f32], right: &mut [f32], events: &[ClapParamEvent]) {
        let n = left.len().min(right.len());
        let mut pos = 0_usize;
        let mut events = events.iter().peekable();
        while pos < n {
            let at = u32::try_from(pos).unwrap_or(u32::MAX);
            let mut any = false;
            while let Some(e) = events.next_if(|e| e.offset <= at) {
                self.take(e);
                any = true;
            }
            if any {
                self.apply_all();
            }
            let next = events
                .peek()
                .and_then(|e| usize::try_from(e.offset).ok())
                .unwrap_or(n)
                .min(n)
                .max(pos.saturating_add(1));
            if let (Some(l), Some(r)) = (left.get_mut(pos..next), right.get_mut(pos..next)) {
                self.inner.process(l, r);
            }
            pos = next;
        }
    }
    fn reset(&mut self) {
        self.inner.reset();
    }
    fn latency(&self) -> u32 {
        self.inner.latency()
    }
    fn save_state(&mut self) -> Option<Vec<u8>> {
        None
    }
    fn load_state(&mut self, _state: &[u8]) -> bool {
        false
    }
    fn mod_source(&self, index: usize) -> Option<f32> {
        (index < self.inner.mod_sources().len()).then(|| self.inner.mod_source(index))
    }
}

/// Builds Signal's blocks as [`Backend::Clap`] over [`LoopbackClap`]: the
/// stand-in for the blocks' real CLAP forms in [`null`].
#[derive(Clone, Copy, Debug, Default)]
pub struct LoopbackClapFactory;

impl BlockFactory for LoopbackClapFactory {
    fn runs_on(&self, kind: &BlockKind) -> Option<Platforms> {
        match kind {
            BlockKind::Signal(name) if crate::blocks::exists(name) => Some(Platforms::ALL),
            _ => None,
        }
    }

    fn build(&self, spec: &BlockSpec, cx: &BuildContext<'_>) -> Result<Built, Unavailable> {
        let BlockKind::Signal(name) = &spec.kind else {
            return Err(Unavailable::UnknownKind);
        };
        let inner = crate::blocks::build(name, spec, cx)?;
        let params = inner.params();
        let sources = inner.mod_sources();
        // The CLAP form's ids for the block's spec indices. A real CLAP's
        // come from its parameter list, matched by the stable string id.
        let ids = (0..params.len())
            .filter_map(|i| u32::try_from(i).ok())
            .collect();
        let hosted = HostedPlugin::new(
            Box::new(LoopbackClap::new(inner)),
            ids,
            alloc::format!("loopback:{name}"),
        );
        Ok(Built {
            backend: Backend::Clap(hosted),
            params,
            sources,
        })
    }
}
