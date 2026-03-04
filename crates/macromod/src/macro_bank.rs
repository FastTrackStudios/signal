//! Macro knob bank — assignable virtual controls that drive real parameters.
//!
//! Each [`MacroKnob`] holds a normalized 0–1 value and a list of
//! [`MacroBinding`]s that map the knob position to target parameter ranges
//! with optional response curves.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::binding::MacroBinding;

/// A single macro knob (up to 8 per bank).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct MacroKnob {
    /// Unique identifier for this knob within the bank.
    pub id: String,
    /// User-facing label, e.g. "Drive", "Tone".
    pub label: String,
    /// Normalized value (0.0–1.0).
    pub value: f32,
    /// Optional accent color (hex string).
    pub color: Option<String>,
    /// Parameter bindings driven by this knob.
    pub bindings: Vec<MacroBinding>,
    /// Whether this knob is bypassed (inactive). Bypassed knobs don't affect their bindings.
    #[serde(default)]
    pub bypassed: bool,
    /// Whether this knob uses bipolar display (-100% to +100%, center = 0%).
    /// Internal value is still 0.0–1.0, where 0.5 = center (0%).
    #[serde(default)]
    pub bipolar: bool,
    /// Sub-macros — e.g. a "Drive" parent with "Drive 1", "Drive 2", "Drive 3" children.
    #[serde(default)]
    pub children: Vec<MacroKnob>,
}

impl MacroKnob {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value: 0.5,
            color: None,
            bindings: Vec::new(),
            bypassed: false,
            bipolar: false,
            children: Vec::new(),
        }
    }

    /// Whether this knob has sub-macros.
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Find a child sub-macro by ID.
    pub fn get_child(&self, id: &str) -> Option<&MacroKnob> {
        self.children.iter().find(|c| c.id == id)
    }

    /// Find a child sub-macro by ID (mutable).
    pub fn get_child_mut(&mut self, id: &str) -> Option<&mut MacroKnob> {
        self.children.iter_mut().find(|c| c.id == id)
    }

    /// Format the current value as a display string.
    /// Bipolar knobs show -100% to +100% (0.5 = 0%). Normal knobs show 0–100%.
    pub fn format_value(&self) -> String {
        if self.bipolar {
            let pct = ((self.value - 0.5) * 200.0).round() as i32;
            if pct > 0 {
                format!("+{pct}%")
            } else {
                format!("{pct}%")
            }
        } else {
            format!("{:.0}%", self.value * 100.0)
        }
    }

    /// Set the normalized value, clamping to [0.0, 1.0].
    pub fn set_value(&mut self, value: f32) {
        self.value = value.clamp(0.0, 1.0);
    }

    /// Compute the output value for a specific binding given the current knob position.
    pub fn compute_binding_value(&self, binding: &MacroBinding) -> f32 {
        let t = self.value as f64;
        let eased = binding.curve.apply(t) as f32;
        binding.min + (binding.max - binding.min) * eased
    }
}

/// Identifies which shared macro knob drives group switching.
///
/// When a `GroupSelector` is set on a `MacroBank`, the referenced knob's
/// current value determines which [`MacroGroup`] is active — analogous to
/// hardware pedals like the Strymon BigSky's EFFECT TYPE knob.  Because the
/// selector is itself a `MacroKnob`, it can have parameter bindings (e.g.
/// bound to the plugin's "effect-type" parameter).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct GroupSelector {
    /// ID of a shared `MacroKnob` whose value drives group switching.
    pub knob_id: String,
}

/// A named group of macro knobs activated by a selector parameter value.
///
/// Each group has its own set of knobs that become visible when the selector
/// parameter is nearest to this group's `selector_value`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct MacroGroup {
    /// Unique identifier for this group.
    pub id: String,
    /// User-facing label, e.g. "BLOOM", "SHIMMER".
    pub label: String,
    /// Normalized parameter value (0.0–1.0) that activates this group.
    pub selector_value: f32,
    /// Hex accent color, e.g. "#E74C3C".
    pub color: String,
    /// Knobs visible only when this group is active.
    pub knobs: Vec<MacroKnob>,
}

/// A bank of up to 8 macro knobs, optionally organized into groups.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Facet)]
pub struct MacroBank {
    /// Shared/always-visible knobs (max 8 total across shared + active group).
    pub knobs: Vec<MacroKnob>,
    /// Optional group selector — which parameter drives group switching.
    #[serde(default)]
    pub group_selector: Option<GroupSelector>,
    /// Macro groups (empty = no groups, all knobs are shared).
    #[serde(default)]
    pub groups: Vec<MacroGroup>,
}

