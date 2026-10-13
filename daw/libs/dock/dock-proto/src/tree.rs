//! Layout tree model — binary tree of splits and tile leaves.
//!
//! This is the external/serializable representation. Internally,
//! [`DockLayout`](crate::layout::DockLayout) stores nodes in a HashMap
//! for O(1) access and mutation.

use facet::Facet;

use crate::id::TileId;
use crate::panel::PanelId;
use crate::tab_group::TabGroup;

/// Direction of a split in the layout tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum SplitDirection {
    /// Left | Right
    Horizontal,
    /// Top / Bottom
    Vertical,
}

/// A node in the layout tree.
///
/// Binary tree: each node is either a Split (two children) or a Tile (leaf).
#[derive(Debug, Clone, Facet)]
#[repr(C)]
pub enum DockNode {
    /// A split dividing space between two child nodes.
    Split {
        direction: SplitDirection,
        /// Percentage of space allocated to the first child (0.0–100.0).
        ratio: f64,
        /// First child (left or top).
        first: Box<DockNode>,
        /// Second child (right or bottom).
        second: Box<DockNode>,
    },
    /// A leaf tile containing one or more panels (via TabGroup).
    Tile {
        /// Unique tile identifier.
        id: TileId,
        /// The panel(s) shown in this tile.
        tabs: TabGroup,
    },
}

impl DockNode {
    /// Create a leaf node with a single panel.
    pub fn tile(panel: PanelId) -> Self {
        Self::Tile {
            id: TileId::new(),
            tabs: TabGroup::single(panel),
        }
    }

    /// Create a horizontal split (left | right).
    pub fn horizontal(first: DockNode, second: DockNode, ratio: f64) -> Self {
        Self::Split {
            direction: SplitDirection::Horizontal,
            ratio: ratio.clamp(5.0, 95.0),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// Create a vertical split (top / bottom).
    pub fn vertical(first: DockNode, second: DockNode, ratio: f64) -> Self {
        Self::Split {
            direction: SplitDirection::Vertical,
            ratio: ratio.clamp(5.0, 95.0),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// Create a tile with multiple tabbed panels.
    pub fn tabbed(panels: Vec<PanelId>, active: usize) -> Self {
        Self::Tile {
            id: TileId::new(),
            tabs: TabGroup::new(panels, active),
        }
    }
}
