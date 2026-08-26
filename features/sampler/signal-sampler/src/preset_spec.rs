//! `.signalpreset` file format — top-level user-facing patch.
//!
//! A Preset composes N Engine instances + (optional) Modules + a routing
//! graph + master FX. It's the entry point for live performance.
//!
//! For v1 the routing graph and Modules are **parse-only** — runtime
//! support lands when the FX block runtime + audio routing graph arrive.
//! The data is captured so files round-trip cleanly.
//!
//! # File shape
//!
//! ```styx
//! // signal-preset v1
//! name "MM2 Modern Kit"
//! description "GM-mapped modern rock drum kit."
//!
//! engines (
//!     { id "kick"  engine "../Engines/MM2 Tama Bubinga Kick.signalengine" }
//!     { id "snare" engine "../Engines/MM2 DW Steel Snare.signalengine" }
//!     …
//! )
//!
//! note_routing (
//!     { note 36 targets ( "kick" ) }
//!     { note 38 targets ( "snare" ) }
//!     …
//! )
//!
//! modules ()         // bus / parallel-comp nodes — runtime deferred
//! routing ()         // explicit graph wiring — runtime deferred
//! master_fx ()       // master chain — runtime deferred
//! ```

use std::path::Path;

use facet::Facet;

use crate::block::ParamOverride;
use crate::SamplerError;

#[derive(Debug, Clone, Facet)]
pub struct PresetSpec {
    pub name: String,
    #[facet(default)]
    pub description: String,

    /// Engine instances that make up this preset. Ids must be unique.
    pub engines: Vec<PresetEngineRef>,

    /// MIDI note → engine-id dispatch. A note can target multiple engines
    /// (e.g. layered kicks); the runtime fires note_on on each.
    #[facet(default)]
    pub note_routing: Vec<NoteRoute>,

    /// Reusable processing-graph nodes (parallel comp bus, rooms sum, …).
    /// Runtime deferred — parsed for round-trip, ignored at audio time.
    #[facet(default)]
    pub modules: Vec<PresetModuleRef>,

    /// Explicit audio routing rules (`from "kick.layers.room" to
    /// "rooms.in"`). Runtime deferred.
    #[facet(default)]
    pub routing: Vec<RoutingRule>,

    /// Master-bus FX chain. Runtime deferred.
    #[facet(default)]
    pub master_fx: Vec<MasterFxSlot>,

    /// Macro definitions (cc → multi-target). Runtime deferred.
    #[facet(default)]
    pub macros: Vec<MacroDef>,
}

impl PresetSpec {
    pub fn from_file(path: &Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path)?;
        facet_styx::from_str(&text).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }
}

/// Reference to one Engine instance inside a Preset.
#[derive(Debug, Clone, Facet)]
pub struct PresetEngineRef {
    /// Stable identifier within the Preset (`"kick"`, `"snare-a"`, …).
    pub id: String,
    /// Path to the `.signalengine`, relative to the Preset file.
    pub engine: String,
    /// Optional transpose applied to all incoming notes for this engine.
    #[facet(default)]
    pub transpose: i8,
    /// Engine-level mute (overridable live).
    #[facet(default)]
    pub mute: bool,
    /// When set, this engine instance fires only zones whose `articulation`
    /// matches (case-insensitive), ignoring the incoming key. Lets one drum
    /// pack be addressed as several performance pieces — e.g. a hats pack
    /// pinned to "Closed Tip" on one note and "Open 1" on another. Only
    /// meaningful for percussion (drum-kit) engines, which always play at
    /// natural pitch, so the routed note is a trigger selector, not a pitch.
    #[facet(default)]
    pub articulation: String,
    /// Engine-wide choke group. When set, this engine's voices join the group
    /// so they can be silenced by a choking hit. Empty = polyphonic
    /// (kick/snare/toms ring freely).
    #[facet(default)]
    pub choke_group: String,
    /// Articulations that actually trigger the choke (silence the group).
    /// Empty (with a group set) = monophonic — every hit chokes (hi-hats, so
    /// open/closed/tight/pedal all cut each other). Non-empty = only these
    /// choke (cymbals: `( "Choke" )` so crashes ring/overlap but the choke
    /// articulation stops the ringing cymbal).
    #[facet(default)]
    pub choke_on: Vec<String>,
    /// Engine-level overrides applied after the engine loads — same
    /// name-keyed dispatch as block params.
    #[facet(default)]
    pub overrides: Vec<ParamOverride>,
}

/// One MIDI-note routing entry.
#[derive(Debug, Clone, Facet)]
pub struct NoteRoute {
    pub note: u8,
    /// Engine ids that should receive this note. Multiple targets =
    /// layered notes (e.g. layered kicks).
    pub targets: Vec<String>,
    /// Optional articulation fired on the target engine(s) for this note,
    /// ignoring key (percussion). Lets one shared drum pack be addressed as
    /// several performance notes — e.g. a hats engine routed as "Closed Tip"
    /// on one note and "Open 1" on another, without duplicating the engine.
    /// Empty = key-based selection (pitched samplers, native drum layouts).
    #[facet(default)]
    pub articulation: String,
}

/// Reference to a `.signalmodule` instance in the Preset's routing graph.
/// Runtime deferred.
#[derive(Debug, Clone, Facet)]
pub struct PresetModuleRef {
    pub id: String,
    pub module: String,
    #[facet(default)]
    pub overrides: Vec<ParamOverride>,
}

/// One routing edge in the Preset's audio graph. Source / target are
/// dotted paths into engines or modules. Runtime deferred.
#[derive(Debug, Clone, Facet)]
pub struct RoutingRule {
    /// `"engine_id.port_id"` or `"module_id.output_id"` or `"engine_id"`
    /// (uses the engine's default `main` port).
    pub from: String,
    /// `"module_id.input_id"` or `"master"`.
    pub to: String,
    #[facet(default)]
    pub gain_db: f32,
}

/// One master-bus FX slot. Same shape as Layer FX slots; runtime deferred.
#[derive(Debug, Clone, Facet)]
pub struct MasterFxSlot {
    pub fx_type: String,
    #[facet(default)]
    pub overrides: Vec<ParamOverride>,
    #[facet(default)]
    pub bypass: bool,
}

/// Macro definition — a named control with N override targets. Runtime
/// deferred.
#[derive(Debug, Clone, Facet)]
pub struct MacroDef {
    pub name: String,
    /// Default value at preset load.
    #[facet(default)]
    pub default: f32,
    /// Targets each describe a path + scale; the macro's value modulates
    /// all targets simultaneously.
    #[facet(default)]
    pub targets: Vec<MacroTarget>,
}

#[derive(Debug, Clone, Facet)]
pub struct MacroTarget {
    /// `"engines.kick.layers.in.gain_db"` style path.
    pub path: String,
    /// Range scaler — at macro=0 target=`min`, at macro=1 target=`max`.
    #[facet(default)]
    pub min: f32,
    #[facet(default)]
    pub max: f32,
}
