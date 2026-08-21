//! Multi-channel runtime for Engine / Module / Preset graphs.
//!
//! Runtime structures that turn the parsed `EngineSpec` / `ModuleSpec` /
//! `PresetSpec` data into actual audio routing. The audio thread MUST NOT
//! allocate — every scratch buffer is sized at construction. The graph is
//! re-rendered each block by:
//!
//! 1. Each `EngineInstance::render` clears its mic scratch buffers, calls
//!    `SamplerBlock::render_multi` to splay voices across mics, then copies
//!    each Layer's referenced mic into the Layer's `out_buffer` with
//!    gain/pan/mute applied.
//! 2. The `PresetRuntime::render` walks resolved edges (engines → modules →
//!    master), summing source buffers into target buffers with edge gain.
//! 3. Each `ModuleInstance::render` zeros its outputs and copies the
//!    summed input mix to all outputs (FX chain is no-op for v1).
//!
//! All path strings (`"engine_id.port_id"`, `"module_id.input_id"`,
//! `"master"`) resolve to `BufferRef` indices once at load time so the
//! audio path performs no string parsing.

use std::collections::HashMap;
use std::path::Path;

use crate::block::SamplerBlock;
use crate::engine::cache::EvictStats;
use crate::engine_spec::{EngineLayerSpec, EngineSpec, PortSpec};
use crate::module_spec::ModuleSpec;
use crate::preset_spec::{PresetSpec, RoutingRule};

// ── Engine runtime ────────────────────────────────────────────────────────

/// One Layer inside an `EngineInstance`. Owns its own pre-allocated
/// stereo output buffer that downstream graph edges read from.
pub struct LayerRuntime {
    pub spec: EngineLayerSpec,
    /// Index into the parent engine's mic_scratches (0..mic_count).
    pub mic_index: usize,
    pub gain_lin: f32,
    pub pan_l: f32,
    pub pan_r: f32,
    /// Pre-allocated stereo output buffer (length = block_frames * 2).
    pub out_buffer: Vec<f32>,
}

/// One named output port of an Engine. References the Layer the port
/// exposes by index. The port itself does not own a buffer — readers
/// follow the `layer_index` to the Layer's `out_buffer`.
pub struct PortRuntime {
    pub spec: PortSpec,
    pub layer_index: usize,
}

/// One Engine instance: source SamplerBlock + N Layers + M Ports.
/// Pre-allocates `mic_count` stereo scratch buffers; never allocates in
/// `render`.
pub struct EngineInstance {
    pub spec: EngineSpec,
    pub block: SamplerBlock,
    pub layers: Vec<LayerRuntime>,
    pub ports: Vec<PortRuntime>,
    pub muted: bool,
    /// One stereo scratch buffer per mic id in the block. Reused per
    /// audio block. Pre-allocated at construction.
    mic_scratches: Vec<Vec<f32>>,
    /// Flat (bus × mic) scratch matrix for stem-split document rendering
    /// ([`render_routed_buses`](Self::render_routed_buses)). Lazily sized —
    /// this path is offline-only, so growth here never touches the audio
    /// thread. Empty until first use.
    routed_scratches: Vec<Vec<f32>>,
    /// Cached so we know if block_frames changed between renders.
    block_frames: usize,
    resize_events: u64,
}

impl EngineInstance {
    /// Build an EngineInstance from a parsed `EngineSpec`. `block_frames`
    /// is the per-callback frame count used to pre-size all scratch
    /// buffers; if the audio callback uses a different size, render resizes
    /// once as an explicit stream block-size change fallback.
    pub fn new(spec: EngineSpec, mut block: SamplerBlock, block_frames: usize) -> Self {
        block.apply_voice_config(&spec.voice);
        block.preallocate_render_scratch(block_frames);

        // Determine mic count: use the block's mic list, else 1 (single
        // implicit "default" mic).
        let mic_count = block.mics().len().max(1);
        let mic_scratches = (0..mic_count)
            .map(|_| vec![0.0; block_frames * 2])
            .collect();

        // Build LayerRuntimes. If spec.layers is empty, synthesize one
        // default layer subscribed to mic 0.
        let layers: Vec<LayerRuntime> = if spec.layers.is_empty() {
            vec![LayerRuntime {
                spec: EngineLayerSpec {
                    id: "main".into(),
                    mic: String::new(),
                    gain_db: 0.0,
                    pan: 0.0,
                    bypass: false,
                    fx_chain: Vec::new(),
                },
                mic_index: 0,
                gain_lin: 1.0,
                pan_l: 1.0,
                pan_r: 1.0,
                out_buffer: vec![0.0; block_frames * 2],
            }]
        } else {
            spec.layers
                .iter()
                .map(|l| {
                    let mic_index = resolve_mic_index(block.mics(), &l.mic);
                    let gain_lin = db_to_lin(l.gain_db);
                    let (pl, pr) = pan_gains(l.pan);
                    LayerRuntime {
                        spec: l.clone(),
                        mic_index,
                        gain_lin,
                        pan_l: pl * gain_lin,
                        pan_r: pr * gain_lin,
                        out_buffer: vec![0.0; block_frames * 2],
                    }
                })
                .collect()
        };

        // Build PortRuntimes. If spec.ports is empty, synthesize one
        // default `main` port that exposes layer 0 (sums across layers
        // is implemented at preset-graph time when multiple layers feed
        // the same downstream input — for the synthesized case we just
        // pick layer 0). We could also auto-mix, but pure-passthrough
        // is the simpler default.
        let ports: Vec<PortRuntime> = if spec.ports.is_empty() {
            vec![PortRuntime {
                spec: PortSpec {
                    id: "main".into(),
                    from: layers[0].spec.id.clone(),
                },
                layer_index: 0,
            }]
        } else {
            spec.ports
                .iter()
                .map(|p| {
                    let li = layers.iter().position(|l| l.spec.id == p.from).unwrap_or(0);
                    PortRuntime {
                        spec: p.clone(),
                        layer_index: li,
                    }
                })
                .collect()
        };

        Self {
            spec,
            block,
            layers,
            ports,
            muted: false,
            mic_scratches,
            routed_scratches: Vec::new(),
            block_frames,
            resize_events: 0,
        }
    }

