//! Tree-render runtime for the composition tree ([`crate::rig_node`]).
//!
//! Compiles a [`Container`] into a [`RenderNode`] tree of boxed processors, then
//! renders audio by walking it: a **Serial** container chains its children
//! (output of N → input of N+1), a **Parallel** container sums them. A leaf
//! **Block** becomes its audio backend — a NAM/IR/plugin/sample-library, or a
//! built-in `Native` DSP — or, when the block is still a placeholder, a
//! **pass-through** (audio flows through untouched). So the same routing
//! renders correctly today (placeholders = silence/thru) and gains sound as
//! each block type's DSP is implemented.
//!
//! **Control-rate modulation (the ModMatrix).** Containers carry
//! [`ModRoute`]s (`source → "Block.param"` at a depth). At compile time each
//! route is resolved: the source becomes a [`ModSource`] (a modulator block —
//! LFO/Envelope — or a MIDI performance source), the target becomes a
//! `(leaf, param id)` pair via the backend's reported parameter names. At
//! render time the engine ticks every source once per block and applies
//! `base + depth × value` to each target as a parameter write — block-rate
//! control, the same rate FX plugins receive automation.
//!
//! Headless / offline: `process` allocates scratch, so it's for tests and bring-up
//! rather than the realtime callback (which will pre-allocate). MIDI events are
//! delivered to every node; sources consume note on/off, effects ignore them.

use crate::rig::{RigBlock, build_block};
use crate::rig_node::{Combine, Container, RigNode, Zone};
use signal_plugin_host::{PluginEvents, PluginInstance, PluginMidiEvent};

/// Build the audio backend for one block at `sample_rate`, or `None` when the
/// block is a placeholder or an unimplemented `Native` type (→ pass-through).
pub fn build_node_backend(block: &RigBlock, sample_rate: u32) -> Option<Box<dyn PluginInstance>> {
    if !block.has_backend() {
        return None;
    }
    if block.is_native() {
        // Built-in DSP via the native registry (one table, one place to grow).
        crate::native::build_native(block, sample_rate)
    } else {
        // NAM / IR / sample-library / hosted plugin via the shared loader.
        build_block(block, sample_rate)
            .map_err(|e| tracing::warn!(error = %e, "node_render: backend build failed"))
            .ok()
            .map(|(boxed, _, _, _)| boxed)
    }
}

// ── The mod engine ───────────────────────────────────────────────────────────

mod modmatrix;

pub use modmatrix::ModEngine;
use modmatrix::{ModCompiler, build_arp};

/// A compiled, renderable node mirroring the container tree.
pub enum RenderNode {
    /// A leaf processor (`inst = None` for a placeholder pass-through).
    /// `id` indexes the mod engine's per-leaf write table.
    Leaf {
        id: usize,
        inst: Option<Box<dyn PluginInstance>>,
    },
    /// Children chained in order.
    Serial(Vec<RenderNode>),
    /// Children summed.
    Parallel(Vec<RenderNode>),
    /// A keyboard-routed subtree: incoming MIDI is filtered + velocity-scaled by
    /// the [`Zone`] (key split + velocity crossfade) before reaching `inner`.
    /// This is the central-MIDI-input router, expressed in the render tree.
    Zoned { zone: Zone, inner: Box<RenderNode> },
    /// Container volume: input trim applied before `inner`, output fader after.
    Gain {
        input: f32,
        output: f32,
        inner: Box<RenderNode>,
    },
    /// A subtree whose output also feeds one or more send buses (unity gain).
    SendTap {
        buses: Vec<usize>,
        inner: Box<RenderNode>,
    },
    /// A send-bus **return**: `inner` processes the bus content and its
    /// output is summed with the pass-through main signal (send/return
    /// semantics — the main chain is not re-processed by the return).
    BusInject { bus: usize, inner: Box<RenderNode> },
    /// The tree root when any mod routes or send buses resolved: ticks the
    /// [`ModEngine`] each block and threads its parameter writes + bus
    /// buffers down the tree.
    Modulated {
        engine: Box<ModEngine>,
        inner: Box<RenderNode>,
    },
}

/// Per-block render context threaded down the tree: mod-engine parameter
/// writes (per leaf) + send-bus buffers.
#[derive(Default)]
struct RenderCtx {
    writes: Vec<Vec<(u32, f64)>>,
    bus_l: Vec<Vec<f32>>,
    bus_r: Vec<Vec<f32>>,
}

