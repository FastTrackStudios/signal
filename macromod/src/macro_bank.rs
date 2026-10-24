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
    pub children: Vec<Self>,
    /// Whether child sub-macros are collapsed in the UI.
    #[serde(default)]
    pub collapsed: bool,
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
            collapsed: false,
        }
    }

    /// Whether this knob has sub-macros.
    #[must_use]
    pub const fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Find a child sub-macro by ID.
    #[must_use]
    pub fn get_child(&self, id: &str) -> Option<&Self> {
        self.children.iter().find(|c| c.id == id)
    }

    /// Find a child sub-macro by ID (mutable).
    pub fn get_child_mut(&mut self, id: &str) -> Option<&mut Self> {
        self.children.iter_mut().find(|c| c.id == id)
    }

    /// Format the current value as a display string.
    /// Bipolar knobs show -100% to +100% (0.5 = 0%). Normal knobs show 0–100%.
    #[must_use]
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
    pub const fn set_value(&mut self, value: f32) {
        self.value = value.clamp(0.0, 1.0);
    }

    /// Compute the output value for a specific binding given the current knob position.
    #[must_use]
    pub fn compute_binding_value(&self, binding: &MacroBinding) -> f32 {
        let t = f64::from(self.value);
        let eased = binding.curve.apply(t) as f32;
        (binding.max - binding.min).mul_add(eased, binding.min)
    }
}

/// Identifies which shared macro knob drives group switching.
///
/// When a `GroupSelector` is set on a `MacroBank`, the referenced knob's
/// current value determines which [`MacroGroup`] is active — analogous to
/// hardware pedals like the Strymon `BigSky`'s EFFECT TYPE knob.  Because the
/// selector is itself a `MacroKnob`, it can have parameter bindings (e.g.
/// bound to the plugin's "effect-type" parameter).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
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

    #[must_use]
    pub const fn new() -> Self {
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
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&MacroKnob> {
        self.knobs.iter().find(|k| k.id == id)
    }

    /// Find a knob by ID in the shared knobs only (mutable).
    pub fn get_mut(&mut self, id: &str) -> Option<&mut MacroKnob> {
        self.knobs.iter_mut().find(|k| k.id == id)
    }

    /// Whether this bank has any groups configured.
    #[must_use]
    pub const fn has_groups(&self) -> bool {
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
    #[must_use]
    pub fn selector_value(&self) -> f32 {
        self.group_selector
            .as_ref()
            .and_then(|sel| self.get(&sel.knob_id))
            .map_or(0.0, |k| k.value)
    }

    /// Get the selector knob (if configured).
    #[must_use]
    pub fn selector_knob(&self) -> Option<&MacroKnob> {
        self.group_selector
            .as_ref()
            .and_then(|sel| self.get(&sel.knob_id))
    }

    /// Find the active group for a given selector value.
    ///
    /// Returns the group whose `selector_value` is closest to the given value.
    /// Returns `None` if no groups are configured.
    #[must_use]
    pub fn active_group_for(&self, selector_value: f32) -> Option<&MacroGroup> {
        self.groups.iter().min_by(|a, b| {
            let da = (a.selector_value - selector_value).abs();
            let db = (b.selector_value - selector_value).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Find the active group using the selector knob's current value.
    #[must_use]
    pub fn active_group(&self) -> Option<&MacroGroup> {
        if self.groups.is_empty() {
            return None;
        }
        self.active_group_for(self.selector_value())
    }

    /// Get all visible knobs for a given selector value: shared knobs +
    /// the active group's knobs (if any).
    #[must_use]
    pub fn visible_knobs_for(&self, selector_value: f32) -> Vec<&MacroKnob> {
        let mut result: Vec<&MacroKnob> = self.knobs.iter().collect();
        if let Some(group) = self.active_group_for(selector_value) {
            result.extend(group.knobs.iter());
        }
        result
    }

    /// Get all visible knobs using the selector knob's current value.
    #[must_use]
    pub fn visible_knobs(&self) -> Vec<&MacroKnob> {
        self.visible_knobs_for(self.selector_value())
    }

    /// Find a knob by ID, searching shared knobs (+ their children) first, then all groups.
    #[must_use]
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
fn find_knob_recursive_mut<'a>(knobs: &'a mut [MacroKnob], id: &str) -> Option<&'a mut MacroKnob> {
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