impl MacroBank {
    pub const MAX_KNOBS: usize = 8;

    pub fn new() -> Self {
        Self {
            knobs: Vec::new(),
            group_selector: None,
            groups: Vec::new(),
        }
    }

    /// Add a shared knob to the bank. Returns `false` if the bank is full.
    pub fn add(&mut self, knob: MacroKnob) -> bool {
        if self.knobs.len() >= Self::MAX_KNOBS {
            return false;
        }
        self.knobs.push(knob);
        true
    }

    /// Remove a shared knob by ID.
    pub fn remove(&mut self, id: &str) {
        self.knobs.retain(|k| k.id != id);
    }

    /// Find a knob by ID in the shared knobs only.
    pub fn get(&self, id: &str) -> Option<&MacroKnob> {
        self.knobs.iter().find(|k| k.id == id)
    }

    /// Find a knob by ID in the shared knobs only (mutable).
    pub fn get_mut(&mut self, id: &str) -> Option<&mut MacroKnob> {
        self.knobs.iter_mut().find(|k| k.id == id)
    }

    /// Whether this bank has any groups configured.
    pub fn has_groups(&self) -> bool {
        !self.groups.is_empty()
    }

    /// Add a macro group.
    pub fn add_group(&mut self, group: MacroGroup) {
        self.groups.push(group);
    }

    /// Remove a macro group by ID.
    pub fn remove_group(&mut self, id: &str) {
        self.groups.retain(|g| g.id != id);
    }

    /// Read the current selector knob value (0.0 if no selector configured or knob not found).
    pub fn selector_value(&self) -> f32 {
        self.group_selector
            .as_ref()
            .and_then(|sel| self.get(&sel.knob_id))
            .map(|k| k.value)
            .unwrap_or(0.0)
    }

    /// Get the selector knob (if configured).
    pub fn selector_knob(&self) -> Option<&MacroKnob> {
        self.group_selector
            .as_ref()
            .and_then(|sel| self.get(&sel.knob_id))
    }

