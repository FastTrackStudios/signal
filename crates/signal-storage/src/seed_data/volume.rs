//! Volume block presets — modeled after volume pedals and utility gain stages.
//!
//! Each preset corresponds to a real pedal with its actual knob layout.
//! Parameter IDs match the physical control names. Values are normalized
//! 0.0–1.0 where 0.5 ≈ noon on the dial.

use signal_proto::{metadata::Metadata, seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};

pub fn presets() -> Vec<Preset> {
    vec![volume_pedal(), utility_gain()]
}

// ─── Volume Pedal ───────────────────────────────────────────────
// Knobs: Level, Curve

fn vol_pedal_block(level: f32, curve: f32) -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("level", "Level", level),
        BlockParameter::new("curve", "Curve", curve),
    ])
}

fn volume_pedal() -> Preset {
    Preset::new(
        seed_id("volume-pedal"),
        "Volume Pedal",
        BlockType::Volume,
        // Default: full level, linear taper
        Snapshot::new(
            seed_id("volume-pedal-default"),
            "Default",
            vol_pedal_block(1.0, 0.50),
        ),
        vec![
            // Swell — start from silence for violin-like swells
            Snapshot::new(
                seed_id("volume-pedal-swell"),
                "Swell",
                vol_pedal_block(0.0, 0.70),
            ),
            // Cut — reduced level for rhythm drop-downs
            Snapshot::new(
                seed_id("volume-pedal-cut"),
                "Cut",
                vol_pedal_block(0.60, 0.50),
            ),
        ],
    )
}

// ─── Utility Gain ───────────────────────────────────────────────
// Knobs: Level

fn utility_block(level: f32) -> Block {
    Block::from_parameters(vec![BlockParameter::new("level", "Level", level)])
}

fn utility_gain() -> Preset {
    Preset::new(
        seed_id("volume-utility"),
        "Utility Gain",
        BlockType::Volume,
        // Default: unity gain (0.5 maps to 0 dB)
        Snapshot::new(
            seed_id("volume-utility-default"),
            "Unity",
            utility_block(0.50),
        ),
        vec![
            // Boost — push signal level up for leads
            Snapshot::new(
                seed_id("volume-utility-boost"),
                "Boost",
                utility_block(0.75),
            ),
            // Pad — attenuate signal for quieter passages
            Snapshot::new(seed_id("volume-utility-pad"), "Pad", utility_block(0.25)),
        ],
    )
    .with_metadata(Metadata::new().with_tag("source:JS: Volume/Pan v3"))
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_preset_count() {
        let presets = presets();
        assert_eq!(presets.len(), 2);
    }

    #[test]
    fn all_volume_presets_are_volume_type() {
        for preset in presets() {
            assert_eq!(preset.block_type(), BlockType::Volume);
        }
    }

    #[test]
    fn volume_pedal_has_3_snapshots() {
        let vp = &presets()[0];
        assert_eq!(vp.name(), "Volume Pedal");
        assert_eq!(vp.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn utility_gain_has_3_snapshots() {
        let ug = &presets()[1];
        assert_eq!(ug.name(), "Utility Gain");
        assert_eq!(ug.snapshots().len(), 3); // default + 2
    }

    #[test]
    fn volume_pedal_has_2_params() {
        for snap in presets()[0].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                2,
                "Volume Pedal should have 2 params"
            );
        }
    }

    #[test]
    fn utility_gain_has_1_param() {
        for snap in presets()[1].snapshots() {
            assert_eq!(
                snap.block().parameters().len(),
                1,
                "Utility Gain should have 1 param"
            );
        }
    }

    #[test]
    fn volume_pedal_swell_starts_at_zero() {
        let vp = &presets()[0];
        let swell = vp.snapshots().iter().find(|s| s.name() == "Swell").unwrap();
        let block = swell.block();
        let params = block.parameters();
        let level = params.iter().find(|p| p.id() == "level").unwrap();
        assert!((level.value().get() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parameter_values_in_range() {
        for preset in presets() {
            for snapshot in preset.snapshots() {
                let block = snapshot.block();
                for param in block.parameters() {
                    let v = param.value().get();
                    assert!(
                        (0.0..=1.0).contains(&v),
                        "preset '{}' snapshot '{}' param '{}' value {} out of range",
                        preset.name(),
                        snapshot.name(),
                        param.id(),
                        v,
                    );
                }
            }
        }
    }

    #[test]
    fn preset_ids_are_unique() {
        let presets = presets();
        let mut ids = std::collections::HashSet::new();
        for preset in &presets {
            assert!(
                ids.insert(preset.id().to_string()),
                "duplicate preset id: {}",
                preset.id()
            );
        }
    }

    #[test]
    fn snapshot_ids_globally_unique() {
        let presets = presets();
        let mut ids = std::collections::HashSet::new();
        for preset in &presets {
            for snapshot in preset.snapshots() {
                assert!(
                    ids.insert(snapshot.id().to_string()),
                    "duplicate snapshot id: {}",
                    snapshot.id()
                );
            }
        }
    }
}
