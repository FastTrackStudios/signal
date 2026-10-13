//! Layout history — undo/redo based on compact layout diffs.

use crate::layout::DockLayout;
use crate::LayoutDiff;

// region: --- LayoutHistory

/// Undo/redo history for dock layout changes.
///
/// Records layout states on mutation and allows stepping backward (undo)
/// and forward (redo) through the history. The redo stack is cleared
/// whenever a new mutation is recorded, matching standard undo semantics.
///
/// # Example
///
/// ```
/// use dock_proto::history::LayoutHistory;
/// use dock_proto::layout::DockLayout;
/// use dock_proto::panel::PanelId;
///
/// let mut layout = DockLayout::single(PanelId::Performance);
/// let mut history = LayoutHistory::new();
///
/// let before = layout.clone();
/// // ... mutate layout ...
/// history.push(&before, &layout);
///
/// // Now we can undo:
/// history.undo(&mut layout);
/// ```
#[derive(Debug, Clone)]
pub struct LayoutHistory {
    undo_stack: Vec<LayoutDiff>,
    redo_stack: Vec<LayoutDiff>,
    max_depth: usize,
}

impl LayoutHistory {
    /// Default maximum undo depth.
    pub const DEFAULT_MAX_DEPTH: usize = 50;

    /// Create a new empty history with the default max depth (50).
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth: Self::DEFAULT_MAX_DEPTH,
        }
    }

    /// Create a new empty history with a custom max depth.
    pub fn with_max_depth(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth: max_depth.max(1),
        }
    }

    /// Record a layout change. Pushes the before/after state onto the undo
    /// stack and clears the redo stack (standard undo semantics).
    pub fn push(&mut self, before: &DockLayout, after: &DockLayout) {
        let diff = DockLayout::diff(before, after);
        if diff.is_empty() {
            return;
        }

        self.redo_stack.clear();
        self.undo_stack.push(diff);

        // Trim oldest entries if we exceed max depth
        if self.undo_stack.len() > self.max_depth {
            let excess = self.undo_stack.len() - self.max_depth;
            self.undo_stack.drain(..excess);
        }
    }

    /// Undo the most recent change. Restores `current` to the before-state
    /// and moves the entry to the redo stack. Returns `true` if an undo
    /// was performed, `false` if the undo stack is empty.
    pub fn undo(&mut self, current: &mut DockLayout) -> bool {
        let Some(diff) = self.undo_stack.pop() else {
            return false;
        };

        current.apply(&diff.invert());
        self.redo_stack.push(diff);
        true
    }

    /// Redo the most recently undone change. Restores `current` to the
    /// after-state and moves the entry back to the undo stack. Returns
    /// `true` if a redo was performed, `false` if the redo stack is empty.
    pub fn redo(&mut self, current: &mut DockLayout) -> bool {
        let Some(diff) = self.redo_stack.pop() else {
            return false;
        };

        current.apply(&diff);
        self.undo_stack.push(diff);
        true
    }

    /// Whether there are any changes to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether there are any undone changes to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Number of entries on the undo stack.
    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    /// Number of entries on the redo stack.
    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }

    /// Clear all history (both undo and redo).
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

impl Default for LayoutHistory {
    fn default() -> Self {
        Self::new()
    }
}

