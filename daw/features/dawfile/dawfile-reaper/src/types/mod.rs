//! # REAPER Types
//!
//! REAPER-specific data structures and types for the RPP file format.
//! These modules provide strongly-typed representations of REAPER's
//! data structures, built on top of the generic parsing primitives.
//!
//! ## Modules
//!
//! - **`track`**: Track data structures and parsing
//! - **`item`**: Media item data structures and parsing
//! - **`envelope`**: Envelope data structures and parsing
//! - **`fx_chain`**: FX chain data structures and parsing
//! - **`project`**: REAPER project data structures and parsing
//!
//! ## Architecture
//!
//! These types form the domain layer of RPP parsing:
//! 1. **Primitives** provide generic RPP parsing
//! 2. **Types** provide REAPER-specific data structures
//! 3. **Conversion** from primitives to types happens here
//!
//! ## Example
//!
//! ```rust
//! use dawfile_reaper::{parse_rpp_file, ReaperProject};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let rpp_content = r#"<REAPER_PROJECT 0.1 "6.75/linux-x86_64" 1681651369
//!       <TRACK
//!         NAME "Track 1"
//!         VOL 1.0 0.0
//!       >
//!     >"#;
//!
//!     let project = parse_rpp_file(rpp_content)?;
//!     let reaper_project = ReaperProject::from_rpp_project(&project)?;
//!     // Now you have strongly-typed REAPER data structures!
//!     Ok(())
//! }
//! ```

pub mod envelope;
pub mod fx_chain;
pub mod item;
pub mod marker_region;
pub mod project;
pub mod serialize;
pub mod time_pos_utils;
pub mod time_tempo;
pub mod track;
pub mod track_ops;

// Re-export the main types for convenience
pub use envelope::Envelope;
pub use fx_chain::{
    FxChain, FxChainNode, FxContainer, FxEnvelopePoint, FxParamEnvelope, FxParamRef, FxPlugin,
    JsParamValue, PluginType, parse_js_params,
};
pub use item::{
    Item, MidiEvent, MidiEventType, MidiSource, MidiSourceEvent, SourceBlock, SourceType,
    StretchMarker,
};
pub use marker_region::{MarkerRegion, MarkerRegionCollection};
pub use project::{DecodeOptions, ReaperProject};
pub use serialize::RppSerialize;
pub use time_pos_utils::{
    time_to_beat_position, time_to_beat_position_structured,
    time_to_beat_position_structured_with_envelope, time_to_beat_position_with_envelope,
};
pub use time_tempo::{TempoTimeEnvelope, TempoTimePoint};
pub use track::{Track, TrackParseOptions};
pub use track_ops::{
    TrackRole, classify_track, group_by_names, group_by_predicate, group_into_folder,
    move_into_existing_folder, organize_into_fts_hierarchy, wrap_in_folder,
};
