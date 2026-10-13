//! Layout diff and patch operations.

use std::collections::HashSet;

use crate::drop_zone::DropZone;
use crate::id::NodeId;
use crate::layout::{DockLayout, FlatNode};
use crate::panel::PanelId;
use crate::tree::{DockNode, SplitDirection};

/// Primitive operation in a layout diff.
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutOp {
    AddPanel {
        panel: PanelId,
        target_panel: Option<PanelId>,
        zone: DropZone,
    },
    RemovePanel {
        panel: PanelId,
        target_panel: Option<PanelId>,
        zone: DropZone,
    },
    MovePanel {
        panel: PanelId,
        source_target: Option<PanelId>,
        source_zone: DropZone,
        target_target: Option<PanelId>,
        target_zone: DropZone,
    },
    SetRatio {
        node_id: NodeId,
        from: f64,
        to: f64,
    },
    AddTab {
        panel: PanelId,
        target_panel: PanelId,
    },
    RemoveTab {
        panel: PanelId,
        target_panel: PanelId,
    },
    SwitchActiveTab {
        target_panel: PanelId,
        from_active: PanelId,
        to_active: PanelId,
    },
}

/// Sequence of operations transforming one layout into another.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayoutDiff {
    pub ops: Vec<LayoutOp>,
}

impl LayoutDiff {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn invert(&self) -> Self {
        let mut inv = Vec::with_capacity(self.ops.len());
        for op in self.ops.iter().rev() {
            inv.push(match op {
                LayoutOp::AddPanel {
                    panel,
                    target_panel,
                    zone,
                } => LayoutOp::RemovePanel {
                    panel: *panel,
                    target_panel: *target_panel,
                    zone: *zone,
                },
                LayoutOp::RemovePanel {
                    panel,
                    target_panel,
                    zone,
                } => LayoutOp::AddPanel {
                    panel: *panel,
                    target_panel: *target_panel,
                    zone: *zone,
                },
                LayoutOp::MovePanel {
                    panel,
                    source_target,
                    source_zone,
                    target_target,
                    target_zone,
                } => LayoutOp::MovePanel {
                    panel: *panel,
                    source_target: *target_target,
                    source_zone: *target_zone,
                    target_target: *source_target,
                    target_zone: *source_zone,
                },
                LayoutOp::SetRatio { node_id, from, to } => LayoutOp::SetRatio {
                    node_id: *node_id,
                    from: *to,
                    to: *from,
                },
                LayoutOp::AddTab {
                    panel,
                    target_panel,
                } => LayoutOp::RemoveTab {
                    panel: *panel,
                    target_panel: *target_panel,
                },
                LayoutOp::RemoveTab {
                    panel,
                    target_panel,
                } => LayoutOp::AddTab {
                    panel: *panel,
                    target_panel: *target_panel,
                },
                LayoutOp::SwitchActiveTab {
                    target_panel,
                    from_active,
                    to_active,
                } => LayoutOp::SwitchActiveTab {
                    target_panel: *target_panel,
                    from_active: *to_active,
                    to_active: *from_active,
                },
            });
        }
        Self { ops: inv }
    }
}

impl DockLayout {
    /// Compute a diff that transforms `from` into `to`.
    pub fn diff(from: &DockLayout, to: &DockLayout) -> LayoutDiff {
        let from_panels: HashSet<PanelId> = from.all_panels().into_iter().collect();
        let to_panels: HashSet<PanelId> = to.all_panels().into_iter().collect();
        let mut ops = Vec::new();

        for panel in from_panels.difference(&to_panels) {
            let (target_panel, zone) =
                insertion_hint(from, *panel).unwrap_or((None, DropZone::Center));
            ops.push(LayoutOp::RemovePanel {
                panel: *panel,
                target_panel,
                zone,
            });
        }

        for panel in to_panels.difference(&from_panels) {
            let (target_panel, zone) =
                insertion_hint(to, *panel).unwrap_or((None, DropZone::Center));
            ops.push(LayoutOp::AddPanel {
                panel: *panel,
                target_panel,
                zone,
            });
        }

        for panel in from_panels.intersection(&to_panels) {
            let from_peers = peers(from, *panel);
            let to_peers = peers(to, *panel);
            if from_peers != to_peers {
                let (source_target, source_zone) =
                    insertion_hint(from, *panel).unwrap_or((None, DropZone::Center));
                let (target_target, target_zone) =
                    insertion_hint(to, *panel).unwrap_or((None, DropZone::Center));
                ops.push(LayoutOp::MovePanel {
                    panel: *panel,
                    source_target,
                    source_zone,
                    target_target,
                    target_zone,
                });
            }
        }

        for (_, tabs) in to.all_tiles() {
            if tabs.len() < 2 {
                continue;
            }
            let Some(anchor) = tabs.panels.first().copied() else {
                continue;
            };
            let Some(to_active) = tabs.active_panel() else {
                continue;
            };
            let Some(from_active) = active_panel_in_tile(from, anchor) else {
                continue;
            };
            if from_active != to_active {
                ops.push(LayoutOp::SwitchActiveTab {
                    target_panel: anchor,
                    from_active,
                    to_active,
                });
            }
        }

        let to_ratios = split_ratios(to);
        for (node_id, ratio) in split_ratios(from) {
            if let Some((_, to_ratio)) = to_ratios.iter().find(|(id, _)| *id == node_id) {
                if (ratio - *to_ratio).abs() > f64::EPSILON {
                    ops.push(LayoutOp::SetRatio {
                        node_id,
                        from: ratio,
                        to: *to_ratio,
                    });
                }
            }
        }

        LayoutDiff { ops }
    }