// endregion: --- LayoutHistory

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::PanelId;
    use crate::tree::SplitDirection;

    // -- Helpers

    /// Assert that a layout contains exactly the given panels (order-independent).
    fn assert_panels(layout: &DockLayout, expected: &[PanelId]) {
        for panel in expected {
            assert!(
                layout.contains_panel(*panel),
                "expected layout to contain {:?}",
                panel
            );
        }
        // Count tiles to verify no extra panels
        let total_panels: usize = layout.all_tiles().iter().map(|(_, tabs)| tabs.len()).sum();
        assert_eq!(
            total_panels,
            expected.len(),
            "expected {} panels, found {}",
            expected.len(),
            total_panels
        );
    }

    // -- Tests

    #[test]
    fn split_then_undo_restores_original() {
        // -- Setup & Fixtures
        let mut layout = DockLayout::single(PanelId::Performance);
        let mut history = LayoutHistory::new();

        // -- Exec: split the layout, recording history
        let before = layout.clone();
        let root_id = layout.root().unwrap();
        layout.split_tile(
            root_id,
            SplitDirection::Horizontal,
            PanelId::ChartEditor,
            50.0,
        );
        history.push(&before, &layout);

        assert_panels(&layout, &[PanelId::Performance, PanelId::ChartEditor]);

        // -- Exec: undo
        let undone = history.undo(&mut layout);

        // -- Check
        assert!(undone);
        assert_panels(&layout, &[PanelId::Performance]);
        assert_eq!(layout.node_count(), 1);
    }

    #[test]
    fn undo_then_redo_restores_split() {
        // -- Setup & Fixtures
        let mut layout = DockLayout::single(PanelId::Performance);
        let mut history = LayoutHistory::new();

        let before = layout.clone();
        let root_id = layout.root().unwrap();
        layout.split_tile(
            root_id,
            SplitDirection::Horizontal,
            PanelId::ChartEditor,
            50.0,
        );
        history.push(&before, &layout);

        // -- Exec: undo then redo
        history.undo(&mut layout);
        assert_panels(&layout, &[PanelId::Performance]);

        let redone = history.redo(&mut layout);

        // -- Check
        assert!(redone);
        assert_panels(&layout, &[PanelId::Performance, PanelId::ChartEditor]);
        assert_eq!(layout.node_count(), 3); // 1 split + 2 tiles
    }

    #[test]
    fn new_mutation_after_undo_clears_redo() {
        // -- Setup & Fixtures
        let mut layout = DockLayout::single(PanelId::Performance);
        let mut history = LayoutHistory::new();

        // First mutation: split
        let before1 = layout.clone();
        let root_id = layout.root().unwrap();
        layout.split_tile(
            root_id,
            SplitDirection::Horizontal,
            PanelId::ChartEditor,
            50.0,
        );
        history.push(&before1, &layout);

        // Undo the split
        history.undo(&mut layout);
        assert!(history.can_redo());

        // -- Exec: new mutation (different split)
        let before2 = layout.clone();
        let root_id = layout.root().unwrap();
        layout.split_tile(root_id, SplitDirection::Vertical, PanelId::Navigator, 30.0);
        history.push(&before2, &layout);

        // -- Check: redo stack should be cleared
        assert!(!history.can_redo());
        assert!(history.can_undo());
        assert_panels(&layout, &[PanelId::Performance, PanelId::Navigator]);
    }

    #[test]
    fn history_depth_capped_at_max_depth() {
        // -- Setup & Fixtures
        let max = 5;
        let mut layout = DockLayout::single(PanelId::Performance);
        let mut history = LayoutHistory::with_max_depth(max);

        // -- Exec: push more entries than max_depth
        for i in 0..(max + 3) {
            let before = layout.clone();
            // Just update a split ratio to create a "change" — we need a split first
            if i == 0 {
                let root_id = layout.root().unwrap();
                layout.split_tile(
                    root_id,
                    SplitDirection::Horizontal,
                    PanelId::ChartEditor,
                    50.0,
                );
            } else {
                let root_id = layout.root().unwrap();
                layout.update_split_ratio(root_id, 50.0 + i as f64);
            }
            history.push(&before, &layout);
        }

        // -- Check: depth should be capped
        assert_eq!(history.undo_depth(), max);
    }

    #[test]
    fn can_undo_and_can_redo() {
        // -- Setup & Fixtures
        let mut layout = DockLayout::single(PanelId::Performance);
        let mut history = LayoutHistory::new();

        // -- Check: empty history
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        // -- Exec: push a change
        let before = layout.clone();
        let root_id = layout.root().unwrap();
        layout.split_tile(
            root_id,
            SplitDirection::Horizontal,
            PanelId::ChartEditor,
            50.0,
        );
        history.push(&before, &layout);

        // -- Check: can undo, cannot redo
        assert!(history.can_undo());
        assert!(!history.can_redo());

        // -- Exec: undo
        history.undo(&mut layout);

        // -- Check: cannot undo, can redo
        assert!(!history.can_undo());
        assert!(history.can_redo());
    }

    #[test]
    fn undo_on_empty_returns_false() {
        // -- Setup & Fixtures
        let mut layout = DockLayout::single(PanelId::Performance);
        let mut history = LayoutHistory::new();

        // -- Exec & Check
        assert!(!history.undo(&mut layout));
    }

    #[test]
    fn redo_on_empty_returns_false() {
        // -- Setup & Fixtures
        let mut layout = DockLayout::single(PanelId::Performance);
        let mut history = LayoutHistory::new();

        // -- Exec & Check
        assert!(!history.redo(&mut layout));
    }

    #[test]
    fn multiple_undos_and_redos() {
        // -- Setup & Fixtures
        let mut layout = DockLayout::single(PanelId::Performance);
        let mut history = LayoutHistory::new();

        // Mutation 1: split with ChartEditor
        let before1 = layout.clone();
        let root_id = layout.root().unwrap();
        layout.split_tile(
            root_id,
            SplitDirection::Horizontal,
            PanelId::ChartEditor,
            50.0,
        );
        history.push(&before1, &layout);

        // Mutation 2: split with Navigator
        let before2 = layout.clone();
        let editor_node = layout.find_node_for_panel(PanelId::ChartEditor).unwrap();
        layout.split_tile(
            editor_node,
            SplitDirection::Vertical,
            PanelId::Navigator,
            40.0,
        );
        history.push(&before2, &layout);

        // -- Exec: undo twice
        assert!(history.undo(&mut layout)); // undo Navigator split
        assert_panels(&layout, &[PanelId::Performance, PanelId::ChartEditor]);

        assert!(history.undo(&mut layout)); // undo ChartEditor split
        assert_panels(&layout, &[PanelId::Performance]);

        // -- Exec: redo twice
        assert!(history.redo(&mut layout));
        assert_panels(&layout, &[PanelId::Performance, PanelId::ChartEditor]);

        assert!(history.redo(&mut layout));
        assert_panels(
            &layout,
            &[
                PanelId::Performance,
                PanelId::ChartEditor,
                PanelId::Navigator,
            ],
        );
    }

    #[test]
    fn clear_empties_both_stacks() {
        // -- Setup & Fixtures
        let mut layout = DockLayout::single(PanelId::Performance);
        let mut history = LayoutHistory::new();

        let before = layout.clone();
        let root_id = layout.root().unwrap();
        layout.split_tile(
            root_id,
            SplitDirection::Horizontal,
            PanelId::ChartEditor,
            50.0,
        );
        history.push(&before, &layout);
        history.undo(&mut layout);

        assert!(history.can_redo());

        // -- Exec
        history.clear();

        // -- Check
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_depth(), 0);
        assert_eq!(history.redo_depth(), 0);
    }

    #[test]
    fn with_max_depth_minimum_is_one() {
        // -- Setup & Fixtures
        let history = LayoutHistory::with_max_depth(0);

        // -- Check: should be clamped to at least 1
        let mut layout = DockLayout::single(PanelId::Performance);
        let mut h = history;
        let before = layout.clone();
        let root_id = layout.root().unwrap();
        layout.split_tile(
            root_id,
            SplitDirection::Horizontal,
            PanelId::ChartEditor,
            50.0,
        );
        h.push(&before, &layout);

        assert_eq!(h.undo_depth(), 1);
        assert!(h.can_undo());
    }
}

// endregion: --- Tests
