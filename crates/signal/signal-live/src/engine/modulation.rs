//! Modulation runtime — async DAW bridge for the macromod modulation processor.
//!
//! Wraps [`ModulationProcessor`] with base value tracking, deduplication, and
//! concrete parameter write output. Follows the same pure-compute + DAW-output
//! pattern as [`MorphEngine`](super::morph::MorphEngine).

use std::collections::HashMap;

use macromod::runtime::processor::{ModulationProcessor, TickContext};
use macromod::target::ParamTarget;
use macromod::routing::ModulationRoute;

/// Binding between a [`ParamTarget`] and a concrete DAW FX parameter.
#[derive(Debug, Clone)]
pub struct ParamBinding {
    /// FX identifier (e.g. plugin GUID).
    pub fx_id: String,
    /// Parameter index within the FX.
    pub param_index: u32,
}

/// A concrete parameter write to send to the DAW.
#[derive(Debug, Clone)]
pub struct ParamWrite {
    /// FX identifier.
    pub fx_id: String,
    /// Parameter index within the FX.
    pub param_index: u32,
    /// Final value (0.0–1.0, clamped).
    pub value: f64,
}

/// Runtime that bridges the modulation processor to DAW parameter writes.
///
/// # Usage
///
/// ```ignore
/// let mut runtime = ModulationRuntime::new(routes);
/// runtime.bind_param(ParamTarget::new("amp", "gain"), "fx-guid", 3);
/// runtime.set_base_value(&ParamTarget::new("amp", "gain"), 0.5);
///
/// // Each frame (~30Hz):
/// let writes = runtime.tick(0.033, 120.0);
/// for w in writes {
///     daw.set_parameter(&w.fx_id, w.param_index, w.value);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ModulationRuntime {
    processor: ModulationProcessor,
    /// Maps ParamTarget → DAW FX binding.
    bindings: HashMap<ParamTarget, ParamBinding>,
    /// Base values for each target (the "dry" value before modulation).
    base_values: HashMap<ParamTarget, f64>,
    /// Last written values for deduplication.
    last_written: HashMap<ParamTarget, f64>,
}

/// Epsilon for deduplication — values closer than this are considered identical.
const DEDUP_EPSILON: f64 = 1e-5;

impl ModulationRuntime {
    /// Create a new runtime from modulation routes.
    pub fn new(routes: Vec<ModulationRoute>) -> Self {
        Self {
            processor: ModulationProcessor::new(routes),
            bindings: HashMap::new(),
            base_values: HashMap::new(),
            last_written: HashMap::new(),
        }
    }

    /// Bind a `ParamTarget` to a concrete DAW FX parameter.
    pub fn bind_param(
        &mut self,
        target: ParamTarget,
        fx_id: impl Into<String>,
        param_index: u32,
    ) {
        self.bindings.insert(
            target,
            ParamBinding {
                fx_id: fx_id.into(),
                param_index,
            },
        );
    }

    /// Set the base ("dry") value for a parameter target.
    ///
    /// Call this when a scene changes or a snapshot is applied.
    pub fn set_base_value(&mut self, target: &ParamTarget, value: f64) {
        self.base_values.insert(target.clone(), value.clamp(0.0, 1.0));
    }

    /// Access the underlying processor for external input and trigger methods.
    pub fn processor(&mut self) -> &mut ModulationProcessor {
        &mut self.processor
    }

    /// Advance modulation by `dt` seconds at the given BPM.
    ///
    /// Returns only the parameter writes that actually changed since the last tick
    /// (deduplication within [`DEDUP_EPSILON`]).
    pub fn tick(&mut self, dt: f64, bpm: f64) -> Vec<ParamWrite> {
        let ctx = TickContext { dt, bpm };
        let outputs = self.processor.tick(ctx);

        let mut writes = Vec::new();

        for output in outputs {
            let Some(binding) = self.bindings.get(&output.target) else {
                continue; // No DAW binding for this target
            };

            let base = self.base_values.get(&output.target).copied().unwrap_or(0.5);
            let final_value = (base + output.offset).clamp(0.0, 1.0);

            // Deduplication: skip if value hasn't changed significantly
            if let Some(&last) = self.last_written.get(&output.target) {
                if (final_value - last).abs() < DEDUP_EPSILON {
                    continue;
                }
            }

            self.last_written.insert(output.target.clone(), final_value);

            writes.push(ParamWrite {
                fx_id: binding.fx_id.clone(),
                param_index: binding.param_index,
                value: final_value,
            });
        }

        writes
    }

