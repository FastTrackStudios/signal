//! Docsite State
//!
//! Reactive state management for the documentation site.

/// Global application state for the docsite.
#[derive(Clone, Debug, Default)]
pub struct DocsiteState {
    /// Currently selected pattern ID
    pub selected_pattern: Option<String>,
    /// Filter by category (None = show all)
    pub category_filter: Option<String>,
}

impl DocsiteState {
    /// Create new docsite state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the selected pattern.
    pub fn select_pattern(&mut self, id: String) {
        self.selected_pattern = Some(id);
    }

    /// Clear pattern selection.
    pub fn clear_selection(&mut self) {
        self.selected_pattern = None;
    }

    /// Set category filter.
    pub fn filter_by_category(&mut self, category: String) {
        self.category_filter = Some(category);
    }

    /// Clear category filter.
    pub fn clear_filter(&mut self) {
        self.category_filter = None;
    }
}
