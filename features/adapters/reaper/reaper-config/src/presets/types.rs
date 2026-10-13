//! Type definitions for REAPER plugin preset files.

use serde::{Deserialize, Serialize};

/// A single preset entry stored in a numbered `[N]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preset {
    /// Human-readable preset name (`Name=`).
    pub name: String,
    /// Declared byte length of the binary state (`Len=`).
    pub len: u32,
    /// Hex-encoded binary plugin state (`Data=`).
    pub data: String,
}

/// The complete contents of a plugin preset `.ini` file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPresets {
    /// Timestamp of the last default-preset import (`LastDefImpTime=`).
    pub last_def_imp_time: u64,
    /// Declared preset count (`NbPresets=`).
    ///
    /// After a successful parse this equals `presets.len()`, but the raw
    /// value is preserved here for round-trip accuracy.
    pub nb_presets: u32,
    /// All preset entries in index order.
    pub presets: Vec<Preset>,
}
