//! The **ModMatrix engine** and its compiler: control-rate sources, resolved
//! routes, send buses, tempo and the MIDI-domain arpeggiator — everything the
//! render tree root ticks once per block.

use signal_plugin_host::PluginEvents;
use signal_proto::block::BlockType;

use crate::native::{ControlEnv, ControlLfo, LfoWave, ModSource};
use crate::rig::RigBlock;
use crate::rig_node::{Container, ModRoute, RigNode};

/// Build the preset's arpeggiator from an active Arp modulator on the root
/// container (`on` ≠ 0). Steps come from `step{i}_on/vel/gate` params.
pub(super) fn build_arp(container: &Container) -> Option<crate::native::ArpEngine> {
    use crate::native::{ArpEngine, ArpStep};
    let arp = container.modulators.iter().find(|m| {
        m.block_type == BlockType::Arpeggiator && m.param_f32("on").unwrap_or(0.0) > 0.0
    })?;
    let step_beats = arp.param_f32("step_beats").unwrap_or(0.25);
    let count = arp.param_f32("steps").unwrap_or(0.0).max(0.0) as usize;
    let mut steps = Vec::with_capacity(count);
    for i in 0..count.min(64) {
        steps.push(ArpStep {
            on: arp.param_f32(&format!("step{i}_on")).unwrap_or(1.0) > 0.0,
            velocity: (arp.param_f32(&format!("step{i}_vel")).unwrap_or(100.0) as u8).clamp(1, 127),
            gate: arp
                .param_f32(&format!("step{i}_gate"))
                .unwrap_or(0.8)
                .clamp(0.05, 1.0),
        });
    }
    Some(ArpEngine::new(steps, step_beats))
}

/// One resolved ModMatrix row.
pub(super) struct CompiledRoute {
    source: usize,
    leaf: usize,
    param: u32,
    /// Base (unmodulated) normalized value the depth adds onto.
    base: f64,
    depth: f32,
}

/// The compiled control-rate modulation engine + send-bus state for one tree.
#[derive(Default)]
pub struct ModEngine {
    pub(super) sources: Vec<ModSource>,
    pub(super) routes: Vec<CompiledRoute>,
    /// Per-leaf pending parameter writes, rebuilt each block.
    pub(super) writes: Vec<Vec<(u32, f64)>>,
    /// Send buses (indexed by compile-time bus id), zeroed each block.
    pub(super) bus_l: Vec<Vec<f32>>,
    pub(super) bus_r: Vec<Vec<f32>>,
    /// Tempo for synced LFOs (set by the host via `RenderNode::set_tempo`).
    pub(super) tempo_bpm: f32,
    /// Sample rate captured at prepare (drives the arp clock).
    pub(super) sample_rate: f32,
    /// MIDI-domain arpeggiator, when the preset carries an active Arp.
    pub(super) arp: Option<crate::native::ArpEngine>,
}

impl ModEngine {
    pub(super) fn prepare(&mut self, sample_rate: f64, leaf_count: usize) {
        self.sample_rate = sample_rate as f32;
        for s in &mut self.sources {
            s.set_sample_rate(sample_rate as f32);
        }
        self.writes = vec![Vec::new(); leaf_count];
    }

    /// Tick all sources through one block and rebuild the per-leaf writes.
    pub(super) fn tick(&mut self, events: &PluginEvents<'_>, frames: usize) {
        // Evaluate each source once, then apply every route additively.
        let tempo = self.tempo_bpm;
        let values: Vec<f32> = self
            .sources
            .iter_mut()
            .map(|s| s.tick_at(events, frames, tempo))
            .collect();
        accumulate_writes(&self.routes, &values, &mut self.writes);
    }

    pub fn route_count(&self) -> usize {
        self.routes.len()
    }
}

