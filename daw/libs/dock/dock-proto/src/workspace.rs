//! Multi-window dock workspace model.

use std::collections::HashMap;
use std::collections::HashSet;

use facet::Facet;

use crate::drop_zone::DropZone;
use crate::id::{NodeId, WindowId};
use crate::layout::DockLayout;
use crate::panel::PanelId;
use crate::registry::{DockPosition, PanelRegistry};
use crate::ActionContext;

/// Bounds for an OS window.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct WindowBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub monitor_name: String,
}

impl WindowBounds {
    pub fn new(x: f64, y: f64, width: f64, height: f64, monitor_name: impl Into<String>) -> Self {
        Self {
            x,
            y,
            width,
            height,
            monitor_name: monitor_name.into(),
        }
    }
}

/// A single OS window in the workspace.
#[derive(Debug, Clone, Facet)]
pub struct DockWindow {
    pub id: WindowId,
    pub layout: DockLayout,
    pub title: String,
    pub bounds: WindowBounds,
    pub is_main: bool,
}

impl DockWindow {
    pub fn new(
        title: impl Into<String>,
        layout: DockLayout,
        bounds: WindowBounds,
        is_main: bool,
    ) -> Self {
        Self {
            id: WindowId::new(),
            layout,
            title: title.into(),
            bounds,
            is_main,
        }
    }
}

/// Serializable workspace state (does not include runtime panel registry).
#[derive(Debug, Clone, Facet)]
pub struct DockWorkspaceState {
    pub windows: HashMap<WindowId, DockWindow>,
    pub main_window: WindowId,
}

/// Dock workspace containing all windows + runtime panel registry.
#[derive(Debug, Clone)]
pub struct DockWorkspace {
    pub windows: HashMap<WindowId, DockWindow>,
    pub main_window: WindowId,
    pub panel_registry: PanelRegistry,
    user_closed_contextual: HashSet<String>,
}

impl DockWorkspace {
    /// Create a workspace with a single main window.
    pub fn with_main_window(
        layout: DockLayout,
        title: impl Into<String>,
        bounds: WindowBounds,
        panel_registry: PanelRegistry,
    ) -> Self {
        let main = DockWindow::new(title, layout, bounds, true);
        let main_window = main.id;
        let mut windows = HashMap::new();
        windows.insert(main_window, main);
        Self {
            windows,
            main_window,
            panel_registry,
            user_closed_contextual: HashSet::new(),
        }
    }

    /// Create a serializable state snapshot.
    pub fn to_state(&self) -> DockWorkspaceState {
        DockWorkspaceState {
            windows: self.windows.clone(),
            main_window: self.main_window,
        }
    }

    /// Restore a workspace from state + runtime registry.
    pub fn from_state(state: DockWorkspaceState, panel_registry: PanelRegistry) -> Self {
        Self {
            windows: state.windows,
            main_window: state.main_window,
            panel_registry,
            user_closed_contextual: HashSet::new(),
        }
    }

    /// Mark a context-managed panel as explicitly user-closed.
    pub fn mark_user_closed(&mut self, panel_id: &str) {
        self.user_closed_contextual.insert(panel_id.to_string());
    }

    /// Apply context-driven panel visibility rules.
    pub fn apply_context_visibility(&mut self, ctx: &ActionContext) {
        let should_visible: HashSet<String> = self
            .panel_registry
            .visible_panels(ctx)
            .into_iter()
            .collect();

        // Once context no longer requires a panel, clear the user-close override.
        self.user_closed_contextual
            .retain(|panel_id| should_visible.contains(panel_id));

        let managed: HashSet<String> = self
            .panel_registry
            .context_managed_panels()
            .into_iter()
            .collect();

        for panel_id in &managed {
            let should_show = should_visible.contains(panel_id);
            let is_present = panel_from_str(panel_id)
                .and_then(|panel| self.find_panel(panel))
                .is_some();

            if should_show && !is_present {
                if self.user_closed_contextual.contains(panel_id) {
                    continue;
                }
                self.auto_show_panel(panel_id);
            } else if !should_show && is_present {
                self.auto_hide_panel(panel_id);
            }
        }
    }

    /// Find a panel across all windows. Returns `(window_id, node_id)` if found.
    pub fn find_panel(&self, panel: PanelId) -> Option<(WindowId, NodeId)> {
        self.windows.iter().find_map(|(window_id, window)| {
            window
                .layout
                .find_node_for_panel(panel)
                .map(|node| (*window_id, node))
        })
    }

    /// Tear a panel into a new floating window.
    pub fn tear_off(
        &mut self,
        panel: PanelId,
        source_window: WindowId,
        bounds: WindowBounds,
    ) -> WindowId {
        let can_move = self
            .windows
            .get(&source_window)
            .is_some_and(|w| w.layout.contains_panel(panel));
        if !can_move {
            return source_window;
        }

        if let Some(source) = self.windows.get_mut(&source_window) {
            source.layout.remove_panel(panel);
        }

        let title = format!("{} (Floating)", panel.display_name());
        let new_window = DockWindow::new(title, DockLayout::single(panel), bounds, false);
        let new_id = new_window.id;
        self.windows.insert(new_id, new_window);
        new_id
    }

