//! A preset built into blocks: lanes from input channels to output
//! channels. Built off the audio thread (everything is allocated here);
//! processed on it without allocating.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::block::{
    Backend, BlockFactory, BlockKind, BlockStatus, BuildContext, Built, ChainBlock, Midi,
    ParamBank, ParamChange, Passthrough, Unavailable,
};
use crate::host::SignalHost;
use crate::param::MACROS;
use crate::platform::Platforms;
use crate::state::{BlockSpec, LaneSpec, MacroMode, Preset};

/// What a graph is built for.
pub struct BuildOptions<'a> {
    pub sample_rate: f32,
    /// The longest piece the processor hands a block (≤ [`crate::MAX_BLOCK`]).
    pub max_block: usize,
    /// The port this instance is, in the per-track mode; `None` for the
    /// multi-channel mode.
    pub port: Option<&'a str>,
    /// Asked in turn for each block. Put [`crate::BuiltinFactory`] in.
    pub factories: &'a [&'a dyn BlockFactory],
    pub host: &'a dyn SignalHost,
}

/// One block's outcome: what the UI lists, and what usage is logged from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockReport {
    pub lane: String,
    pub block: String,
    pub kind: BlockKind,
    /// `linked` or `clap`.
    pub backend: String,
    #[serde(flatten)]
    pub status: BlockStatus,
}

/// What building a preset came to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildReport {
    pub blocks: Vec<BlockReport>,
    /// Things in the preset that were ignored (an unknown parameter, a
    /// macro target that names no block).
    pub warnings: Vec<String>,
}

impl BuildReport {
    /// Every block runs as the preset has it.
    #[must_use]
    pub fn all_active(&self) -> bool {
        self.blocks.iter().all(|b| b.status == BlockStatus::Active)
    }
}

struct Lane {
    input: Vec<usize>,
    output: usize,
    gain: f32,
    mute: bool,
    blocks: Vec<ChainBlock>,
    l: Vec<f32>,
    r: Vec<f32>,
}

struct MacroBinding {
    lane: usize,
    block: usize,
    param: usize,
    mode: MacroMode,
    min: f32,
    max: f32,
    /// This binding's current modulation offset (Modulate), so a change
    /// replaces its own contribution and keeps the others'.
    applied: f32,
}

/// A built preset.
pub struct Graph {
    max_block: usize,
    lanes: Vec<Lane>,
    macros: [Vec<MacroBinding>; MACROS],
    /// The inputs, copied out before the in-place outputs are written.
    input_copy: Vec<Vec<f32>>,
    latency: u32,
}

/// The lanes of `preset` an instance runs, in the order its graph holds
/// them: every lane in the multi-channel mode; in the per-track mode the
/// lanes of `port`, or the lanes with no port when it has none.
#[must_use]
pub fn lanes_for<'p>(preset: &'p Preset, port: Option<&str>) -> Vec<&'p LaneSpec> {
    match port {
        None => preset.lanes.iter().collect(),
        Some(port) => {
            let own: Vec<_> = preset
                .lanes
                .iter()
                .filter(|l| l.port.as_deref() == Some(port))
                .collect();
            if own.is_empty() {
                preset.lanes.iter().filter(|l| l.port.is_none()).collect()
            } else {
                own
            }
        }
    }
}

/// Where every block of `kind` is in the graph [`build`] makes of `preset`
/// for `port`, as `(lane, block)`: what the control side sends
/// [`Command::SwapBackend`](crate::engine::Command::SwapBackend) to when a
/// block is overridden by its CLAP ("every instance of that block").
#[must_use]
pub fn positions_of(preset: &Preset, port: Option<&str>, kind: &BlockKind) -> Vec<(usize, usize)> {
    lanes_for(preset, port)
        .iter()
        .enumerate()
        .flat_map(|(l, lane)| {
            lane.blocks
                .iter()
                .enumerate()
                .filter(|(_, b)| &b.kind == kind)
                .map(move |(b, _)| (l, b))
        })
        .collect()
}

