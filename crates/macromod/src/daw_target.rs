//! DAW-level parameter target — addresses a specific FX parameter in the DAW.
//!
//! Unlike `ParamTarget` which uses Signal's abstract block/param IDs,
//! `DawParamTarget` references a REAPER FX parameter directly by
//! track GUID, FX index, and parameter index. Used by the arm/learn
//! workflow to bind macros to arbitrary FX parameters discovered via
//! `GetLastTouchedFX`.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// Identifies a specific FX parameter in the DAW.
///
/// This is the raw DAW coordinate system — no Signal abstraction layer.
/// Created during the arm/learn workflow from `LastTouchedFx` data.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
pub struct DawParamTarget {
    /// GUID of the track containing the FX.
    pub track_guid: String,
    /// Index of the FX in the chain (0-based).
    pub fx_index: u32,
    /// Index of the parameter (0-based).
    pub param_index: u32,
    /// Whether the FX is in the input FX chain.
    pub is_input_fx: bool,
}

impl DawParamTarget {
    pub fn new(track_guid: impl Into<String>, fx_index: u32, param_index: u32) -> Self {
        Self {
            track_guid: track_guid.into(),
            fx_index,
            param_index,
            is_input_fx: false,
        }
    }

    pub fn input_fx(
        track_guid: impl Into<String>,
        fx_index: u32,
        param_index: u32,
    ) -> Self {
        Self {
            track_guid: track_guid.into(),
            fx_index,
            param_index,
            is_input_fx: true,
        }
    }
}
