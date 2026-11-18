//! Morph engine for parameter interpolation between two snapshots.
//!
//! Pre-computes a diff of only the parameters that differ between A and B,
//! so `morph(t)` runs in `O(diff_count)`, not `O(total_params)`.

use serde::{Deserialize, Serialize};
use signal_proto::easing::EasingCurve;

/// A single parameter that differs between the two morph endpoints.
#[derive(Debug, Clone)]
pub struct MorphDiffEntry {
    /// FX identifier (e.g. plugin GUID string).
    pub fx_id: String,
    /// Parameter index within the FX.
    pub param_index: u32,
    /// Human-readable parameter name (for UI display).
    pub param_name: String,
    /// Value in snapshot A.
    pub value_a: f64,
    /// Value in snapshot B.
    pub value_b: f64,
}

/// A parameter change produced by `morph()`.
#[derive(Debug, Clone)]
pub struct MorphParamChange {
    pub fx_id: String,
    pub param_index: u32,
    pub param_name: String,
    pub from_value: f64,
    pub to_value: f64,
    /// The interpolated value at the current morph position.
    pub current_value: f64,
}

/// A single parameter value captured from the DAW.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DawParamValue {
    /// FX identifier (e.g. plugin GUID string).
    pub fx_id: String,
    /// Parameter index within the FX.
    pub param_index: u32,
    /// Human-readable parameter name.
    pub param_name: String,
    /// Current value (0.0–1.0 normalized).
    pub value: f64,
}

/// A snapshot of all parameter values for a scene.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DawParameterSnapshot {
    pub params: Vec<DawParamValue>,
}

impl DawParameterSnapshot {
    #[must_use]
    pub const fn new(params: Vec<DawParamValue>) -> Self {
        Self { params }
    }
}

/// Engine that interpolates between two parameter snapshots.
///
/// # Usage
///
/// ```ignore
/// let mut engine = MorphEngine::new();
/// engine.set_a(snapshot_a);
/// engine.set_b(snapshot_b);  // precomputes diff
/// let changes = engine.morph(0.5, EasingCurve::EaseInOut);
/// ```
#[derive(Debug, Clone)]
pub struct MorphEngine {
    snapshot_a: Option<DawParameterSnapshot>,
    snapshot_b: Option<DawParameterSnapshot>,
    /// Pre-computed diff — only params that differ between A and B.
    diffs: Vec<MorphDiffEntry>,
}

impl Default for MorphEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MorphEngine {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            snapshot_a: None,
            snapshot_b: None,
            diffs: Vec::new(),
        }
    }

    /// Set the "A" endpoint and recompute diffs (if B is also set).
    pub fn set_a(&mut self, snapshot: DawParameterSnapshot) {
        self.snapshot_a = Some(snapshot);
        self.recompute_diffs();
    }

    /// Set the "B" endpoint and recompute diffs (if A is also set).
    pub fn set_b(&mut self, snapshot: DawParameterSnapshot) {
        self.snapshot_b = Some(snapshot);
        self.recompute_diffs();
    }

    /// Whether both endpoints are set and ready to morph.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.snapshot_a.is_some() && self.snapshot_b.is_some()
    }

    /// Number of parameters that differ between A and B.
    #[must_use]
    pub const fn diff_count(&self) -> usize {
        self.diffs.len()
    }

    /// The pre-computed diffs (for UI display of which params will change).
    #[must_use]
    pub fn diffs(&self) -> &[MorphDiffEntry] {
        &self.diffs
    }

    /// Interpolate between A and B at position `t` (0.0 = A, 1.0 = B).
    ///
    /// Returns only the parameters that differ, with their interpolated values.
    /// Runs in `O(diff_count)`, not `O(total_params)`.
    #[must_use]
    pub fn morph(&self, t: f64, easing: EasingCurve) -> Vec<MorphParamChange> {
        let eased_t = easing.apply(t);

        self.diffs
            .iter()
            .map(|d| {
                let current = eased_t.mul_add(d.value_b - d.value_a, d.value_a);
                MorphParamChange {
                    fx_id: d.fx_id.clone(),
                    param_index: d.param_index,
                    param_name: d.param_name.clone(),
                    from_value: d.value_a,
                    to_value: d.value_b,
                    current_value: current,
                }
            })
            .collect()
    }

    /// Clear both endpoints and diffs.
    pub fn reset(&mut self) {
        self.snapshot_a = None;
        self.snapshot_b = None;
        self.diffs.clear();
    }

    /// Recompute the diff between A and B.
    fn recompute_diffs(&mut self) {
        self.diffs.clear();

        let (Some(a), Some(b)) = (&self.snapshot_a, &self.snapshot_b) else {
            return;
        };

        // Build a lookup from B's params by (fx_id, param_index).
        let b_lookup: std::collections::HashMap<(&str, u32), &DawParamValue> = b
            .params
            .iter()
            .map(|p| ((p.fx_id.as_str(), p.param_index), p))
            .collect();

        // Walk A's params, find diffs.
        let mut seen = std::collections::HashSet::new();

        for pa in &a.params {
            let key = (pa.fx_id.as_str(), pa.param_index);
            seen.insert((pa.fx_id.clone(), pa.param_index));

            let value_b = b_lookup.get(&key).map_or(0.0, |pb| pb.value); // param only in A → morph toward 0

            if (pa.value - value_b).abs() > f64::EPSILON {
                self.diffs.push(MorphDiffEntry {
                    fx_id: pa.fx_id.clone(),
                    param_index: pa.param_index,
                    param_name: pa.param_name.clone(),
                    value_a: pa.value,
                    value_b,
                });
            }
        }

        // Params only in B (not in A) → morph from 0.
        for pb in &b.params {
            if !seen.contains(&(pb.fx_id.clone(), pb.param_index)) && pb.value.abs() > f64::EPSILON
            {
                self.diffs.push(MorphDiffEntry {
                    fx_id: pb.fx_id.clone(),
                    param_index: pb.param_index,
                    param_name: pb.param_name.clone(),
                    value_a: 0.0,
                    value_b: pb.value,
                });
            }
        }
    }
}