/// Build `preset`.
#[must_use]
pub fn build(preset: &Preset, opts: &BuildOptions<'_>) -> (Graph, BuildReport) {
    let max_block = opts.max_block.clamp(1, crate::MAX_BLOCK);
    let cx = BuildContext {
        sample_rate: opts.sample_rate,
        max_block,
        host: opts.host,
    };
    let mut report = BuildReport::default();

    let lanes_here = lanes_for(preset, opts.port);

    let mut lanes = Vec::with_capacity(lanes_here.len());
    for spec in &lanes_here {
        let (input, output) = if opts.port.is_some() {
            // The track's own channels.
            let input = match spec.input.len() {
                0 => Vec::new(),
                1 => vec![0],
                _ => vec![0, 1],
            };
            (input, 0)
        } else {
            (
                spec.input.iter().map(|c| usize::from(*c)).collect(),
                usize::from(spec.output),
            )
        };
        let mut blocks = Vec::with_capacity(spec.blocks.len());
        for b in &spec.blocks {
            let block = build_block(b, &cx, opts.factories, &mut report.warnings, &spec.name);
            log_usage(b, block.status());
            report.blocks.push(BlockReport {
                lane: spec.name.clone(),
                block: b.id.clone(),
                kind: b.kind.clone(),
                backend: if block.backend().is_linked() {
                    "linked".into()
                } else {
                    "clap".into()
                },
                status: block.status().clone(),
            });
            blocks.push(block);
        }
        lanes.push(Lane {
            input,
            output,
            gain: crate::util::db_to_gain(spec.gain_db),
            mute: spec.mute,
            blocks,
            l: vec![0.0; max_block],
            r: vec![0.0; max_block],
        });
    }

    let mut macros: [Vec<MacroBinding>; MACROS] = core::array::from_fn(|_| Vec::new());
    for (m, spec) in preset.macros.iter().enumerate() {
        let Some(bindings) = macros.get_mut(m) else {
            report.warnings.push(alloc::format!(
                "macro {} beyond the {MACROS} macros: ignored",
                spec.name
            ));
            continue;
        };
        for t in &spec.targets {
            let Some(lane) = lanes_here.iter().position(|l| l.name == t.lane) else {
                // A lane of another port: not this instance's.
                continue;
            };
            let found = lanes.get(lane).and_then(|l: &Lane| {
                let block = l.blocks.iter().position(|b| b.id == t.block)?;
                let param = l.blocks.get(block)?.params().index(&t.param)?;
                Some((block, param))
            });
            match found {
                Some((block, param)) => {
                    bindings.push(MacroBinding {
                        lane,
                        block,
                        param,
                        mode: t.mode,
                        min: t.min,
                        max: t.max,
                        applied: 0.0,
                    });
                }
                None => report.warnings.push(alloc::format!(
                    "macro {}: no {}/{}/{} here",
                    spec.name,
                    t.lane,
                    t.block,
                    t.param
                )),
            }
        }
    }

    let inputs = lanes
        .iter()
        .flat_map(|l| l.input.iter().copied())
        .max()
        .map_or(0, |m| m.saturating_add(1));
    let latency = lanes
        .iter()
        .map(|l| {
            l.blocks
                .iter()
                .filter(|b| !b.bypass)
                .map(ChainBlock::latency)
                .fold(0_u32, u32::saturating_add)
        })
        .max()
        .unwrap_or(0);
    let graph = Graph {
        max_block,
        lanes,
        macros,
        input_copy: vec![vec![0.0; max_block]; inputs],
        latency,
    };
    (graph, report)
}

fn log_usage(spec: &BlockSpec, status: &BlockStatus) {
    // Which third-party blocks presets use: a block that keeps showing up
    // is the next one Signal builds itself.
    if let BlockKind::Plugin(p) = &spec.kind {
        tracing::info!(
            target: "signal::usage",
            plugin_title = %p.name,
            plugin_id = %p.id,
            plugin_format = ?p.format,
            plugin_version = p.version.as_deref().unwrap_or(""),
            block_status = ?status,
            "third-party block in a preset"
        );
    }
}

