//! Morph engine for parameter interpolation between two snapshots.
//!
//! Pre-computes a diff of only the parameters that differ between A and B,
//! so `morph(t)` runs in O(diff_count), not O(total_params).

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
    pub fn new(params: Vec<DawParamValue>) -> Self {
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
    pub fn new() -> Self {
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
    pub fn is_ready(&self) -> bool {
        self.snapshot_a.is_some() && self.snapshot_b.is_some()
    }

    /// Number of parameters that differ between A and B.
    pub fn diff_count(&self) -> usize {
        self.diffs.len()
    }

    /// The pre-computed diffs (for UI display of which params will change).
    pub fn diffs(&self) -> &[MorphDiffEntry] {
        &self.diffs
    }

    /// Interpolate between A and B at position `t` (0.0 = A, 1.0 = B).
    ///
    /// Returns only the parameters that differ, with their interpolated values.
    /// Runs in O(diff_count), not O(total_params).
    pub fn morph(&self, t: f64, easing: EasingCurve) -> Vec<MorphParamChange> {
        let eased_t = easing.apply(t);

        self.diffs
            .iter()
            .map(|d| {
                let current = d.value_a + eased_t * (d.value_b - d.value_a);
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

            let value_b = b_lookup.get(&key).map(|pb| pb.value).unwrap_or(0.0); // param only in A → morph toward 0

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

#[cfg(test)]
mod tests {
    use super::*;

    fn param(fx: &str, idx: u32, val: f64) -> DawParamValue {
        DawParamValue {
            fx_id: fx.into(),
            param_index: idx,
            param_name: format!("Param {idx}"),
            value: val,
        }
    }

    #[test]
    fn empty_engine_not_ready() {
        let engine = MorphEngine::new();
        assert!(!engine.is_ready());
        assert_eq!(engine.diff_count(), 0);
    }

    #[test]
    fn identical_snapshots_zero_diffs() {
        let mut engine = MorphEngine::new();
        let snap = DawParameterSnapshot::new(vec![param("fx1", 0, 0.5), param("fx1", 1, 0.8)]);
        engine.set_a(snap.clone());
        engine.set_b(snap);
        assert!(engine.is_ready());
        assert_eq!(engine.diff_count(), 0);
    }

    #[test]
    fn different_values_produce_diffs() {
        let mut engine = MorphEngine::new();
        engine.set_a(DawParameterSnapshot::new(vec![
            param("fx1", 0, 0.0),
            param("fx1", 1, 0.5),
        ]));
        engine.set_b(DawParameterSnapshot::new(vec![
            param("fx1", 0, 1.0),
            param("fx1", 1, 0.5), // same — no diff
        ]));
        assert_eq!(engine.diff_count(), 1);
        assert_eq!(engine.diffs()[0].param_index, 0);
    }

    #[test]
    fn morph_at_zero_returns_a_values() {
        let mut engine = MorphEngine::new();
        engine.set_a(DawParameterSnapshot::new(vec![param("fx1", 0, 0.2)]));
        engine.set_b(DawParameterSnapshot::new(vec![param("fx1", 0, 0.8)]));

        let changes = engine.morph(0.0, EasingCurve::Linear);
        assert_eq!(changes.len(), 1);
        assert!((changes[0].current_value - 0.2).abs() < 1e-10);
    }

    #[test]
    fn morph_at_one_returns_b_values() {
        let mut engine = MorphEngine::new();
        engine.set_a(DawParameterSnapshot::new(vec![param("fx1", 0, 0.2)]));
        engine.set_b(DawParameterSnapshot::new(vec![param("fx1", 0, 0.8)]));

        let changes = engine.morph(1.0, EasingCurve::Linear);
        assert_eq!(changes.len(), 1);
        assert!((changes[0].current_value - 0.8).abs() < 1e-10);
    }

    #[test]
    fn morph_midpoint_linear() {
        let mut engine = MorphEngine::new();
        engine.set_a(DawParameterSnapshot::new(vec![param("fx1", 0, 0.0)]));
        engine.set_b(DawParameterSnapshot::new(vec![param("fx1", 0, 1.0)]));

        let changes = engine.morph(0.5, EasingCurve::Linear);
        assert!((changes[0].current_value - 0.5).abs() < 1e-10);
    }

    #[test]
    fn morph_with_easing() {
        let mut engine = MorphEngine::new();
        engine.set_a(DawParameterSnapshot::new(vec![param("fx1", 0, 0.0)]));
        engine.set_b(DawParameterSnapshot::new(vec![param("fx1", 0, 1.0)]));

        let changes = engine.morph(0.25, EasingCurve::EaseIn);
        // EaseIn(0.25) = 0.25^2 = 0.0625
        assert!((changes[0].current_value - 0.0625).abs() < 1e-10);
    }

    #[test]
    fn param_only_in_a_morphs_to_zero() {
        let mut engine = MorphEngine::new();
        engine.set_a(DawParameterSnapshot::new(vec![param("fx1", 0, 0.8)]));
        engine.set_b(DawParameterSnapshot::new(vec![]));

        assert_eq!(engine.diff_count(), 1);
        let changes = engine.morph(1.0, EasingCurve::Linear);
        assert!((changes[0].current_value - 0.0).abs() < 1e-10);
    }

    #[test]
    fn param_only_in_b_morphs_from_zero() {
        let mut engine = MorphEngine::new();
        engine.set_a(DawParameterSnapshot::new(vec![]));
        engine.set_b(DawParameterSnapshot::new(vec![param("fx1", 0, 0.8)]));

        assert_eq!(engine.diff_count(), 1);
        let changes = engine.morph(1.0, EasingCurve::Linear);
        assert!((changes[0].current_value - 0.8).abs() < 1e-10);
    }

    #[test]
    fn reset_clears_everything() {
        let mut engine = MorphEngine::new();
        engine.set_a(DawParameterSnapshot::new(vec![param("fx1", 0, 0.0)]));
        engine.set_b(DawParameterSnapshot::new(vec![param("fx1", 0, 1.0)]));
        assert!(engine.is_ready());

        engine.reset();
        assert!(!engine.is_ready());
        assert_eq!(engine.diff_count(), 0);
    }
}
