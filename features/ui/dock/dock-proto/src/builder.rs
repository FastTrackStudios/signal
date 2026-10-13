//! Fluent builder API for constructing dock layouts.
//!
//! # Example
//!
//! ```rust
//! use dock_proto::builder::DockLayoutBuilder;
//! use dock_proto::panel::PanelId;
//!
//! let layout = DockLayoutBuilder::horizontal()
//!     .left(DockLayoutBuilder::tile(PanelId::Navigator))
//!     .right(DockLayoutBuilder::vertical()
//!         .top(DockLayoutBuilder::tile(PanelId::Performance))
//!         .bottom(DockLayoutBuilder::tile(PanelId::Transport))
//!         .ratio(70.0)
//!         .build_node())
//!     .ratio(25.0)
//!     .build();
//! ```

use crate::layout::DockLayout;
use crate::panel::PanelId;
use crate::tree::{DockNode, SplitDirection};

/// Fluent builder for constructing dock layouts.
pub struct DockLayoutBuilder {
    direction: SplitDirection,
    first: Option<DockNode>,
    second: Option<DockNode>,
    ratio: f64,
}

impl DockLayoutBuilder {
    /// Start building a horizontal split (left | right).
    pub fn horizontal() -> Self {
        Self {
            direction: SplitDirection::Horizontal,
            first: None,
            second: None,
            ratio: 50.0,
        }
    }

    /// Start building a vertical split (top / bottom).
    pub fn vertical() -> Self {
        Self {
            direction: SplitDirection::Vertical,
            first: None,
            second: None,
            ratio: 50.0,
        }
    }

    /// Create a leaf tile node for a single panel.
    pub fn tile(panel: PanelId) -> DockNode {
        DockNode::tile(panel)
    }

    /// Create a leaf tile with multiple tabbed panels.
    pub fn tabbed(panels: Vec<PanelId>) -> DockNode {
        DockNode::tabbed(panels, 0)
    }

    // Semantic aliases for first/second
    pub fn left(mut self, node: DockNode) -> Self {
        self.first = Some(node);
        self
    }
    pub fn right(mut self, node: DockNode) -> Self {
        self.second = Some(node);
        self
    }
    pub fn top(mut self, node: DockNode) -> Self {
        self.first = Some(node);
        self
    }
    pub fn bottom(mut self, node: DockNode) -> Self {
        self.second = Some(node);
        self
    }
    pub fn first(mut self, node: DockNode) -> Self {
        self.first = Some(node);
        self
    }
    pub fn second(mut self, node: DockNode) -> Self {
        self.second = Some(node);
        self
    }

    /// Set the split ratio (percentage to first child, 5.0–95.0).
    pub fn ratio(mut self, ratio: f64) -> Self {
        self.ratio = ratio;
        self
    }

    /// Build as a DockNode (for nesting in other builders).
    pub fn build_node(self) -> DockNode {
        let first = self
            .first
            .unwrap_or_else(|| DockNode::tile(PanelId::Performance));
        let second = self
            .second
            .unwrap_or_else(|| DockNode::tile(PanelId::Settings));
        DockNode::Split {
            direction: self.direction,
            ratio: self.ratio.clamp(5.0, 95.0),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// Build as a complete DockLayout.
    pub fn build(self) -> DockLayout {
        DockLayout::from_tree(self.build_node())
    }
}