/// Rebuild the per-leaf parameter writes for one block: each target parameter
/// is its **base** plus the **sum** of every route's signed offset
/// (`depth × source`), clamped once to the normalized range.
///
/// Additive by design — two modulators onto one cutoff sum, they don't clobber
/// (spec `signal.parameter.modulatable` / `signal.modulator.route`: a
/// parameter's value is `base + Σ(route offsets)`). Before this, the last route
/// to a target won and the others were silently dropped.
fn accumulate_writes(routes: &[CompiledRoute], values: &[f32], writes: &mut [Vec<(u32, f64)>]) {
    for w in writes.iter_mut() {
        w.clear();
    }
    for r in routes {
        let Some(&source) = values.get(r.source) else {
            continue;
        };
        let offset = r.depth as f64 * source as f64;
        let Some(w) = writes.get_mut(r.leaf) else {
            continue;
        };
        // First route to a (leaf, param) seeds `base + offset`; the rest add
        // their offset onto the running sum. `base` is a property of the
        // parameter, so it's identical across a param's routes.
        match w.iter_mut().find(|(p, _)| *p == r.param) {
            Some(entry) => entry.1 += offset,
            None => w.push((r.param, r.base + offset)),
        }
    }
    for w in writes.iter_mut() {
        for (_, v) in w.iter_mut() {
            *v = v.clamp(0.0, 1.0);
        }
    }
}

/// Compile-time state: modulator scope stack + leaf registry + route table
/// + send-bus registry.
pub(super) struct ModCompiler {
    pub(super) sources: Vec<ModSource>,
    /// (lower-cased modulator name, source index) — scoped stack.
    pub(super) scope: Vec<(String, usize)>,
    /// Dedup for MIDI sources.
    midi: Vec<(crate::native::MidiMod, usize)>,
    pub(super) routes: Vec<CompiledRoute>,
    /// Per-leaf (lower-cased display name, params) for target resolution.
    pub(super) leaves: Vec<(String, Vec<signal_plugin_host::PluginParamInfo>)>,
    /// Send-bus names (lower-cased), collected in a pre-pass; index = bus id.
    pub(super) buses: Vec<String>,
    sample_rate: u32,
}

impl ModCompiler {
    pub(super) fn new(sample_rate: u32) -> Self {
        Self {
            sources: Vec::new(),
            scope: Vec::new(),
            midi: Vec::new(),
            routes: Vec::new(),
            leaves: Vec::new(),
            buses: Vec::new(),
            sample_rate,
        }
    }

    /// Pre-pass: register every send target in the tree as a bus.
    pub(super) fn collect_buses(&mut self, container: &Container) {
        for s in &container.sends {
            let key = s.target.to_lowercase();
            if !self.buses.contains(&key) {
                self.buses.push(key);
            }
        }
        for child in &container.children {
            if let RigNode::Container { container: c } = child {
                self.collect_buses(c);
            }
        }
    }

    pub(super) fn bus_id(&self, name: &str) -> Option<usize> {
        let key = name.to_lowercase();
        self.buses.iter().position(|b| *b == key)
    }

    /// Instantiate a modulator block as a control source, honoring its
    /// build-time params (LFO `rate` Hz; envelope `attack`/`decay`/
    /// `sustain`/`release` seconds).
    pub(super) fn instantiate(&mut self, block: &RigBlock) -> Option<usize> {
        let sr = self.sample_rate as f32;
        let src = match block.block_type {
            BlockType::Lfo => {
                let rate = block.param_f32("rate").unwrap_or(2.0).clamp(0.01, 40.0);
                let wave = match block.param_f32("wave").unwrap_or(0.0).round() as u32 {
                    1 => LfoWave::Triangle,
                    2 => LfoWave::Saw,
                    3 => LfoWave::Square,
                    4 => LfoWave::SampleHold,
                    _ => LfoWave::Sine,
                };
                let mut lfo = ControlLfo::new(wave, rate);
                if let Some(beats) = block.param_f32("sync_beats") {
                    lfo = lfo.with_sync_beats(beats);
                }
                if block.param_f32("retrigger").unwrap_or(0.0) > 0.0 {
                    lfo = lfo.with_retrigger(true);
                }
                ModSource::lfo(lfo, sr)
            }
            BlockType::Envelope | BlockType::MultisegEnvelope => {
                let mut p = crate::native::AdsrParams::default();
                if let Some(v) = block.param_f32("attack") {
                    p.attack_s = v.max(0.0);
                }
                if let Some(v) = block.param_f32("decay") {
                    p.decay_s = v.max(0.0);
                }
                if let Some(v) = block.param_f32("sustain") {
                    p.sustain = v.clamp(0.0, 1.0);
                }
                if let Some(v) = block.param_f32("release") {
                    p.release_s = v.max(0.0);
                }
                ModSource::env(ControlEnv::new(sr, p), sr)
            }
            _ => return None,
        };
        self.sources.push(src);
        Some(self.sources.len() - 1)
    }