    /// Render this engine into its Layer out_buffers.
    /// Always writes Layer outputs (zeroed when `muted`) — downstream
    /// edges still read them, just summing zeros.
    pub fn render(&mut self, block_frames: usize) {
        // Resize scratches if the audio callback changed block size.
        // This is the only place allocation can happen — and it's a
        // change-of-size event, NOT a hot path. In normal operation
        // block_frames is stable between callbacks.
        if block_frames != self.block_frames {
            for buf in &mut self.mic_scratches {
                buf.resize(block_frames * 2, 0.0);
            }
            for layer in &mut self.layers {
                layer.out_buffer.resize(block_frames * 2, 0.0);
            }
            self.block_frames = block_frames;
            self.resize_events = self.resize_events.saturating_add(1);
            tracing::warn!(
                engine = %self.spec.name,
                block_frames,
                "EngineInstance: resized render buffers"
            );
        }

        // Zero per-mic scratches and Layer outputs.
        for buf in &mut self.mic_scratches {
            for s in buf.iter_mut() {
                *s = 0.0;
            }
        }
        for layer in &mut self.layers {
            for s in layer.out_buffer.iter_mut() {
                *s = 0.0;
            }
        }

        if self.muted {
            return;
        }

        // Splay voices across mic scratches. `render_multi` takes
        // `&mut [Vec<f32>]` directly so there's NO heap allocation in
        // the audio callback — we hand the existing scratch storage
        // straight through.
        self.block.render_multi(&mut self.mic_scratches);

        // Copy each Layer's mic scratch into its out_buffer with
        // gain/pan applied. Bypass = silent contribution.
        for layer in &mut self.layers {
            if layer.spec.bypass {
                continue;
            }
            let src = &self.mic_scratches[layer.mic_index];
            let pl = layer.pan_l;
            let pr = layer.pan_r;
            for (out_pair, in_pair) in layer
                .out_buffer
                .chunks_exact_mut(2)
                .zip(src.chunks_exact(2))
            {
                out_pair[0] += in_pair[0] * pl;
                out_pair[1] += in_pair[1] * pr;
            }
        }
    }

    /// Render this engine into per-bus stereo buffers routed by articulation
    /// class (stem-split document rendering; `route_longs`/`route_shorts`
    /// index into `outputs`). Mirrors [`render`](Self::render)'s exact op
    /// sequence — voices accumulate into per-(bus, mic) scratches in pool
    /// order, then the first port's layer copies its mic scratch into each
    /// bus with the same gain/pan weighting — so routing every class to one
    /// bus is bit-identical to `render` + reading the first port, and split
    /// buses carry each voice wholly. `outputs` must arrive zeroed.
    /// Offline-only (allocates on first use / size change).
    pub fn render_routed_buses(
        &mut self,
        outputs: &mut [Vec<f32>],
        route_longs: usize,
        route_shorts: usize,
        block_frames: usize,
    ) {
        let nmics = self.mic_scratches.len().max(1);
        let nbuses = outputs.len().max(1);
        let want = nbuses * nmics;
        if self.routed_scratches.len() != want {
            self.routed_scratches = (0..want).map(|_| Vec::new()).collect();
        }
        for buf in &mut self.routed_scratches {
            buf.clear();
            buf.resize(block_frames * 2, 0.0);
        }
        if self.muted {
            return;
        }

        self.block
            .render_matrix(&mut self.routed_scratches, nmics, route_longs, route_shorts);

        // Same fold as render() + bank::render's first-port read: one layer,
        // gain/pan-weighted copy of its mic scratch — done once per bus.
        let Some(port) = self.ports.first() else {
            return;
        };
        let layer = &self.layers[port.layer_index];
        if layer.spec.bypass {
            return;
        }
        let (pl, pr) = (layer.pan_l, layer.pan_r);
        for (bus, out) in outputs.iter_mut().enumerate() {
            let src = &self.routed_scratches[bus * nmics + layer.mic_index];
            for (out_pair, in_pair) in out.chunks_exact_mut(2).zip(src.chunks_exact(2)) {
                out_pair[0] += in_pair[0] * pl;
                out_pair[1] += in_pair[1] * pr;
            }
        }
    }

    /// Per-mic interleaved-stereo scratch buffers, in `mic_ids()` order.
    /// Valid after [`render`](Self::render); the drum mixer reads these to
    /// route each mic (close → direct, overhead/room → bus send).
    pub fn mic_scratches(&self) -> &[Vec<f32>] {
        &self.mic_scratches
    }

    /// Mic ids in `mic_scratches` order (pack mic positions). A single
    /// implicit `"default"` mic when the pack declares none.
    pub fn mic_ids(&self) -> Vec<String> {
        let mics = self.block.mics();
        if mics.is_empty() {
            vec!["default".to_string()]
        } else {
            mics.to_vec()
        }
    }

    /// Accumulate this engine's raw per-mic scratch buffers (interleaved
    /// stereo) into `buses`, keyed by mic id. Call after [`render`](Self::render).
    /// Same mic id across engines (e.g. "Overhead", "Room Close") sums into one
    /// shared bus — the drum mic taxonomy. Pre-layer gain (the bus applies its
    /// own); skips a muted engine (its scratches are already zero).
    pub fn accumulate_mic_buses(
        &self,
        buses: &mut std::collections::BTreeMap<String, Vec<f32>>,
        block_frames: usize,
    ) {
        let ids = self.mic_ids();
        for (i, scratch) in self.mic_scratches.iter().enumerate() {
            let id = ids.get(i).cloned().unwrap_or_else(|| format!("mic{i}"));
            let buf = buses
                .entry(id)
                .or_insert_with(|| vec![0.0; block_frames * 2]);
            let n = buf.len().min(scratch.len());
            for k in 0..n {
                buf[k] += scratch[k];
            }
        }
    }