impl RenderNode {
    /// Compile a container tree into a render tree at `sample_rate`,
    /// resolving its ModMatrix routes. A container with a non-full [`Zone`]
    /// is wrapped in [`RenderNode::Zoned`] so only its in-window notes reach it.
    pub fn compile(container: &Container, sample_rate: u32) -> RenderNode {
        let mut mc = ModCompiler::new(sample_rate);
        mc.collect_buses(container);
        let root = Self::compile_container(container, sample_rate, &mut mc);
        if mc.routes.is_empty() && mc.buses.is_empty() && build_arp(container).is_none() {
            root
        } else {
            let bus_count = mc.buses.len();
            RenderNode::Modulated {
                engine: Box::new(ModEngine {
                    sources: mc.sources,
                    routes: mc.routes,
                    writes: Vec::new(),
                    bus_l: vec![Vec::new(); bus_count],
                    bus_r: vec![Vec::new(); bus_count],
                    tempo_bpm: 120.0,
                    sample_rate: sample_rate as f32,
                    arp: build_arp(container),
                }),
                inner: Box::new(root),
            }
        }
    }

    /// Compile one container, returning its node; `mc` accumulates leaves,
    /// modulator scope and resolved routes.
    fn compile_container(
        container: &Container,
        sample_rate: u32,
        mc: &mut ModCompiler,
    ) -> RenderNode {
        // A bypassed subtree renders as a pass-through (no leaves, no routes).
        if container.bypassed {
            return RenderNode::Serial(Vec::new());
        }
        // Bring this container's modulators into scope.
        let scope_mark = mc.scope.len();
        for m in &container.modulators {
            if let Some(idx) = mc.instantiate(m) {
                mc.scope.push((m.display_name().to_lowercase(), idx));
            }
        }

        let subtree_start = mc.leaves.len();
        let kids = container
            .children
            .iter()
            .map(|n| Self::compile_node(n, sample_rate, mc))
            .collect();
        let subtree: Vec<usize> = (subtree_start..mc.leaves.len()).collect();

        // Resolve this container's routes against its own subtree.
        mc.resolve_routes(&container.mod_routes, &subtree);

        mc.scope.truncate(scope_mark);

        let base = match container.combine {
            Combine::Serial => RenderNode::Serial(kids),
            Combine::Parallel => RenderNode::Parallel(kids),
        };
        let mut node = if container.zone.is_full() {
            base
        } else {
            RenderNode::Zoned {
                zone: container.zone,
                inner: Box::new(base),
            }
        };
        // Container volumes: input trim + output fader (dB → linear).
        if container.input_db != 0.0 || container.output_db != 0.0 {
            node = RenderNode::Gain {
                input: 10f32.powf(container.input_db / 20.0),
                output: 10f32.powf(container.output_db / 20.0),
                inner: Box::new(node),
            };
        }
        // This container is a send target → it becomes a bus return.
        if let Some(bus) = mc.bus_id(&container.name) {
            node = RenderNode::BusInject {
                bus,
                inner: Box::new(node),
            };
        }
        // This container sends its output to buses.
        let taps: Vec<usize> = container
            .sends
            .iter()
            .filter_map(|s| mc.bus_id(&s.target))
            .collect();
        if !taps.is_empty() {
            node = RenderNode::SendTap {
                buses: taps,
                inner: Box::new(node),
            };
        }
        node
    }

    fn compile_node(node: &RigNode, sample_rate: u32, mc: &mut ModCompiler) -> RenderNode {
        match node {
            RigNode::Block { block: b } => {
                let mut inst = build_node_backend(b, sample_rate);
                // Snapshot the params with their LIVE values (build-time
                // block params applied) — routes modulate around these bases.
                let params = inst
                    .as_mut()
                    .map(|i| {
                        let mut ps = i.params();
                        for p in &mut ps {
                            if let Some(v) = i.param_value(p.id) {
                                p.default = v;
                            }
                        }
                        ps
                    })
                    .unwrap_or_default();
                let id = mc.leaves.len();
                mc.leaves.push((b.display_name().to_lowercase(), params));
                let leaf = RenderNode::Leaf { id, inst };
                // A block can also be a send target (e.g. the global Rotary).
                match mc.bus_id(&b.display_name()) {
                    Some(bus) => RenderNode::BusInject {
                        bus,
                        inner: Box::new(leaf),
                    },
                    None => leaf,
                }
            }
            RigNode::Container { container: c } => Self::compile_container(c, sample_rate, mc),
        }
    }

    /// Prepare every leaf processor for `sample_rate` / `block_size`.
    pub fn prepare(&mut self, sample_rate: f64, block_size: u32) {
        match self {
            RenderNode::Leaf {
                inst: Some(inst), ..
            } => {
                let _ = inst.prepare(sample_rate, block_size);
            }
            RenderNode::Leaf { inst: None, .. } => {}
            RenderNode::Serial(v) | RenderNode::Parallel(v) => {
                v.iter_mut()
                    .for_each(|n| n.prepare(sample_rate, block_size));
            }
            RenderNode::Zoned { inner, .. }
            | RenderNode::Gain { inner, .. }
            | RenderNode::SendTap { inner, .. }
            | RenderNode::BusInject { inner, .. } => inner.prepare(sample_rate, block_size),
            RenderNode::Modulated { engine, inner } => {
                inner.prepare(sample_rate, block_size);
                engine.prepare(sample_rate, inner.leaf_count());
            }
        }
    }