    fn resolve_source(&mut self, name: &str) -> Option<usize> {
        let key = name.to_lowercase();
        // Innermost modulator scope wins.
        if let Some((_, idx)) = self.scope.iter().rev().find(|(n, _)| *n == key) {
            return Some(*idx);
        }
        // MIDI performance source (deduped).
        let m = ModSource::midi_by_name(name)?;
        if let Some((_, idx)) = self.midi.iter().find(|(mm, _)| *mm == m) {
            return Some(*idx);
        }
        self.sources.push(ModSource::midi(m));
        let idx = self.sources.len() - 1;
        self.midi.push((m, idx));
        Some(idx)
    }

    /// Resolve this container's routes against its subtree's leaves
    /// (`subtree` = leaf indices compiled beneath it).
    pub(super) fn resolve_routes(&mut self, routes: &[ModRoute], subtree: &[usize]) {
        for route in routes {
            let Some((block_name, param_name)) = route.target.rsplit_once('.') else {
                tracing::warn!(target = %route.target, "mod route target missing .param");
                continue;
            };
            let Some(source) = self.resolve_source(&route.source) else {
                tracing::warn!(source = %route.source, "mod route source not found");
                continue;
            };
            let bkey = block_name.to_lowercase();
            let pkey = param_name.to_lowercase();
            let mut hit = false;
            for &leaf in subtree {
                let (name, params) = &self.leaves[leaf];
                if *name != bkey {
                    continue;
                }
                if let Some(p) = params.iter().find(|p| p.name.to_lowercase() == pkey) {
                    self.routes.push(CompiledRoute {
                        source,
                        leaf,
                        param: p.id,
                        base: p.default,
                        depth: route.depth,
                    });
                    hit = true;
                }
            }
            if !hit {
                tracing::warn!(
                    target = %route.target,
                    "mod route target not resolved (placeholder block or unknown param)"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(source: usize, leaf: usize, param: u32, base: f64, depth: f32) -> CompiledRoute {
        CompiledRoute { source, leaf, param, base, depth }
    }

    #[test]
    fn multiple_routes_to_one_param_sum_additively() {
        // Two sources at full scale, two routes onto leaf 0 / param 7.
        let routes = vec![
            route(0, 0, 7, 0.2, 0.3), // base 0.2, +0.3
            route(1, 0, 7, 0.2, 0.1), // +0.1
        ];
        let values = [1.0f32, 1.0];
        let mut writes = vec![Vec::new()];
        accumulate_writes(&routes, &values, &mut writes);
        assert_eq!(writes[0].len(), 1, "one accumulated write per param");
        let (param, v) = writes[0][0];
        assert_eq!(param, 7);
        assert!((v - 0.6).abs() < 1e-6, "0.2 + 0.3 + 0.1 = 0.6, got {v}");
    }

    #[test]
    fn accumulated_value_clamps_to_unit_range() {
        let routes = vec![
            route(0, 0, 1, 0.8, 0.5),
            route(0, 0, 1, 0.8, 0.5), // → 1.8, clamps to 1.0
        ];
        let values = [1.0f32];
        let mut writes = vec![Vec::new()];
        accumulate_writes(&routes, &values, &mut writes);
        assert_eq!(writes[0][0].1, 1.0);
    }

    #[test]
    fn negative_depth_subtracts_from_base() {
        let routes = vec![route(0, 0, 3, 0.5, -0.5)];
        let values = [1.0f32];
        let mut writes = vec![Vec::new()];
        accumulate_writes(&routes, &values, &mut writes);
        assert!((writes[0][0].1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn distinct_params_stay_separate() {
        let routes = vec![
            route(0, 0, 1, 0.1, 0.2),
            route(0, 0, 2, 0.3, 0.2),
        ];
        let values = [1.0f32];
        let mut writes = vec![Vec::new()];
        accumulate_writes(&routes, &values, &mut writes);
        assert_eq!(writes[0].len(), 2);
    }
}
