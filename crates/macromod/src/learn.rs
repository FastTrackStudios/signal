//! Macro arm/learn state machine.
//!
//! Tracks the arm/learn workflow for building macro bindings interactively:
//!
//! 1. **Arm** a macro knob → enters learn mode
//! 2. **Touch** an FX parameter in the DAW → `GetLastTouchedFX` captures it
//! 3. **Set Point** → reads the macro knob position + current param value,
//!    adds a `CurvePoint` to the pending binding
//! 4. Repeat for more points or more parameters
//! 5. **Disarm** → finalizes all pending bindings into the macro knob

use serde::{Deserialize, Serialize};

use crate::curve::{CurvePoint, MultiPointCurve};
use crate::daw_target::DawParamTarget;

/// State of the macro arm/learn workflow.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LearnState {
    /// Which macro knob is currently armed (None = not learning).
    pub armed_knob_id: Option<String>,
    /// Bindings being built during the current learn session.
    /// Each entry is a DAW parameter target with its accumulated curve points.
    pub pending_bindings: Vec<PendingBinding>,
    /// The last FX parameter that was touched (set by polling `GetLastTouchedFX`).
    pub last_touched: Option<DawParamTarget>,
}

/// A binding being built during an arm/learn session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingBinding {
    /// The DAW parameter this binding will control.
    pub target: DawParamTarget,
    /// Display name of the parameter (for UI feedback).
    pub param_name: String,
    /// Display name of the FX plugin (for UI feedback).
    pub fx_name: String,
    /// Curve points accumulated so far.
    pub curve: MultiPointCurve,
}

impl LearnState {
    /// Arm a macro knob for learning.
    pub fn arm(&mut self, knob_id: impl Into<String>) {
        self.armed_knob_id = Some(knob_id.into());
        self.pending_bindings.clear();
        self.last_touched = None;
    }

    /// Disarm — returns the pending bindings for finalization.
    pub fn disarm(&mut self) -> Vec<PendingBinding> {
        self.armed_knob_id = None;
        self.last_touched = None;
        std::mem::take(&mut self.pending_bindings)
    }

    /// Whether a macro is currently armed.
    pub fn is_armed(&self) -> bool {
        self.armed_knob_id.is_some()
    }

    /// Update the last touched parameter (called when polling `GetLastTouchedFX`).
    pub fn set_last_touched(&mut self, target: DawParamTarget) {
        self.last_touched = Some(target);
    }

    /// Set a curve point for the last touched parameter.
    ///
    /// If this is the first point for this parameter, creates a new pending binding.
    /// Returns `Err` if no parameter has been touched yet.
    pub fn set_point(
        &mut self,
        macro_value: f64,
        param_value: f64,
        param_name: &str,
        fx_name: &str,
    ) -> Result<(), &'static str> {
        let target = self
            .last_touched
            .as_ref()
            .ok_or("No FX parameter touched yet")?
            .clone();

        let point = CurvePoint::new(macro_value, param_value);

        // Find or create pending binding for this target
        if let Some(binding) = self
            .pending_bindings
            .iter_mut()
            .find(|b| b.target == target)
        {
            binding.curve.set_point(point);
        } else {
            let mut curve = MultiPointCurve::from_points(vec![]);
            curve.set_point(point);
            self.pending_bindings.push(PendingBinding {
                target,
                param_name: param_name.to_string(),
                fx_name: fx_name.to_string(),
                curve,
            });
        }

        Ok(())
    }

    /// Remove the last point added for the last touched parameter.
    pub fn remove_last_point(&mut self) -> Result<(), &'static str> {
        let target = self
            .last_touched
            .as_ref()
            .ok_or("No FX parameter touched yet")?;

        if let Some(binding) = self
            .pending_bindings
            .iter_mut()
            .find(|b| &b.target == target)
        {
            if let Some(last) = binding.curve.points.last() {
                let mv = last.macro_value;
                binding.curve.remove_nearest(mv);
            }
            // Remove the binding entirely if no points remain
            if binding.curve.is_empty() {
                self.pending_bindings
                    .retain(|b| &b.target != target);
            }
            Ok(())
        } else {
            Err("No pending binding for the last touched parameter")
        }
    }

    /// Clear all pending bindings for the current armed session.
    pub fn clear(&mut self) {
        self.pending_bindings.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target_a() -> DawParamTarget {
        DawParamTarget::new("track-1", 0, 3)
    }

    fn target_b() -> DawParamTarget {
        DawParamTarget::new("track-1", 0, 5)
    }

    #[test]
    fn arm_disarm_cycle() {
        let mut state = LearnState::default();
        assert!(!state.is_armed());

        state.arm("macro-1");
        assert!(state.is_armed());
        assert_eq!(state.armed_knob_id.as_deref(), Some("macro-1"));

        let bindings = state.disarm();
        assert!(!state.is_armed());
        assert!(bindings.is_empty());
    }

    #[test]
    fn set_point_accumulates() {
        let mut state = LearnState::default();
        state.arm("macro-1");
        state.set_last_touched(target_a());

        // Set min
        state
            .set_point(0.0, 0.1, "Gain", "ReaComp")
            .unwrap();
        assert_eq!(state.pending_bindings.len(), 1);
        assert_eq!(state.pending_bindings[0].curve.len(), 1);

        // Set max
        state
            .set_point(1.0, 0.9, "Gain", "ReaComp")
            .unwrap();
        assert_eq!(state.pending_bindings[0].curve.len(), 2);

        // Set midpoint
        state
            .set_point(0.5, 0.4, "Gain", "ReaComp")
            .unwrap();
        assert_eq!(state.pending_bindings[0].curve.len(), 3);
    }

    #[test]
    fn multiple_parameters() {
        let mut state = LearnState::default();
        state.arm("macro-1");

        state.set_last_touched(target_a());
        state
            .set_point(0.0, 0.1, "Gain", "ReaComp")
            .unwrap();

        state.set_last_touched(target_b());
        state
            .set_point(0.0, 0.5, "Freq", "ReaEQ")
            .unwrap();

        assert_eq!(state.pending_bindings.len(), 2);
    }

    #[test]
    fn set_point_fails_without_touch() {
        let mut state = LearnState::default();
        state.arm("macro-1");
        assert!(state.set_point(0.0, 0.5, "Gain", "ReaComp").is_err());
    }

    #[test]
    fn disarm_returns_bindings() {
        let mut state = LearnState::default();
        state.arm("macro-1");
        state.set_last_touched(target_a());
        state
            .set_point(0.0, 0.0, "Gain", "ReaComp")
            .unwrap();
        state
            .set_point(1.0, 1.0, "Gain", "ReaComp")
            .unwrap();

        let bindings = state.disarm();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].curve.len(), 2);
        assert!(state.pending_bindings.is_empty());
    }
}
