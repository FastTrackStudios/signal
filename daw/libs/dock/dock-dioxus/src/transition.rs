//! Layout transition computation for animated preset switching.

use std::collections::HashMap;

use dock_proto::{DockLayout, DockNode, LayoutOp, PanelId, SplitDirection};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransitionRect {
    pub x_pct: f64,
    pub y_pct: f64,
    pub w_pct: f64,
    pub h_pct: f64,
}

impl TransitionRect {
    fn centered() -> Self {
        Self {
            x_pct: 45.0,
            y_pct: 45.0,
            w_pct: 10.0,
            h_pct: 10.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionKind {
    Persisted,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PanelTransition {
    pub panel: PanelId,
    pub from_rect: TransitionRect,
    pub to_rect: TransitionRect,
    pub kind: TransitionKind,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutTransition {
    pub panels: Vec<PanelTransition>,
    pub duration_ms: u64,
}

impl LayoutTransition {
    pub fn is_empty(&self) -> bool {
        self.panels.is_empty()
    }
}

/// Compute logical transition params from one layout to another.
pub fn compute_transition(
    from: &DockLayout,
    to: &DockLayout,
    duration_ms: u64,
) -> LayoutTransition {
    let from_rects = panel_rects(from);
    let to_rects = panel_rects(to);
    let diff = DockLayout::diff(from, to);

    let mut added = std::collections::HashSet::new();
    let mut removed = std::collections::HashSet::new();
    for op in diff.ops {
        match op {
            LayoutOp::AddPanel { panel, .. } => {
                added.insert(panel);
            }
            LayoutOp::RemovePanel { panel, .. } => {
                removed.insert(panel);
            }
            _ => {}
        }
    }

    let mut all = std::collections::HashSet::new();
    all.extend(from_rects.keys().copied());
    all.extend(to_rects.keys().copied());

    let mut panels = Vec::new();
    for panel in all {
        let from_rect = from_rects
            .get(&panel)
            .copied()
            .unwrap_or_else(TransitionRect::centered);
        let to_rect = to_rects
            .get(&panel)
            .copied()
            .unwrap_or_else(TransitionRect::centered);

        let kind = if added.contains(&panel) {
            TransitionKind::Added
        } else if removed.contains(&panel) {
            TransitionKind::Removed
        } else {
            TransitionKind::Persisted
        };

        panels.push(PanelTransition {
            panel,
            from_rect,
            to_rect,
            kind,
            duration_ms,
        });
    }

    LayoutTransition {
        panels,
        duration_ms,
    }
}

fn panel_rects(layout: &DockLayout) -> HashMap<PanelId, TransitionRect> {
    let mut out = HashMap::new();
    let Some(tree) = layout.to_tree() else {
        return out;
    };
    fill_rects(
        &tree,
        TransitionRect {
            x_pct: 0.0,
            y_pct: 0.0,
            w_pct: 100.0,
            h_pct: 100.0,
        },
        &mut out,
    );
    out
}

fn fill_rects(node: &DockNode, rect: TransitionRect, out: &mut HashMap<PanelId, TransitionRect>) {
    match node {
        DockNode::Tile { tabs, .. } => {
            for panel in &tabs.panels {
                out.insert(*panel, rect);
            }
        }
        DockNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let ratio = (*ratio).clamp(0.0, 100.0) / 100.0;
            match direction {
                SplitDirection::Horizontal => {
                    let first_w = rect.w_pct * ratio;
                    let second_w = rect.w_pct - first_w;
                    fill_rects(
                        first,
                        TransitionRect {
                            x_pct: rect.x_pct,
                            y_pct: rect.y_pct,
                            w_pct: first_w,
                            h_pct: rect.h_pct,
                        },
                        out,
                    );
                    fill_rects(
                        second,
                        TransitionRect {
                            x_pct: rect.x_pct + first_w,
                            y_pct: rect.y_pct,
                            w_pct: second_w,
                            h_pct: rect.h_pct,
                        },
                        out,
                    );
                }
                SplitDirection::Vertical => {
                    let first_h = rect.h_pct * ratio;
                    let second_h = rect.h_pct - first_h;
                    fill_rects(
                        first,
                        TransitionRect {
                            x_pct: rect.x_pct,
                            y_pct: rect.y_pct,
                            w_pct: rect.w_pct,
                            h_pct: first_h,
                        },
                        out,
                    );
                    fill_rects(
                        second,
                        TransitionRect {
                            x_pct: rect.x_pct,
                            y_pct: rect.y_pct + first_h,
                            w_pct: rect.w_pct,
                            h_pct: second_h,
                        },
                        out,
                    );
                }
            }
        }
    }
}