    /// Total leaves (placeholder or live) — sizes the mod write table.
    fn leaf_count(&self) -> usize {
        match self {
            RenderNode::Leaf { .. } => 1,
            RenderNode::Serial(v) | RenderNode::Parallel(v) => {
                v.iter().map(|n| n.leaf_count()).sum()
            }
            RenderNode::Zoned { inner, .. }
            | RenderNode::Modulated { inner, .. }
            | RenderNode::Gain { inner, .. }
            | RenderNode::SendTap { inner, .. }
            | RenderNode::BusInject { inner, .. } => inner.leaf_count(),
        }
    }

    /// Count the leaf processors that actually have a backend (for tests/metering).
    pub fn live_leaves(&self) -> usize {
        match self {
            RenderNode::Leaf { inst, .. } => inst.is_some() as usize,
            RenderNode::Serial(v) | RenderNode::Parallel(v) => {
                v.iter().map(|n| n.live_leaves()).sum()
            }
            RenderNode::Zoned { inner, .. }
            | RenderNode::Modulated { inner, .. }
            | RenderNode::Gain { inner, .. }
            | RenderNode::SendTap { inner, .. }
            | RenderNode::BusInject { inner, .. } => inner.live_leaves(),
        }
    }

    /// The compiled ModMatrix, when any routes resolved (for tests/UI).
    pub fn mod_engine(&self) -> Option<&ModEngine> {
        match self {
            RenderNode::Modulated { engine, .. } => Some(engine),
            _ => None,
        }
    }

    /// Set the tempo used by synced LFOs (no-op on trees without a mod engine).
    pub fn set_tempo(&mut self, bpm: f32) {
        if let RenderNode::Modulated { engine, .. } = self {
            engine.tempo_bpm = bpm.max(1.0);
        }
    }

    /// Render one block. `out` is overwritten with this node's output.
    pub fn process(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) {
        // Root-level: tick the mod engine, then thread its writes + bus
        // buffers down the tree.
        if let RenderNode::Modulated { engine, inner } = self {
            let frames = out_l.len().min(out_r.len());
            // The arpeggiator rewrites the MIDI stream before anything else
            // (steps replace held notes; CC/bend pass through).
            let arp_midi;
            let arp_events;
            let events = if let Some(arp) = engine.arp.as_mut() {
                arp_midi = arp.process(events.midi, frames, engine.sample_rate, engine.tempo_bpm);
                arp_events = PluginEvents {
                    params: events.params,
                    midi: &arp_midi,
                    note_expressions: events.note_expressions,
                };
                &arp_events
            } else {
                events
            };
            engine.tick(events, frames);
            let mut ctx = RenderCtx {
                writes: std::mem::take(&mut engine.writes),
                bus_l: std::mem::take(&mut engine.bus_l),
                bus_r: std::mem::take(&mut engine.bus_r),
            };
            for b in ctx.bus_l.iter_mut().chain(ctx.bus_r.iter_mut()) {
                b.clear();
                b.resize(frames, 0.0);
            }
            inner.process_inner(in_l, in_r, out_l, out_r, events, &mut ctx);
            engine.writes = ctx.writes;
            engine.bus_l = ctx.bus_l;
            engine.bus_r = ctx.bus_r;
            return;
        }
        let mut ctx = RenderCtx::default();
        self.process_inner(in_l, in_r, out_l, out_r, events, &mut ctx);
    }