fn try_build(
    spec: &BlockSpec,
    cx: &BuildContext<'_>,
    factories: &[&dyn BlockFactory],
) -> Result<Built, Unavailable> {
    let required = spec.requires.unwrap_or(Platforms::ALL);
    let Some((factory, runs_on)) = factories
        .iter()
        .find_map(|f| f.runs_on(&spec.kind).map(|p| (*f, p)))
    else {
        return Err(match &spec.kind {
            // No host for this format in this build: say where it would run.
            BlockKind::Plugin(p) if !p.format.platforms().intersection(required).here() => {
                Unavailable::Platform {
                    needs: p.format.platforms().intersection(required),
                }
            }
            _ => Unavailable::UnknownKind,
        });
    };
    let needs = runs_on.intersection(required);
    if !needs.here() {
        return Err(Unavailable::Platform { needs });
    }
    factory.build(spec, cx)
}

fn build_block(
    spec: &BlockSpec,
    cx: &BuildContext<'_>,
    factories: &[&dyn BlockFactory],
    warnings: &mut Vec<String>,
    lane: &str,
) -> ChainBlock {
    let (built, status, params_of) = match try_build(spec, cx, factories) {
        Ok(b) => (b, BlockStatus::Active, spec),
        Err(reason) => match spec
            .fallback
            .as_deref()
            .map(|fb| (fb, try_build(fb, cx, factories)))
        {
            Some((fb, Ok(b))) => (
                b,
                BlockStatus::Fallback {
                    reason,
                    fallback: fb.kind.clone(),
                },
                fb,
            ),
            _ => {
                tracing::warn!(block.lane = lane, block.id = %spec.id, block.kind = %spec.kind, block.reason = %reason, "block bypassed");
                let pass = Built {
                    backend: Backend::Linked(Box::new(Passthrough)),
                    params: &[],
                    sources: &[],
                };
                (pass, BlockStatus::Bypassed { reason }, spec)
            }
        },
    };
    let bank = ParamBank::new(built.params);
    let mut values = Vec::new();
    for (id, value) in &params_of.params {
        match bank.index(id) {
            Some(i) => values.push((i, *value)),
            None if !matches!(status, BlockStatus::Bypassed { .. }) => {
                warnings.push(alloc::format!("{lane}/{}: no parameter {id}", spec.id));
            }
            None => {}
        }
    }
    let mut block = ChainBlock::new(
        spec.id.clone(),
        params_of.kind.clone(),
        bank,
        built.sources,
        built.backend,
        status,
    );
    for (i, v) in values {
        block.apply(i, ParamChange::Set(v));
    }
    block.bypass = spec.bypass;
    // A third-party block's own state (`spec.state`) is the CLAP
    // factory's to load: it built the instance from `spec`.
    block.reset();
    block
}

impl Graph {
    /// Frames of latency, the longest lane's.
    #[must_use]
    pub const fn latency(&self) -> u32 {
        self.latency
    }

    /// The longest piece [`process_piece`](Self::process_piece) takes.
    #[must_use]
    pub const fn max_block(&self) -> usize {
        self.max_block
    }

    /// The block at `lane`, `block`.
    #[must_use]
    pub fn block(&self, lane: usize, block: usize) -> Option<&ChainBlock> {
        self.lanes.get(lane)?.blocks.get(block)
    }

