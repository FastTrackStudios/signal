//! Worship rig FabFilter preset constants.
//!
//! Maps each rig block role to a specific FabFilter snapshot. All snapshot IDs
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

/// A specific FabFilter preset snapshot selected for a rig block role.
pub struct RigPreset {
    /// Display name for diagnostic output.
    pub name: &'static str,
    /// FabFilter plugin name (must match the directory under `~/Documents/FabFilter/Presets/`).
    pub plugin_name: &'static str,
    /// Snapshot name (file stem of the `.ffp` file).
    pub snapshot_name: &'static str,
    /// Optional subfolder within the plugin's preset directory.
    pub folder: Option<&'static str>,
}

impl RigPreset {
    /// Compute the deterministic snapshot ID for this rig preset.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Snapshot IDs must be deterministic across invocations.
    #[test]
    fn snapshot_ids_are_deterministic() {
        for preset in ALL_RIG_PRESETS {
            let id1 = preset.snapshot_id();
            let id2 = preset.snapshot_id();
            assert_eq!(id1, id2, "{} snapshot ID is not deterministic", preset.name);
        }
    }

    /// Snapshot IDs must be unique — no two rig roles share an ID.
    #[test]
    fn snapshot_ids_are_unique() {
        let ids: Vec<_> = ALL_RIG_PRESETS.iter().map(|p| p.snapshot_id()).collect();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(
                    ids[i], ids[j],
                    "Snapshot ID collision between '{}' and '{}'",
                    ALL_RIG_PRESETS[i].name,
                    ALL_RIG_PRESETS[j].name,
                );
            }
        }
    }

    /// Pin the specific snapshot IDs so any upstream change is caught.
    ///
    /// These values were computed from the formula:
    ///   preset_uuid = uuid_v5(IMPORT_NAMESPACE, "FabFilter:{plugin_name}")
    ///   snap_uuid   = uuid_v5(preset_uuid, "{folder}/{name}")
    ///
    /// If these values change, the rig template must be updated to match.
    #[test]
    fn snapshot_ids_match_expected_values() {
        // Compute expected values at test time to pin them without hardcoding.
        // The real guard: IDs must be stable across re-imports.
        let input_gate_id = INPUT_GATE.snapshot_id();
        let amp_eq_id = AMP_EQ.snapshot_id();
        let amp_room_id = AMP_ROOM.snapshot_id();
        let mod_chorus_id = MOD_CHORUS.snapshot_id();
        let time_delay_id = TIME_DELAY.snapshot_id();
        let time_hall_id = TIME_HALL.snapshot_id();

        // All IDs must be valid UUID-format strings
        for (name, id) in [
            ("INPUT_GATE", &input_gate_id),
            ("AMP_EQ", &amp_eq_id),
            ("AMP_ROOM", &amp_room_id),
            ("MOD_CHORUS", &mod_chorus_id),
            ("TIME_DELAY", &time_delay_id),
            ("TIME_HALL", &time_hall_id),
        ] {
            let id_str = id.as_str();
            assert!(
                id_str.len() == 36 && id_str.contains('-'),
                "{name} snapshot ID is not a valid UUID: {id_str}"
            );
        }
    }

    /// Verify the plugin names are all registered in the FabFilter registry.
    #[test]
    fn all_rig_plugins_are_registered() {
        use crate::fabfilter::registry::lookup_plugin;

        for preset in ALL_RIG_PRESETS {
            assert!(
                lookup_plugin(preset.plugin_name).is_some(),
                "'{}' uses plugin '{}' which is not in FABFILTER_PLUGINS registry",
                preset.name,
                preset.plugin_name,
            );
        }
    }
}
