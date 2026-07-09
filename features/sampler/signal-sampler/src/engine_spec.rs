//! `.signalengine` file format — Engine = source + per-mic Layers.
//!
//! An Engine is a complete playable sound generator: one [`SamplerBlock`]
//! source plus N Layers, each subscribing to a mic from the source pack.
//! Layers carry per-mic gain/pan and an optional FX chain (which is empty
//! in factory Engines and gets populated when a user saves a processed
//! variant). The Engine is the unit of "a kick", "a piano", "a Wurlitzer".
//!
//! See the module-level docs in `signal-sampler` for the full
//! Preset → Engine → (Layer | Module | Block) → Pack hierarchy.
//!
//! # File shape
//!
//! ```styx
//! // signal-engine v1
//! name "MM2 Tama Bubinga Kick"
//! engine_type "Kick"
//! block {
//!     pack "../Packs/Kick/22x18'' Tama Bubinga.signalpack"
//!     overrides ()
//! }
//! layers (
//!     { id "in"   mic "Close" gain_db 0 pan 0 bypass false fx_chain () }
//!     { id "room" mic "Room"  gain_db 0 pan 0 bypass false fx_chain () }
//! )
//! ports (
//!     { id "main" from "in" }
//!     { id "room" from "room" }
//! )
//! voice {
//!     polyphony   8
//!     voice_steal "oldest"
//! }
//! ```

use std::path::Path;

use facet::Facet;

use crate::SamplerError;
use crate::block::ParamOverride;

/// Parsed `.signalengine` file. The Engine type discriminator (`Kick`,
/// `Snare`, `Piano`, …) lives in `engine_type`; the Engine runtime
/// dispatches per-type behavior off of that. Most fields default to
/// permissive values so factory Engines can be tiny.
#[derive(Debug, Clone, Facet)]
pub struct EngineSpec {
    pub name: String,
    #[facet(default)]
    pub description: String,
    /// `"Kick"`, `"Snare"`, `"Hi-Hat"`, `"Piano"`, `"Wurlitzer"`, …
    /// Lowercase / hyphenated where applicable. Future runtimes look this
    /// up in an Engine type registry to choose UI / voice semantics.
    #[facet(default)]
    pub engine_type: String,

    pub block: BlockRef,

    /// One Layer per mic the engine wants to expose. In factory Engines
    /// the `fx_chain` is empty — Layers are pure pass-throughs with
    /// gain/pan only. Users save processed variants by populating it.
    #[facet(default)]
    pub layers: Vec<EngineLayerSpec>,

    /// Named outputs that a Preset can route from. Each port references
    /// a Layer id. If empty, the Engine exposes a single implicit `main`
    /// port that sums all Layers.
    #[facet(default)]
    pub ports: Vec<PortSpec>,

    /// Voice manager configuration. Defaults: unlimited polyphony, oldest
    /// voice stolen on overflow.
    #[facet(default)]
    pub voice: VoiceConfig,
}

impl EngineSpec {
    pub fn from_file(path: &Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path)?;
        facet_styx::from_str(&text).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }
}

/// Reference to the source pack + block-level overrides applied at
/// SamplerBlock construction.
#[derive(Debug, Clone, Facet)]
pub struct BlockRef {
    /// Path to a `.signalpack`, relative to the `.signalengine` file.
    pub pack: String,
    #[facet(default)]
    pub overrides: Vec<ParamOverride>,
}

/// One Layer slot inside an Engine. Subscribes to one mic of the source
/// pack and applies per-mic gain/pan. The optional FX chain is parsed
/// but not yet processed at the audio rate (FX runtime is a follow-up;
/// see crate docs).
#[derive(Debug, Clone, Facet)]
pub struct EngineLayerSpec {
    /// Stable identifier within the Engine (`"in"`, `"out"`, `"room"`, …).
    /// Preset routing addresses this id to expose a Layer's audio.
    pub id: String,
    /// Mic id from the source pack's `LibrarySpec.mics`. When the pack
    /// only has one (or zero) mics, set to `"default"` — the renderer
    /// folds everything to one output regardless.
    #[facet(default)]
    pub mic: String,
    /// Per-Layer gain in dB. Default 0 dB.
    #[facet(default)]
    pub gain_db: f32,
    /// Per-Layer pan in [-1.0, 1.0]. Default 0 (centre).
    #[facet(default)]
    pub pan: f32,
    /// When `true`, the Layer's audio contribution is silenced (still
    /// routed for graph purposes — useful for live-mode mute/unmute).
    #[facet(default)]
    pub bypass: bool,
    /// FX chain applied to the Layer's audio. Empty in factory Engines.
    /// Runtime is deferred — fields parse and round-trip but no audio
    /// processing happens until concrete FX blocks land.
    #[facet(default)]
    pub fx_chain: Vec<FxChainSlot>,
}

/// One FX block in a Layer's chain. Runtime not yet implemented; this
/// stores the configuration so it survives round-trips.
#[derive(Debug, Clone, Facet)]
pub struct FxChainSlot {
    /// Discriminator: `"compressor"`, `"eq"`, `"reverb"`, …
    pub fx_type: String,
    #[facet(default)]
    pub overrides: Vec<ParamOverride>,
    #[facet(default)]
    pub bypass: bool,
}

/// One audio port on the Engine — names a Layer that a Preset can route
/// from.
#[derive(Debug, Clone, Facet)]
pub struct PortSpec {
    pub id: String,
    /// Layer id this port exposes.
    pub from: String,
}

/// Voice-manager configuration. Empty/default fields use the engine runtime's
/// default limits and release-first/quietest stealing behavior.
#[derive(Debug, Clone, Default, Facet)]
pub struct VoiceConfig {
    /// Maximum simultaneous voices. 0 = engine default.
    #[facet(default)]
    pub polyphony: u32,
    /// `"oldest"`, `"quietest"`, `"same-note-first"`, `"none"`. Empty string =
    /// engine default.
    #[facet(default)]
    pub voice_steal: String,
}