    fn process_inner(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
        ctx: &mut RenderCtx,
    ) {
        let frames = out_l.len().min(out_r.len());
        match self {
            RenderNode::Leaf {
                id,
                inst: Some(inst),
            } => {
                let mods = ctx.writes.get(*id).map(|w| w.as_slice()).unwrap_or(&[]);
                if mods.is_empty() {
                    let _ = inst.process_block(in_l, in_r, out_l, out_r, events);
                } else {
                    // Merge external param writes with this leaf's mod writes.
                    let mut params: Vec<(u32, f64)> =
                        Vec::with_capacity(events.params.len() + mods.len());
                    params.extend_from_slice(events.params);
                    params.extend_from_slice(mods);
                    let ev = PluginEvents {
                        params: &params,
                        midi: events.midi,
                        note_expressions: events.note_expressions,
                    };
                    let _ = inst.process_block(in_l, in_r, out_l, out_r, &ev);
                }
            }
            RenderNode::Leaf { inst: None, .. } => copy_in(in_l, in_r, out_l, out_r, frames),
            RenderNode::Serial(nodes) => {
                if nodes.is_empty() {
                    return copy_in(in_l, in_r, out_l, out_r, frames);
                }
                // Ping-pong the signal through each child.
                let mut cur_l = in_l[..frames].to_vec();
                let mut cur_r = in_r[..frames].to_vec();
                let mut nxt_l = vec![0.0f32; frames];
                let mut nxt_r = vec![0.0f32; frames];
                for node in nodes.iter_mut() {
                    nxt_l.iter_mut().for_each(|x| *x = 0.0);
                    nxt_r.iter_mut().for_each(|x| *x = 0.0);
                    node.process_inner(&cur_l, &cur_r, &mut nxt_l, &mut nxt_r, events, ctx);
                    std::mem::swap(&mut cur_l, &mut nxt_l);
                    std::mem::swap(&mut cur_r, &mut nxt_r);
                }
                out_l[..frames].copy_from_slice(&cur_l[..frames]);
                out_r[..frames].copy_from_slice(&cur_r[..frames]);
            }
            RenderNode::Parallel(nodes) => {
                out_l[..frames].iter_mut().for_each(|x| *x = 0.0);
                out_r[..frames].iter_mut().for_each(|x| *x = 0.0);
                let mut tl = vec![0.0f32; frames];
                let mut tr = vec![0.0f32; frames];
                for node in nodes.iter_mut() {
                    tl.iter_mut().for_each(|x| *x = 0.0);
                    tr.iter_mut().for_each(|x| *x = 0.0);
                    node.process_inner(in_l, in_r, &mut tl, &mut tr, events, ctx);
                    for f in 0..frames {
                        out_l[f] += tl[f];
                        out_r[f] += tr[f];
                    }
                }
            }
            RenderNode::Zoned { zone, inner } => {
                // Central-MIDI-input routing: keep only notes in this zone's
                // window, scaling each NoteOn's velocity by the crossfade gain.
                // Note-offs and CC pass through (so held notes always release).
                let filtered = filter_events_by_zone(*zone, events);
                let fe = PluginEvents {
                    params: events.params,
                    midi: &filtered,
                    note_expressions: events.note_expressions,
                };
                inner.process_inner(in_l, in_r, out_l, out_r, &fe, ctx);
            }
            RenderNode::Gain {
                input,
                output,
                inner,
            } => {
                if *input == 1.0 {
                    inner.process_inner(in_l, in_r, out_l, out_r, events, ctx);
                } else {
                    let gl: Vec<f32> = in_l[..frames.min(in_l.len())]
                        .iter()
                        .map(|s| s * *input)
                        .collect();
                    let gr: Vec<f32> = in_r[..frames.min(in_r.len())]
                        .iter()
                        .map(|s| s * *input)
                        .collect();
                    inner.process_inner(&gl, &gr, out_l, out_r, events, ctx);
                }
                if *output != 1.0 {
                    for f in 0..frames {
                        out_l[f] *= *output;
                        out_r[f] *= *output;
                    }
                }
            }
            RenderNode::SendTap { buses, inner } => {
                inner.process_inner(in_l, in_r, out_l, out_r, events, ctx);
                // Feed this node's output into each bus (unity gain).
                for &bus in buses.iter() {
                    if let Some(bl) = ctx.bus_l.get_mut(bus) {
                        for f in 0..frames.min(bl.len()) {
                            bl[f] += out_l[f];
                        }
                    }
                    if let Some(br) = ctx.bus_r.get_mut(bus) {
                        for f in 0..frames.min(br.len()) {
                            br[f] += out_r[f];
                        }
                    }
                }
            }
            RenderNode::BusInject { bus, inner } => {
                // Send/return: `inner` processes the BUS content only; its
                // output sums onto the pass-through main signal.
                let bl: Vec<f32> = ctx
                    .bus_l
                    .get(*bus)
                    .map(|b| b[..frames.min(b.len())].to_vec())
                    .unwrap_or_else(|| vec![0.0; frames]);
                let br: Vec<f32> = ctx
                    .bus_r
                    .get(*bus)
                    .map(|b| b[..frames.min(b.len())].to_vec())
                    .unwrap_or_else(|| vec![0.0; frames]);
                let mut tl = vec![0.0f32; frames];
                let mut tr = vec![0.0f32; frames];
                inner.process_inner(&bl, &br, &mut tl, &mut tr, events, ctx);
                for f in 0..frames {
                    out_l[f] = in_l.get(f).copied().unwrap_or(0.0) + tl[f];
                    out_r[f] = in_r.get(f).copied().unwrap_or(0.0) + tr[f];
                }
            }
            RenderNode::Modulated { .. } => {
                // Only ever the root; handled in `process`.
                debug_assert!(false, "nested Modulated node");
            }
        }
    }

    /// Convenience: render `frames` of output from silence + the given MIDI.
    pub fn render(&mut self, out_l: &mut [f32], out_r: &mut [f32], midi: &PluginEvents<'_>) {
        let frames = out_l.len().min(out_r.len());
        let silence = vec![0.0f32; frames];
        self.process(&silence, &silence, out_l, out_r, midi);
    }
}