    /// Get the buffer of a port by id. Returns None if the port doesn't
    /// exist on this engine.
    pub fn port_buffer(&self, port_id: &str) -> Option<&[f32]> {
        let port = self.ports.iter().find(|p| p.spec.id == port_id)?;
        Some(&self.layers[port.layer_index].out_buffer)
    }

    pub fn active_voices(&self) -> usize {
        self.block.active_voices()
    }

    pub fn stolen_voices(&self) -> usize {
        self.block.stolen_voices()
    }

    pub fn cache_misses(&self) -> usize {
        self.block.cache_misses()
    }

    pub fn loaded_sample_bytes(&self) -> usize {
        self.block.loaded_sample_bytes()
    }

    pub fn evict_cache_until_under_budget(&self, budget_bytes: usize) -> EvictStats {
        self.block.evict_cache_until_under_budget(budget_bytes)
    }

    pub fn sample_misses(&self) -> usize {
        self.block.sample_misses()
    }

    pub fn recent_cache_misses(&self) -> Vec<String> {
        self.block.recent_cache_misses()
    }

    pub fn recent_sample_misses(&self) -> Vec<String> {
        self.block.recent_sample_misses()
    }

    pub fn resize_events(&self) -> u64 {
        self.resize_events
            .saturating_add(self.block.resize_events())
    }

    pub fn all_notes_off(&mut self) {
        self.block.all_notes_off();
    }

    pub fn panic(&mut self) {
        self.block.panic();
    }

    /// Live-mode: set Layer mute. Identifies the layer by its spec id.
    pub fn set_layer_bypass(&mut self, layer_id: &str, bypass: bool) -> bool {
        if let Some(layer) = self.layers.iter_mut().find(|l| l.spec.id == layer_id) {
            layer.spec.bypass = bypass;
            true
        } else {
            false
        }
    }

    /// Live-mode: set Layer gain (dB). Recomputes pan multipliers.
    pub fn set_layer_gain_db(&mut self, layer_id: &str, gain_db: f32) -> bool {
        if let Some(layer) = self.layers.iter_mut().find(|l| l.spec.id == layer_id) {
            layer.spec.gain_db = gain_db;
            layer.gain_lin = db_to_lin(gain_db);
            let (l, r) = pan_gains(layer.spec.pan);
            layer.pan_l = l * layer.gain_lin;
            layer.pan_r = r * layer.gain_lin;
            true
        } else {
            false
        }
    }
}

// ── Module runtime ────────────────────────────────────────────────────────

/// One Module instance. Holds pre-allocated input + output buffers.
/// Render: sums all input buffers into one internal mix, copies the mix
/// to each output buffer. FX chain is parse-only.
pub struct ModuleInstance {
    pub spec: ModuleSpec,
    pub muted: bool,
    /// Input buffers, parallel to `spec.inputs`. Pre-allocated.
    input_buffers: Vec<Vec<f32>>,
    /// Output buffers, parallel to `spec.outputs`. Pre-allocated.
    output_buffers: Vec<Vec<f32>>,
    block_frames: usize,
}

impl ModuleInstance {
    pub fn new(spec: ModuleSpec, block_frames: usize) -> Self {
        let input_buffers = (0..spec.inputs.len().max(1))
            .map(|_| vec![0.0; block_frames * 2])
            .collect();
        let output_buffers = (0..spec.outputs.len().max(1))
            .map(|_| vec![0.0; block_frames * 2])
            .collect();
        Self {
            spec,
            muted: false,
            input_buffers,
            output_buffers,
            block_frames,
        }
    }

    pub fn input_index(&self, port_id: &str) -> Option<usize> {
        self.spec.inputs.iter().position(|p| p.id == port_id)
    }

    pub fn output_index(&self, port_id: &str) -> Option<usize> {
        self.spec.outputs.iter().position(|p| p.id == port_id)
    }

    pub fn input_buffer_mut(&mut self, idx: usize) -> Option<&mut [f32]> {
        self.input_buffers.get_mut(idx).map(|v| v.as_mut_slice())
    }

    pub fn output_buffer(&self, idx: usize) -> Option<&[f32]> {
        self.output_buffers.get(idx).map(|v| v.as_slice())
    }

    /// Zero input buffers. Call at the start of each audio block before
    /// edges sum into them.
    pub fn clear_inputs(&mut self, block_frames: usize) {
        if block_frames != self.block_frames {
            for buf in &mut self.input_buffers {
                buf.resize(block_frames * 2, 0.0);
            }
            for buf in &mut self.output_buffers {
                buf.resize(block_frames * 2, 0.0);
            }
            self.block_frames = block_frames;
        }
        for buf in &mut self.input_buffers {
            for s in buf.iter_mut() {
                *s = 0.0;
            }
        }
    }

    /// Render: sum all inputs to a mix, copy to each output. FX chain
    /// no-op. Output buffers are zeroed first.
    pub fn render(&mut self) {
        for buf in &mut self.output_buffers {
            for s in buf.iter_mut() {
                *s = 0.0;
            }
        }
        if self.muted {
            return;
        }
        // Sum all inputs into output[0], then mirror to other outputs.
        if self.output_buffers.is_empty() {
            return;
        }
        // First output = mix of all inputs.
        for input in &self.input_buffers {
            let out = &mut self.output_buffers[0];
            for (o, i) in out.iter_mut().zip(input.iter()) {
                *o += *i;
            }
        }
        // Subsequent outputs = copy of first.
        if self.output_buffers.len() > 1 {
            // Split the slice so we can borrow [0] immutably and rest mutably.
            let (first, rest) = self.output_buffers.split_at_mut(1);
            for out in rest {
                out.copy_from_slice(&first[0]);
            }
        }
    }
}

// ── Preset runtime ────────────────────────────────────────────────────────

/// Resolved buffer reference into the Preset's render graph.
#[derive(Debug, Clone, Copy)]
pub enum BufferRef {
    /// `engines[idx].ports[port_idx]` → reads from a Layer's out_buffer.
    EnginePort { engine_idx: usize, port_idx: usize },
    /// `modules[idx].outputs[port_idx]`.
    ModuleOutput { module_idx: usize, port_idx: usize },
    /// `modules[idx].inputs[port_idx]`.
    ModuleInput { module_idx: usize, port_idx: usize },
    /// The preset master output (the buffer passed to `render`).
    Master,
}