    /// Produce writes that restore all parameters to their base values.
    ///
    /// Call this on shutdown or when disabling modulation to return the DAW
    /// to its un-modulated state.
    pub fn restore_base_values(&mut self) -> Vec<ParamWrite> {
        let mut writes = Vec::new();

        for (target, &base) in &self.base_values {
            if let Some(binding) = self.bindings.get(target) {
                writes.push(ParamWrite {
                    fx_id: binding.fx_id.clone(),
                    param_index: binding.param_index,
                    value: base,
                });
            }
        }

        self.last_written.clear();
        writes
    }

    /// Reset the processor and clear deduplication state.
    pub fn reset(&mut self) {
        self.processor.reset();
        self.last_written.clear();
    }

    /// Number of active bindings.
    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    /// Number of routes in the processor.
    pub fn route_count(&self) -> usize {
        self.processor.route_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use macromod::sources::lfo::{LfoConfig, LfoWaveform};
    use macromod::sources::ModulationSource;

    fn lfo_route(id: &str, target: ParamTarget, amount: f32) -> ModulationRoute {
        ModulationRoute::new(
            id,
            ModulationSource::Lfo(LfoConfig {
                waveform: LfoWaveform::Sine,
                rate_hz: 1.0,
                depth: 1.0,
                ..Default::default()
            }),
            target,
            amount,
        )
    }

    fn cc_route(id: &str, cc: u8, target: ParamTarget, amount: f32) -> ModulationRoute {
        ModulationRoute::new(
            id,
            ModulationSource::MidiCc { cc_number: cc },
            target,
            amount,
        )
    }

    #[test]
    fn empty_runtime_produces_no_writes() {
        let mut rt = ModulationRuntime::new(vec![]);
        let writes = rt.tick(0.033, 120.0);
        assert!(writes.is_empty());
    }

    #[test]
    fn unbound_target_produces_no_write() {
        let target = ParamTarget::new("amp", "gain");
        let mut rt = ModulationRuntime::new(vec![cc_route("r1", 1, target.clone(), 1.0)]);
        // No binding set
        rt.processor().set_midi_cc(1, 1.0);
        let writes = rt.tick(0.033, 120.0);
        assert!(writes.is_empty());
    }

    #[test]
    fn bound_target_produces_write() {
        let target = ParamTarget::new("amp", "gain");
        let mut rt = ModulationRuntime::new(vec![cc_route("r1", 1, target.clone(), 1.0)]);
        rt.bind_param(target.clone(), "fx-guid-1", 3);
        rt.set_base_value(&target, 0.5);
        rt.processor().set_midi_cc(1, 1.0);

        let writes = rt.tick(0.033, 120.0);
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].fx_id, "fx-guid-1");
        assert_eq!(writes[0].param_index, 3);
        // base=0.5, CC=1.0 → bipolar=1.0, amount=1.0 → offset=1.0 → final=1.5→clamped to 1.0
        assert!((writes[0].value - 1.0).abs() < 1e-10);
    }

    #[test]
    fn deduplication_suppresses_identical_writes() {
        let target = ParamTarget::new("amp", "gain");
        let mut rt = ModulationRuntime::new(vec![cc_route("r1", 1, target.clone(), 1.0)]);
        rt.bind_param(target.clone(), "fx-guid-1", 3);
        rt.set_base_value(&target, 0.5);
        rt.processor().set_midi_cc(1, 0.75);

        // First tick produces a write
        let writes = rt.tick(0.033, 120.0);
        assert_eq!(writes.len(), 1);

        // Second tick with same input — deduplicated
        let writes = rt.tick(0.033, 120.0);
        assert!(writes.is_empty(), "should deduplicate identical writes");
    }

    #[test]
    fn value_change_breaks_deduplication() {
        let target = ParamTarget::new("amp", "gain");
        let mut rt = ModulationRuntime::new(vec![cc_route("r1", 1, target.clone(), 1.0)]);
        rt.bind_param(target.clone(), "fx-guid-1", 3);
        rt.set_base_value(&target, 0.5);
        rt.processor().set_midi_cc(1, 0.75);

        let _ = rt.tick(0.033, 120.0);

        // Change CC value
        rt.processor().set_midi_cc(1, 0.25);
        let writes = rt.tick(0.033, 120.0);
        assert_eq!(writes.len(), 1, "should write after value change");
    }

    #[test]
    fn base_value_affects_output() {
        let target = ParamTarget::new("amp", "gain");
        let mut rt = ModulationRuntime::new(vec![cc_route("r1", 1, target.clone(), 0.5)]);
        rt.bind_param(target.clone(), "fx-guid-1", 0);

        // CC=0.5 → bipolar=0.0 → offset=0.0
        rt.processor().set_midi_cc(1, 0.5);

        // Base=0.3 → final should be 0.3
        rt.set_base_value(&target, 0.3);
        let writes = rt.tick(0.033, 120.0);
        assert_eq!(writes.len(), 1);
        assert!((writes[0].value - 0.3).abs() < 1e-10);
    }

    #[test]
    fn restore_base_values_returns_dry_state() {
        let target = ParamTarget::new("amp", "gain");
        let mut rt = ModulationRuntime::new(vec![cc_route("r1", 1, target.clone(), 1.0)]);
        rt.bind_param(target.clone(), "fx-guid-1", 3);
        rt.set_base_value(&target, 0.6);

        let writes = rt.restore_base_values();
        assert_eq!(writes.len(), 1);
        assert!((writes[0].value - 0.6).abs() < 1e-10);
    }

    #[test]
    fn clamping_prevents_out_of_range() {
        let target = ParamTarget::new("amp", "gain");
        let mut rt = ModulationRuntime::new(vec![cc_route("r1", 1, target.clone(), 1.0)]);
        rt.bind_param(target.clone(), "fx-guid-1", 0);
        rt.set_base_value(&target, 0.9);

        // CC=1.0 → bipolar=1.0 → offset=1.0 → base+offset = 1.9 → clamped to 1.0
        rt.processor().set_midi_cc(1, 1.0);
        let writes = rt.tick(0.033, 120.0);
        assert!((writes[0].value - 1.0).abs() < 1e-10);

        // CC=0.0 → bipolar=-1.0 → offset=-1.0 → base+offset = -0.1 → clamped to 0.0
        rt.processor().set_midi_cc(1, 0.0);
        let writes = rt.tick(0.033, 120.0);
        assert!(writes[0].value.abs() < 1e-10);
    }

    #[test]
    fn lfo_modulation_varies_over_time() {
        let target = ParamTarget::new("amp", "gain");
        let mut rt = ModulationRuntime::new(vec![lfo_route("r1", target.clone(), 0.5)]);
        rt.bind_param(target.clone(), "fx-guid-1", 0);
        rt.set_base_value(&target, 0.5);

        // Collect several ticks
        let mut values = Vec::new();
        for _ in 0..10 {
            let writes = rt.tick(0.033, 120.0);
            if let Some(w) = writes.first() {
                values.push(w.value);
            }
        }

        // LFO should produce varying values
        assert!(values.len() >= 2, "LFO should produce writes");
        let all_same = values.windows(2).all(|w| (w[0] - w[1]).abs() < DEDUP_EPSILON);
        assert!(!all_same, "LFO values should vary: {values:?}");
    }

    #[test]
    fn reset_clears_dedup_state() {
        let target = ParamTarget::new("amp", "gain");
        let mut rt = ModulationRuntime::new(vec![cc_route("r1", 1, target.clone(), 1.0)]);
        rt.bind_param(target.clone(), "fx-guid-1", 3);
        rt.set_base_value(&target, 0.5);
        rt.processor().set_midi_cc(1, 0.75);

        let _ = rt.tick(0.033, 120.0); // First write
        rt.reset();

        // After reset, CC defaults to 0 → different value → should write
        rt.processor().set_midi_cc(1, 0.75);
        let writes = rt.tick(0.033, 120.0);
        assert_eq!(writes.len(), 1);
    }
}
