//! Dumb presentation components for signal UI.
//!
//! These components are domain-agnostic: they take all data via props and
//! have zero knowledge of signal types, services, or global signals.
//! Domain-aware wrappers compose these into full editor views.

// Primitives — shared UI building blocks (shadcn-style)
pub mod audio_controls;
pub mod audio_viz;
mod context_menu;
mod dialog;
mod manage_buttons;
mod side_sheet;
mod slider;
mod tabs;

// Tier 1 — direct ports (zero domain deps)
mod entity_editor;
mod review_list;
mod star_rating;

// Tier 2 — ported with domain type erasure
mod block_colors;
mod create_modal;
mod crossfade_indicator;
pub mod dynamic_grid;
mod grid_model;
mod morph_slider;
pub mod node_graph;
mod pan_zoom_canvas;
mod scene_tile;
mod signal_chain_grid;
mod signal_flow_grid_view;

// Re-exports: primitives
pub use audio_controls::{Knob, KnobSize, XYPad};
pub use audio_viz::{LevelMeter, LevelMeterOrientation, SpectrumAnalyzer, WaveformDisplay};
pub use context_menu::{ContextMenu, ContextMenuItem, ContextMenuSeparator};
pub use dialog::{Dialog, DialogClose, DialogDescription, DialogFooter, DialogHeader, DialogTitle};
pub use manage_buttons::{
    CaptureButton, CreateProfileButton, CreateRigButton, CreateSetlistButton, CreateSongButton,
};
pub use side_sheet::{SheetSide, SideSheet};
pub use slider::{Slider, SliderOrientation};
pub use tabs::{TabContent, TabList, TabTrigger, Tabs};

// Re-exports: layout
pub use entity_editor::EntityEditor;

// Re-exports: ratings & reviews
pub use review_list::{ReviewCard, ReviewData, ReviewList};
pub use star_rating::{PresetRatingBadge, StarRating, StarRatingInput};

// Re-exports: block colors
pub use block_colors::{
    block_bypassed_style, block_color, block_instance_color, block_style, BlockColor,
};

// Re-exports: scene tiles
pub use scene_tile::{SceneTileCell, SceneTileGrid, TileData};

// Re-exports: morph slider
pub use morph_slider::{DropdownItem, MorphSlider};

// Re-exports: crossfade
pub use crossfade_indicator::CrossfadeIndicator;

// Re-exports: create modal
pub use create_modal::{CreateModal, CreateModalData, ModalConfig, TemplateOption};

// Re-exports: pan/zoom canvas
pub use pan_zoom_canvas::PanZoomCanvas;

// Re-exports: signal chain grid
pub use signal_chain_grid::{FlowBlock, SignalChainGrid};

// Re-exports: signal flow grid view (interactive)
pub use grid_model::{
    BlockWidget, GridBlock, GridConnection, GridJack, GridPosition, GridSize, SignalFlowGrid,
};
pub use signal_flow_grid_view::{
    EngineGridData, LayerGridData, ModuleBrowserModal, ModuleCategory, ModuleChainGridData,
    SignalFlowGridView,
};

// Re-exports: node graph
pub use node_graph::{
    EngineData, GraphModule, LayerData, ModuleChainInput, ModuleContainer, Node, NodeBlock,
    NodeGraph, NodeGraphView, NodePosition, NodeSize, Wire,
};

// Re-exports: dynamic grid view
pub use dynamic_grid::{
    BlockPickerDropdown, DynamicGridView, GridConnection as DynGridConnection, GridSelection,
    GridSlot, PICKER_CELL, PICKER_CLICK_POS,
};
