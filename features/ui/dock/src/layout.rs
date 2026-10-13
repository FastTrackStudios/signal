//! Dock layout — HashMap-backed flat storage with tree conversion.
//!
//! Stores nodes in a `HashMap<NodeId, FlatNode>` for O(1) lookups and
//! mutations. Provides tree conversion for serialization and construction.

use std::collections::HashMap;

use facet::Facet;

use crate::id::{NodeId, TileId};
use crate::panel::PanelId;
use crate::registry::PanelConstraints;
use crate::tab_group::TabGroup;
use crate::tree::{DockNode, SplitDirection};

/// Display mode for a dock zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum DockZoneMode {
    Pinned,
    AutoHide,
    Float,
}

/// Semantic layout regions for composition and preset mixing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum LayoutRegion {
    LeftSidebar,
    RightSidebar,
    Center,
    BottomPanel,
    TopBar,
}

/// Internal flat node representation stored in the HashMap.
#[derive(Debug, Clone, Facet)]
#[repr(C)]
pub enum FlatNode {
    Split {
        direction: SplitDirection,
        ratio: f64,
        first: NodeId,
        second: NodeId,
    },
    Tile {
        tile_id: TileId,
        tabs: TabGroup,
    },
}

/// The main layout data structure.
///
/// Stores nodes in a HashMap for O(1) lookups and mutations.
/// The root field points to the top-level node.
#[derive(Debug, Clone, Facet)]
pub struct DockLayout {
    nodes: HashMap<NodeId, FlatNode>,
    root: Option<NodeId>,
    constraints: HashMap<PanelId, PanelConstraints>,
    zone_modes: HashMap<NodeId, DockZoneMode>,
    expanded_zones: std::collections::HashSet<NodeId>,
}

impl DockLayout {
    /// Create an empty layout.
    pub fn empty() -> Self {
        Self {
            nodes: HashMap::new(),
            root: None,
            constraints: HashMap::new(),
            zone_modes: HashMap::new(),
            expanded_zones: std::collections::HashSet::new(),
        }
    }

    /// Create a layout with a single panel.
    pub fn single(panel: PanelId) -> Self {
        let mut layout = Self::empty();
        let node_id = NodeId::new();
        layout.nodes.insert(
            node_id,
            FlatNode::Tile {
                tile_id: TileId::new(),
                tabs: TabGroup::single(panel),
            },
        );
        layout.root = Some(node_id);
        layout
    }

    /// Build from a tree representation (converts tree -> flat HashMap).
    pub fn from_tree(tree: DockNode) -> Self {
        let mut layout = Self::empty();
        let root_id = layout.insert_tree_node(tree);
        layout.root = Some(root_id);
        layout
    }

    /// Convert to a tree representation (for serialization or display).
    pub fn to_tree(&self) -> Option<DockNode> {
        self.root.map(|root_id| self.build_tree_node(root_id))
    }

    /// Get the root node ID.
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    /// Get a node by ID.
    pub fn get_node(&self, id: NodeId) -> Option<&FlatNode> {
        self.nodes.get(&id)
    }

    /// Get a mutable node by ID.
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut FlatNode> {
        self.nodes.get_mut(&id)
    }

    /// Update a split ratio (clamped to 5.0–95.0).
    pub fn update_split_ratio(&mut self, node_id: NodeId, new_ratio: f64) {
        let constrained = self
            .constrain_ratio(node_id, new_ratio.clamp(5.0, 95.0), 1000.0)
            .clamp(5.0, 95.0);
        if let Some(FlatNode::Split { ratio, .. }) = self.nodes.get_mut(&node_id) {
            *ratio = constrained;
        }
    }

    /// Collapsed strip size for auto-hide zones.
    pub const AUTO_HIDE_COLLAPSED_SIZE: f64 = 32.0;

    /// Set a zone's display mode.
    pub fn set_zone_mode(&mut self, node_id: NodeId, mode: DockZoneMode) {
        self.zone_modes.insert(node_id, mode);
        if mode != DockZoneMode::AutoHide {
            self.expanded_zones.remove(&node_id);
        }
    }

    /// Get a zone's display mode. Defaults to pinned.
    pub fn zone_mode(&self, node_id: NodeId) -> DockZoneMode {
        self.zone_modes
            .get(&node_id)
            .copied()
            .unwrap_or(DockZoneMode::Pinned)
    }

    /// Toggle between pinned and auto-hide for a zone.
    pub fn toggle_auto_hide(&mut self, node_id: NodeId) {
        match self.zone_mode(node_id) {
            DockZoneMode::AutoHide => self.set_zone_mode(node_id, DockZoneMode::Pinned),
            _ => self.set_zone_mode(node_id, DockZoneMode::AutoHide),
        }
    }

    /// Mark an auto-hide zone as expanded.
    pub fn expand_zone(&mut self, node_id: NodeId) {
        if self.zone_mode(node_id) == DockZoneMode::AutoHide {
            self.expanded_zones.insert(node_id);
        }
    }

    /// Mark an auto-hide zone as collapsed.
    pub fn collapse_zone(&mut self, node_id: NodeId) {
        self.expanded_zones.remove(&node_id);
    }

