//! Drop zones — used for drag-and-drop panel rearrangement.
//!
//! When a panel is dragged over a tile, the tile area is divided into
//! four edge zones. Dropping on a zone creates a split in that direction.

use facet::Facet;

use crate::tree::SplitDirection;

/// Which edge of a tile the cursor is hovering over during a drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum DropZone {
    /// Top edge — creates vertical split, dragged panel on top.
    Top,
    /// Bottom edge — creates vertical split, dragged panel on bottom.
    Bottom,
    /// Left edge — creates horizontal split, dragged panel on left.
    Left,
    /// Right edge — creates horizontal split, dragged panel on right.
    Right,
    /// Center — add as tab to existing tile group.
    Center,
}

impl DropZone {
    /// The split direction this drop zone implies.
    pub fn split_direction(self) -> SplitDirection {
        match self {
            Self::Top | Self::Bottom => SplitDirection::Vertical,
            Self::Left | Self::Right | Self::Center => SplitDirection::Horizontal,
        }
    }

    /// Whether the dragged panel should be the first (top/left) child.
    pub fn dragged_is_first(self) -> bool {
        matches!(self, Self::Top | Self::Left)
    }

    /// Calculate which drop zone the cursor is in based on relative position.
    ///
    /// `rel_x` and `rel_y` are in the range 0.0–1.0 (position within the tile).
    /// Returns `None` if the position is outside the valid range.
    pub fn from_relative_position(rel_x: f64, rel_y: f64) -> Option<Self> {
        let rel_x = rel_x.clamp(0.0, 1.0);
        let rel_y = rel_y.clamp(0.0, 1.0);

        const EDGE_MARGIN: f64 = 0.25;
        const CENTER_MIN: f64 = 0.35;
        const CENTER_MAX: f64 = 0.65;

        // Check center first (for tab-drop)
        if rel_x > CENTER_MIN && rel_x < CENTER_MAX && rel_y > CENTER_MIN && rel_y < CENTER_MAX {
            return Some(Self::Center);
        }

        // Edge zones — priority: top/bottom over left/right when near corners
        if rel_y < EDGE_MARGIN {
            Some(Self::Top)
        } else if rel_y > 1.0 - EDGE_MARGIN {
            Some(Self::Bottom)
        } else if rel_x < EDGE_MARGIN {
            Some(Self::Left)
        } else if rel_x > 1.0 - EDGE_MARGIN {
            Some(Self::Right)
        } else {
            Some(Self::Center)
        }
    }

    /// CSS overlay style for rendering the drop zone highlight.
    pub fn overlay_style(self, is_active: bool) -> &'static str {
        let base = match self {
            Self::Top => "top: 0; left: 0; right: 0; height: 30%;",
            Self::Bottom => "bottom: 0; left: 0; right: 0; height: 30%;",
            Self::Left => "top: 0; bottom: 0; left: 0; width: 30%;",
            Self::Right => "top: 0; bottom: 0; right: 0; width: 30%;",
            Self::Center => "top: 30%; bottom: 30%; left: 30%; right: 30%;",
        };
        if is_active {
            // Active styles are handled in the component via dynamic formatting
            base
        } else {
            base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_zone() {
        assert_eq!(
            DropZone::from_relative_position(0.5, 0.1),
            Some(DropZone::Top)
        );
    }

    #[test]
    fn bottom_zone() {
        assert_eq!(
            DropZone::from_relative_position(0.5, 0.9),
            Some(DropZone::Bottom)
        );
    }

    #[test]
    fn left_zone() {
        assert_eq!(
            DropZone::from_relative_position(0.1, 0.5),
            Some(DropZone::Left)
        );
    }

    #[test]
    fn right_zone() {
        assert_eq!(
            DropZone::from_relative_position(0.9, 0.5),
            Some(DropZone::Right)
        );
    }

    #[test]
    fn center_zone() {
        assert_eq!(
            DropZone::from_relative_position(0.5, 0.5),
            Some(DropZone::Center)
        );
    }

    #[test]
    fn dragged_is_first_for_top_left() {
        assert!(DropZone::Top.dragged_is_first());
        assert!(DropZone::Left.dragged_is_first());
        assert!(!DropZone::Bottom.dragged_is_first());
        assert!(!DropZone::Right.dragged_is_first());
    }
}
