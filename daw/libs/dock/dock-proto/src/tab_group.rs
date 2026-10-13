//! Tab groups — multiple panels sharing a single tile area with tab switching.

use facet::Facet;

use crate::panel::PanelId;

/// A group of panels sharing a single tile area, with tab switching.
///
/// When the group has a single panel, no tab bar is shown.
/// When it has multiple panels, a tab bar appears at the top of the tile.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TabGroup {
    /// Ordered list of panels in this tab group.
    pub panels: Vec<PanelId>,
    /// Index of the currently active/visible panel.
    pub active_index: usize,
}

impl TabGroup {
    /// Create a tab group with a single panel.
    pub fn single(panel: PanelId) -> Self {
        Self {
            panels: vec![panel],
            active_index: 0,
        }
    }

    /// Create a tab group with multiple panels.
    pub fn new(panels: Vec<PanelId>, active_index: usize) -> Self {
        let active = active_index.min(panels.len().saturating_sub(1));
        Self {
            panels,
            active_index: active,
        }
    }

    /// Get the currently active panel.
    pub fn active_panel(&self) -> Option<PanelId> {
        self.panels.get(self.active_index).copied()
    }

    /// Switch to a different tab by index.
    pub fn set_active(&mut self, index: usize) {
        if index < self.panels.len() {
            self.active_index = index;
        }
    }

    /// Add a panel to this tab group.
    pub fn add_panel(&mut self, panel: PanelId) {
        self.panels.push(panel);
    }

    /// Remove a panel from this tab group. Returns true if removed.
    pub fn remove_panel(&mut self, panel: PanelId) -> bool {
        if let Some(pos) = self.panels.iter().position(|p| *p == panel) {
            self.panels.remove(pos);
            if self.active_index >= self.panels.len() && self.active_index > 0 {
                self.active_index -= 1;
            }
            true
        } else {
            false
        }
    }

    /// Whether this group has tabs (more than one panel).
    pub fn has_tabs(&self) -> bool {
        self.panels.len() > 1
    }

    /// Number of panels in this group.
    pub fn len(&self) -> usize {
        self.panels.len()
    }

    /// Whether this group is empty.
    pub fn is_empty(&self) -> bool {
        self.panels.is_empty()
    }
}
