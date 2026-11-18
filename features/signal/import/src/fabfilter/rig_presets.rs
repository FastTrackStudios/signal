//! Worship rig `FabFilter` preset constants.
//!
//! Maps each rig block role to a specific `FabFilter` snapshot. All snapshot IDs
//! are computed deterministically via the same UUID v5 scheme used at import
//! time, so these constants remain valid across re-imports.
//!
//! # ID computation
//!
//! ```text
//! preset_uuid = uuid_v5(IMPORT_NAMESPACE, "FabFilter:{plugin_name}")
//! snap_uuid   = uuid_v5(preset_uuid,      "{folder}/{name}" | "{name}")
//! ```
//!
//! See `signal_import::fabfilter_snapshot_id` (in `reaper_fabfilter_load.rs`)
//! for the reference implementation.

use signal_proto::SnapshotId;
use uuid::Uuid;

use crate::IMPORT_NAMESPACE;

/// A specific `FabFilter` preset snapshot selected for a rig block role.
pub struct RigPreset {
    /// Display name for diagnostic output.
    pub name: &'static str,
    /// `FabFilter` plugin name (must match the directory under `~/Documents/FabFilter/Presets/`).
    pub plugin_name: &'static str,
    /// Snapshot name (file stem of the `.ffp` file).
    pub snapshot_name: &'static str,
    /// Optional subfolder within the plugin's preset directory.
    pub folder: Option<&'static str>,
}

impl RigPreset {
    /// Compute the deterministic snapshot ID for this rig preset.
    #[must_use]
    pub fn snapshot_id(&self) -> SnapshotId {
        let preset_uuid = Uuid::new_v5(
            &IMPORT_NAMESPACE,
            format!("FabFilter:{}", self.plugin_name).as_bytes(),
        );
        let snap_key = match self.folder {
            Some(f) => format!("{f}/{}", self.snapshot_name),
            None => self.snapshot_name.to_string(),
        };
        let snap_uuid = Uuid::new_v5(&preset_uuid, snap_key.as_bytes());
        SnapshotId::from(snap_uuid.to_string())
    }
}

// ─── Input module ────────────────────────────────────────────────────────────

/// Pro-G gate for the Input module.
///
/// "Guitar Before Distortion" — a clean gate tuned for guitar pickup signals
/// that prevents low-level hum and string noise when not playing.
pub const INPUT_GATE: RigPreset = RigPreset {
    name: "Input Gate",
    plugin_name: "Pro-G",
    snapshot_name: "Guitar Before Distortion",
    folder: Some("Guitar"),
};

// ─── Amp module ──────────────────────────────────────────────────────────────

/// Pro-Q 4 EQ for the Amp module.
///
/// "Forward Bite" — guitar-specific shape EQ that adds presence and mid-range
/// definition, complementing the character of an amp sim.
pub const AMP_EQ: RigPreset = RigPreset {
    name: "Amp EQ",
    plugin_name: "Pro-Q 4",
    snapshot_name: "Forward Bite",
    folder: Some("Guitar"),
};

/// Pro-R 2 reverb for the Amp module (short room sound).
///
/// "Colored Room A" — a small, colored room that adds subtle spatial depth to
/// an amp sim without washing out the dry signal.
pub const AMP_ROOM: RigPreset = RigPreset {
    name: "Amp Room",
    plugin_name: "Pro-R 2",
    snapshot_name: "Colored Room A",
    folder: Some("_2 Small"),
};

// ─── Modulation module ───────────────────────────────────────────────────────

/// Timeless 3 chorus preset for the Modulation module.
///
/// "Tele Roomy Chorus bM" — a warm, organic chorus that adds shimmer and
/// width ideal for worship electric guitar tones.
pub const MOD_CHORUS: RigPreset = RigPreset {
    name: "Modulation Chorus",
    plugin_name: "Timeless 3",
    snapshot_name: "Tele Roomy Chorus bM",
    folder: Some("_05 Modulation"),
};

// ─── Time module ─────────────────────────────────────────────────────────────

/// Timeless 3 delay preset for the Time module.
///
/// "Basic - Modern Delay bM" — a clean, modern tempo-synced delay.
pub const TIME_DELAY: RigPreset = RigPreset {
    name: "Time Delay",
    plugin_name: "Timeless 3",
    snapshot_name: "Basic - Modern Delay bM",
    folder: Some("_01 Medium"),
};

/// Pro-R 2 hall reverb for the Time module.
///
/// "Medium Hall 1" — a natural medium hall reverb for lush sustain and
/// ambiance in the time effects section.
pub const TIME_HALL: RigPreset = RigPreset {
    name: "Time Hall Reverb",
    plugin_name: "Pro-R 2",
    snapshot_name: "Medium Hall 1",
    folder: Some("_3 Medium"),
};

/// All rig presets in definition order, for bulk import verification.
pub const ALL_RIG_PRESETS: &[&RigPreset] = &[
    &INPUT_GATE,
    &AMP_EQ,
    &AMP_ROOM,
    &MOD_CHORUS,
    &TIME_DELAY,
    &TIME_HALL,
];
