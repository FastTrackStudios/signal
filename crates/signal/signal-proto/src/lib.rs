//! Signal protocol types -- the domain model for rig control.
//!
//! This is the foundational crate of the signal architecture. It defines every
//! domain type, ID, service trait, and structural concept used by the rest of
//! the signal stack. It has **no internal signal dependencies** (only `macromod`
//! for macro/parameter types), making it the leaf of the dependency graph.
//!
//! # Hierarchy
//!
//! **Physical**: Block -> Module -> Layer -> Engine -> Rig
//!
//! **Performance**: Profile (Patches) -> Song (Sections)
//!
//! **Templates**: Structural blueprints with [`Assignment::Unassigned`](template::Assignment)
//! placeholders at every level.
//!
//! # Sub-module organization
//!
//! - [`ids`] -- ID macros (`typed_uuid_id!`, `typed_string_id!`), seed helpers, core ID types
//! - [`model`] -- Core data structures: Block, Snapshot, Preset, Module, ModulePreset
//! - [`traits`] -- Collection/Variant architecture, HasMetadata, Tagged, Described
//! - [`services`] -- Async service trait definitions (BlockService, LayerService, etc.)
//!
//! All types are re-exported at the crate root for convenience.
//!
//! # Key types and traits
//!
//! - **Entity types**: [`Block`], [`Preset`], [`Snapshot`], [`Module`], [`ModulePreset`],
//!   [`ModuleSnapshot`]
//! - **Hierarchy types**: `Layer`, `Engine`, `Rig`, `Profile`, `Song`, `Setlist`
//! - **Service traits**: [`BlockService`], [`LayerService`], [`EngineService`],
//!   [`RigService`], [`ProfileService`], [`SongService`], [`SetlistService`],
//!   [`BrowserService`], [`ResolveService`], [`SceneTemplateService`], [`RackService`]
//! - **Resolution**: `ResolvedGraph`, `ResolvedBlock`, `ResolvedLayer`, `ResolvedEngine`
//! - **ID macros**: `typed_uuid_id!`, `typed_string_id!`
//!
//! # Dependents
//!
//! Every other signal crate depends on `signal-proto`. Direct dependents include
//! `signal-storage`, `signal-live`, `signal-controller`, `signal-import`,
//! `signal-daw-bridge`, `nam-manager`, and the `signal` facade.

// ─── ID macros (re-exported from utils shared crate) ────────────
//
// The macros are defined in `utils` so both signal-proto and session-proto
// can use them without depending on each other. Re-exporting here preserves
// backward compatibility: `signal_proto::typed_uuid_id!` still works.

pub use utils::typed_string_id;
pub use utils::typed_uuid_id;

// ─── Organizational sub-modules ────────────────────────────────

pub mod ids;
pub mod model;
pub mod services;
pub mod collection_macro;

// ─── Domain modules ─────────────────────────────────────────────

pub mod actions;
pub mod automation;
pub mod block;
pub mod builder;
pub mod catalog;
pub mod defaults;
pub mod engine;
pub mod fx_send;
pub mod layer;
pub mod metadata;
pub mod midi;
pub mod midi_actions;
pub mod module_type;
pub mod override_policy;
pub mod overrides;
pub mod plugin_block;
pub mod profile;
pub mod rack;
pub mod resolve;
pub mod rig;
pub mod rig_template;
pub mod routing;
pub mod scene_template;
pub mod setlist;
pub mod signal_chain;
pub mod song;
pub mod tagging;
pub mod template;
pub mod traits;
pub mod versioning;

// ─── Re-exported from macromod ──────────────────────────────────

pub use macromod::easing;
pub use macromod::macro_bank;
pub use macromod::curation as param_curation;
pub use macromod::runtime;
pub use macromod::{BlockParameter, MacroBinding, ParameterValue, ParamTarget, ResponseCurve};

/// Backward-compatible `modulation` module path.
pub mod modulation {
    pub use macromod::sources::*;
    pub use macromod::routing::*;
}

// ─── Re-exports from ids ────────────────────────────────────────

pub use ids::{
    IdFactory, ModulePresetId, ModuleSnapshotId, PresetId, RuntimeIdFactory, SEED_UUID_NS,
    SnapshotId, seed_id,
};

// ─── Re-exports from model ──────────────────────────────────────

pub use model::{
    Block, BlockParameterOverride, EngineType, Module, ModuleBlock, ModuleBlockSource,
    ModulePreset, ModuleSnapshot, Preset, PresetLike, Snapshot, SnapshotLike,
};

// ─── Re-exports from domain modules ────────────────────────────

pub use block::*;
pub use module_type::*;
pub use signal_chain::*;

// ─── Re-exports from services ──────────────────────────────────

pub use services::{
    BlockService, BrowserService, EngineService, LayerService, ProfileService, RackService,
    ResolveService, RigService, SceneTemplateService, SetlistService, SignalServiceError,
    SongService,
};