    /// Where every block of `kind` is: what an override ("every instance of
    /// that block switches to its CLAP") swaps.
    #[must_use]
    pub fn blocks_of(&self, kind: &BlockKind) -> Vec<(usize, usize)> {
        self.lanes
            .iter()
            .enumerate()
            .flat_map(|(l, lane)| {
                lane.blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| &b.kind == kind)
                    .map(move |(b, _)| (l, b))
            })
            .collect()
    }

    /// Hand a MIDI event to every block.
    pub fn midi(&mut self, event: Midi) {
        for lane in &mut self.lanes {
            for block in &mut lane.blocks {
                block.midi(event);
            }
        }
    }

    /// Change a block's parameter, by index: lands before the next piece.
    pub fn apply(&mut self, lane: usize, block: usize, param: usize, change: ParamChange) {
        if let Some(b) = self
            .lanes
            .get_mut(lane)
            .and_then(|l| l.blocks.get_mut(block))
        {
            b.apply(param, change);
        }
    }

    /// Swap a block's backend at this block boundary (see
    /// [`ChainBlock::swap_backend`]). Returns the old one, or `backend`
    /// back if there is no such block.
    pub fn swap_backend(&mut self, lane: usize, block: usize, backend: Backend) -> Backend {
        match self
            .lanes
            .get_mut(lane)
            .and_then(|l| l.blocks.get_mut(block))
        {
            Some(b) => b.swap_backend(backend),
            None => backend,
        }
    }

    /// Move macro `index` to `value` (0–1).
    pub fn apply_macro(&mut self, index: usize, value: f32) {
        let Some(bindings) = self.macros.get_mut(index) else {
            return;
        };
        let v = value.clamp(0.0, 1.0);
        for b in bindings {
            let Some(block) = self
                .lanes
                .get_mut(b.lane)
                .and_then(|l| l.blocks.get_mut(b.block))
            else {
                continue;
            };
            let target = v * (b.max - b.min) + b.min;
            match b.mode {
                MacroMode::Set => block.apply(b.param, ParamChange::Set(target)),
                MacroMode::Modulate => {
                    let delta = target - b.applied;
                    b.applied = target;
                    block.apply(b.param, ParamChange::Modulate(delta));
                }
            }
        }
    }

    /// Clear every block's state.
    pub fn reset(&mut self) {
        for lane in &mut self.lanes {
            for block in &mut lane.blocks {
                block.reset();
            }
        }
    }

    /// Process `start..start + len` of `io` in place (`len` ≤
    /// [`max_block`](Self::max_block)): on entry its first `in_channels`
    /// hold the input, on exit every channel holds the output.
    pub fn process_piece(
        &mut self,
        io: &mut [&mut [f32]],
        in_channels: usize,
        start: usize,
        len: usize,
        in_gain: f32,
    ) {
        let len = len.min(self.max_block);
        let end = start.saturating_add(len);
        for (ch, copy) in self.input_copy.iter_mut().enumerate() {
            let dst = copy.get_mut(..len);
            let src = if ch < in_channels {
                io.get(ch).and_then(|c| c.get(start..end))
            } else {
                None
            };
            match (dst, src) {
                (Some(d), Some(s)) => d.iter_mut().zip(s).for_each(|(d, s)| *d = *s * in_gain),
                (Some(d), None) => d.fill(0.0),
                _ => {}
            }
        }
        for ch in io.iter_mut() {
            if let Some(s) = ch.get_mut(start..end) {
                s.fill(0.0);
            }
        }
        let input_copy = &self.input_copy;
        let channel = |c: Option<&usize>| {
            c.and_then(|c| input_copy.get(*c))
                .and_then(|b| b.get(..len))
        };
        for lane in &mut self.lanes {
            let (Some(l), Some(r)) = (lane.l.get_mut(..len), lane.r.get_mut(..len)) else {
                continue;
            };
            match (channel(lane.input.first()), channel(lane.input.get(1))) {
                (Some(a), Some(b)) => {
                    l.copy_from_slice(a);
                    r.copy_from_slice(b);
                }
                (Some(a), None) => {
                    l.copy_from_slice(a);
                    r.copy_from_slice(a);
                }
                _ => {
                    l.fill(0.0);
                    r.fill(0.0);
                }
            }
            for block in &mut lane.blocks {
                block.process(l, r);
            }
            if lane.mute {
                continue;
            }
            let g = lane.gain;
            let right_out = lane.output.saturating_add(1);
            let has_right = io.get(right_out).is_some_and(|c| c.len() >= end);
            if let Some(out) = io.get_mut(lane.output).and_then(|c| c.get_mut(start..end)) {
                if has_right {
                    out.iter_mut().zip(l.iter()).for_each(|(o, s)| *o += s * g);
                } else {
                    // One channel out: the lane folded to mono.
                    out.iter_mut()
                        .zip(l.iter().zip(r.iter()))
                        .for_each(|(o, (a, b))| *o += (a + b) * 0.5 * g);
                }
            }
            if has_right {
                if let Some(out) = io.get_mut(right_out).and_then(|c| c.get_mut(start..end)) {
                    out.iter_mut().zip(r.iter()).for_each(|(o, s)| *o += s * g);
                }
            }
        }
    }
}
