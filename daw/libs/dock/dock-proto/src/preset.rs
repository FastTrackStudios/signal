//! Dock presets — named layout snapshots (screensets).
//!
//! Equivalent to REAPER screensets: save/load/cycle complete layout configurations.

use facet::Facet;
use std::collections::HashMap;

use crate::id::PresetId;
use crate::layout::DockLayout;

/// A named dock layout preset (equivalent to a REAPER screenset).
#[derive(Debug, Clone, Facet)]
pub struct DockPreset {
    pub id: PresetId,
    pub name: String,
    pub layout: DockLayout,
    /// Optional hotkey hint (e.g. "F5", "Ctrl+1") for display in the preset bar.
    pub hotkey_hint: Option<String>,
    /// Whether this preset was created by the user or is a built-in default.
    pub is_builtin: bool,
    /// Opaque per-panel state blobs (panel_id -> JSON string).
    pub panel_states: HashMap<String, String>,
}

impl DockPreset {
    pub fn new(name: impl Into<String>, layout: DockLayout) -> Self {
        Self {
            id: PresetId::new(),
            name: name.into(),
            layout,
            hotkey_hint: None,
            is_builtin: false,
            panel_states: HashMap::new(),
        }
    }

    pub fn builtin(name: impl Into<String>, layout: DockLayout) -> Self {
        Self {
            id: PresetId::new(),
            name: name.into(),
            layout,
            hotkey_hint: None,
            is_builtin: true,
            panel_states: HashMap::new(),
        }
    }

    pub fn with_hotkey(mut self, hotkey: impl Into<String>) -> Self {
        self.hotkey_hint = Some(hotkey.into());
        self
    }

    /// Save opaque panel state JSON for a panel ID.
    pub fn save_panel_state(&mut self, panel_id: impl Into<String>, state_json: impl Into<String>) {
        self.panel_states.insert(panel_id.into(), state_json.into());
    }

    /// Load panel state JSON if present.
    pub fn load_panel_state(&self, panel_id: &str) -> Option<&str> {
        self.panel_states.get(panel_id).map(String::as_str)
    }

    /// Capture panel states via callback before saving preset.
    pub fn capture_panel_states<I, F>(&mut self, panel_ids: I, mut capture: F)
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
        F: FnMut(&str) -> Option<String>,
    {
        for panel_id in panel_ids {
            let panel_id = panel_id.as_ref();
            if let Some(state) = capture(panel_id) {
                self.save_panel_state(panel_id.to_string(), state);
            }
        }
    }

    /// Replay saved panel states to a callback after loading preset.
    pub fn restore_panel_states<F>(&self, mut restore: F)
    where
        F: FnMut(&str, &str),
    {
        for (panel_id, state) in &self.panel_states {
            restore(panel_id, state);
        }
    }
}

/// Collection of dock presets with an active selection.
#[derive(Debug, Clone, Facet)]
pub struct PresetCollection {
    pub presets: Vec<DockPreset>,
    pub active_index: usize,
}

impl PresetCollection {
    pub fn new(presets: Vec<DockPreset>) -> Self {
        Self {
            presets,
            active_index: 0,
        }
    }

    pub fn active_preset(&self) -> Option<&DockPreset> {
        self.presets.get(self.active_index)
    }

    pub fn active_layout(&self) -> Option<&DockLayout> {
        self.active_preset().map(|p| &p.layout)
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.presets.len() {
            self.active_index = index;
        }
    }

    pub fn cycle_next(&mut self) {
        if !self.presets.is_empty() {
            self.active_index = (self.active_index + 1) % self.presets.len();
        }
    }

    pub fn cycle_prev(&mut self) {
        if !self.presets.is_empty() {
            self.active_index = if self.active_index == 0 {
                self.presets.len() - 1
            } else {
                self.active_index - 1
            };
        }
    }

    pub fn add_preset(&mut self, preset: DockPreset) {
        self.presets.push(preset);
    }

    pub fn remove_preset(&mut self, index: usize) -> Option<DockPreset> {
        if index < self.presets.len() {
            let preset = self.presets.remove(index);
            if self.active_index >= self.presets.len() && self.active_index > 0 {
                self.active_index -= 1;
            }
            Some(preset)
        } else {
            None
        }
    }

    /// Save the current layout back into the active preset.
    pub fn save_active(&mut self, layout: DockLayout) {
        if let Some(preset) = self.presets.get_mut(self.active_index) {
            preset.layout = layout;
        }
    }

    /// Save one panel's state into the active preset.
    pub fn save_active_panel_state(
        &mut self,
        panel_id: impl Into<String>,
        state_json: impl Into<String>,
    ) {
        if let Some(preset) = self.presets.get_mut(self.active_index) {
            preset.save_panel_state(panel_id, state_json);
        }
    }

    /// Load one panel's state from the active preset.
    pub fn load_active_panel_state(&self, panel_id: &str) -> Option<&str> {
        self.active_preset()?.load_panel_state(panel_id)
    }

    /// Capture panel states into active preset before save.
    pub fn capture_active_panel_states<I, F>(&mut self, panel_ids: I, capture: F)
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
        F: FnMut(&str) -> Option<String>,
    {
        if let Some(preset) = self.presets.get_mut(self.active_index) {
            preset.capture_panel_states(panel_ids, capture);
        }
    }

    /// Replay panel states from active preset after load.
    pub fn restore_active_panel_states<F>(&self, restore: F)
    where
        F: FnMut(&str, &str),
    {
        if let Some(preset) = self.active_preset() {
            preset.restore_panel_states(restore);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::DockLayout;
    use crate::panel::PanelId;

    #[test]
    fn missing_panel_state_returns_none() {
        let preset = DockPreset::new("Test", DockLayout::single(PanelId::Performance));
        assert_eq!(preset.load_panel_state("panel.missing"), None);
    }
}