    /// Find the active group for a given selector value.
    ///
    /// Returns the group whose `selector_value` is closest to the given value.
    /// Returns `None` if no groups are configured.
    pub fn active_group_for(&self, selector_value: f32) -> Option<&MacroGroup> {
        self.groups.iter().min_by(|a, b| {
            let da = (a.selector_value - selector_value).abs();
            let db = (b.selector_value - selector_value).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Find the active group using the selector knob's current value.
    pub fn active_group(&self) -> Option<&MacroGroup> {
        if self.groups.is_empty() {
            return None;
        }
        self.active_group_for(self.selector_value())
    }

    /// Get all visible knobs for a given selector value: shared knobs +
    /// the active group's knobs (if any).
    pub fn visible_knobs_for(&self, selector_value: f32) -> Vec<&MacroKnob> {
        let mut result: Vec<&MacroKnob> = self.knobs.iter().collect();
        if let Some(group) = self.active_group_for(selector_value) {
            result.extend(group.knobs.iter());
        }
        result
    }

    /// Get all visible knobs using the selector knob's current value.
    pub fn visible_knobs(&self) -> Vec<&MacroKnob> {
        self.visible_knobs_for(self.selector_value())
    }

    /// Find a knob by ID, searching shared knobs (+ their children) first, then all groups.
    pub fn get_knob(&self, id: &str) -> Option<&MacroKnob> {
        if let Some(k) = find_knob_recursive(&self.knobs, id) {
            return Some(k);
        }
        for group in &self.groups {
            if let Some(k) = find_knob_recursive(&group.knobs, id) {
                return Some(k);
            }
        }
        None
    }

    /// Find a knob by ID (mutable), searching shared knobs (+ their children) first, then all groups.
    pub fn get_knob_mut(&mut self, id: &str) -> Option<&mut MacroKnob> {
        if find_knob_recursive(&self.knobs, id).is_some() {
            return find_knob_recursive_mut(&mut self.knobs, id);
        }
        for group in &mut self.groups {
            if let Some(k) = find_knob_recursive_mut(&mut group.knobs, id) {
                return Some(k);
            }
        }
        None
    }
}

/// Recursively search a slice of knobs (and their children) for a knob by ID.
fn find_knob_recursive<'a>(knobs: &'a [MacroKnob], id: &str) -> Option<&'a MacroKnob> {
    for knob in knobs {
        if knob.id == id {
            return Some(knob);
        }
        if let Some(child) = find_knob_recursive(&knob.children, id) {
            return Some(child);
        }
    }
    None
}

/// Recursively search a mutable slice of knobs (and their children) for a knob by ID.
fn find_knob_recursive_mut<'a>(
    knobs: &'a mut [MacroKnob],
    id: &str,
) -> Option<&'a mut MacroKnob> {
    for knob in knobs {
        if knob.id == id {
            return Some(knob);
        }
        if let Some(child) = find_knob_recursive_mut(&mut knob.children, id) {
            return Some(child);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::EasingCurve;
    use crate::response::ResponseCurve;
    use crate::target::ParamTarget;

    #[test]
    fn macro_knob_clamps_value() {
        let mut knob = MacroKnob::new("k1", "Test");
        knob.set_value(1.5);
        assert_eq!(knob.value, 1.0);
        knob.set_value(-0.5);
        assert_eq!(knob.value, 0.0);
    }

    #[test]
    fn compute_binding_linear() {
        let mut knob = MacroKnob::new("k1", "Drive");
        knob.set_value(0.5);
        let binding = MacroBinding::from_ids("amp", "gain", 0.0, 1.0);
        let result = knob.compute_binding_value(&binding);
        assert!((result - 0.5).abs() < 1e-6);
    }

    #[test]
    fn compute_binding_with_range() {
        let mut knob = MacroKnob::new("k1", "Tone");
        knob.set_value(1.0);
        let binding = MacroBinding::from_ids("eq", "freq", 200.0, 8000.0);
        let result = knob.compute_binding_value(&binding);
        assert!((result - 8000.0).abs() < 1e-3);
    }

    #[test]
    fn compute_binding_with_param_target() {
        let mut knob = MacroKnob::new("k1", "Drive");
        knob.set_value(0.5);
        let binding = MacroBinding::new(ParamTarget::new("amp", "gain"), 0.0, 1.0);
        let result = knob.compute_binding_value(&binding);
        assert!((result - 0.5).abs() < 1e-6);
    }

    #[test]
    fn compute_binding_with_response_curve() {
        let mut knob = MacroKnob::new("k1", "Drive");
        knob.set_value(0.5);
        let binding = MacroBinding::from_ids("amp", "gain", 0.0, 1.0)
            .with_curve(ResponseCurve::Power { exponent: 2.0 });
        let result = knob.compute_binding_value(&binding);
        // 0.5^2 = 0.25, so result = 0.0 + (1.0 - 0.0) * 0.25 = 0.25
        assert!((result - 0.25).abs() < 1e-6);
    }

    #[test]
    fn bank_max_capacity() {
        let mut bank = MacroBank::new();
        for i in 0..8 {
            assert!(bank.add(MacroKnob::new(format!("k{i}"), format!("Knob {i}"))));
        }
        assert!(!bank.add(MacroKnob::new("k8", "Too Many")));
        assert_eq!(bank.knobs.len(), 8);
    }

    #[test]
    fn bank_remove_and_get() {
        let mut bank = MacroBank::new();
        bank.add(MacroKnob::new("k1", "First"));
        bank.add(MacroKnob::new("k2", "Second"));
        assert!(bank.get("k1").is_some());
        bank.remove("k1");
        assert!(bank.get("k1").is_none());
        assert_eq!(bank.knobs.len(), 1);
    }

    #[test]
    fn serde_round_trip() {
        let mut bank = MacroBank::new();
        let mut knob = MacroKnob::new("k1", "Drive");
        knob.bindings.push(
            MacroBinding::from_ids("amp", "gain", 0.0, 1.0)
                .with_curve(EasingCurve::CubicInOut),
        );
        bank.add(knob);
        let json = serde_json::to_string(&bank).unwrap();
        let parsed: MacroBank = serde_json::from_str(&json).unwrap();
        assert_eq!(bank, parsed);
    }

    #[test]
    fn serde_backward_compat_old_json_without_groups() {
        // Old serialized JSON only had "knobs" — no group_selector or groups
        let old_json = r#"{"knobs":[{"id":"k1","label":"Drive","value":0.5,"color":null,"bindings":[]}]}"#;
        let bank: MacroBank = serde_json::from_str(old_json).unwrap();
        assert_eq!(bank.knobs.len(), 1);
        assert!(bank.group_selector.is_none());
        assert!(bank.groups.is_empty());
    }

    #[test]
    fn serde_round_trip_with_groups() {
        let mut bank = MacroBank::new();
        bank.add(MacroKnob::new("selector", "Effect Type"));
        bank.group_selector = Some(GroupSelector {
            knob_id: "selector".into(),
        });
        bank.add_group(MacroGroup {
            id: "bloom".into(),
            label: "BLOOM".into(),
            selector_value: 0.0,
            color: "#E74C3C".into(),
            knobs: vec![MacroKnob::new("bloom-decay", "Decay")],
        });
        bank.add_group(MacroGroup {
            id: "shimmer".into(),
            label: "SHIMMER".into(),
            selector_value: 0.5,
            color: "#3498DB".into(),
            knobs: vec![MacroKnob::new("shimmer-pitch", "Pitch")],
        });

        let json = serde_json::to_string(&bank).unwrap();
        let parsed: MacroBank = serde_json::from_str(&json).unwrap();
        assert_eq!(bank, parsed);
        assert_eq!(parsed.groups.len(), 2);
        assert_eq!(parsed.groups[0].knobs[0].label, "Decay");
    }

    #[test]
    fn active_group_for_nearest_value() {
        let mut bank = MacroBank::new();
        bank.add_group(MacroGroup {
            id: "a".into(),
            label: "A".into(),
            selector_value: 0.0,
            color: "#000".into(),
            knobs: vec![],
        });
        bank.add_group(MacroGroup {
            id: "b".into(),
            label: "B".into(),
            selector_value: 0.5,
            color: "#000".into(),
            knobs: vec![],
        });
        bank.add_group(MacroGroup {
            id: "c".into(),
            label: "C".into(),
            selector_value: 1.0,
            color: "#000".into(),
            knobs: vec![],
        });

        assert_eq!(bank.active_group_for(0.1).unwrap().id, "a");
        assert_eq!(bank.active_group_for(0.3).unwrap().id, "b");
        assert_eq!(bank.active_group_for(0.5).unwrap().id, "b");
        assert_eq!(bank.active_group_for(0.8).unwrap().id, "c");
        assert_eq!(bank.active_group_for(1.0).unwrap().id, "c");
    }

    #[test]
    fn active_group_returns_none_when_no_groups() {
        let bank = MacroBank::new();
        assert!(bank.active_group().is_none());
    }

    #[test]
    fn active_group_driven_by_selector_knob() {
        let mut bank = MacroBank::new();
        let mut selector = MacroKnob::new("selector", "Effect Type");
        selector.set_value(0.0);
        bank.add(selector);
        bank.group_selector = Some(GroupSelector {
            knob_id: "selector".into(),
        });
        bank.add_group(MacroGroup {
            id: "bloom".into(),
            label: "BLOOM".into(),
            selector_value: 0.0,
            color: "#000".into(),
            knobs: vec![],
        });
        bank.add_group(MacroGroup {
            id: "shimmer".into(),
            label: "SHIMMER".into(),
            selector_value: 1.0,
            color: "#000".into(),
            knobs: vec![],
        });

        // Selector at 0.0 → BLOOM
        assert_eq!(bank.active_group().unwrap().id, "bloom");

        // Turn the selector knob to 1.0 → SHIMMER
        bank.get_mut("selector").unwrap().set_value(1.0);
        assert_eq!(bank.active_group().unwrap().id, "shimmer");

        // Midway → nearest group
        bank.get_mut("selector").unwrap().set_value(0.3);
        assert_eq!(bank.active_group().unwrap().id, "bloom");
        bank.get_mut("selector").unwrap().set_value(0.7);
        assert_eq!(bank.active_group().unwrap().id, "shimmer");
    }

    #[test]
    fn visible_knobs_shared_plus_active_group() {
        let mut bank = MacroBank::new();
        let mut selector = MacroKnob::new("selector", "Effect Type");
        selector.set_value(0.0);
        bank.add(selector);
        bank.add(MacroKnob::new("shared", "Mix"));
        bank.group_selector = Some(GroupSelector {
            knob_id: "selector".into(),
        });
        bank.add_group(MacroGroup {
            id: "bloom".into(),
            label: "BLOOM".into(),
            selector_value: 0.0,
            color: "#000".into(),
            knobs: vec![MacroKnob::new("bloom-k", "Decay")],
        });
        bank.add_group(MacroGroup {
            id: "shimmer".into(),
            label: "SHIMMER".into(),
            selector_value: 1.0,
            color: "#000".into(),
            knobs: vec![MacroKnob::new("shimmer-k", "Pitch")],
        });

        // Selector at 0.0 → shared (selector + mix) + bloom
        let visible = bank.visible_knobs();
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[0].id, "selector");
        assert_eq!(visible[1].id, "shared");
        assert_eq!(visible[2].id, "bloom-k");

        // Turn selector to 1.0 → shared + shimmer
        bank.get_mut("selector").unwrap().set_value(1.0);
        let visible = bank.visible_knobs();
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[2].id, "shimmer-k");
    }

    #[test]
    fn get_knob_mut_searches_across_groups() {
        let mut bank = MacroBank::new();
        bank.add(MacroKnob::new("shared", "Mix"));
        bank.add_group(MacroGroup {
            id: "bloom".into(),
            label: "BLOOM".into(),
            selector_value: 0.0,
            color: "#000".into(),
            knobs: vec![MacroKnob::new("bloom-decay", "Decay")],
        });

        // Can find shared knob
        assert!(bank.get_knob("shared").is_some());
        // Can find group knob
        assert!(bank.get_knob("bloom-decay").is_some());
        // Can mutate group knob
        bank.get_knob_mut("bloom-decay").unwrap().set_value(0.75);
        assert_eq!(bank.get_knob("bloom-decay").unwrap().value, 0.75);
        // Not found
        assert!(bank.get_knob("nonexistent").is_none());
    }

    #[test]
    fn has_groups() {
        let mut bank = MacroBank::new();
        assert!(!bank.has_groups());
        bank.add_group(MacroGroup {
            id: "g1".into(),
            label: "G1".into(),
            selector_value: 0.0,
            color: "#000".into(),
            knobs: vec![],
        });
        assert!(bank.has_groups());
        bank.remove_group("g1");
        assert!(!bank.has_groups());
    }

    // ── Children (sub-macro) tests ──────────────────────────────────

    #[test]
    fn has_children() {
        let mut knob = MacroKnob::new("drive", "Drive");
        assert!(!knob.has_children());
        knob.children.push(MacroKnob::new("drive-1", "Drive 1"));
        assert!(knob.has_children());
    }

    #[test]
    fn get_child_and_get_child_mut() {
        let mut knob = MacroKnob::new("drive", "Drive");
        knob.children.push(MacroKnob::new("drive-1", "Drive 1"));
        knob.children.push(MacroKnob::new("drive-2", "Drive 2"));

        assert_eq!(knob.get_child("drive-1").unwrap().label, "Drive 1");
        assert!(knob.get_child("nonexistent").is_none());

        knob.get_child_mut("drive-2").unwrap().set_value(0.8);
        assert_eq!(knob.get_child("drive-2").unwrap().value, 0.8);
    }

    #[test]
    fn serde_backward_compat_old_json_without_children() {
        // Old JSON without "children" field should still deserialize
        let old_json = r#"{"id":"k1","label":"Drive","value":0.5,"color":null,"bindings":[]}"#;
        let knob: MacroKnob = serde_json::from_str(old_json).unwrap();
        assert_eq!(knob.id, "k1");
        assert!(knob.children.is_empty());
    }

    #[test]
    fn serde_round_trip_with_children() {
        let mut parent = MacroKnob::new("drive", "Drive");
        parent.children.push(MacroKnob::new("drive-1", "Drive 1"));
        parent.children.push(MacroKnob::new("drive-2", "Drive 2"));

        let json = serde_json::to_string(&parent).unwrap();
        let parsed: MacroKnob = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.children.len(), 2);
        assert_eq!(parsed.children[0].id, "drive-1");
        assert_eq!(parsed.children[1].id, "drive-2");
    }

    #[test]
    fn get_knob_finds_nested_children() {
        let mut bank = MacroBank::new();
        let mut parent = MacroKnob::new("drive", "Drive");
        parent.children.push(MacroKnob::new("drive-1", "Drive 1"));
        parent.children.push(MacroKnob::new("drive-2", "Drive 2"));
        bank.add(parent);

        // Can find parent
        assert_eq!(bank.get_knob("drive").unwrap().label, "Drive");
        // Can find children
        assert_eq!(bank.get_knob("drive-1").unwrap().label, "Drive 1");
        assert_eq!(bank.get_knob("drive-2").unwrap().label, "Drive 2");
        // Not found
        assert!(bank.get_knob("drive-3").is_none());
    }

    #[test]
    fn get_knob_mut_finds_nested_children() {
        let mut bank = MacroBank::new();
        let mut parent = MacroKnob::new("drive", "Drive");
        parent.children.push(MacroKnob::new("drive-1", "Drive 1"));
        bank.add(parent);

        bank.get_knob_mut("drive-1").unwrap().set_value(0.9);
        assert_eq!(bank.get_knob("drive-1").unwrap().value, 0.9);
    }
}