    /// Whether the given zone is currently expanded.
    pub fn is_zone_expanded(&self, node_id: NodeId) -> bool {
        self.expanded_zones.contains(&node_id)
    }

    /// Visible size for a zone given its full expanded size.
    pub fn zone_visible_size(&self, node_id: NodeId, expanded_size: f64) -> f64 {
        match self.zone_mode(node_id) {
            DockZoneMode::AutoHide if !self.is_zone_expanded(node_id) => {
                Self::AUTO_HIDE_COLLAPSED_SIZE
            }
            _ => expanded_size,
        }
    }

    /// Return a view of the layout with auto-hidden zones collapsed.
    pub fn visible_layout(&self) -> DockLayout {
        let mut visible = self.clone();
        visible.apply_auto_hide_projection();
        visible
    }

    /// Register constraints for a panel.
    pub fn set_constraints(&mut self, panel: PanelId, constraints: PanelConstraints) {
        self.constraints.insert(panel, constraints);
    }

    /// Compute effective split ratio with panel constraints applied.
    pub fn effective_ratio(&self, node_id: NodeId, available_size: f64) -> f64 {
        let Some(FlatNode::Split {
            ratio,
            first,
            second,
            ..
        }) = self.nodes.get(&node_id)
        else {
            return 50.0;
        };

        let first_panels = self.collect_panels(*first);
        let second_panels = self.collect_panels(*second);
        if first_panels.is_empty() || second_panels.is_empty() || available_size <= 0.0 {
            return *ratio;
        }

        let (first_min, first_pref, first_pri) = self.aggregate_constraints(&first_panels);
        let (second_min, second_pref, second_pri) = self.aggregate_constraints(&second_panels);

        if first_min + second_min > available_size {
            let total = (first_pri as f64 + second_pri as f64).max(1.0);
            return ((first_pri as f64 / total) * 100.0).clamp(5.0, 95.0);
        }

        let mut target = *ratio;
        if first_pref > 0.0 || second_pref > 0.0 {
            let pref_total = first_pref + second_pref;
            if pref_total > 0.0 {
                target = (first_pref / pref_total) * 100.0;
            }
        }

        self.constrain_ratio(node_id, target, available_size)
    }

    /// Split an existing tile, creating a new split node.
    ///
    /// The original tile becomes `first`, a new tile with `new_panel` becomes `second`.
    /// The split node reuses the original tile's NodeId so parent references stay valid.
    pub fn split_tile(
        &mut self,
        tile_node_id: NodeId,
        direction: SplitDirection,
        new_panel: PanelId,
        ratio: f64,
    ) -> Option<NodeId> {
        // Verify the target is a tile
        let existing_tile = self.nodes.remove(&tile_node_id)?;
        if !matches!(existing_tile, FlatNode::Tile { .. }) {
            self.nodes.insert(tile_node_id, existing_tile);
            return None;
        }

        // Re-insert the existing tile under a new ID
        let existing_child_id = NodeId::new();
        if let Some(mode) = self.zone_modes.remove(&tile_node_id) {
            self.zone_modes.insert(existing_child_id, mode);
        }
        if self.expanded_zones.remove(&tile_node_id) {
            self.expanded_zones.insert(existing_child_id);
        }
        self.nodes.insert(existing_child_id, existing_tile);

        // Create the new tile
        let new_child_id = NodeId::new();
        self.nodes.insert(
            new_child_id,
            FlatNode::Tile {
                tile_id: TileId::new(),
                tabs: TabGroup::single(new_panel),
            },
        );

        // Create the split node, reusing the original node ID
        self.nodes.insert(
            tile_node_id,
            FlatNode::Split {
                direction,
                ratio: ratio.clamp(5.0, 95.0),
                first: existing_child_id,
                second: new_child_id,
            },
        );

        Some(new_child_id)
    }

    /// Close a tile and promote its sibling to take the parent split's place.
    pub fn close_tile(&mut self, tile_node_id: NodeId) -> bool {
        // Find parent of this node
        let parent_info = self.find_parent(tile_node_id);
        let Some((parent_id, is_first_child)) = parent_info else {
            // It's the root tile — clear layout
            if self.root == Some(tile_node_id) {
                self.nodes.remove(&tile_node_id);
                self.root = None;
                return true;
            }
            return false;
        };

        // Get the sibling
        let sibling_id =
            if let Some(FlatNode::Split { first, second, .. }) = self.nodes.get(&parent_id) {
                if is_first_child {
                    *second
                } else {
                    *first
                }
            } else {
                return false;
            };

        // Remove the closing tile and the parent split
        self.nodes.remove(&tile_node_id);
        self.zone_modes.remove(&tile_node_id);
        self.expanded_zones.remove(&tile_node_id);
        let sibling = self.nodes.remove(&sibling_id);
        let sibling_mode = self.zone_modes.remove(&sibling_id);
        let sibling_expanded = self.expanded_zones.remove(&sibling_id);
        self.zone_modes.remove(&parent_id);
        self.expanded_zones.remove(&parent_id);

        if let Some(sibling_node) = sibling {
            // Replace parent with sibling (reuse parent_id so grandparent refs stay valid)
            self.nodes.insert(parent_id, sibling_node);
            if let Some(mode) = sibling_mode {
                self.zone_modes.insert(parent_id, mode);
            }
            if sibling_expanded {
                self.expanded_zones.insert(parent_id);
            }
        }

        true
    }