#[derive(Debug, Clone)]
pub struct ResolvedEdge {
    pub from: BufferRef,
    pub to: BufferRef,
    pub gain_lin: f32,
}

/// One full Preset runtime. Holds N engines, M modules, the routing
/// edges, and topology metadata so render is single-pass after engines
/// are rendered.
pub struct PresetRuntime {
    pub name: String,
    pub engines: Vec<EngineInstance>,
    pub engine_id_to_idx: HashMap<String, usize>,
    pub modules: Vec<ModuleInstance>,
    pub module_id_to_idx: HashMap<String, usize>,
    /// Topological order of modules (indices into `modules`).
    pub module_order: Vec<usize>,
    pub edges: Vec<ResolvedEdge>,
    /// Per-MIDI-note → `(engine index, optional articulation)` to dispatch to.
    /// The articulation, when present, fires that articulation on the target
    /// engine ignoring key (percussion routing); `None` = key-based selection.
    pub note_routing: HashMap<u8, Vec<(usize, Option<String>)>>,
    /// Send-based drum mixer (close mics direct, OH/Room mics sent to shared
    /// buses). Built only for presets with no authored master edges (the drum
    /// fallback path); `None` keeps the original graph render.
    pub mixer: Option<crate::mixer::DrumMixer>,
    /// Master FX chain applied to the preset's master sum after the engine /
    /// edge graph (or after the drum mixer's own master_fx). Used by
    /// non-drum presets (orchestral, synth, …) for a REAPER-style master
    /// FX bus when there is no per-strip mixer. Mutated by the bank API.
    pub master_fx: crate::mixer::FxChain,
    /// Sample rate this runtime is rendering at — needed so `install_plugin`
    /// can `prepare` the hosted plugin without a callback.
    pub sample_rate: u32,
}

