//! Dock layout global signals.
//!
//! Global Dioxus signals for dock state. Components subscribe to these
//! for reactive layout updates.

use crate::context::PanelRenderer;
use crate::prelude::*;
use crate::transition::LayoutTransition;
use dock_proto::{
    DockLayout, DockWorkspace, DropZone, NodeId, PanelId, PanelRegistry, PresetCollection,
    WindowBounds, WindowId,
};
use std::collections::HashSet;

fn default_workspace() -> DockWorkspace {
    let mut registry = PanelRegistry::new();
    PanelId::register_all(&mut registry);
    DockWorkspace::with_main_window(
        DockLayout::single(PanelId::Performance),
        "Main",
        WindowBounds::new(120.0, 120.0, 1400.0, 900.0, "main"),
        registry,
    )
}

/// The current active dock layout.
#[deprecated(note = "Use DOCK_WORKSPACE (main window layout mirror only).")]
pub static DOCK_LAYOUT: GlobalSignal<DockLayout> =
    Signal::global(|| DockLayout::single(PanelId::Performance));

/// Full multi-window workspace state.
pub static DOCK_WORKSPACE: GlobalSignal<DockWorkspace> = Signal::global(default_workspace);

/// Window currently being rendered by the active `DockRoot`.
pub static DOCK_ACTIVE_WINDOW: GlobalSignal<Option<WindowId>> = Signal::global(|| None);

/// Floating workspace windows already opened at the OS level.
pub static DOCK_OPENED_FLOATING_WINDOWS: GlobalSignal<HashSet<WindowId>> =
    Signal::global(HashSet::new);

/// Shared panel renderer used by additional desktop windows.
pub static DOCK_PANEL_RENDERER: GlobalSignal<Option<PanelRenderer>> = Signal::global(|| None);

/// All available dock presets (screensets).
pub static DOCK_PRESETS: GlobalSignal<PresetCollection> =
    Signal::global(|| PresetCollection::new(Vec::new()));

/// Index of the currently active preset.
pub static DOCK_ACTIVE_PRESET_INDEX: GlobalSignal<usize> = Signal::global(|| 0);

/// Node ID of the split currently being resized (drag in progress).
pub static DOCK_RESIZING: GlobalSignal<Option<NodeId>> = Signal::global(|| None);

/// Panel that is currently maximized (takes full area), if any.
pub static DOCK_MAXIMIZED_PANEL: GlobalSignal<Option<PanelId>> = Signal::global(|| None);

/// State for drag-and-drop panel rearrangement.
pub static DOCK_DRAG_STATE: GlobalSignal<DragState> = Signal::global(DragState::default);

/// Active animated transition between two layouts, if any.
pub static DOCK_LAYOUT_TRANSITION: GlobalSignal<Option<LayoutTransition>> = Signal::global(|| None);

/// Default transition duration in milliseconds.
pub static DOCK_TRANSITION_DURATION_MS: GlobalSignal<u64> = Signal::global(|| 200);

/// Panel context menu UI state.
pub static DOCK_CONTEXT_MENU: GlobalSignal<Option<ContextMenuState>> = Signal::global(|| None);
/// True when current drag finished with a successful drop.
pub static DOCK_DRAG_DROPPED: GlobalSignal<bool> = Signal::global(|| false);

// ─── Rig Dock Signals ───────────────────────────────────────────────────

/// WindowId for the dedicated rig dock window within DOCK_WORKSPACE.
pub static RIG_DOCK_WINDOW_ID: GlobalSignal<Option<WindowId>> = Signal::global(|| None);

/// Rig-specific dock presets (rig screensets).
pub static RIG_DOCK_PRESETS: GlobalSignal<PresetCollection> =
    Signal::global(|| PresetCollection::new(Vec::new()));

/// Index of the active rig preset.
pub static RIG_DOCK_ACTIVE_PRESET_INDEX: GlobalSignal<usize> = Signal::global(|| 0);

/// Tracks an in-progress panel drag operation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DragState {
    /// The panel currently being dragged, if any.
    pub dragging_panel: Option<PanelId>,
    /// The source node ID the panel is being dragged from.
    pub source_node: Option<NodeId>,
    /// The node currently being hovered over for a drop.
    pub hover_target: Option<NodeId>,
    /// Which drop zone the cursor is in on the hover target.
    pub hover_zone: Option<DropZone>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextMenuState {
    pub node_id: NodeId,
    pub panel_id: PanelId,
    pub x: f64,
    pub y: f64,
}

impl DragState {
    /// Start dragging a panel from a specific node.
    pub fn start(&mut self, panel: PanelId, source: NodeId) {
        self.dragging_panel = Some(panel);
        self.source_node = Some(source);
        self.hover_target = None;
        self.hover_zone = None;
    }

    /// Update hover target and zone during drag.
    pub fn update_hover(&mut self, target: NodeId, zone: DropZone) {
        self.hover_target = Some(target);
        self.hover_zone = Some(zone);
    }

    /// Clear hover (cursor left a tile area).
    pub fn clear_hover(&mut self) {
        self.hover_target = None;
        self.hover_zone = None;
    }

    /// End the drag operation.
    pub fn end(&mut self) {
        *self = Self::default();
    }

    /// Whether a drag is currently in progress.
    pub fn is_dragging(&self) -> bool {
        self.dragging_panel.is_some()
    }
}