    /// Apply a diff sequentially.
    pub fn apply(&mut self, diff: &LayoutDiff) {
        for op in &diff.ops {
            self.apply_op(op);
        }
    }

    fn apply_op(&mut self, op: &LayoutOp) {
        match op {
            LayoutOp::AddPanel {
                panel,
                target_panel,
                zone,
            } => {
                add_panel_with_hint(self, *panel, *target_panel, *zone);
            }
            LayoutOp::RemovePanel { panel, .. } => {
                self.remove_panel(*panel);
            }
            LayoutOp::MovePanel {
                panel,
                target_target,
                target_zone,
                ..
            } => {
                self.remove_panel(*panel);
                add_panel_with_hint(self, *panel, *target_target, *target_zone);
            }
            LayoutOp::SetRatio { node_id, to, .. } => {
                self.update_split_ratio(*node_id, *to);
            }
            LayoutOp::AddTab {
                panel,
                target_panel,
            } => {
                if let Some(node) = self.find_node_for_panel(*target_panel) {
                    if let Some(FlatNode::Tile { tabs, .. }) = self.get_node_mut(node) {
                        if !tabs.panels.contains(panel) {
                            tabs.add_panel(*panel);
                        }
                    }
                }
            }
            LayoutOp::RemoveTab { panel, .. } => {
                self.remove_panel(*panel);
            }
            LayoutOp::SwitchActiveTab {
                target_panel,
                to_active,
                ..
            } => {
                self.set_active_panel(*target_panel, *to_active);
            }
        }
    }
}

fn add_panel_with_hint(
    layout: &mut DockLayout,
    panel: PanelId,
    target_panel: Option<PanelId>,
    zone: DropZone,
) {
    if layout.contains_panel(panel) {
        return;
    }

    if let Some(target) = target_panel {
        if let Some(node) = layout.find_node_for_panel(target) {
            let _ = layout.add_panel_at(panel, node, zone);
            return;
        }
    }

    if let Some(root) = layout.root() {
        let _ = layout.add_panel_at(panel, root, zone);
    } else {
        *layout = DockLayout::single(panel);
    }
}

