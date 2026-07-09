//! `.signalmodule` file format — reusable bus / processing nodes.
//!
//! A Module is a named processing block with declared input + output
//! ports and an optional FX chain. For v1 the runtime is the simplest
//! possible thing: every input sums into one internal mix, output
//! buffers each receive a copy of that mix. The FX chain is **parse-only**
//! — fields round-trip but no audio processing happens.
//!
//! # File shape
//!
//! ```styx
//! // signal-module v1
//! name "Drum Bus"
//! description "Parallel drum sub-bus."
//! inputs ( { id "in" } )
//! outputs ( { id "out" } )
//! fx_chain ()
//! ```

use std::path::Path;

use facet::Facet;

use crate::SamplerError;
use crate::engine_spec::FxChainSlot;

#[derive(Debug, Clone, Facet)]
pub struct ModuleSpec {
    pub name: String,
    #[facet(default)]
    pub description: String,

    /// Named input ports the routing graph can target.
    #[facet(default)]
    pub inputs: Vec<ModulePort>,

    /// Named output ports the routing graph can source from.
    #[facet(default)]
    pub outputs: Vec<ModulePort>,

    /// FX chain applied between input mix and output. Parse-only for v1.
    #[facet(default)]
    pub fx_chain: Vec<FxChainSlot>,
}

#[derive(Debug, Clone, Facet)]
pub struct ModulePort {
    pub id: String,
}

impl ModuleSpec {
    pub fn from_file(path: &Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path)?;
        facet_styx::from_str(&text).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }

    /// Synthesize a default stereo passthrough module — one `in` port,
    /// one `out` port. Used as a fallback when a Preset references a
    /// module file that fails to parse.
    pub fn passthrough(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            inputs: vec![ModulePort { id: "in".into() }],
            outputs: vec![ModulePort { id: "out".into() }],
            fx_chain: Vec::new(),
        }
    }
}
