//! Modulation runtime — async DAW bridge for the macromod modulation processor.
//!
//! Wraps [`ModulationProcessor`] with base value tracking, deduplication, and
//! concrete parameter write output. Follows the same pure-compute + DAW-output
//! pattern as [`MorphEngine`](super::morph::MorphEngine).

use std::collections::HashMap;

use signal_macromod::routing::ModulationRoute;
use signal_macromod::runtime::processor::{ModulationProcessor, TickContext};
use signal_macromod::target::ParamTarget;

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
    /// Maps `ParamTarget` → DAW FX binding.
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
    #[must_use]
    pub fn new(routes: Vec<ModulationRoute>) -> Self {
        Self {
            processor: ModulationProcessor::new(routes),
            bindings: HashMap::new(),
            base_values: HashMap::new(),
            last_written: HashMap::new(),
        }
    }

    /// Bind a `ParamTarget` to a concrete DAW FX parameter.
    pub fn bind_param(&mut self, target: ParamTarget, fx_id: impl Into<String>, param_index: u32) {
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
        self.base_values
            .insert(target.clone(), value.clamp(0.0, 1.0));
    }

    /// Access the underlying processor for external input and trigger methods.
    pub const fn processor(&mut self) -> &mut ModulationProcessor {
        &mut self.processor
    }

    /// Advance modulation by `dt` seconds at the given BPM.
    ///
    /// Returns only the parameter writes that actually changed since the last tick
    /// (deduplication within `DEDUP_EPSILON`).
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
    #[must_use]
    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    /// Number of routes in the processor.
    #[must_use]
    pub const fn route_count(&self) -> usize {
        self.processor.route_count()
    }
}