fn peers(layout: &DockLayout, panel: PanelId) -> Vec<PanelId> {
    let mut out = layout
        .all_tiles()
        .into_iter()
        .find_map(|(_, tabs)| {
            if tabs.panels.contains(&panel) {
                Some(
                    tabs.panels
                        .iter()
                        .copied()
                        .filter(|p| *p != panel)
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            }
        })
        .unwrap_or_default();
    out.sort_by_key(|p| *p as u8);
    out
}

fn active_panel_in_tile(layout: &DockLayout, anchor_panel: PanelId) -> Option<PanelId> {
    layout.all_tiles().into_iter().find_map(|(_, tabs)| {
        if tabs.panels.contains(&anchor_panel) {
            tabs.active_panel()
        } else {
            None
        }
    })
}

fn insertion_hint(layout: &DockLayout, panel: PanelId) -> Option<(Option<PanelId>, DropZone)> {
    let tree = layout.to_tree()?;
    insertion_hint_node(&tree, panel).map(|(target, zone)| (Some(target), zone))
}

fn insertion_hint_node(node: &DockNode, panel: PanelId) -> Option<(PanelId, DropZone)> {
    match node {
        DockNode::Tile { tabs, .. } => {
            if !tabs.panels.contains(&panel) {
                return None;
            }
            let target = tabs.panels.iter().copied().find(|p| *p != panel)?;
            Some((target, DropZone::Center))
        }
        DockNode::Split {
            direction,
            first,
            second,
            ..
        } => {
            let in_first = node_contains_panel(first, panel);
            let in_second = node_contains_panel(second, panel);
            if in_first {
                if let Some(target) = node_first_panel(second) {
                    let zone = match direction {
                        SplitDirection::Horizontal => DropZone::Left,
                        SplitDirection::Vertical => DropZone::Top,
                    };
                    return Some((target, zone));
                }
            }
            if in_second {
                if let Some(target) = node_first_panel(first) {
                    let zone = match direction {
                        SplitDirection::Horizontal => DropZone::Right,
                        SplitDirection::Vertical => DropZone::Bottom,
                    };
                    return Some((target, zone));
                }
            }

            insertion_hint_node(first, panel).or_else(|| insertion_hint_node(second, panel))
        }
    }
}

fn node_contains_panel(node: &DockNode, panel: PanelId) -> bool {
    match node {
        DockNode::Tile { tabs, .. } => tabs.panels.contains(&panel),
        DockNode::Split { first, second, .. } => {
            node_contains_panel(first, panel) || node_contains_panel(second, panel)
        }
    }
}

fn node_first_panel(node: &DockNode) -> Option<PanelId> {
    match node {
        DockNode::Tile { tabs, .. } => tabs.panels.first().copied(),
        DockNode::Split { first, second, .. } => {
            node_first_panel(first).or_else(|| node_first_panel(second))
        }
    }
}

fn split_ratios(layout: &DockLayout) -> Vec<(NodeId, f64)> {
    let mut out = Vec::new();
    for (node_id, node) in layout.raw_nodes() {
        if let FlatNode::Split { ratio, .. } = node {
            out.push((node_id, *ratio));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::DockNode;

    #[test]
    fn diff_identical_layouts_empty() {
        let a = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            40.0,
        ));
        let diff = DockLayout::diff(&a, &a);
        assert!(diff.is_empty());
    }

    #[test]
    fn diff_added_panel_produces_add_panel() {
        let a = DockLayout::single(PanelId::Performance);
        let b = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Performance),
            DockNode::tile(PanelId::Navigator),
            50.0,
        ));
        let diff = DockLayout::diff(&a, &b);
        assert!(diff.ops.iter().any(|op| matches!(
            op,
            LayoutOp::AddPanel {
                panel: PanelId::Navigator,
                ..
            }
        )));
    }

    #[test]
    fn diff_moved_panel_produces_move_panel() {
        let a = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Navigator),
            DockNode::tile(PanelId::Performance),
            50.0,
        ));
        let b = DockLayout::from_tree(DockNode::tabbed(
            vec![PanelId::Performance, PanelId::Navigator],
            0,
        ));
        let diff = DockLayout::diff(&a, &b);
        assert!(diff.ops.iter().any(|op| matches!(
            op,
            LayoutOp::MovePanel {
                panel: PanelId::Navigator,
                ..
            }
        )));
    }

    #[test]
    fn apply_diff_transforms_a_to_b() {
        let mut a = DockLayout::single(PanelId::Performance);
        let b = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Performance),
            DockNode::tile(PanelId::Navigator),
            50.0,
        ));

        let diff = DockLayout::diff(&a, &b);
        a.apply(&diff);
        assert!(a.contains_panel(PanelId::Performance));
        assert!(a.contains_panel(PanelId::Navigator));
        assert_eq!(a.all_tiles().len(), b.all_tiles().len());
    }

    #[test]
    fn apply_invert_restores_original() {
        let a = DockLayout::single(PanelId::Performance);
        let b = DockLayout::from_tree(DockNode::horizontal(
            DockNode::tile(PanelId::Performance),
            DockNode::tile(PanelId::Navigator),
            50.0,
        ));

        let diff = DockLayout::diff(&a, &b);
        let inv = diff.invert();
        let mut current = a.clone();
        current.apply(&diff);
        current.apply(&inv);

        assert_eq!(current.all_tiles().len(), a.all_tiles().len());
        assert!(current.contains_panel(PanelId::Performance));
        assert!(!current.contains_panel(PanelId::Navigator));
    }
}