    /// Dock a panel from source window back into target window at target node + zone.
    pub fn dock_back(
        &mut self,
        panel: PanelId,
        source_window: WindowId,
        target_window: WindowId,
        target_node: NodeId,
        zone: DropZone,
    ) -> bool {
        if source_window == target_window {
            return self
                .windows
                .get_mut(&source_window)
                .is_some_and(|w| w.layout.move_panel(panel, target_node, zone));
        }

        let source_ok = self
            .windows
            .get_mut(&source_window)
            .is_some_and(|w| w.layout.remove_panel(panel));
        if !source_ok {
            return false;
        }

        let target_ok = self
            .windows
            .get_mut(&target_window)
            .is_some_and(|w| w.layout.add_panel_at(panel, target_node, zone));
        if !target_ok {
            if let Some(source) = self.windows.get_mut(&source_window) {
                if let Some(root) = source.layout.root() {
                    let _ = source.layout.add_panel_at(panel, root, DropZone::Center);
                } else {
                    source.layout = DockLayout::single(panel);
                }
            }
            return false;
        }

        let should_close = source_window != self.main_window
            && self
                .windows
                .get(&source_window)
                .is_some_and(|w| w.layout.is_empty());
        if should_close {
            self.windows.remove(&source_window);
        }

        true
    }

    /// Close a window and move all its panels back to the main window.
    pub fn close_window(&mut self, id: WindowId) -> bool {
        if id == self.main_window {
            return false;
        }

        let Some(closing) = self.windows.remove(&id) else {
            return false;
        };

        let panels = closing.layout.all_panels();
        for panel in panels {
            let Some(main_window) = self.windows.get_mut(&self.main_window) else {
                break;
            };
            if main_window.layout.contains_panel(panel) {
                continue;
            }
            if let Some(root) = main_window.layout.root() {
                let _ = main_window
                    .layout
                    .add_panel_at(panel, root, DropZone::Center);
            } else {
                main_window.layout = DockLayout::single(panel);
            }
        }

        true
    }

    fn auto_show_panel(&mut self, panel_id: &str) {
        let Some(panel) = panel_from_str(panel_id) else {
            return;
        };
        let Some(desc) = self.panel_registry.get(panel_id) else {
            return;
        };
        let default_position = desc.default_position;

        if self
            .windows
            .get(&self.main_window)
            .is_some_and(|w| w.layout.contains_panel(panel))
        {
            return;
        }

        if default_position == DockPosition::Float {
            let Some(source_bounds) = self
                .windows
                .get(&self.main_window)
                .map(|w| w.bounds.clone())
            else {
                return;
            };
            let bounds = WindowBounds::new(
                source_bounds.x + 40.0,
                source_bounds.y + 40.0,
                480.0,
                360.0,
                source_bounds.monitor_name,
            );
            let _ = self.tear_off(panel, self.main_window, bounds);
            return;
        }

        let Some(main_window) = self.windows.get_mut(&self.main_window) else {
            return;
        };

        if let Some(root) = main_window.layout.root() {
            let zone = match default_position {
                DockPosition::Left => DropZone::Left,
                DockPosition::Right => DropZone::Right,
                DockPosition::Bottom => DropZone::Bottom,
                DockPosition::Center | DockPosition::Float => DropZone::Center,
            };
            let _ = main_window.layout.add_panel_at(panel, root, zone);
        } else {
            main_window.layout = DockLayout::single(panel);
        }
    }

    fn auto_hide_panel(&mut self, panel_id: &str) {
        let Some(panel) = panel_from_str(panel_id) else {
            return;
        };

        if let Some((window_id, node_id)) = self.find_panel(panel) {
            if let Some(window) = self.windows.get_mut(&window_id) {
                if window.layout.zone_mode(node_id) == crate::DockZoneMode::AutoHide {
                    window.layout.collapse_zone(node_id);
                } else {
                    let _ = window.layout.remove_panel(panel);
                }
            }
            if window_id != self.main_window
                && self
                    .windows
                    .get(&window_id)
                    .is_some_and(|w| w.layout.is_empty())
            {
                self.windows.remove(&window_id);
            }
        }
    }
}