impl PresetRuntime {
    /// Build engines + modules from a preset; resolve all routing strings
    /// to `BufferRef`s. Cycle detection happens here — returns an error
    /// if the module graph contains one.
    pub fn build(
        preset: &PresetSpec,
        preset_dir: &Path,
        block_frames: usize,
        sample_rate: u32,
        engines: Vec<(String, EngineSpec, SamplerBlock)>,
    ) -> Result<Self, crate::SamplerError> {
        // Engines come pre-built; sample_rate is forwarded to the drum mixer
        // so hosted FX plugins can be `prepare`d at install time.
        let mut engine_instances: Vec<EngineInstance> = Vec::new();
        let mut engine_id_to_idx: HashMap<String, usize> = HashMap::new();
        for (id, spec, block) in engines {
            engine_id_to_idx.insert(id.clone(), engine_instances.len());
            engine_instances.push(EngineInstance::new(spec, block, block_frames));
        }
        // Apply preset-level engine refs (mute) — caller still applies
        // `block` overrides via SamplerBlock::apply_overrides earlier.
        for er in &preset.engines {
            if let Some(idx) = engine_id_to_idx.get(&er.id) {
                engine_instances[*idx].muted = er.mute;
                if !er.articulation.is_empty() {
                    engine_instances[*idx]
                        .block
                        .pin_articulation(Some(er.articulation.clone()));
                }
                if !er.choke_group.is_empty() {
                    engine_instances[*idx]
                        .block
                        .set_choke_group(Some(&er.choke_group), &er.choke_on);
                }
            }
        }

        let mut module_instances: Vec<ModuleInstance> = Vec::new();
        let mut module_id_to_idx: HashMap<String, usize> = HashMap::new();
        for mr in &preset.modules {
            let mod_path = if Path::new(&mr.module).is_absolute() {
                std::path::PathBuf::from(&mr.module)
            } else {
                preset_dir.join(&mr.module)
            };
            let spec = match ModuleSpec::from_file(&mod_path) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        module = %mr.module,
                        err = %e,
                        "module parse failed — falling back to passthrough"
                    );
                    ModuleSpec::passthrough(&mr.id)
                }
            };
            module_id_to_idx.insert(mr.id.clone(), module_instances.len());
            module_instances.push(ModuleInstance::new(spec, block_frames));
        }

        // Resolve routing edges.
        let mut edges: Vec<ResolvedEdge> = Vec::new();
        for rule in &preset.routing {
            let from = match resolve_source(
                &rule.from,
                &engine_id_to_idx,
                &module_id_to_idx,
                &engine_instances,
                &module_instances,
            ) {
                Some(r) => r,
                None => {
                    tracing::warn!(from = %rule.from, "routing source unresolved — skipping");
                    continue;
                }
            };
            let to = match resolve_target(&rule.to, &module_id_to_idx, &module_instances) {
                Some(r) => r,
                None => {
                    tracing::warn!(to = %rule.to, "routing target unresolved — skipping");
                    continue;
                }
            };
            edges.push(ResolvedEdge {
                from,
                to,
                gain_lin: db_to_lin(rule.gain_db),
            });
        }

        // Topological sort modules by dependencies expressed via edges.
        // Edge from module_a.output → module_b.input means a precedes b.
        let module_order = topo_sort_modules(&edges, module_instances.len())?;

        // Note routing — resolve target ids to engine indices.
        let mut note_routing: HashMap<u8, Vec<(usize, Option<String>)>> = HashMap::new();
        for nr in &preset.note_routing {
            let artic = if nr.articulation.is_empty() {
                None
            } else {
                Some(nr.articulation.clone())
            };
            let mut targets: Vec<(usize, Option<String>)> = Vec::new();
            for t in &nr.targets {
                if let Some(idx) = engine_id_to_idx.get(t) {
                    targets.push((*idx, artic.clone()));
                }
            }
            if !targets.is_empty() {
                note_routing.insert(nr.note, targets);
            }
        }

        // Build the send-based drum mixer for presets that rely on the
        // direct-to-master fallback (no authored module/master edges) — i.e.
        // drum kits. Graph-routed presets (orchestral, etc.) keep the edge
        // render untouched. The mixer routes each engine's close mics direct
        // and its overhead/room mics as sends to shared bus tracks.
        let has_master_edges = edges.iter().any(|e| matches!(e.to, BufferRef::Master));
        let mixer = if has_master_edges {
            None
        } else {
            let idx_to_id: HashMap<usize, String> = engine_id_to_idx
                .iter()
                .map(|(id, &idx)| (idx, id.clone()))
                .collect();
            let engine_mics: Vec<(String, Vec<String>)> = engine_instances
                .iter()
                .enumerate()
                .map(|(idx, eng)| {
                    let label = idx_to_id
                        .get(&idx)
                        .cloned()
                        .unwrap_or_else(|| eng.spec.name.clone());
                    (label, eng.mic_ids())
                })
                .collect();
            let m = crate::mixer::DrumMixer::build(&engine_mics, sample_rate);
            if m.is_empty() { None } else { Some(m) }
        };

        Ok(Self {
            name: preset.name.clone(),
            engines: engine_instances,
            engine_id_to_idx,
            modules: module_instances,
            module_id_to_idx,
            module_order,
            edges,
            note_routing,
            mixer,
            master_fx: crate::mixer::FxChain::default(),
            sample_rate,
        })
    }

    /// Install a hosted plugin in the preset's master FX chain (the chain
    /// applied after engine/edge render or after the drum mixer's master).
    /// Returns the new slot index.
    pub fn install_master_plugin(
        &mut self,
        plugin: signal_plugin_host::HostedPlugin,
    ) -> Result<usize, signal_plugin_host::PluginError> {
        let mut plugin = plugin;
        plugin.prepare(self.sample_rate as f64, crate::mixer::FX_PREPARE_BLOCK)?;
        let display_name = plugin.descriptor().name.clone();
        self.master_fx.slots.push(crate::mixer::FxSlot {
            backend: crate::mixer::FxBackend::Hosted(plugin),
            bypassed: false,
            display_name,
        });
        Ok(self.master_fx.slots.len() - 1)
    }

    /// Install a NAM model on the preset's master FX chain.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn install_master_nam(
        &mut self,
        model_path: impl AsRef<std::path::Path>,
    ) -> Result<usize, String> {
        let nam = crate::nam::NamProcessor::load(
            model_path,
            self.sample_rate as f64,
            crate::mixer::FX_PREPARE_BLOCK as usize,
        )?;
        let display_name = nam.display_name.clone();
        self.master_fx.slots.push(crate::mixer::FxSlot {
            backend: crate::mixer::FxBackend::Nam(nam),
            bypassed: false,
            display_name,
        });
        Ok(self.master_fx.slots.len() - 1)
    }

    pub fn remove_master_plugin(&mut self, slot_idx: usize) {
        if slot_idx < self.master_fx.slots.len() {
            let mut slot = self.master_fx.slots.remove(slot_idx);
            slot.backend.deactivate();
        }
    }

    /// Render the preset into per-mic **buses** (interleaved stereo each),
    /// keyed by mic id. Every engine is rendered, then its per-mic scratch
    /// buffers are summed into the matching bus — so all pieces' "Overhead"
    /// land on one Overhead bus, all "Room Close" on one Room Close bus, etc.
    /// (piece-specific close-mic ids stay separate). The host (e.g. the daw
    /// Sampler Block) maps these buses onto track channels. `block_frames` =
    /// frames per channel.
    pub fn render_buses(
        &mut self,
        block_frames: usize,
    ) -> std::collections::BTreeMap<String, Vec<f32>> {
        for eng in &mut self.engines {
            eng.render(block_frames);
        }
        let mut buses = std::collections::BTreeMap::new();
        for eng in &self.engines {
            eng.accumulate_mic_buses(&mut buses, block_frames);
        }
        buses
    }

    /// Render the entire preset graph into `master` (interleaved stereo,
    /// `+=` accumulation). No allocation in the hot path.
    pub fn render(&mut self, master: &mut [f32]) {
        // Drum presets route through the send-based mixer instead of the edge
        // graph / first-port fallback.
        if self.mixer.is_some() {
            self.render_drum_mix(master);
            return;
        }

        let block_frames = master.len() / 2;

        // 1. Render engines.
        for eng in &mut self.engines {
            eng.render(block_frames);
        }

        // 2. Clear all module inputs (they accumulate from edges).
        for m in &mut self.modules {
            m.clear_inputs(block_frames);
        }

        // 3. Walk edges in two passes: first sum into modules (any order
        //    is fine — module inputs are accumulators), then sum into
        //    master AFTER modules render.
        //
        //    To honour module->module dependencies we render modules in
        //    topological order: for each module in order, first sum its
        //    incoming edges, then call render(). Edges leaving the
        //    module then become valid sources for downstream modules
        //    and master.
        //
        //    Implementation: pre-bucket edges by target module index for
        //    cheap per-module lookup. Since edges count is small we
        //    iterate naively.

        for &mod_idx in &self.module_order {
            // Sum every edge targeting this module's inputs.
            for edge in &self.edges {
                let (target_mod_idx, target_port_idx) = match edge.to {
                    BufferRef::ModuleInput {
                        module_idx,
                        port_idx,
                    } => (module_idx, port_idx),
                    _ => continue,
                };
                if target_mod_idx != mod_idx {
                    continue;
                }
                // Source might be an engine port or another (already-rendered) module output.
                // Borrow split: we read `from` (immutable) and write to `modules[mod_idx]` (mutable).
                // Since `from` may itself be a module output (different index), we use a manual
                // approach: read the source slice into a local accumulation via copy.
                accumulate_edge_into_module(
                    edge,
                    &self.engines,
                    &mut self.modules,
                    target_mod_idx,
                    target_port_idx,
                );
            }
            self.modules[mod_idx].render();
        }

        // 4. Edges into master: sum into the user-supplied output buffer.
        for edge in &self.edges {
            if !matches!(edge.to, BufferRef::Master) {
                continue;
            }
            sum_buffer_ref_into(
                &edge.from,
                &self.engines,
                &self.modules,
                edge.gain_lin,
                master,
            );
        }

        // 5. If there are NO routing edges to master at all, fall back
        //    to summing every engine's first port into master. This is
        //    the back-compat path used by single-engine presets and
        //    `load_pack` / `load_engine_spec`.
        let has_master_edges = self.edges.iter().any(|e| matches!(e.to, BufferRef::Master));
        if !has_master_edges {
            for eng in &self.engines {
                if let Some(port) = eng.ports.first() {
                    let src = &eng.layers[port.layer_index].out_buffer;
                    for (m, s) in master.iter_mut().zip(src.iter()) {
                        *m += *s;
                    }
                }
            }
        }

        // 6. Master FX chain — applied after the entire preset master has
        //    been summed. Non-drum presets use this for REAPER-style master
        //    bus FX (the drum-mixer path applies its own master_fx earlier).
        if !self.master_fx.is_empty() {
            let len = master.len();
            self.master_fx.process(&mut master[..len]);
        }
    }

    /// Send-based drum render: each engine's close mics go direct to master;
    /// its overhead/room mics are sent (per-piece level) to shared bus tracks
    /// that sum and feed master. The whole kit is summed into a scratch so the
    /// master fader + meter apply to this preset alone, then added to `master`.
    /// Writes per-channel/send/bus + master peak meters.
    ///
    /// Solo is solo-in-place: when anything is soloed, only soloed channels /
    /// buses reach master. A send still routes if its own send OR its target
    /// bus is soloed, and a bus still reaches master if it or any of its sends
    /// is soloed — so soloing a bus (or a send into it) is audible.
    fn render_drum_mix(&mut self, master: &mut [f32]) {
        let block_frames = master.len() / 2;
        // Split-borrow: engines (mut, to render), mixer (mut, to route), and
        // master_fx (mut, applied after the mixer's master sum).
        let Self {
            engines,
            mixer,
            master_fx,
            ..
        } = self;
        for eng in engines.iter_mut() {
            eng.render(block_frames);
        }
        let Some(mixer) = mixer.as_mut() else { return };
        let any_solo = mixer.any_solo();
        // Field-destructure so the scratch / bus-acc mutable borrows and the
        // immutable channel/send/meter reads don't collide on `*mixer`.
        let crate::mixer::DrumMixer {
            pieces,
            channels,
            sends,
            buses,
            master_gain_lin,
            master_muted,
            master_fx: drum_master_fx,
            meters,
            master_scratch,
            channel_scratch,
            piece_peak_scratch,
            ..
        } = mixer;

        // Per-piece block-peak accumulator (max over the piece's channels+sends).
        if piece_peak_scratch.len() != pieces.len() {
            piece_peak_scratch.resize(pieces.len(), 0.0);
        }
        for p in piece_peak_scratch.iter_mut() {
            *p = 0.0;
        }

        // Mix this preset into its own scratch (so master gain/meter are local).
        if master_scratch.len() != master.len() {
            master_scratch.resize(master.len(), 0.0);
        }
        for s in master_scratch.iter_mut() {
            *s = 0.0;
        }

        // Channel-scratch sized to one full block; re-used across each direct
        // channel's FX-chain run (REAPER-style pre-fader FX).
        if channel_scratch.len() != master.len() {
            channel_scratch.resize(master.len(), 0.0);
        }

        // Size + zero bus accumulators for this block.
        for bus in buses.iter_mut() {
            if bus.acc.len() != master.len() {
                bus.acc.resize(master.len(), 0.0);
            }
            for s in &mut bus.acc {
                *s = 0.0;
            }
        }

        // Direct close-mic channels → channel scratch → FX → * gain → master.
        // A channel is silent if its own OR its piece's mute is set; solo-in-
        // place passes it if the channel or its piece is soloed. The piece
        // fader multiplies the channel gain (uniform per-drum level).
        for (ci, ch) in channels.iter_mut().enumerate() {
            let piece = pieces.get(ch.engine_idx);
            let p_muted = piece.map(|p| p.muted).unwrap_or(false);
            let p_soloed = piece.map(|p| p.soloed).unwrap_or(false);
            let p_gain = piece.map(|p| p.gain_lin).unwrap_or(1.0);
            let active = !ch.muted && !p_muted && (!any_solo || ch.soloed || p_soloed);
            let mut peak = 0.0f32;
            if active {
                if let Some(src) = engines
                    .get(ch.engine_idx)
                    .and_then(|e| e.mic_scratches().get(ch.mic_idx))
                {
                    let n = channel_scratch.len().min(src.len());
                    // Copy mic source into the channel scratch and run any FX
                    // in place. Empty chain = single copy + zero work.
                    channel_scratch[..n].copy_from_slice(&src[..n]);
                    for s in &mut channel_scratch[n..] {
                        *s = 0.0;
                    }
                    if !ch.fx.is_empty() {
                        ch.fx.process(&mut channel_scratch[..n]);
                    }
                    let g = ch.gain_lin * p_gain;
                    for k in 0..n {
                        let v = channel_scratch[k] * g;
                        master_scratch[k] += v;
                        let a = v.abs();
                        if a > peak {
                            peak = a;
                        }
                    }
                }
            }
            meters.set_channel_peak(ci, peak);
            if let Some(pp) = piece_peak_scratch.get_mut(ch.engine_idx) {
                if peak > *pp {
                    *pp = peak;
                }
            }
        }

        // Bus-mic sends → bus accumulators. A send routes if its own send or
        // its target bus is soloed (so bus solo is audible).
        for (si, snd) in sends.iter().enumerate() {
            let bus_soloed = buses.get(snd.bus_idx).map(|b| b.soloed).unwrap_or(false);
            let piece = pieces.get(snd.engine_idx);
            let p_muted = piece.map(|p| p.muted).unwrap_or(false);
            let p_soloed = piece.map(|p| p.soloed).unwrap_or(false);
            let p_gain = piece.map(|p| p.gain_lin).unwrap_or(1.0);
            let active =
                !snd.muted && !p_muted && (!any_solo || snd.soloed || p_soloed || bus_soloed);
            let mut peak = 0.0f32;
            if active {
                if let Some(src) = engines
                    .get(snd.engine_idx)
                    .and_then(|e| e.mic_scratches().get(snd.mic_idx))
                {
                    let lvl = snd.level_lin * p_gain;
                    if let Some(bus) = buses.get_mut(snd.bus_idx) {
                        let n = bus.acc.len().min(src.len());
                        for (dst, &s) in bus.acc[..n].iter_mut().zip(&src[..n]) {
                            let v = s * lvl;
                            *dst += v;
                            let a = v.abs();
                            if a > peak {
                                peak = a;
                            }
                        }
                    }
                }
            }
            meters.set_send_peak(si, peak);
            if let Some(pp) = piece_peak_scratch.get_mut(snd.engine_idx) {
                if peak > *pp {
                    *pp = peak;
                }
            }
        }

        // Shared bus tracks: bus.acc → FX → * bus gain → master. A bus reaches
        // master if it or any send feeding it is soloed.
        for (bi, bus) in buses.iter_mut().enumerate() {
            // A bus reaches master under solo if it, one of its sends, or the
            // *piece* feeding one of its sends is soloed.
            let pulled = bus.soloed
                || sends.iter().any(|s| {
                    s.bus_idx == bi
                        && (s.soloed || pieces.get(s.engine_idx).map(|p| p.soloed).unwrap_or(false))
                });
            let active = !bus.muted && (!any_solo || pulled);
            let mut peak = 0.0f32;
            if active {
                if !bus.fx.is_empty() {
                    let len = bus.acc.len();
                    bus.fx.process(&mut bus.acc[..len]);
                }
                let g = bus.gain_lin;
                let n = master_scratch.len().min(bus.acc.len());
                for (dst, &s) in master_scratch[..n].iter_mut().zip(&bus.acc[..n]) {
                    let v = s * g;
                    *dst += v;
                    let a = v.abs();
                    if a > peak {
                        peak = a;
                    }
                }
            }
            meters.set_bus_peak(bi, peak);
        }

        // Publish the per-piece meters (max over each piece's channels + sends).
        for (i, &pk) in piece_peak_scratch.iter().enumerate() {
            meters.set_piece_peak(i, pk);
        }

        // Drum-mixer master FX → preset master FX → master fader + meter.
        // Two chains so a drum preset can have its own per-kit master FX
        // separate from a higher-level preset chain.
        if !drum_master_fx.is_empty() {
            let len = master_scratch.len();
            drum_master_fx.process(&mut master_scratch[..len]);
        }
        if !master_fx.is_empty() {
            let len = master_scratch.len();
            master_fx.process(&mut master_scratch[..len]);
        }

        let mg = if *master_muted { 0.0 } else { *master_gain_lin };
        let mut master_peak = 0.0f32;
        let n = master.len().min(master_scratch.len());
        for k in 0..n {
            let v = master_scratch[k] * mg;
            master[k] += v;
            let a = v.abs();
            if a > master_peak {
                master_peak = a;
            }
        }
        meters.set_master_peak(master_peak);
    }

    /// The drum mixer, if this preset uses send-based mic routing.
    pub fn mixer(&self) -> Option<&crate::mixer::DrumMixer> {
        self.mixer.as_ref()
    }

    /// Mutable access to the drum mixer (faders / mutes).
    pub fn mixer_mut(&mut self) -> Option<&mut crate::mixer::DrumMixer> {
        self.mixer.as_mut()
    }

    pub fn active_voices(&self) -> usize {
        self.engines.iter().map(EngineInstance::active_voices).sum()
    }

    pub fn stolen_voices(&self) -> usize {
        self.engines.iter().map(EngineInstance::stolen_voices).sum()
    }

    pub fn cache_misses(&self) -> usize {
        self.engines.iter().map(EngineInstance::cache_misses).sum()
    }

    pub fn loaded_sample_bytes(&self) -> usize {
        self.engines
            .iter()
            .map(EngineInstance::loaded_sample_bytes)
            .sum()
    }

    pub fn evict_cache_until_under_budget(&self, budget_bytes: usize) -> EvictStats {
        let mut stats = EvictStats::default();
        for engine in &self.engines {
            stats.add(engine.evict_cache_until_under_budget(budget_bytes));
        }
        stats
    }

    pub fn sample_misses(&self) -> usize {
        self.engines.iter().map(EngineInstance::sample_misses).sum()
    }

    pub fn recent_cache_misses(&self) -> Vec<String> {
        self.engines
            .iter()
            .flat_map(EngineInstance::recent_cache_misses)
            .collect()
    }

    pub fn recent_sample_misses(&self) -> Vec<String> {
        self.engines
            .iter()
            .flat_map(EngineInstance::recent_sample_misses)
            .collect()
    }

    pub fn resize_events(&self) -> u64 {
        self.engines.iter().map(EngineInstance::resize_events).sum()
    }

    pub fn all_notes_off(&mut self) {
        for engine in &mut self.engines {
            engine.all_notes_off();
        }
    }

    pub fn panic(&mut self) {
        for engine in &mut self.engines {
            engine.panic();
        }
    }

    /// Live-mode: mute/unmute a module by id.
    pub fn set_module_muted(&mut self, module_id: &str, muted: bool) -> bool {
        if let Some(&idx) = self.module_id_to_idx.get(module_id) {
            self.modules[idx].muted = muted;
            true
        } else {
            false
        }
    }
}