/// Apply a [`Zone`] to a MIDI stream: drop NoteOns outside the window, scale
/// the rest by the crossfade gain (velocity × gain), and pass everything else
/// (NoteOff / CC / …) through unchanged so releases always land.
fn filter_events_by_zone(zone: Zone, events: &PluginEvents<'_>) -> Vec<PluginMidiEvent> {
    use daw::service::MidiMessage;
    let mut out = Vec::with_capacity(events.midi.len());
    for ev in events.midi {
        match ev.message {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => {
                let gain = zone.note_gain(note, velocity);
                if gain > 0.0 {
                    let scaled = ((velocity as f32 * gain).round() as u8).clamp(1, 127);
                    out.push(PluginMidiEvent {
                        offset: ev.offset,
                        message: MidiMessage::note_on(channel, note, scaled),
                    });
                }
            }
            _ => out.push(ev.clone()),
        }
    }
    out
}

fn copy_in(in_l: &[f32], in_r: &[f32], out_l: &mut [f32], out_r: &mut [f32], frames: usize) {
    let n = frames.min(in_l.len()).min(in_r.len());
    out_l[..n].copy_from_slice(&in_l[..n]);
    out_r[..n].copy_from_slice(&in_r[..n]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig_node::Container;
    use signal_plugin_host::PluginMidiEvent;
    use signal_proto::block::BlockType;

    fn note_on(note: u8, vel: u8) -> PluginMidiEvent {
        PluginMidiEvent {
            offset: 0,
            message: daw::service::MidiMessage::note_on(0, note, vel),
        }
    }

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|s| s * s).sum::<f32>() / buf.len().max(1) as f32).sqrt()
    }

    #[test]
    fn oscillator_block_renders_through_a_serial_module() {
        // Module [serial] { Oscillator (native), Filter (native SVF) }
        let tree = Container::module("Osc")
            .block(BlockType::Oscillator, "Osc")
            .block(BlockType::Filter, "Filter");
        let mut rn = RenderNode::compile(&tree, 48_000);
        rn.prepare(48_000.0, 256);
        assert_eq!(
            rn.live_leaves(),
            2,
            "oscillator + filter both have backends"
        );

        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        let midi = [note_on(69, 110)];
        let ev = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        rn.render(&mut l, &mut r, &ev);
        assert!(
            rms(&l) > 1e-3,
            "audio flows osc → filter (default open) → out"
        );
    }

    #[test]
    fn parallel_sums_two_oscillators() {
        let tree = Container::parallel("Voices")
            .add(Container::layer("A").block(BlockType::Oscillator, "Osc"))
            .add(Container::layer("B").block(BlockType::Oscillator, "Osc"));
        let mut rn = RenderNode::compile(&tree, 48_000);
        rn.prepare(48_000.0, 256);
        assert_eq!(rn.live_leaves(), 2);

        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        let midi = [note_on(60, 100)];
        let ev = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        rn.render(&mut l, &mut r, &ev);
        assert!(rms(&l) > 1e-3);
    }

    fn render_note(rn: &mut RenderNode, note: u8, vel: u8) -> f32 {
        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        let midi = [note_on(note, vel)];
        let ev = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        rn.render(&mut l, &mut r, &ev);
        rms(&l)
    }

    #[test]
    fn key_split_routes_notes_to_their_zone() {
        // Low osc keys 0-50, High osc keys 70-127 — with a silent gap 51..69.
        let split = Container::parallel("Split")
            .add(
                Container::layer("Low")
                    .keys(0, 50)
                    .block(BlockType::Oscillator, "Osc"),
            )
            .add(
                Container::layer("High")
                    .keys(70, 127)
                    .block(BlockType::Oscillator, "Osc"),
            );

        // A note in the low zone sounds.
        let mut rn = RenderNode::compile(&split, 48_000);
        rn.prepare(48_000.0, 256);
        assert!(render_note(&mut rn, 40, 100) > 1e-3, "low zone note sounds");

        // A note in the gap reaches no layer → silence.
        let mut rn = RenderNode::compile(&split, 48_000);
        rn.prepare(48_000.0, 256);
        assert!(render_note(&mut rn, 60, 100) < 1e-5, "gap note is silent");
    }

    #[test]
    fn velocity_window_gates_notes() {
        let loud = Container::layer("Loud")
            .velocity(64, 127)
            .block(BlockType::Oscillator, "Osc");

        let mut rn = RenderNode::compile(&loud, 48_000);
        rn.prepare(48_000.0, 256);
        assert!(render_note(&mut rn, 60, 110) > 1e-3, "hard hit sounds");

        let mut rn = RenderNode::compile(&loud, 48_000);
        rn.prepare(48_000.0, 256);
        assert!(
            render_note(&mut rn, 60, 30) < 1e-5,
            "soft hit is below the window"
        );
    }

    /// A Sampler block (BlockImpl::Sample) plays a real library through the
    /// render tree — the keys/piano/drums/orchestral loading path.
    #[test]
    fn sampler_block_plays_a_library() {
        // Fixture: one 220 Hz sine zone covering the whole keyboard.
        let dir =
            std::env::temp_dir().join(format!("signal-sampler-block-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("note.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&wav, spec).unwrap();
        for i in 0..48_000 {
            let t = i as f32 / 48_000.0;
            w.write_sample((core::f32::consts::TAU * 220.0 * t).sin() * 0.8)
                .unwrap();
        }
        w.finalize().unwrap();
        let styx = "\
name TestZoneLib
zones (
    { file note.wav, key_min 0, key_max 127, root_key 60, vel_min 0, vel_max 127 }
)
";
        let spec_path = dir.join("lib.styx");
        std::fs::write(&spec_path, styx).unwrap();

        // Layer { Sampler(lib) } — samples_root defaults to the spec's dir.
        let tree =
            Container::layer("Keys").sample_block("Piano", spec_path.to_string_lossy().to_string());
        let mut rn = RenderNode::compile(&tree, 48_000);
        rn.prepare(48_000.0, 512);
        assert_eq!(rn.live_leaves(), 1, "sampler block has a backend");

        let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
        let midi = [note_on(60, 100)];
        // The cache fills on a background thread — keep (re)triggering the
        // note until the sample is decoded and audible.
        let mut heard = 0.0f32;
        for _ in 0..200 {
            let ev = PluginEvents {
                params: &[],
                midi: &midi,
                note_expressions: &[],
            };
            rn.render(&mut l, &mut r, &ev);
            heard = heard.max(rms(&l));
            if heard > 1e-3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::fs::remove_dir_all(&dir).ok();
        assert!(heard > 1e-3, "sampler block should be audible, rms={heard}");
    }

    /// Sample-mode unison: a Sampler block with unison params spawns detuned,
    /// panned voice copies — audible as stereo side energy on a mono sample.
    #[test]
    fn sampler_unison_spreads_the_image() {
        use crate::rig::RigBlock;
        fn render(unison: bool) -> (Vec<f32>, Vec<f32>) {
            let dir = std::env::temp_dir().join(format!(
                "signal-sampler-unison-test-{}-{}",
                std::process::id(),
                unison
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let wav = dir.join("note.wav");
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 48_000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut w = hound::WavWriter::create(&wav, spec).unwrap();
            for i in 0..48_000 {
                let t = i as f32 / 48_000.0;
                w.write_sample((core::f32::consts::TAU * 220.0 * t).sin() * 0.8)
                    .unwrap();
            }
            w.finalize().unwrap();
            std::fs::write(
                dir.join("lib.styx"),
                "name U\nzones (\n    { file note.wav, key_min 0, key_max 127, root_key 60, vel_min 0, vel_max 127 }\n)\n",
            )
            .unwrap();
            let mut sb =
                RigBlock::sample_lib(dir.join("lib.styx").to_string_lossy().to_string()).named("S");
            if unison {
                sb = sb
                    .with_param("unison_voices", "6")
                    .with_param("unison_detune", "0.3")
                    .with_param("unison_width", "1.0");
            }
            let tree = Container::layer("L").add(sb);
            let mut rn = RenderNode::compile(&tree, 48_000);
            rn.prepare(48_000.0, 512);
            let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
            let midi = [note_on(60, 100)];
            let mut got = (vec![], vec![]);
            for _ in 0..200 {
                let ev = PluginEvents {
                    params: &[],
                    midi: &midi,
                    note_expressions: &[],
                };
                rn.render(&mut l, &mut r, &ev);
                if rms(&l) > 1e-3 {
                    got = (l.clone(), r.clone());
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            std::fs::remove_dir_all(&dir).ok();
            got
        }
        let (ml, mr) = render(false);
        let (wl, wr) = render(true);
        assert!(rms(&ml) > 1e-3 && rms(&wl) > 1e-3, "both audible");
        let side_mono: Vec<f32> = ml.iter().zip(&mr).map(|(a, b)| a - b).collect();
        let side_wide: Vec<f32> = wl.iter().zip(&wr).map(|(a, b)| a - b).collect();
        assert!(rms(&side_mono) < 1e-5, "single voice centered");
        assert!(
            rms(&side_wide) > rms(&wl) * 0.05,
            "unison copies spread the image: side={} mid={}",
            rms(&side_wide),
            rms(&wl)
        );
    }

    /// Machine-local: the real Keyscape LA Custom C7 Grand extraction loads
    /// through a Sampler block and makes sound. Run explicitly:
    /// `cargo test -p signal-sampler --lib keyscape -- --ignored`
    #[test]
    #[ignore = "requires the local Keyscape extraction on AudioHaven"]
    fn keyscape_c7_grand_loads_and_sounds() {
        let spec = "/run/media/AudioHaven/Sampled/Keys/Keyscape/LA Custom C7 Grand/library.styx";
        if !std::path::Path::new(spec).exists() {
            eprintln!("skipping: {spec} not present");
            return;
        }
        let tree = Container::layer("Keys").sample_block("C7 Grand", spec);
        let mut rn = RenderNode::compile(&tree, 48_000);
        rn.prepare(48_000.0, 512);
        assert_eq!(rn.live_leaves(), 1, "keyscape sampler block builds");

        let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
        let midi = [note_on(60, 100)];
        let mut heard = 0.0f32;
        for _ in 0..600 {
            let ev = PluginEvents {
                params: &[],
                midi: &midi,
                note_expressions: &[],
            };
            rn.render(&mut l, &mut r, &ev);
            heard = heard.max(rms(&l));
            if heard > 1e-3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(heard > 1e-3, "C7 grand should be audible, rms={heard}");
    }

    #[test]
    fn full_nord_preset_renders_synth_oscillators() {
        // The whole Nord routing: the 3 synth Oscillators plus every native
        // Filter/Amp are live DSP; the rest are placeholder pass-throughs. A
        // held note must reach the output through the entire tree (synth
        // voices → engines-sum → global thru).
        let preset = crate::nord::nord_stage_preset();
        let mut rn = RenderNode::compile(&preset, 48_000);
        rn.prepare(48_000.0, 512);
        assert_eq!(
            rn.live_leaves(),
            27,
            "3 oscillators + the tree's native Filter/Amp blocks are live"
        );

        let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
        let midi = [note_on(64, 100)];
        let ev = PluginEvents {
            params: &[],
            midi: &midi,
            note_expressions: &[],
        };
        rn.render(&mut l, &mut r, &ev);
        assert!(
            rms(&l) > 1e-3,
            "the synth layers must be audible through the full Nord tree"
        );
    }

    // ── ModMatrix runtime ────────────────────────────────────────────────

    /// An envelope route closes a filter: with a big negative depth from a
    /// note-gated envelope, the held note is far darker than the same tree
    /// without the route.
    #[test]
    fn envelope_route_modulates_filter_cutoff() {
        fn sustain_level(with_route: bool) -> f32 {
            let mut tree = Container::layer("L")
                .block(BlockType::Oscillator, "Osc")
                .block(BlockType::Filter, "Filter")
                .modulator(BlockType::Envelope, "Filter Env");
            if with_route {
                tree = tree.route("Filter Env", "Filter.cutoff", -0.95);
            }
            let mut rn = RenderNode::compile(&tree, 48_000);
            assert_eq!(
                rn.mod_engine().map(|e| e.route_count()),
                with_route.then_some(1),
                "route resolution matches"
            );
            rn.prepare(48_000.0, 512);
            let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
            let mut level = 0.0;
            for b in 0..48 {
                let midi = [note_on(60, 100)];
                let ev = PluginEvents {
                    params: &[],
                    midi: if b == 0 { &midi } else { &[] },
                    note_expressions: &[],
                };
                rn.render(&mut l, &mut r, &ev);
                level = rms(&l);
            }
            level
        }
        let open = sustain_level(false);
        let closed = sustain_level(true);
        assert!(open > 1e-3, "unmodulated tree sounds, rms={open}");
        assert!(
            closed < open * 0.35,
            "envelope route closes the filter: open={open} closed={closed}"
        );
    }

    /// A mod-wheel route opens/closes the filter from CC1.
    #[test]
    fn wheel_route_drives_filter_cutoff() {
        let tree = Container::layer("L")
            .block(BlockType::Oscillator, "Osc")
            .block(BlockType::Filter, "Filter")
            .route("Wheel", "Filter.cutoff", -1.0);
        let mut rn = RenderNode::compile(&tree, 48_000);
        assert_eq!(rn.mod_engine().map(|e| e.route_count()), Some(1));
        rn.prepare(48_000.0, 512);

        let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
        // Note on, wheel at 0 → bright.
        let start = [note_on(60, 100)];
        let ev = PluginEvents {
            params: &[],
            midi: &start,
            note_expressions: &[],
        };
        rn.render(&mut l, &mut r, &ev);
        rn.render(
            &mut l,
            &mut r,
            &PluginEvents {
                params: &[],
                midi: &[],
                note_expressions: &[],
            },
        );
        let bright = rms(&l);

        // Wheel to max → cutoff floor → dark.
        let cc = [PluginMidiEvent {
            offset: 0,
            message: daw::service::MidiMessage::ControlChange {
                channel: 0,
                controller: 1,
                value: 127,
            },
        }];
        let ev = PluginEvents {
            params: &[],
            midi: &cc,
            note_expressions: &[],
        };
        rn.render(&mut l, &mut r, &ev);
        rn.render(
            &mut l,
            &mut r,
            &PluginEvents {
                params: &[],
                midi: &[],
                note_expressions: &[],
            },
        );
        let dark = rms(&l);
        assert!(
            dark < bright * 0.35,
            "wheel closes the filter: bright={bright} dark={dark}"
        );
    }

    /// Build-time block params configure the native backend: a filter block
    /// built with a low imported cutoff is audibly darker than the default.
    #[test]
    fn block_params_configure_the_backend() {
        use crate::rig::RigBlock;
        fn level(cutoff_param: Option<&str>) -> f32 {
            let mut f = RigBlock::of_type(BlockType::Filter).named("F");
            if let Some(v) = cutoff_param {
                f = f.with_param("cutoff", v);
            }
            let tree = Container::module("M")
                .block(BlockType::Oscillator, "Osc")
                .add(f);
            let mut rn = RenderNode::compile(&tree, 48_000);
            rn.prepare(48_000.0, 512);
            let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
            let midi = [note_on(69, 110)]; // A4 = 440 Hz
            for b in 0..4 {
                let ev = PluginEvents {
                    params: &[],
                    midi: if b == 0 { &midi } else { &[] },
                    note_expressions: &[],
                };
                rn.render(&mut l, &mut r, &ev);
            }
            rms(&l)
        }
        let open = level(None);
        let dark = level(Some("0.15")); // ≈ 56 Hz cutoff under a 440 Hz note
        assert!(open > 1e-3);
        assert!(
            dark < open * 0.35,
            "imported cutoff darkens the block: open={open} dark={dark}"
        );
    }

    /// Send/return buses: a layer taps its output to a named return; with
    /// the main path muted after the tap, audio reaches the output ONLY via
    /// the bus (and doesn't without the send).
    #[test]
    fn sends_sum_into_their_return_bus() {
        use crate::rig::RigBlock;
        fn level(with_send: bool) -> f32 {
            let mut src = Container::layer("Src").block(BlockType::Oscillator, "Osc");
            if with_send {
                src = src.send("Return", "To Return");
            }
            let tree = Container::preset("P")
                .add(src)
                // Mute the main path after the tap (amp gain 0).
                .add(
                    Container::module("Mute").add(
                        RigBlock::of_type(BlockType::Amp)
                            .named("Kill")
                            .with_param("gain", "0"),
                    ),
                )
                // The return: placeholder inside = bus passes straight through.
                .add(Container::module("Return"));
            let mut rn = RenderNode::compile(&tree, 48_000);
            rn.prepare(48_000.0, 256);
            let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
            let midi = [note_on(60, 100)];
            let mut level = 0.0;
            for b in 0..4 {
                let ev = PluginEvents {
                    params: &[],
                    midi: if b == 0 { &midi } else { &[] },
                    note_expressions: &[],
                };
                rn.render(&mut l, &mut r, &ev);
                level = rms(&l);
            }
            level
        }
        assert!(
            level(false) < 1e-6,
            "muted main path is silent without a send"
        );
        assert!(
            level(true) > 1e-3,
            "audio reaches the output via the return bus"
        );
    }

    /// Container volumes and bypass render: output_db scales the subtree,
    /// bypassed subtrees pass audio through untouched.
    #[test]
    fn container_volume_and_bypass_apply() {
        // Volume: a −12 dB layer is quieter than a 0 dB one.
        fn level(db: f32) -> f32 {
            let tree = Container::preset("P").add(
                Container::layer("L")
                    .volume(db)
                    .block(BlockType::Oscillator, "Osc"),
            );
            let mut rn = RenderNode::compile(&tree, 48_000);
            rn.prepare(48_000.0, 256);
            render_note(&mut rn, 60, 100)
        }
        let full = level(0.0);
        let quiet = level(-12.0);
        assert!(full > 1e-3);
        let ratio = quiet / full;
        assert!(
            (ratio - 0.251).abs() < 0.02,
            "−12 dB ≈ ×0.25, got ratio {ratio}"
        );

        // Bypass: a bypassed muting-amp module passes audio through.
        use crate::rig::RigBlock;
        fn with_mute(bypassed: bool) -> f32 {
            let mut mute = Container::module("Mute").add(
                RigBlock::of_type(BlockType::Amp)
                    .named("Kill")
                    .with_param("gain", "0"),
            );
            mute.bypassed = bypassed;
            let tree = Container::preset("P")
                .add(Container::layer("L").block(BlockType::Oscillator, "Osc"))
                .add(mute);
            let mut rn = RenderNode::compile(&tree, 48_000);
            rn.prepare(48_000.0, 256);
            render_note(&mut rn, 60, 100)
        }
        assert!(with_mute(false) < 1e-6, "active mute silences");
        assert!(with_mute(true) > 1e-3, "bypassed mute passes through");
    }

    /// Routes on placeholder blocks resolve to nothing (warn, not panic),
    /// and the tree still renders.
    #[test]
    fn unresolved_routes_are_harmless() {
        let tree = Container::layer("L")
            .block(BlockType::Oscillator, "Osc")
            .block(BlockType::Rotary, "Rotary") // placeholder — no params
            .route("Wheel", "Rotary.speed", 1.0)
            .route("Nonexistent Env", "Osc.pitch", 1.0);
        let mut rn = RenderNode::compile(&tree, 48_000);
        assert!(rn.mod_engine().is_none(), "no routes resolved");
        rn.prepare(48_000.0, 256);
        assert!(render_note(&mut rn, 60, 100) > 1e-3, "still renders");
    }
}