    /// Get all tile IDs and their panels.
    pub fn all_tiles(&self) -> Vec<(TileId, &TabGroup)> {
        self.nodes
            .values()
            .filter_map(|node| {
                if let FlatNode::Tile { tile_id, tabs } = node {
                    Some((*tile_id, tabs))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if a panel exists anywhere in the layout.
    pub fn contains_panel(&self, panel: PanelId) -> bool {
        self.nodes.values().any(|node| {
            if let FlatNode::Tile { tabs, .. } = node {
                tabs.panels.contains(&panel)
            } else {
                false
            }
        })
    }

    /// Check if a panel is the *active* tab in its tile (i.e. actually visible).
    /// Returns false if the panel is in a tabbed container but on a background tab.
    pub fn panel_is_visible(&self, panel: PanelId) -> bool {
        self.nodes.values().any(|node| {
            if let FlatNode::Tile { tabs, .. } = node {
                tabs.active_panel() == Some(panel)
            } else {
                false
            }
        })
    }

    /// Get the total number of nodes in the layout.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Iterate raw nodes for internal algorithms.
    pub(crate) fn raw_nodes(&self) -> impl Iterator<Item = (NodeId, &FlatNode)> {
        self.nodes.iter().map(|(id, node)| (*id, node))
    }

    /// Whether the layout has no root/node content.
    pub fn is_empty(&self) -> bool {
        self.root.is_none() || self.nodes.is_empty()
    }

    /// Find the NodeId of the node containing a specific TileId.
    pub fn find_node_for_tile(&self, tile_id: TileId) -> Option<NodeId> {
        self.nodes.iter().find_map(|(node_id, node)| {
            if let FlatNode::Tile { tile_id: tid, .. } = node {
                if *tid == tile_id {
                    return Some(*node_id);
                }
            }
            None
        })
    }

    /// Find the NodeId of the first tile containing a specific panel.
    pub fn find_node_for_panel(&self, panel: PanelId) -> Option<NodeId> {
        self.nodes.iter().find_map(|(node_id, node)| {
            if let FlatNode::Tile { tabs, .. } = node {
                if tabs.panels.contains(&panel) {
                    return Some(*node_id);
                }
            }
            None
        })
    }

    /// Move a panel from one tile to another, creating a split at the drop zone.
    ///
    /// This handles the full drag-and-drop operation:
    /// 1. Remove the panel from its source tile
    /// 2. If the source tile is now empty, close it
    /// 3. Create a new split at the target, placing the panel according to the drop zone
    ///
    /// If `drop_zone` is `Center`, the panel is added as a tab instead of splitting.
    pub fn move_panel(
        &mut self,
        panel: PanelId,
        target_node_id: NodeId,
        drop_zone: crate::drop_zone::DropZone,
    ) -> bool {
        use crate::drop_zone::DropZone;

        // Find the source tile containing this panel
        let source_node_id = match self.find_node_for_panel(panel) {
            Some(id) => id,
            None => return false,
        };

        // Don't drop on self for edge zones (splitting yourself makes no sense)
        if source_node_id == target_node_id && !matches!(drop_zone, DropZone::Center) {
            return false;
        }

        // Center drop: add as tab to the target tile.
        // Must add to target BEFORE closing the empty source, because
        // close_tile can invalidate NodeIds when it promotes siblings.
        if matches!(drop_zone, DropZone::Center) {
            if source_node_id == target_node_id {
                return true; // already there
            }

            // Add to target first (while it's still valid)
            if let Some(FlatNode::Tile { tabs, .. }) = self.nodes.get_mut(&target_node_id) {
                tabs.add_panel(panel);
                let last = tabs.len() - 1;
                tabs.set_active(last);
            } else {
                return false;
            }

            // Now remove from source
            let source_empty =
                if let Some(FlatNode::Tile { tabs, .. }) = self.nodes.get_mut(&source_node_id) {
                    tabs.remove_panel(panel);
                    tabs.is_empty()
                } else {
                    false
                };

            if source_empty {
                self.close_tile(source_node_id);
            }

            return true;
        }

        // Edge drop: remove panel from source, then split the target.
        let source_was_single =
            if let Some(FlatNode::Tile { tabs, .. }) = self.nodes.get_mut(&source_node_id) {
                let was_single = tabs.len() == 1;
                if !tabs.remove_panel(panel) {
                    return false;
                }
                was_single
            } else {
                return false;
            };

        if source_was_single {
            self.close_tile(source_node_id);
        }

        let direction = drop_zone.split_direction();
        let dragged_is_first = drop_zone.dragged_is_first();

        // The target might have been invalidated by closing the source.
        if self.nodes.contains_key(&target_node_id) {
            // Create the new tile for the dragged panel
            let new_child_id = NodeId::new();
            self.nodes.insert(
                new_child_id,
                FlatNode::Tile {
                    tile_id: TileId::new(),
                    tabs: TabGroup::single(panel),
                },
            );

            // Remove existing target and re-insert under new ID
            let existing = match self.nodes.remove(&target_node_id) {
                Some(n) => n,
                None => return false,
            };
            let existing_child_id = NodeId::new();
            self.nodes.insert(existing_child_id, existing);

            // Create split, reusing target_node_id
            let (first, second) = if dragged_is_first {
                (new_child_id, existing_child_id)
            } else {
                (existing_child_id, new_child_id)
            };

            self.nodes.insert(
                target_node_id,
                FlatNode::Split {
                    direction,
                    ratio: 50.0,
                    first,
                    second,
                },
            );

            true
        } else {
            // Target was invalidated (it was the sibling that got promoted).
            // Fallback: just add the panel to whatever is at root.
            if let Some(root_id) = self.root {
                self.split_tile(root_id, direction, panel, 50.0);
                true
            } else {
                // Layout is empty, create a single tile
                *self = DockLayout::single(panel);
                true
            }
        }
    }

    /// Remove a panel from its tile. Closes the tile if it becomes empty.
    pub fn remove_panel(&mut self, panel: PanelId) -> bool {
        let Some(source_node_id) = self.find_node_for_panel(panel) else {
            return false;
        };

        let source_empty =
            if let Some(FlatNode::Tile { tabs, .. }) = self.nodes.get_mut(&source_node_id) {
                if !tabs.remove_panel(panel) {
                    return false;
                }
                tabs.is_empty()
            } else {
                return false;
            };

        if source_empty {
            self.close_tile(source_node_id);
        }

        true
    }

    /// Insert a panel into this layout at a target node + drop zone.
    ///
    /// If the layout is empty, this creates a single-tile layout.
    pub fn add_panel_at(
        &mut self,
        panel: PanelId,
        target_node_id: NodeId,
        drop_zone: crate::drop_zone::DropZone,
    ) -> bool {
        use crate::drop_zone::DropZone;

        if self.contains_panel(panel) {
            return true;
        }

        if self.is_empty() {
            *self = DockLayout::single(panel);
            return true;
        }

        if matches!(drop_zone, DropZone::Center) {
            if let Some(FlatNode::Tile { tabs, .. }) = self.nodes.get_mut(&target_node_id) {
                tabs.add_panel(panel);
                tabs.set_active(tabs.len().saturating_sub(1));
                return true;
            }
            return false;
        }

        let direction = drop_zone.split_direction();

        if self.nodes.contains_key(&target_node_id) {
            self.split_tile(target_node_id, direction, panel, 50.0)
                .is_some()
        } else if let Some(root_id) = self.root {
            self.split_tile(root_id, direction, panel, 50.0).is_some()
        } else {
            *self = DockLayout::single(panel);
            true
        }
    }

    /// Return all panel IDs currently present in this layout.
    pub fn all_panels(&self) -> Vec<PanelId> {
        let mut out = Vec::new();
        for node in self.nodes.values() {
            if let FlatNode::Tile { tabs, .. } = node {
                out.extend(tabs.panels.iter().copied());
            }
        }
        out
    }

    /// Set active tab by panel identity within the containing tile.
    pub fn set_active_panel(&mut self, tile_anchor: PanelId, active_panel: PanelId) -> bool {
        let Some(node_id) = self.find_node_for_panel(tile_anchor) else {
            return false;
        };
        let Some(FlatNode::Tile { tabs, .. }) = self.nodes.get_mut(&node_id) else {
            return false;
        };
        let Some(idx) = tabs.panels.iter().position(|p| *p == active_panel) else {
            return false;
        };
        tabs.set_active(idx);
        true
    }

    /// Extract a semantic region as a subtree.
    pub fn extract_region(&self, region: LayoutRegion) -> Option<DockNode> {
        let node_id = self.detect_region_node(region)?;
        Some(self.build_tree_node(node_id))
    }

    /// Replace a semantic region with a provided subtree node.
    pub fn replace_region(&mut self, region: LayoutRegion, node: DockNode) -> bool {
        let Some(target_id) = self.detect_region_node(region) else {
            return false;
        };

        if self.root == Some(target_id) {
            *self = DockLayout::from_tree(node);
            return true;
        }

        let Some((parent_id, is_first_child)) = self.find_parent(target_id) else {
            return false;
        };

        let new_id = self.insert_tree_node(node);
        if let Some(FlatNode::Split { first, second, .. }) = self.nodes.get_mut(&parent_id) {
            if is_first_child {
                *first = new_id;
            } else {
                *second = new_id;
            }
        } else {
            return false;
        }

        self.remove_subtree(target_id);
        true
    }

    /// Compose a layout by taking named regions from source layouts.
    pub fn compose(regions: &[(LayoutRegion, &DockLayout)]) -> DockLayout {
        if regions.is_empty() {
            return DockLayout::empty();
        }

        let mut result = regions[0].1.clone();

        for (region, source) in regions {
            if let Some(node) = source.extract_region(*region) {
                if !result.replace_region(*region, node.clone()) && result.is_empty() {
                    result = DockLayout::from_tree(node);
                }
            }
        }

        result
    }

    // --- Private helpers ---

    fn insert_tree_node(&mut self, tree: DockNode) -> NodeId {
        let node_id = NodeId::new();
        match tree {
            DockNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let first_id = self.insert_tree_node(*first);
                let second_id = self.insert_tree_node(*second);
                self.nodes.insert(
                    node_id,
                    FlatNode::Split {
                        direction,
                        ratio,
                        first: first_id,
                        second: second_id,
                    },
                );
            }
            DockNode::Tile { id, tabs } => {
                self.nodes
                    .insert(node_id, FlatNode::Tile { tile_id: id, tabs });
            }
        }
        node_id
    }

    fn build_tree_node(&self, node_id: NodeId) -> DockNode {
        match self.nodes.get(&node_id) {
            Some(FlatNode::Split {
                direction,
                ratio,
                first,
                second,
            }) => DockNode::Split {
                direction: *direction,
                ratio: *ratio,
                first: Box::new(self.build_tree_node(*first)),
                second: Box::new(self.build_tree_node(*second)),
            },
            Some(FlatNode::Tile { tile_id, tabs }) => DockNode::Tile {
                id: *tile_id,
                tabs: tabs.clone(),
            },
            None => DockNode::tile(PanelId::Performance), // fallback
        }
    }

    fn find_parent(&self, child_id: NodeId) -> Option<(NodeId, bool)> {
        for (node_id, node) in &self.nodes {
            if let FlatNode::Split { first, second, .. } = node {
                if *first == child_id {
                    return Some((*node_id, true));
                }
                if *second == child_id {
                    return Some((*node_id, false));
                }
            }
        }
        None
    }

    fn collect_panels(&self, node_id: NodeId) -> Vec<PanelId> {
        let Some(node) = self.nodes.get(&node_id) else {
            return Vec::new();
        };
        match node {
            FlatNode::Tile { tabs, .. } => tabs.panels.clone(),
            FlatNode::Split { first, second, .. } => {
                let mut out = self.collect_panels(*first);
                out.extend(self.collect_panels(*second));
                out
            }
        }
    }

    fn aggregate_constraints(&self, panels: &[PanelId]) -> (f64, f64, u8) {
        let mut min_size: f64 = 0.0;
        let mut pref_size: f64 = 0.0;
        let mut max_priority = 0_u8;

        for panel in panels {
            if let Some(c) = self.constraints.get(panel) {
                if let Some(min) = c.min_width {
                    min_size = min_size.max(min);
                }
                if let Some(pref) = c.preferred_width {
                    pref_size = pref_size.max(pref);
                }
                max_priority = max_priority.max(c.resize_priority);
            }
        }

        let priority = if max_priority == 0 { 128 } else { max_priority };
        (min_size, pref_size, priority)
    }

    fn constrain_ratio(&self, node_id: NodeId, proposed_ratio: f64, available_size: f64) -> f64 {
        let Some(FlatNode::Split { first, second, .. }) = self.nodes.get(&node_id) else {
            return proposed_ratio;
        };

        let first_panels = self.collect_panels(*first);
        let second_panels = self.collect_panels(*second);
        if first_panels.is_empty() || second_panels.is_empty() || available_size <= 0.0 {
            return proposed_ratio;
        }

        let (first_min, _, _) = self.aggregate_constraints(&first_panels);
        let (second_min, _, _) = self.aggregate_constraints(&second_panels);

        let min_ratio = (first_min / available_size * 100.0).clamp(0.0, 95.0);
        let max_ratio = ((available_size - second_min) / available_size * 100.0).clamp(5.0, 100.0);
        proposed_ratio.clamp(min_ratio, max_ratio).clamp(5.0, 95.0)
    }

    fn apply_auto_hide_projection(&mut self) {
        let split_nodes: Vec<NodeId> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| {
                if matches!(node, FlatNode::Split { .. }) {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();

        for split_id in split_nodes {
            let Some(FlatNode::Split { first, second, .. }) = self.nodes.get(&split_id) else {
                continue;
            };
            let first = *first;
            let second = *second;

            let first_collapsed = self.zone_mode(first) == DockZoneMode::AutoHide
                && !self.expanded_zones.contains(&first);
            let second_collapsed = self.zone_mode(second) == DockZoneMode::AutoHide
                && !self.expanded_zones.contains(&second);

            let collapsed_ratio =
                (Self::AUTO_HIDE_COLLAPSED_SIZE / 1000.0 * 100.0).clamp(1.0, 20.0);
            let next_ratio = if first_collapsed && !second_collapsed {
                Some(collapsed_ratio)
            } else if second_collapsed && !first_collapsed {
                Some(100.0 - collapsed_ratio)
            } else {
                None
            };

            if let Some(r) = next_ratio {
                if let Some(FlatNode::Split { ratio, .. }) = self.nodes.get_mut(&split_id) {
                    *ratio = r;
                }
            }
        }
    }

    fn remove_subtree(&mut self, node_id: NodeId) {
        let children = match self.nodes.get(&node_id) {
            Some(FlatNode::Split { first, second, .. }) => Some((*first, *second)),
            _ => None,
        };

        if let Some((first, second)) = children {
            self.remove_subtree(first);
            self.remove_subtree(second);
        }

        self.nodes.remove(&node_id);
        self.zone_modes.remove(&node_id);
        self.expanded_zones.remove(&node_id);
    }

    fn detect_region_node(&self, region: LayoutRegion) -> Option<NodeId> {
        let root = self.root?;
        let mut tiles = Vec::new();
        self.collect_tile_rects(root, 0.0, 0.0, 1.0, 1.0, &mut tiles);
        if tiles.is_empty() {
            return None;
        }

        match region {
            LayoutRegion::LeftSidebar => tiles
                .iter()
                .min_by(|a, b| {
                    a.x.total_cmp(&b.x)
                        .then_with(|| b.area().total_cmp(&a.area()))
                })
                .map(|r| r.id),
            LayoutRegion::RightSidebar => tiles
                .iter()
                .max_by(|a, b| {
                    a.right()
                        .total_cmp(&b.right())
                        .then_with(|| a.area().total_cmp(&b.area()))
                })
                .map(|r| r.id),
            LayoutRegion::Center => tiles
                .iter()
                .max_by(|a, b| a.area().total_cmp(&b.area()))
                .map(|r| r.id),
            LayoutRegion::BottomPanel => tiles
                .iter()
                .max_by(|a, b| {
                    a.bottom()
                        .total_cmp(&b.bottom())
                        .then_with(|| a.y.total_cmp(&b.y))
                        .then_with(|| a.width.total_cmp(&b.width))
                })
                .map(|r| r.id),
            LayoutRegion::TopBar => tiles
                .iter()
                .min_by(|a, b| {
                    a.y.total_cmp(&b.y)
                        .then_with(|| b.width.total_cmp(&a.width))
                })
                .map(|r| r.id),
        }
    }

    fn collect_tile_rects(
        &self,
        node_id: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        out: &mut Vec<RegionRect>,
    ) {
        let Some(node) = self.nodes.get(&node_id) else {
            return;
        };

        match node {
            FlatNode::Tile { .. } => out.push(RegionRect {
                id: node_id,
                x,
                y,
                width,
                height,
            }),
            FlatNode::Split {
                direction,
                ratio,
                first,
                second,
            } => match direction {
                SplitDirection::Horizontal => {
                    let first_w = width * (*ratio / 100.0);
                    let second_w = (width - first_w).max(0.0);
                    self.collect_tile_rects(*first, x, y, first_w, height, out);
                    self.collect_tile_rects(*second, x + first_w, y, second_w, height, out);
                }
                SplitDirection::Vertical => {
                    let first_h = height * (*ratio / 100.0);
                    let second_h = (height - first_h).max(0.0);
                    self.collect_tile_rects(*first, x, y, width, first_h, out);
                    self.collect_tile_rects(*second, x, y + first_h, width, second_h, out);
                }
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RegionRect {
    id: NodeId,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl RegionRect {
    fn area(self) -> f64 {
        self.width * self.height
    }

    fn right(self) -> f64 {
        self.x + self.width
    }

    fn bottom(self) -> f64 {
        self.y + self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_panel_layout() {
        let layout = DockLayout::single(PanelId::Performance);
        assert!(layout.root().is_some());
        assert_eq!(layout.node_count(), 1);
        assert!(layout.contains_panel(PanelId::Performance));
        assert!(!layout.contains_panel(PanelId::ChartEditor));
    }

    #[test]
    fn from_tree_roundtrip() {
        let tree = DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            30.0,
        );
        let layout = DockLayout::from_tree(tree);
        assert_eq!(layout.node_count(), 3); // 1 split + 2 tiles

        let restored = layout.to_tree().unwrap();
        match restored {
            DockNode::Split {
                direction, ratio, ..
            } => {
                assert_eq!(direction, SplitDirection::Horizontal);
                assert!((ratio - 30.0).abs() < f64::EPSILON);
            }
            _ => panic!("expected Split"),
        }
    }

    #[test]
    fn split_tile_creates_correct_structure() {
        let mut layout = DockLayout::single(PanelId::Performance);
        let root_id = layout.root().unwrap();

        let new_id = layout.split_tile(
            root_id,
            SplitDirection::Horizontal,
            PanelId::ChartEditor,
            50.0,
        );
        assert!(new_id.is_some());

        // Root should now be a split
        assert!(matches!(
            layout.get_node(root_id),
            Some(FlatNode::Split { .. })
        ));

        // Should have 3 nodes: 1 split + 2 tiles
        assert_eq!(layout.node_count(), 3);
        assert!(layout.contains_panel(PanelId::Performance));
        assert!(layout.contains_panel(PanelId::ChartEditor));
    }

    #[test]
    fn close_tile_promotes_sibling() {
        let mut layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            30.0,
        ));
        let root_id = layout.root().unwrap();

        // Find the Navigator tile's node ID
        let nav_node_id = match layout.get_node(root_id) {
            Some(FlatNode::Split { first, .. }) => *first,
            _ => panic!("expected split"),
        };

        assert!(layout.close_tile(nav_node_id));

        // Layout should now have just one tile (Performance promoted to root's place)
        assert_eq!(layout.node_count(), 1);
        assert!(layout.contains_panel(PanelId::Performance));
        assert!(!layout.contains_panel(PanelId::Navigator));
    }

    #[test]
    fn close_root_tile_clears_layout() {
        let mut layout = DockLayout::single(PanelId::Performance);
        let root_id = layout.root().unwrap();

        assert!(layout.close_tile(root_id));
        assert!(layout.root().is_none());
        assert_eq!(layout.node_count(), 0);
    }

    #[test]
    fn update_split_ratio_clamps() {
        let mut layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            50.0,
        ));
        let root_id = layout.root().unwrap();

        layout.update_split_ratio(root_id, 150.0);
        if let Some(FlatNode::Split { ratio, .. }) = layout.get_node(root_id) {
            assert!((ratio - 95.0).abs() < f64::EPSILON);
        }

        layout.update_split_ratio(root_id, -10.0);
        if let Some(FlatNode::Split { ratio, .. }) = layout.get_node(root_id) {
            assert!((ratio - 5.0).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn facet_json_roundtrip() {
        let layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::vertical(
                DockNode::tile(PanelId::Performance),
                DockNode::tile(PanelId::Transport),
                80.0,
            ),
            25.0,
        ));

        let json = crate::persistence::layout_to_json(&layout).unwrap();
        let restored = crate::persistence::layout_from_json(&json).unwrap();

        assert_eq!(restored.node_count(), layout.node_count());
        assert!(restored.contains_panel(PanelId::Navigator));
        assert!(restored.contains_panel(PanelId::Performance));
        assert!(restored.contains_panel(PanelId::Transport));
    }

    #[test]
    fn all_tiles_returns_only_leaves() {
        let layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            30.0,
        ));
        let tiles = layout.all_tiles();
        assert_eq!(tiles.len(), 2);
    }

    #[test]
    fn find_node_for_panel() {
        let layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            30.0,
        ));
        assert!(layout.find_node_for_panel(PanelId::Navigator).is_some());
        assert!(layout.find_node_for_panel(PanelId::Performance).is_some());
        assert!(layout.find_node_for_panel(PanelId::ChartEditor).is_none());
    }

    #[test]
    fn move_panel_to_edge_creates_split() {
        use crate::drop_zone::DropZone;

        let mut layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            50.0,
        ));
        let target = layout.find_node_for_panel(PanelId::Performance).unwrap();

        assert!(layout.move_panel(PanelId::Navigator, target, DropZone::Right));

        // Navigator should still exist but in a new position
        assert!(layout.contains_panel(PanelId::Navigator));
        assert!(layout.contains_panel(PanelId::Performance));
    }

    #[test]
    fn move_panel_to_center_adds_tab() {
        use crate::drop_zone::DropZone;

        let mut layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            50.0,
        ));
        let target = layout.find_node_for_panel(PanelId::Performance).unwrap();