fn accumulate_edge_into_module(
    edge: &ResolvedEdge,
    engines: &[EngineInstance],
    modules: &mut [ModuleInstance],
    target_mod_idx: usize,
    target_port_idx: usize,
) {
    let g = edge.gain_lin;
    // Get source slice WITHOUT overlapping borrow with the target module.
    match edge.from {
        BufferRef::EnginePort {
            engine_idx,
            port_idx,
        } => {
            let eng = &engines[engine_idx];
            if let Some(port) = eng.ports.get(port_idx) {
                let src = &eng.layers[port.layer_index].out_buffer;
                if let Some(dst) = modules[target_mod_idx].input_buffer_mut(target_port_idx) {
                    for (d, s) in dst.iter_mut().zip(src.iter()) {
                        *d += *s * g;
                    }
                }
            }
        }
        BufferRef::ModuleOutput {
            module_idx,
            port_idx,
        } => {
            // Disjoint mutable borrow: split modules around target_mod_idx.
            if module_idx == target_mod_idx {
                // Self-loop — ignored (cycle detection should have caught
                // this; defensively skip).
                return;
            }
            let (lo, hi) = if module_idx < target_mod_idx {
                let (a, b) = modules.split_at_mut(target_mod_idx);
                (&a[module_idx], &mut b[0])
            } else {
                let (a, b) = modules.split_at_mut(module_idx);
                (&b[0], &mut a[target_mod_idx])
            };
            if let Some(src) = lo.output_buffer(port_idx) {
                if let Some(dst) = hi.input_buffer_mut(target_port_idx) {
                    for (d, s) in dst.iter_mut().zip(src.iter()) {
                        *d += *s * g;
                    }
                }
            }
        }
        _ => {}
    }
}

