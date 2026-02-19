//! Typestate types for the grid view — making invalid states unrepresentable.

use signal::block::BlockColor;

// ─────────────────────────────────────────────────────────────────────────────
// Grid slot — the signal equivalent of CompositionSlot
// ─────────────────────────────────────────────────────────────────────────────

/// A single cell in the grid. Replaces legacy `CompositionSlot`.
#[derive(Debug, Clone, PartialEq)]
pub struct GridSlot {
    pub id: uuid::Uuid,
    pub block_type: signal::BlockType,
    pub block_preset_name: Option<String>,
    pub plugin_name: Option<String>,
    /// Grid column position (0-indexed).
    pub col: usize,
    /// Grid row position (0-indexed).
    pub row: usize,
    /// Module group key — slots with the same key are grouped visually.
    pub module_group: Option<String>,
    /// Module type for coloring the group container.
    pub module_type: Option<signal::ModuleType>,
    /// Layer group key — modules within the same layer share this key.
    pub layer_group: Option<String>,
    /// Engine group key — layers within the same engine share this key.
    pub engine_group: Option<String>,
    /// True when the block has no plugin loaded yet (template placeholder).
    pub is_template: bool,
    /// True when the block is bypassed (signal passes through unprocessed).
    pub bypassed: bool,
    /// Phantom slot — participates in layout (group bounds, grid sizing)
    /// but does not render a visible cell. Used for dry pass-through lanes.
    pub is_phantom: bool,
    /// Resolved block parameters (name, value 0..1) for the inspector panel.
    pub parameters: Vec<(String, f32)>,
    /// Preset ID this block was loaded from (for save-back). `None` for inline/template blocks.
    pub preset_id: Option<String>,
    /// Snapshot ID this block was loaded from. `None` for default snapshots or inline blocks.
    pub snapshot_id: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Block visual state
// ─────────────────────────────────────────────────────────────────────────────

/// Visual state of a block cell. Exactly one state at a time.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BlockVisualState {
    Normal,
    Selected,
    Template,
    Bypassed,
    Dragging,
    DropTarget,
}

impl BlockVisualState {
    pub fn resolve(
        is_being_dragged: bool,
        is_drop_target: bool,
        is_bypassed: bool,
        is_template: bool,
        is_selected: bool,
    ) -> Self {
        if is_being_dragged {
            Self::Dragging
        } else if is_drop_target {
            Self::DropTarget
        } else if is_bypassed {
            Self::Bypassed
        } else if is_template {
            Self::Template
        } else if is_selected {
            Self::Selected
        } else {
            Self::Normal
        }
    }

    /// Inline style string for the block cell.
    pub fn cell_style(&self, color: &BlockColor) -> String {
        match self {
            Self::Dragging => format!(
                "background-color: {}10; border-color: {}20; color: {}40; opacity: 0.4; border-style: dashed;",
                color.bg, color.bg, color.fg,
            ),
            Self::Bypassed => format!(
                "background-color: {}08; border-color: {}15; color: {}30; opacity: 0.25; border-style: solid;",
                color.bg, color.bg, color.fg,
            ),
            Self::Template => format!(
                "background-color: {}08; border-color: {}25; color: {}60; opacity: 1; border-style: dashed;",
                color.bg, color.bg, color.fg,
            ),
            Self::Selected => format!(
                "background-color: {}25; border-color: {}; color: {}; opacity: 1; border-style: solid;",
                color.bg, color.bg, color.fg,
            ),
            Self::DropTarget => format!(
                "background-color: {}20; border-color: {}60; color: {}; opacity: 1; border-style: solid;",
                color.bg, color.bg, color.fg,
            ),
            Self::Normal => format!(
                "background-color: {}15; border-color: {}40; color: {}; opacity: 1; border-style: solid;",
                color.bg, color.bg, color.fg,
            ),
        }
    }

    pub fn port_opacity(&self) -> &'static str {
        match self {
            Self::Bypassed => "0.25",
            _ => "1",
        }
    }

    pub fn cell_class(&self) -> &'static str {
        match self {
            Self::Dragging => {
                "absolute inset-0 flex flex-col items-center justify-center gap-1 \
                 rounded-lg border-2 border-dashed"
            }
            _ => {
                "absolute inset-0 flex flex-col items-center justify-center gap-1 \
                 rounded-lg border-2 cursor-grab \
                 hover:brightness-110 active:cursor-grabbing"
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Module visual state
// ─────────────────────────────────────────────────────────────────────────────

/// Visual state of a module container background.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ModuleVisualState {
    Normal,
    Selected,
    Template,
    Bypassed,
    Dragging,
}

impl ModuleVisualState {
    pub fn from_slots(slots: &[&GridSlot], is_selected: bool, is_dragging: bool) -> Self {
        if is_dragging {
            return Self::Dragging;
        }
        let all_bypassed = !slots.is_empty() && slots.iter().all(|s| s.bypassed);
        let all_template = !slots.is_empty() && slots.iter().all(|s| s.is_template);
        if all_bypassed {
            Self::Bypassed
        } else if all_template {
            Self::Template
        } else if is_selected {
            Self::Selected
        } else {
            Self::Normal
        }
    }

    pub fn opacity(&self) -> &'static str {
        match self {
            Self::Dragging => "0.85",
            Self::Bypassed => "0.25",
            _ => "1",
        }
    }

    pub fn extra_style(&self) -> &'static str {
        match self {
            Self::Dragging => "z-index: 50; border-style: dashed;",
            Self::Template => "border-style: dashed;",
            _ => "border-style: solid;",
        }
    }

    pub fn transition(&self) -> &'static str {
        match self {
            Self::Dragging => "none",
            _ => "transform 0.15s ease",
        }
    }

    pub fn selection_glow(&self, bg_color: &str) -> String {
        match self {
            Self::Selected => {
                format!(
                    "box-shadow: 0 0 0 2px {}90, 0 0 12px {}30;",
                    bg_color, bg_color
                )
            }
            _ => "box-shadow: none;".to_string(),
        }
    }
}