        assert!(layout.move_panel(PanelId::Navigator, target, DropZone::Center));

        // Both panels should be in the same tile now
        assert!(layout.contains_panel(PanelId::Navigator));
        assert!(layout.contains_panel(PanelId::Performance));
        // Only 1 tile left (the navigator's empty tile was closed)
        let tiles = layout.all_tiles();
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].1.len(), 2);
    }

    #[test]
    fn default_presets_are_valid() {
        let presets = crate::defaults::default_presets();
        assert!(!presets.presets.is_empty());
        for preset in &presets.presets {
            assert!(!preset.name.is_empty());
            let layout = &preset.layout;
            assert!(layout.root().is_some());
            assert!(layout.node_count() > 0);
            // All presets should survive a tree roundtrip
            let tree = layout.to_tree().unwrap();
            let restored = DockLayout::from_tree(tree);
            assert_eq!(restored.node_count(), layout.node_count());
        }
    }

    #[test]
    fn remove_panel_closes_empty_source_tile() {
        let mut layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            50.0,
        ));
        assert!(layout.remove_panel(PanelId::Navigator));
        assert!(!layout.contains_panel(PanelId::Navigator));
        assert!(layout.contains_panel(PanelId::Performance));
        assert_eq!(layout.all_tiles().len(), 1);
    }

    #[test]
    fn add_panel_at_center_adds_tab() {
        let mut layout = DockLayout::single(PanelId::Performance);
        let root = layout.root().unwrap();
        assert!(layout.add_panel_at(PanelId::Navigator, root, crate::drop_zone::DropZone::Center));
        let tiles = layout.all_tiles();
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].1.len(), 2);
    }

    #[test]
    fn all_panels_collects_all_tile_panels() {
        let layout = DockLayout::from_tree(DockNode::tabbed(
            vec![PanelId::Performance, PanelId::Navigator, PanelId::Transport],
            0,
        ));
        let panels = layout.all_panels();
        assert_eq!(panels.len(), 3);
        assert!(panels.contains(&PanelId::Performance));
        assert!(panels.contains(&PanelId::Navigator));
        assert!(panels.contains(&PanelId::Transport));
    }

    #[test]
    fn resize_blocked_at_min_constraint() {
        let mut layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            50.0,
        ));
        layout.set_constraints(
            PanelId::Navigator,
            PanelConstraints::new().with_min_width(700.0),
        );
        let root = layout.root().unwrap();
        layout.update_split_ratio(root, 10.0);
        let effective = layout.effective_ratio(root, 1000.0);
        assert!(effective >= 70.0);
    }

    #[test]
    fn effective_ratio_adjusts_for_constraints() {
        let mut layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            50.0,
        ));
        layout.set_constraints(
            PanelId::Navigator,
            PanelConstraints::new().with_preferred_width(700.0),
        );
        layout.set_constraints(
            PanelId::Performance,
            PanelConstraints::new().with_preferred_width(300.0),
        );
        let root = layout.root().unwrap();
        let ratio = layout.effective_ratio(root, 1000.0);
        assert!(ratio > 60.0);
    }

    #[test]
    fn priority_ordering_when_panels_compete() {
        let mut layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            50.0,
        ));
        layout.set_constraints(
            PanelId::Navigator,
            PanelConstraints::new()
                .with_min_width(700.0)
                .with_resize_priority(240),
        );
        layout.set_constraints(
            PanelId::Performance,
            PanelConstraints::new()
                .with_min_width(700.0)
                .with_resize_priority(20),
        );
        let root = layout.root().unwrap();
        let ratio = layout.effective_ratio(root, 1000.0);
        assert!(ratio > 70.0);
    }

    #[test]
    fn auto_hide_collapsed_in_visible_layout() {
        let mut layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            40.0,
        ));
        let nav_node = layout.find_node_for_panel(PanelId::Navigator).unwrap();
        let root = layout.root().unwrap();
        layout.set_zone_mode(nav_node, DockZoneMode::AutoHide);
        layout.collapse_zone(nav_node);

        let visible = layout.visible_layout();
        let ratio = match visible.get_node(root) {
            Some(FlatNode::Split { ratio, .. }) => *ratio,
            _ => 0.0,
        };
        assert!(ratio < 10.0);
    }

    #[test]
    fn expand_collapse_toggles() {
        let mut layout = DockLayout::single(PanelId::Navigator);
        let node = layout.root().unwrap();
        layout.set_zone_mode(node, DockZoneMode::AutoHide);
        assert!(!layout.is_zone_expanded(node));
        layout.expand_zone(node);
        assert!(layout.is_zone_expanded(node));
        layout.collapse_zone(node);
        assert!(!layout.is_zone_expanded(node));
    }

    #[test]
    fn pinned_zone_always_visible() {
        let mut layout = DockLayout::single(PanelId::Navigator);
        let node = layout.root().unwrap();
        layout.set_zone_mode(node, DockZoneMode::Pinned);
        layout.collapse_zone(node);
        let size = layout.zone_visible_size(node, 280.0);
        assert!((size - 280.0).abs() < f64::EPSILON);
    }

    #[test]
    fn extract_left_sidebar_region() {
        let layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::vertical(
                DockNode::tile(PanelId::Performance),
                DockNode::tile(PanelId::Transport),
                75.0,
            ),
            20.0,
        ));

        let left = layout
            .extract_region(LayoutRegion::LeftSidebar)
            .expect("left sidebar");
        let extracted = DockLayout::from_tree(left);
        assert!(extracted.contains_panel(PanelId::Navigator));
        assert!(!extracted.contains_panel(PanelId::Performance));
    }

    #[test]
    fn compose_center_from_a_and_sidebar_from_b() {
        let layout_a = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            25.0,
        ));
        let layout_b = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Setlist),
            DockNode::tile(PanelId::ChartEditor),
            30.0,
        ));

        let composed = DockLayout::compose(&[
            (LayoutRegion::Center, &layout_a),
            (LayoutRegion::LeftSidebar, &layout_b),
        ]);

        assert!(composed.contains_panel(PanelId::Performance));
        assert!(composed.contains_panel(PanelId::Setlist));
        assert!(!composed.contains_panel(PanelId::Navigator));
    }

    #[test]
    fn replace_region_preserves_other_areas() {
        let mut layout = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::vertical(
                DockNode::tile(PanelId::Performance),
                DockNode::tile(PanelId::Transport),
                70.0,
            ),
            25.0,
        ));

        assert!(layout.replace_region(
            LayoutRegion::BottomPanel,
            DockNode::tile(PanelId::ChartEditor),
        ));

        assert!(layout.contains_panel(PanelId::Navigator));
        assert!(layout.contains_panel(PanelId::Performance));
        assert!(layout.contains_panel(PanelId::ChartEditor));
        assert!(!layout.contains_panel(PanelId::Transport));
    }
}