fn sum_buffer_ref_into(
    bref: &BufferRef,
    engines: &[EngineInstance],
    modules: &[ModuleInstance],
    gain: f32,
    dst: &mut [f32],
) {
    match bref {
        BufferRef::EnginePort {
            engine_idx,
            port_idx,
        } => {
            let eng = &engines[*engine_idx];
            if let Some(port) = eng.ports.get(*port_idx) {
                let src = &eng.layers[port.layer_index].out_buffer;
                for (d, s) in dst.iter_mut().zip(src.iter()) {
                    *d += *s * gain;
                }
            }
        }
        BufferRef::ModuleOutput {
            module_idx,
            port_idx,
        } => {
            if let Some(src) = modules[*module_idx].output_buffer(*port_idx) {
                for (d, s) in dst.iter_mut().zip(src.iter()) {
                    *d += *s * gain;
                }
            }
        }
        _ => {}
    }
}

fn resolve_source(
    s: &str,
    engine_idx: &HashMap<String, usize>,
    module_idx: &HashMap<String, usize>,
    engines: &[EngineInstance],
    modules: &[ModuleInstance],
) -> Option<BufferRef> {
    let (head, tail) = match s.split_once('.') {
        Some((h, t)) => (h, Some(t)),
        None => (s, None),
    };
    if let Some(&ei) = engine_idx.get(head) {
        let eng = &engines[ei];
        let port_id = tail.unwrap_or("main");
        let pi = eng.ports.iter().position(|p| p.spec.id == port_id)?;
        return Some(BufferRef::EnginePort {
            engine_idx: ei,
            port_idx: pi,
        });
    }
    if let Some(&mi) = module_idx.get(head) {
        let m = &modules[mi];
        let port_id = tail.unwrap_or("out");
        let pi = m.spec.outputs.iter().position(|p| p.id == port_id)?;
        return Some(BufferRef::ModuleOutput {
            module_idx: mi,
            port_idx: pi,
        });
    }
    None
}

