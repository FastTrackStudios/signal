//! Interaction state types for the grid view — drag, pan, wire draft, selection.

use dioxus::prelude::*;
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Drag state
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub(super) struct GridDragState {
    pub(super) slot_id: Uuid,
    pub(super) origin_col: usize,
    pub(super) origin_row: usize,
    pub(super) start_mouse_x: f64,
    pub(super) start_mouse_y: f64,
    pub(super) mouse_x: f64,
    pub(super) mouse_y: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct GridWireDraft {
    pub(super) from_slot_id: Uuid,
    pub(super) from_pos: (f64, f64),
    pub(super) is_from_output: bool,
    pub(super) mouse_pos: (f64, f64),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct GroupDragState {
    pub(super) group_name: String,
    pub(super) start_mouse_x: f64,
    pub(super) start_mouse_y: f64,
    pub(super) mouse_x: f64,
    pub(super) mouse_y: f64,
    pub(super) shift_held: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum GroupDropTarget {
    SwapWith(String),
    MoveDelta(isize, isize),
}

// ─────────────────────────────────────────────────────────────────────────────
// Interaction mode
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InteractionMode {
    Idle,
    Pan {
        start_mouse_x: f64,
        start_mouse_y: f64,
        start_pan_x: f64,
        start_pan_y: f64,
    },
    BlockDrag(GridDragState),
    GroupDrag(GroupDragState),
    WireDraft(GridWireDraft),
}

impl InteractionMode {
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    pub fn is_any_drag(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub fn dragged_slot_id(&self) -> Option<Uuid> {
        match self {
            Self::BlockDrag(d) => Some(d.slot_id),
            _ => None,
        }
    }

    pub fn group_drag(&self) -> Option<&GroupDragState> {
        match self {
            Self::GroupDrag(gd) => Some(gd),
            _ => None,
        }
    }

    pub fn wire_draft(&self) -> Option<&GridWireDraft> {
        match self {
            Self::WireDraft(wd) => Some(wd),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Selection and connections
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct GridConnection {
    pub from_slot_id: Uuid,
    pub to_slot_id: Uuid,
}

pub static GRID_CONNECTIONS: GlobalSignal<Vec<GridConnection>> = Signal::global(Vec::new);

#[derive(Debug, Clone, PartialEq)]
pub enum GridSelection {
    Block(Uuid),
    Module(String),
}

/// Payload emitted when the user right-clicks a block or module in the grid.
#[derive(Debug, Clone, PartialEq)]
pub struct GridContextMenuEvent {
    pub target: GridSelection,
    pub client_x: f64,
    pub client_y: f64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Block picker portal state
// ─────────────────────────────────────────────────────────────────────────────

pub static PICKER_CELL: GlobalSignal<Option<(usize, usize)>> = Signal::global(|| None);
pub static PICKER_CLICK_POS: GlobalSignal<(f64, f64)> = Signal::global(|| (0.0, 0.0));
