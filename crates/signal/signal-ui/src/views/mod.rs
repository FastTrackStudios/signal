//! Domain-aware smart views for the signal UI.
//!
//! These components use [`signal::Signal`] and signal domain types
//! to fetch data, manage state, and compose the dumb [`crate::components`]
//! building blocks into full editor/browser views.

mod block_detail;
mod block_editor;
mod collection_browser;
mod metadata_display;
mod module_view;
mod rig_preset_canvas;
mod scene_grid;
mod signal_chain_layout;
mod signal_slider;

// New views
mod ab_comparison;
mod automation_lane;
mod editor_inspector;
mod fx_binding_status;
mod midi_learn;
mod param_inspector;
mod performance_view;
mod profile_editor;
mod profile_patch_grid;
mod snapshot_panel;
mod song_section_grid;
mod song_setlist_editor;

pub use block_detail::{
    BlockCustomGui, BlockDetailPanel, BlockDetailTab, BlockMacros, BlockModulation, BlockRawParams,
};
pub use block_editor::{BlockCard, BlockEditor, MiniKnob};
pub use collection_browser::{
    engines_to_grid_slots, resolve_layer_engines, resolve_scene_engines, rig_type_to_engine_type,
    BrowseLevel, BrowserAssignment, CollectionBrowser, EngineFlowData, EngineParamLookup,
    LayerFlowData, RigGridPanel,
};
pub use metadata_display::MetadataDisplay;
pub use module_view::{ModuleView, ModuleViewMode, ParamChange};
pub use rig_preset_canvas::RigPresetCanvas;
pub use scene_grid::RigSceneGrid;
pub use signal_slider::SignalSlider;

// New view re-exports
pub use ab_comparison::{ABComparison, ComparisonRow, DiffDirection, DiffFilter, PresetHeader};
pub use automation_lane::{
    AutomationLane, AutomationLaneData, AutomationLaneList, AutomationPoint,
};
pub use editor_inspector::EditorInspectorPanel;
pub use fx_binding_status::{BindingHealth, FxBindingIndicator, FxBindingPanel, FxBindingRow};
pub use midi_learn::{LearnState, MidiLearnPanel, MidiMapping};
pub use param_inspector::{ParamInspector, ParamRow, ParamSource, SortColumn, SortDirection};
pub use performance_view::{
    MorphSlider, PerfSceneGrid, PerfSceneTile, PerformanceView, RigStatus, RigStatusBanner,
    SnapshotBank, SnapshotSlot, SongNav, SongNavState,
};
pub use profile_editor::{OverrideEntry, PatchEditor, PatchEntry, ProfileList, ProfileListEntry};
pub use profile_patch_grid::ProfilePatchGrid;
pub use snapshot_panel::{CaptureType, SnapshotEntry, SnapshotPanel};
pub use song_section_grid::SongSectionGrid;
pub use song_setlist_editor::{SectionEntry, SetlistEditor, SetlistEntry, SongEditor, SongEntry};