fn resolve_target(
    s: &str,
    module_idx: &HashMap<String, usize>,
    modules: &[ModuleInstance],
) -> Option<BufferRef> {
    if s == "master" {
        return Some(BufferRef::Master);
    }
    let (head, tail) = match s.split_once('.') {
        Some((h, t)) => (h, Some(t)),
        None => (s, None),
    };
    if let Some(&mi) = module_idx.get(head) {
        let m = &modules[mi];
        let port_id = tail.unwrap_or("in");
        let pi = m.spec.inputs.iter().position(|p| p.id == port_id)?;
        return Some(BufferRef::ModuleInput {
            module_idx: mi,
            port_idx: pi,
        });
    }
    None
}

fn topo_sort_modules(edges: &[ResolvedEdge], n: usize) -> Result<Vec<usize>, crate::SamplerError> {
    // Build module->module dependency graph.
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n]; // deps[i] = nodes that must come BEFORE i
    for edge in edges {
        if let (
            BufferRef::ModuleOutput {
                module_idx: src, ..
            },
            BufferRef::ModuleInput {
                module_idx: dst, ..
            },
        ) = (edge.from, edge.to)
        {
            if src != dst && !deps[dst].contains(&src) {
                deps[dst].push(src);
            }
        }
    }
    // Kahn's algorithm.
    let mut indeg: Vec<usize> = (0..n).map(|i| deps[i].len()).collect();
    let mut order: Vec<usize> = Vec::with_capacity(n);
    let mut queue: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    while let Some(node) = queue.pop() {
        order.push(node);
        for i in 0..n {
            if deps[i].contains(&node) {
                indeg[i] -= 1;
                if indeg[i] == 0 {
                    queue.push(i);
                }
            }
        }
    }
    if order.len() != n {
        return Err(crate::SamplerError::SpecParse(
            "preset routing graph contains a cycle".into(),
        ));
    }
    Ok(order)
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn resolve_mic_index(mics: &[String], mic_id: &str) -> usize {
    if mics.is_empty() {
        return 0;
    }
    if mic_id.is_empty() || mic_id == "default" {
        return 0;
    }
    mics.iter().position(|m| m == mic_id).unwrap_or(0)
}

fn db_to_lin(db: f32) -> f32 {
    if db == 0.0 {
        1.0
    } else {
        10f32.powf(db / 20.0)
    }
}

fn pan_gains(pan: f32) -> (f32, f32) {
    let p = pan.clamp(-1.0, 1.0);
    let theta = (p + 1.0) * std::f32::consts::FRAC_PI_4;
    (theta.cos(), theta.sin())
}

// `RoutingRule` reference so unused import doesn't trip clippy.
#[allow(dead_code)]
fn _typecheck(_r: &RoutingRule) {}