fn panel_from_str(id: &str) -> Option<PanelId> {
    PanelId::from_str_id(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PanelDescriptor;

    fn bounds() -> WindowBounds {
        WindowBounds::new(0.0, 0.0, 1200.0, 800.0, "main")
    }

    fn make_workspace() -> DockWorkspace {
        let mut registry = PanelRegistry::new();
        PanelId::register_all(&mut registry);
        DockWorkspace::with_main_window(
            DockLayout::from_tree(crate::tree::DockNode::horizontal(
                crate::tree::DockNode::tile(PanelId::Navigator),
                crate::tree::DockNode::tile(PanelId::Performance),
                40.0,
            )),
            "Main",
            bounds(),
            registry,
        )
    }

    fn make_contextual_workspace() -> DockWorkspace {
        let mut registry = PanelRegistry::new();
        registry.register(
            PanelDescriptor::new(PanelId::Transport.as_str(), "Transport")
                .with_default_position(DockPosition::Bottom)
                .with_visibility_when("tab:performance"),
        );
        DockWorkspace::with_main_window(
            DockLayout::single(PanelId::Performance),
            "Main",
            bounds(),
            registry,
        )
    }

    #[test]
    fn tear_off_creates_window_and_removes_from_source() {
        let mut ws = make_workspace();
        let source = ws.main_window;
        let new_id = ws.tear_off(
            PanelId::Navigator,
            source,
            WindowBounds::new(100.0, 100.0, 600.0, 500.0, "secondary"),
        );

        assert_ne!(new_id, source);
        assert!(ws.windows.contains_key(&new_id));
        assert!(!ws
            .windows
            .get(&source)
            .unwrap()
            .layout
            .contains_panel(PanelId::Navigator));
        assert!(ws
            .windows
            .get(&new_id)
            .unwrap()
            .layout
            .contains_panel(PanelId::Navigator));
    }

    #[test]
    fn dock_back_moves_panel_and_closes_empty_window() {
        let mut ws = make_workspace();
        let source = ws.main_window;
        let floating = ws.tear_off(
            PanelId::Navigator,
            source,
            WindowBounds::new(100.0, 100.0, 600.0, 500.0, "secondary"),
        );
        let target_node = ws
            .windows
            .get(&source)
            .unwrap()
            .layout
            .find_node_for_panel(PanelId::Performance)
            .unwrap();

        assert!(ws.dock_back(
            PanelId::Navigator,
            floating,
            source,
            target_node,
            DropZone::Center
        ));
        assert!(!ws.windows.contains_key(&floating));
        assert!(ws
            .windows
            .get(&source)
            .unwrap()
            .layout
            .contains_panel(PanelId::Navigator));
    }

    #[test]
    fn find_panel_searches_across_windows() {
        let mut ws = make_workspace();
        let source = ws.main_window;
        let floating = ws.tear_off(
            PanelId::Navigator,
            source,
            WindowBounds::new(100.0, 100.0, 600.0, 500.0, "secondary"),
        );

        let found = ws.find_panel(PanelId::Navigator).unwrap();
        assert_eq!(found.0, floating);
    }

    #[test]
    fn close_window_returns_panels_to_main() {
        let mut ws = make_workspace();
        let source = ws.main_window;
        let floating = ws.tear_off(
            PanelId::Navigator,
            source,
            WindowBounds::new(100.0, 100.0, 600.0, 500.0, "secondary"),
        );

        assert!(ws.close_window(floating));
        assert!(!ws.windows.contains_key(&floating));
        assert!(ws
            .windows
            .get(&source)
            .unwrap()
            .layout
            .contains_panel(PanelId::Navigator));
    }

    #[test]
    fn context_change_shows_panel() {
        let mut ws = make_contextual_workspace();
        assert!(!ws
            .windows
            .get(&ws.main_window)
            .unwrap()
            .layout
            .contains_panel(PanelId::Transport));

        let mut ctx = ActionContext::new();
        ctx.set_tab("performance");
        ws.apply_context_visibility(&ctx);

        assert!(ws
            .windows
            .get(&ws.main_window)
            .unwrap()
            .layout
            .contains_panel(PanelId::Transport));
    }

    #[test]
    fn context_change_hides_panel() {
        let mut ws = make_contextual_workspace();
        let mut ctx = ActionContext::new();
        ctx.set_tab("performance");
        ws.apply_context_visibility(&ctx);
        assert!(ws
            .windows
            .get(&ws.main_window)
            .unwrap()
            .layout
            .contains_panel(PanelId::Transport));

        ctx.set_tab("chart");
        ws.apply_context_visibility(&ctx);
        assert!(!ws
            .windows
            .get(&ws.main_window)
            .unwrap()
            .layout
            .contains_panel(PanelId::Transport));
    }

    #[test]
    fn user_closed_panel_stays_closed_until_context_changes() {
        let mut ws = make_contextual_workspace();
        let mut ctx = ActionContext::new();
        ctx.set_tab("performance");
        ws.apply_context_visibility(&ctx);
        assert!(ws
            .windows
            .get(&ws.main_window)
            .unwrap()
            .layout
            .contains_panel(PanelId::Transport));

        ws.windows
            .get_mut(&ws.main_window)
            .unwrap()
            .layout
            .remove_panel(PanelId::Transport);
        ws.mark_user_closed(PanelId::Transport.as_str());

        ws.apply_context_visibility(&ctx);
        assert!(!ws
            .windows
            .get(&ws.main_window)
            .unwrap()
            .layout
            .contains_panel(PanelId::Transport));

        ctx.set_tab("chart");
        ws.apply_context_visibility(&ctx);
        ctx.set_tab("performance");
        ws.apply_context_visibility(&ctx);
        assert!(ws
            .windows
            .get(&ws.main_window)
            .unwrap()
            .layout
            .contains_panel(PanelId::Transport));
    }
}
